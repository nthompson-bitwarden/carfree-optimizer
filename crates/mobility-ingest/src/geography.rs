//! E02-S01 canonical geography catalogue, schema v1.
//!
//! Coordinates are longitude/latitude degrees (EPSG:4326). Validation is
//! structural; polygon topology and suitability for analysis are not certified.
use std::collections::HashSet;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeographyKind {
    City,
    MetropolitanArea,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coordinate {
    pub longitude: f64,
    pub latitude: f64,
}

#[derive(Debug, Clone)]
pub struct Geography {
    pub id: String,
    pub name: String,
    pub kind: GeographyKind,
    pub states: Vec<String>,
    pub rings: Vec<Vec<Coordinate>>,
    pub source_url: String,
    pub vintage: String,
    pub retrieved_date: String,
    pub license: String,
}

#[derive(Debug)]
pub struct CatalogueError {
    pub line: usize,
    pub message: String,
}
impl fmt::Display for CatalogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}
impl std::error::Error for CatalogueError {}

#[derive(Debug, PartialEq, Eq)]
pub enum SearchError {
    InvalidQuery,
}
impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("enter a nonempty query of at most 256 bytes without control characters")
    }
}
impl std::error::Error for SearchError {}

/// A catalogue owns validated records; accessors return shared references.
pub struct Catalogue {
    entries: Vec<Geography>,
}

fn invalid(line: usize, message: impl Into<String>) -> CatalogueError {
    CatalogueError {
        line,
        message: message.into(),
    }
}

fn normalized(text: &str) -> String {
    text.to_ascii_lowercase()
        .chars()
        .map(|c| match c {
            ',' | '-' | ':' => ' ',
            other => other,
        })
        .collect()
}

fn is_state_token(token: &str) -> bool {
    "al ak az ar ca co ct de dc fl ga hi id il in ia ks ky la me md ma mi mn ms mo mt ne nv nh nj nm ny nc nd oh ok or pa ri sc sd tn tx ut vt va wa wv wi wy as gu mp pr vi um"
        .split_whitespace()
        .any(|state| state == token)
}

fn validate_area(area: &Geography, line: usize) -> Result<(), CatalogueError> {
    if area.rings.is_empty() {
        return Err(invalid(
            line,
            format!("{} needs at least one closed boundary ring", area.id),
        ));
    }
    Ok(())
}

