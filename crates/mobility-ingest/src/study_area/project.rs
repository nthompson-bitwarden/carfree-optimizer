use super::*;

const MAX_PROJECT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SCENARIOS: usize = 10_000;

fn scenario_row(scenario: &ScenarioRecord) -> String {
    format!(
        "scenario\t{}\t{}\t{}\n",
        scenario.id, scenario.baseline_id, scenario.stale
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioRecord {
    pub id: String,
    pub baseline_id: String,
    pub stale: bool,
}
#[derive(Debug, Clone)]
pub struct CityProject {
    id: String,
    area: StudyArea,
    snapshot_date: String,
    baseline_id: String,
    revision: u64,
    import_attempts: u64,
    coverage: CoverageReport,
    scenarios: Vec<ScenarioRecord>,
    // Derived, never persisted: keeps scenario additions independent of geometry
    // and potentially large metadata sizes. Recomputed on initialize/expansion;
    // successful loading records the verified canonical input length.
    encoded_bytes: usize,
}
pub fn validate_date(date: &str) -> Result<(), OnboardingError> {
    let b = date.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Err(err("snapshot date must be YYYY-MM-DD"));
    }
    let y = date[..4].parse::<u16>().map_err(|_| err("invalid year"))?;
    let m = date[5..7]
        .parse::<usize>()
        .map_err(|_| err("invalid month"))?;
    let d = date[8..].parse::<u8>().map_err(|_| err("invalid day"))?;
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let months = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if y == 0 || !(1..=12).contains(&m) || d == 0 || d > months[m - 1] {
        return Err(err("snapshot date is not a calendar date"));
    }
    Ok(())
}
fn validate_report(report: &CoverageReport) -> Result<(), OnboardingError> {
    let mut seen = Vec::new();
    for e in &report.entries {
        if seen.contains(&e.kind) {
            return Err(err("duplicate dataset coverage kind"));
        }
        seen.push(e.kind);
        text(&e.detail)?;
        for v in [&e.source, &e.version].into_iter().flatten() {
            text(v)?
        }
        if e.year == Some(0) {
            return Err(err("invalid source year"));
        }
        if let Some([w, s, x, n]) = e.extent {
            if [w, s, x, n].iter().any(|v| !v.is_finite())
                || w >= x
                || s >= n
                || w < -180.0
                || x > 180.0
                || s < -90.0
                || n > 90.0
            {
                return Err(err("invalid dataset spatial extent"));
            }
        }
        if e.status == DatasetStatus::Available
            && (e.source.is_none() || e.version.is_none() || e.year.is_none() || e.extent.is_none())
        {
            return Err(err(
                "available dataset requires source, version, year and extent",
            ));
        }
    }
    if seen.len() != 5 {
        return Err(err("coverage report requires exactly Boundary, Roads, Transit, Population and Demand; omitted datasets must be explicitly Missing"));
    }
    let expected: Vec<_> = report
        .entries
        .iter()
        .filter(|e| e.status != DatasetStatus::Available)
        .map(|e| e.kind)
        .collect();
    if report.missing_required != expected {
        return Err(err(
            "missing required datasets disagree with coverage statuses",
        ));
    }
    Ok(())
}
impl CityProject {
    pub fn initialize(
        id: &str,
        area: StudyArea,
        snapshot_date: &str,
        importer: &mut impl Importer,
    ) -> Result<Self, OnboardingError> {
        validate_project_id(id)?;
        validate_date(snapshot_date)?;
        let coverage = importer.ingest(&area, snapshot_date)?;
        validate_report(&coverage)?;
        let mut p = Self {
            id: id.into(),
            area,
            snapshot_date: snapshot_date.into(),
            baseline_id: String::new(),
            revision: 1,
            import_attempts: 1,
            coverage,
            scenarios: Vec::new(),
            encoded_bytes: 0,
        };
        p.baseline_id = p.fingerprint();
        p.check_encoded_size()?;
        Ok(p)
    }
    pub fn expand_buffer(
        &mut self,
        km: f64,
        importer: &mut impl Importer,
    ) -> Result<(), OnboardingError> {
        let area = self.area.buffer_envelope(km)?;
        if km < self.area.buffer_km {
            return Err(err("buffer expansion cannot shrink the existing buffer"));
        }
        if area == self.area {
            return Ok(());
        }
        let coverage = importer.ingest(&area, &self.snapshot_date)?;
        validate_report(&coverage)?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| err("revision overflow"))?;
        let attempts = self
            .import_attempts
            .checked_add(1)
            .ok_or_else(|| err("import attempt overflow"))?;
        // Stage the complete next state. An importer may return valid but very
        // large metadata, so schema-size validation must precede any mutation.
        let mut next = self.clone();
        next.area = area;
        next.coverage = coverage;
        next.revision = revision;
        next.import_attempts = attempts;
        next.baseline_id = next.fingerprint();
        for scenario in &mut next.scenarios {
            scenario.stale = true;
        }
        next.check_encoded_size()?;
        *self = next;
        Ok(())
    }
    pub fn add_scenario(&mut self, id: &str) -> Result<(), OnboardingError> {
        validate_project_id(id)?;
        if self.scenarios.iter().any(|s| s.id == id) {
            return Err(err("scenario ID already exists"));
        }
        if self.scenarios.len() >= MAX_SCENARIOS {
            return Err(err("project cannot exceed 10000 scenarios"));
        }
        let scenario = ScenarioRecord {
            id: id.into(),
            baseline_id: self.baseline_id.clone(),
            stale: false,
        };
        // The only other serialized change is the scenario-count header. The
        // shared encoder avoids copying/reformatting the entire project here.
        let count_digits = self.scenarios.len().to_string().len();
        let next_count_digits = (self.scenarios.len() + 1).to_string().len();
        let next_bytes = self
            .encoded_bytes
            .checked_add(scenario_row(&scenario).len())
            .and_then(|n| n.checked_add(next_count_digits - count_digits))
            .ok_or_else(|| err("project encoded size overflow"))?;
        if next_bytes > MAX_PROJECT_BYTES {
            return Err(err("project exceeds 16 MiB schema limit"));
        }
        self.scenarios.push(scenario);
        self.encoded_bytes = next_bytes;
        Ok(())
    }
    fn check_encoded_size(&mut self) -> Result<(), OnboardingError> {
        let bytes = self.to_tsv().len();
        if bytes > MAX_PROJECT_BYTES {
            return Err(err("project exceeds 16 MiB schema limit"));
        }
        self.encoded_bytes = bytes;
        Ok(())
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn area(&self) -> &StudyArea {
        &self.area
    }
    pub fn snapshot_date(&self) -> &str {
        &self.snapshot_date
    }
    pub fn baseline_id(&self) -> &str {
        &self.baseline_id
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn import_attempts(&self) -> u64 {
        self.import_attempts
    }
    pub fn coverage(&self) -> &CoverageReport {
        &self.coverage
    }
    pub fn scenarios(&self) -> &[ScenarioRecord] {
        &self.scenarios
    }
    fn content(&self) -> String {
        let a = &self.area;
        let mut out = format!(
            "project\t{}\t{}\t{}\t{}\narea\t{}\t{}\t{}\t{}\t{}\t{}\n",
            self.id,
            self.snapshot_date,
            self.revision,
            self.import_attempts,
            a.id,
            a.name,
            a.crs,
            a.region.country,
            a.region.states.join(","),
            a.buffer_km
        );
        match &a.source {
            None => out.push_str("source\tnone\n"),
            Some(s) => out.push_str(&format!(
                "source\t{}\t{}\t{}\t{}\n",
                s.url, s.version, s.retrieved, s.license
            )),
        }
        for (tag, rings) in [("core", &a.core_rings), ("boundary", &a.rings)] {
            out.push_str(&format!("{tag}\t{}\n", rings.len()));
            for r in rings {
                out.push_str("ring");
                for p in r {
                    out.push_str(&format!("\t{},{}", p.longitude, p.latitude))
                }
                out.push('\n')
            }
        }
        out.push_str(&format!("coverage\t{}\n", self.coverage.entries.len()));
        for e in &self.coverage.entries {
            let opt = |s: &Option<String>| {
                s.as_ref()
                    .map(|x| format!("={x}"))
                    .unwrap_or_else(|| "-".into())
            };
            let extent = e
                .extent
                .map(|x| format!("{},{},{},{}", x[0], x[1], x[2], x[3]))
                .unwrap_or_else(|| "-".into());
            out.push_str(&format!(
                "dataset\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                e.kind.as_str(),
                e.status.as_str(),
                opt(&e.source),
                opt(&e.version),
                e.year.map(|x| x.to_string()).unwrap_or_else(|| "-".into()),
                extent,
                e.detail
            ))
        }
        out.push_str(&format!(
            "missing\t{}\n",
            self.coverage
                .missing_required
                .iter()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ));
        out
    }
    // Stable FNV-1a checksum is a reproducibility identifier, not a security digest.
    fn fingerprint(&self) -> String {
        let mut h = 14695981039346656037_u64;
        for b in self.content().bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(1099511628211)
        }
        format!("baseline-{:016x}-r{}", h, self.revision)
    }
    pub fn to_tsv(&self) -> String {
        let mut out = String::from("carfree-study-project-v1\n");
        out.push_str(&self.content());
        out.push_str(&format!(
            "baseline\t{}\nscenarios\t{}\n",
            self.baseline_id,
            self.scenarios.len()
        ));
        for s in &self.scenarios {
            out.push_str(&scenario_row(s))
        }
        out
    }
    pub fn from_tsv(input: &str) -> Result<Self, OnboardingError> {
        if input.len() > MAX_PROJECT_BYTES {
            return Err(err("project exceeds 16 MiB schema limit"));
        }
        let mut lines = input.lines();
        if lines.next() != Some("carfree-study-project-v1") {
            return Err(err("expected carfree-study-project-v1 schema"));
        }
        let p = row(&mut lines, "project", 5)?;
        validate_project_id(p[1])?;
        validate_date(p[2])?;
        let revision = num(p[3])?;
        let import_attempts = num(p[4])?;
        if revision == 0 || revision != import_attempts {
            return Err(err("invalid revision/import attempts"));
        }
        let a = row(&mut lines, "area", 7)?;
        let buffer = num::<f64>(a[6])?;
        let src = lines
            .next()
            .ok_or_else(|| err("missing source"))?
            .split('\t')
            .collect::<Vec<_>>();
        let source = match src.as_slice() {
            ["source", "none"] => None,
            ["source", url, version, retrieved, license] => {
                for s in [url, version, retrieved, license] {
                    text(s)?
                }
                validate_date(retrieved)?;
                Some(Source {
                    url: (*url).into(),
                    version: (*version).into(),
                    retrieved: (*retrieved).into(),
                    license: (*license).into(),
                })
            }
            _ => return Err(err("invalid source record")),
        };
        if let Some(source) = &source {
            source.validate()?;
        }
        let cores = read_rings(&mut lines, "core")?;
        let boundary = read_rings(&mut lines, "boundary")?;
        let mut area = StudyArea::drawn(
            a[1],
            a[2],
            Region {
                country: a[4].into(),
                states: a[5].split(',').map(str::to_owned).collect(),
            },
            a[3],
            cores,
        )?;
        area.source = source;
        area = area.buffer_envelope(buffer)?;
        if area.rings != boundary {
            return Err(err(
                "saved boundary does not match original core and buffer",
            ));
        }
        let c = row(&mut lines, "coverage", 2)?;
        let count = num::<usize>(c[1])?;
        if count > 5 {
            return Err(err("too many coverage entries"));
        }
        let mut entries = Vec::new();
        for _ in 0..count {
            let e = row(&mut lines, "dataset", 8)?;
            let optional = |s: &str| -> Result<Option<String>, OnboardingError> {
                if s == "-" {
                    Ok(None)
                } else if let Some(v) = s.strip_prefix('=') {
                    text(v)?;
                    Ok(Some(v.into()))
                } else {
                    Err(err("invalid optional metadata"))
                }
            };
            let extent = if e[6] == "-" {
                None
            } else {
                let v = e[6]
                    .split(',')
                    .map(num::<f64>)
                    .collect::<Result<Vec<_>, _>>()?;
                Some(
                    v.try_into()
                        .map_err(|_| err("extent needs west,south,east,north"))?,
                )
            };
            entries.push(CoverageEntry {
                kind: kind(e[1])?,
                status: status(e[2])?,
                source: optional(e[3])?,
                version: optional(e[4])?,
                year: if e[5] == "-" { None } else { Some(num(e[5])?) },
                extent,
                detail: e[7].into(),
            })
        }
        let m = row(&mut lines, "missing", 2)?;
        let missing_required = if m[1].is_empty() {
            vec![]
        } else {
            m[1].split(',').map(kind).collect::<Result<Vec<_>, _>>()?
        };
        let coverage = CoverageReport {
            entries,
            missing_required,
        };
        validate_report(&coverage)?;
        let baseline = row(&mut lines, "baseline", 2)?[1].to_owned();
        let s = row(&mut lines, "scenarios", 2)?;
        let count = num::<usize>(s[1])?;
        if count > MAX_SCENARIOS {
            return Err(err("too many scenarios"));
        }
        let mut scenarios: Vec<ScenarioRecord> = Vec::new();
        for _ in 0..count {
            let s = row(&mut lines, "scenario", 4)?;
            validate_project_id(s[1])?;
            if scenarios.iter().any(|r| r.id == s[1]) {
                return Err(err("duplicate scenario ID"));
            }
            let stale = match s[3] {
                "true" => true,
                "false" => false,
                _ => return Err(err("invalid stale flag")),
            };
            if !s[2].starts_with("baseline-") || stale == (s[2] == baseline) {
                return Err(err("scenario staleness disagrees with baseline"));
            }
            scenarios.push(ScenarioRecord {
                id: s[1].into(),
                baseline_id: s[2].into(),
                stale,
            })
        }
        if lines.next().is_some() {
            return Err(err("unexpected trailing project record"));
        }
        let project = Self {
            id: p[1].into(),
            area,
            snapshot_date: p[2].into(),
            baseline_id: baseline,
            revision,
            import_attempts,
            coverage,
            scenarios,
            encoded_bytes: input.len(),
        };
        if project.baseline_id != project.fingerprint() {
            return Err(err("baseline does not match persisted project inputs"));
        }
        if project.to_tsv() != input {
            return Err(err("project is not in canonical schema form"));
        }
        Ok(project)
    }
}
fn num<T: std::str::FromStr>(s: &str) -> Result<T, OnboardingError> {
    s.parse()
        .map_err(|_| err(format!("invalid numeric field: {s}")))
}
fn row<'a>(
    lines: &mut std::str::Lines<'a>,
    tag: &str,
    n: usize,
) -> Result<Vec<&'a str>, OnboardingError> {
    let fields = lines
        .next()
        .ok_or_else(|| err(format!("missing {tag} record")))?
        .split('\t')
        .collect::<Vec<_>>();
    if fields.len() != n || fields[0] != tag {
        return Err(err(format!("expected {tag} record with {n} fields")));
    }
    Ok(fields)
}
fn read_rings(
    lines: &mut std::str::Lines<'_>,
    tag: &str,
) -> Result<Vec<Vec<Coordinate>>, OnboardingError> {
    let count = num::<usize>(row(lines, tag, 2)?[1])?;
    if count == 0 || count > 10000 {
        return Err(err("invalid ring count"));
    }
    let mut rings = Vec::new();
    for _ in 0..count {
        let line = lines.next().ok_or_else(|| err("missing ring"))?;
        let mut fields = line.split('\t');
        if fields.next() != Some("ring") {
            return Err(err("expected ring record"));
        }
        let ring = fields
            .map(|s| {
                let (x, y) = s
                    .split_once(',')
                    .ok_or_else(|| err("vertex requires longitude,latitude"))?;
                Ok(Coordinate {
                    longitude: num(x)?,
                    latitude: num(y)?,
                })
            })
            .collect::<Result<Vec<_>, OnboardingError>>()?;
        rings.push(ring)
    }
    Ok(rings)
}
fn kind(s: &str) -> Result<DatasetKind, OnboardingError> {
    match s {
        "Boundary" => Ok(DatasetKind::Boundary),
        "Roads" => Ok(DatasetKind::Roads),
        "Transit" => Ok(DatasetKind::Transit),
        "Population" => Ok(DatasetKind::Population),
        "Demand" => Ok(DatasetKind::Demand),
        _ => Err(err("unknown dataset kind")),
    }
}
fn status(s: &str) -> Result<DatasetStatus, OnboardingError> {
    match s {
        "Available" => Ok(DatasetStatus::Available),
        "Missing" => Ok(DatasetStatus::Missing),
        "Outdated" => Ok(DatasetStatus::Outdated),
        "Partial" => Ok(DatasetStatus::Partial),
        "Failed" => Ok(DatasetStatus::Failed),
        _ => Err(err("unknown dataset status")),
    }
}