impl Catalogue {
    /// Parse TSV schema v1 in O(input bytes), preserving every ring and vertex.
    pub fn from_tsv(input: &str) -> Result<Self, CatalogueError> {
        let mut lines = input.lines().enumerate();
        if lines.next().map(|(_, line)| line) != Some("carfree-geography-v1") {
            return Err(invalid(1, "expected schema header carfree-geography-v1"));
        }
        let mut entries: Vec<Geography> = Vec::new();
        let mut ids = HashSet::new();
        let mut area_line = 1;
        for (index, line) in lines {
            let number = index + 1;
            if line.trim().is_empty() {
                continue;
            }
            if line.chars().any(|c| c.is_control() && c != '\t') {
                return Err(invalid(number, "control characters are not permitted"));
            }
            let fields: Vec<_> = line.split('\t').collect();
            match fields[0] {
                "area" => {
                    if let Some(previous) = entries.last() {
                        validate_area(previous, area_line)?;
                    }
                    if fields.len() != 9 || fields[1..].iter().any(|f| f.trim().is_empty()) {
                        return Err(invalid(
                            number,
                            "area requires exactly 8 nonempty metadata fields",
                        ));
                    }
                    let (kind, prefix, digits) = match fields[2] {
                        "city" => (GeographyKind::City, "place:", 7),
                        "metro" => (GeographyKind::MetropolitanArea, "cbsa:", 5),
                        _ => return Err(invalid(number, "kind must be city or metro")),
                    };
                    let id_digits = fields[1].strip_prefix(prefix).ok_or_else(|| {
                        invalid(number, "canonical ID prefix must match geography kind")
                    })?;
                    if id_digits.len() != digits || !id_digits.bytes().all(|c| c.is_ascii_digit()) {
                        return Err(invalid(
                            number,
                            "canonical ID has an incorrect GEOID length or non-digit characters",
                        ));
                    }
                    if !ids.insert(fields[1].to_owned()) {
                        return Err(invalid(number, "duplicate canonical ID"));
                    }
                    let states: Vec<_> = fields[4].split(',').map(str::to_owned).collect();
                    if states.iter().any(|state| {
                        state.len() != 2 || !state.bytes().all(|c| c.is_ascii_uppercase())
                    }) {
                        return Err(invalid(
                            number,
                            "states must be comma-separated uppercase two-letter abbreviations",
                        ));
                    }
                    let host = fields[5]
                        .strip_prefix("https://")
                        .and_then(|rest| rest.split('/').next())
                        .unwrap_or("");
                    if host.is_empty() || fields[5].chars().any(char::is_whitespace) {
                        return Err(invalid(
                            number,
                            "source URL must use https with a nonempty host and no whitespace",
                        ));
                    }
                    entries.push(Geography {
                        id: fields[1].to_owned(),
                        name: fields[3].to_owned(),
                        kind,
                        states,
                        rings: Vec::new(),
                        source_url: fields[5].to_owned(),
                        vintage: fields[6].to_owned(),
                        retrieved_date: fields[7].to_owned(),
                        license: fields[8].to_owned(),
                    });
                    area_line = number;
                }
                "ring" => {
                    let area = entries
                        .last_mut()
                        .ok_or_else(|| invalid(number, "ring must follow an area record"))?;
                    if fields.len() < 5 {
                        return Err(invalid(number, "ring needs at least four longitude,latitude vertices including closure"));
                    }
                    let mut ring = Vec::with_capacity(fields.len() - 1);
                    for field in &fields[1..] {
                        let (lon, lat) = field
                            .split_once(',')
                            .ok_or_else(|| invalid(number, "vertex must be longitude,latitude"))?;
                        let point = Coordinate {
                            longitude: lon
                                .parse()
                                .map_err(|_| invalid(number, "invalid longitude"))?,
                            latitude: lat
                                .parse()
                                .map_err(|_| invalid(number, "invalid latitude"))?,
                        };
                        if !point.longitude.is_finite()
                            || !point.latitude.is_finite()
                            || !(-180.0..=180.0).contains(&point.longitude)
                            || !(-90.0..=90.0).contains(&point.latitude)
                        {
                            return Err(invalid(number, "vertex must contain finite longitude [-180,180] and latitude [-90,90] degrees"));
                        }
                        ring.push(point);
                    }
                    if ring.first() != ring.last() {
                        return Err(invalid(number, "ring must close with its first vertex"));
                    }
                    let min_lon = ring
                        .iter()
                        .map(|p| p.longitude)
                        .fold(f64::INFINITY, f64::min);
                    let max_lon = ring
                        .iter()
                        .map(|p| p.longitude)
                        .fold(f64::NEG_INFINITY, f64::max);
                    let min_lat = ring
                        .iter()
                        .map(|p| p.latitude)
                        .fold(f64::INFINITY, f64::min);
                    let max_lat = ring
                        .iter()
                        .map(|p| p.latitude)
                        .fold(f64::NEG_INFINITY, f64::max);
                    if min_lon == max_lon || min_lat == max_lat {
                        return Err(invalid(
                            number,
                            "ring needs a usable longitude and latitude extent",
                        ));
                    }
                    area.rings.push(ring);
                }
                _ => return Err(invalid(number, "expected area or ring record")),
            }
        }
        if let Some(last) = entries.last() {
            validate_area(last, area_line)?;
        }
        if entries.is_empty() {
            return Err(invalid(1, "catalogue needs at least one area record"));
        }
        Ok(Self { entries })
    }

    /// Load the explicitly bounded, fixed-vintage Census pilot without network IO.
    pub fn pilot() -> Result<Self, CatalogueError> {
        Self::from_tsv(include_str!("../../../fixtures/geography/pilot.tsv"))
    }
    pub fn entries(&self) -> &[Geography] {
        &self.entries
    }
    pub fn get(&self, id: &str) -> Option<&Geography> {
        self.entries.iter().find(|area| area.id == id)
    }
    /// AND-match normalized token prefixes, with recognized state abbreviations
    /// matching the state metadata exactly. Results are sorted by canonical ID.
    /// Unknown queries return no candidates rather than a substitute city.
    pub fn search(&self, query: &str) -> Result<Vec<&Geography>, SearchError> {
        if query.len() > 256 || query.chars().any(char::is_control) {
            return Err(SearchError::InvalidQuery);
        }
        let normalized_query = normalized(query);
        let tokens: Vec<_> = normalized_query.split_whitespace().collect();
        if tokens.is_empty() {
            return Err(SearchError::InvalidQuery);
        }
        let mut result: Vec<_> = self
            .entries
            .iter()
            .filter(|area| {
                let haystack = normalized(&format!(
                    "{} {} {}",
                    area.id,
                    area.name,
                    area.states.join(" ")
                ));
                tokens.iter().all(|token| {
                    if is_state_token(token) {
                        area.states
                            .iter()
                            .any(|state| state.eq_ignore_ascii_case(token))
                    } else {
                        haystack
                            .split_whitespace()
                            .any(|word| word.starts_with(token))
                    }
                })
            })
            .collect();
        result.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        Ok(result)
    }
}
