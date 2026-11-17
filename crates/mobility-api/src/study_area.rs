//! E02 static project presentation and local persistence. No model predictions.
use mobility_ingest::study_area::{
    validate_project_id, CatalogueImporter, CityProject, Region, StudyArea,
};
use mobility_ingest::{Catalogue, Coordinate};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
type Error = Box<dyn std::error::Error>;
fn escape(s: &str) -> String {
    s.chars().fold(String::new(), |mut out, c| {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
        out
    })
}
fn svg_path(rings: &[Vec<Coordinate>], extent: [f64; 4]) -> String {
    let [w, s, e, n] = extent;
    let span = (e - w).max(n - s);
    let width = (e - w) / span;
    let height = (n - s) / span;
    let scale = (600.0 / width).min(320.0 / height);
    let left = (640.0 - width * scale) / 2.0;
    let top = (360.0 - height * scale) / 2.0;
    let mut path = String::new();
    for r in rings {
        for (i, p) in r.iter().enumerate() {
            let _ = write!(
                path,
                "{}{:.6},{:.6} ",
                if i == 0 { 'M' } else { 'L' },
                left + scale * ((p.longitude - w) / span),
                top + scale * ((n - p.latitude) / span)
            );
        }
        path.push_str("Z ")
    }
    path
}
pub fn render_project(project: &CityProject) -> String {
    let a = project.area();
    let ex = a.rings().iter().flatten().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |[w, s, e, n], p| {
            [
                w.min(p.longitude),
                s.min(p.latitude),
                e.max(p.longitude),
                n.max(p.latitude),
            ]
        },
    );
    let mut html=String::from("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Study area onboarding</title><style>body{font:16px/1.6 system-ui,sans-serif;background:#f3f7f5;color:#16352d;margin:0}main{max-width:1100px;margin:auto;padding:2rem 1rem}section{background:white;padding:1.25rem;margin:1rem 0;border:1px solid #a6bdb2;border-radius:12px}svg{width:100%;height:auto;background:#edf4f0}table{width:100%;border-collapse:collapse}th,td{text-align:left;vertical-align:top;padding:.5rem;border-bottom:1px solid #ccd9d3;overflow-wrap:anywhere}caption{text-align:left;font-weight:bold}dt{font-weight:bold}dd{margin:0 0 .6rem;overflow-wrap:anywhere}.scroll{overflow-x:auto}.notice{border-left:4px solid #236955;padding:1rem}</style></head><body><main><h1>Study area onboarding</h1>");
    let _=write!(html,"<section><h2>{}</h2><dl><dt>Study area</dt><dd>{}</dd><dt>Region</dt><dd>{}: {}</dd><dt>Coordinate reference</dt><dd>{}: longitude, latitude in degrees</dd><dt>Snapshot date</dt><dd>{}</dd><dt>Baseline identifier</dt><dd>{}</dd><dt>Boundary revision</dt><dd>{}</dd><dt>Import attempts</dt><dd>{}</dd><dt>Import status</dt><dd>{}</dd></dl><svg viewBox=\"0 0 640 360\" role=\"img\" aria-labelledby=\"map-title map-description\"><title id=\"map-title\">Boundary of {}</title><desc id=\"map-description\">Schematic north-up boundary in longitude and latitude degrees. Nested rings use even-odd containment, preserving holes and islands. Dashed line shows original core.</desc><path d=\"{}\" fill=\"#99cdb8\" stroke=\"#174e3c\" fill-rule=\"evenodd\"/><path d=\"{}\" fill=\"none\" stroke=\"#713c25\" stroke-dasharray=\"4 3\"/></svg></section>",escape(a.name()),escape(a.id()),escape(&a.region().country),escape(&a.region().states.join(", ")),escape(a.crs()),escape(project.snapshot_date()),escape(project.baseline_id()),project.revision(),project.import_attempts(),if project.coverage().missing_required.is_empty(){"Inputs imported; model calibration has not been evaluated"}else{"Missing required data; model cannot be initialized"},escape(a.name()),svg_path(a.rings(),ex),svg_path(a.core_rings(),ex));
    let _=write!(html,"<p class=\"notice\">Buffer: {} km. A positive buffer is a conservative spherical bounding envelope around the original core, not an exact polygon-distance buffer or a modeled commuter shed. Earth radius: 6371.0088 km. Polar and antimeridian crossing envelopes are unsupported. Geometry uses finite floating-point checks; this preview is not a legal or engineering boundary certification.</p><section><h2>Dataset coverage and gaps</h2><div class=\"scroll\"><table><caption>Imported inputs for the snapshot</caption><thead><tr><th scope=\"col\">Dataset</th><th scope=\"col\">Source / version / year</th><th scope=\"col\">Spatial extent (west, south, east, north degrees)</th><th scope=\"col\">Status</th><th scope=\"col\">Details</th></tr></thead><tbody>",a.buffer_km());
    for e in &project.coverage().entries {
        let _=write!(html,"<tr><th scope=\"row\">{}</th><td>{}<br>Version: {}<br>Year: {}</td><td>{}</td><td>{}</td><td>{}</td></tr>",e.kind.as_str(),escape(e.source.as_deref().unwrap_or("Unknown / not imported")),escape(e.version.as_deref().unwrap_or("Unknown")),e.year.map(|x|x.to_string()).unwrap_or_else(||"Unknown".into()),e.extent.map(|x|format!("{}, {}, {}, {}",x[0],x[1],x[2],x[3])).unwrap_or_else(||"Unknown".into()),e.status.as_str(),escape(&e.detail));
    }
    html.push_str("</tbody></table></div><h3>Missing required data</h3><ul>");
    for kind in &project.coverage().missing_required {
        let _ = write!(html, "<li>{}</li>", kind.as_str());
    }
    html.push_str("</ul></section><section><h2>Scenario baseline status</h2><ul>");
    for s in project.scenarios() {
        let _ = write!(
            html,
            "<li>{}: {} — baseline {}</li>",
            escape(&s.id),
            if s.stale {
                "Stale: boundary or imported inputs changed"
            } else {
                "Current onboarding baseline"
            },
            escape(&s.baseline_id)
        );
    }
    html.push_str("</ul></section><p class=\"notice\">The bundled catalogue supports three Census 2024 pilot areas only. No transportation network, travel demand, realistic journeys, calibration, accessibility results, or policy recommendations have been produced. Transit availability alone does not establish car independence. Drawn geometry and generated envelopes are user inputs and derived geometry, respectively, not observed transportation measurements.</p></main></body></html>\n");
    html
}
fn create(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| {
            format!(
                "cannot create {}: {e}; choose a new output path",
                path.display()
            )
        })?;
    if let Err(e) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let cleanup = std::fs::remove_file(path);
        return Err(format!("write failed: {e}; partial output cleanup: {cleanup:?}").into());
    }
    Ok(())
}
fn read(path: &Path) -> Result<String, Error> {
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() {
        return Err("input must be a regular file, not a symlink or directory".into());
    }
    if meta.len() > 16 * 1024 * 1024 {
        return Err("input exceeds 16 MiB limit".into());
    }
    Ok(std::fs::read_to_string(path)?)
}
fn load(path: &Path) -> Result<(CityProject, String), Error> {
    let s = read(path)?;
    Ok((CityProject::from_tsv(&s)?, s))
}
fn replace(path: &Path, old: &str, new: &str) -> Result<(), Error> {
    if old == new {
        return Ok(());
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or("project path requires filename")?
        .to_string_lossy();
    let temp = parent.join(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    create(&temp, new.as_bytes())?;
    let result = (|| {
        if read(path)? != old {
            return Err("project changed during update; retry with the new file".into());
        }
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        if let Err(cleanup) = std::fs::remove_file(&temp) {
            return Err(format!(
                "atomic replacement failed; temporary-file cleanup failed: {cleanup}"
            )
            .into());
        }
    }
    result
}
fn canonical(catalogue: &Catalogue, id: &str) -> Result<StudyArea, Error> {
    Ok(StudyArea::from_geography(catalogue.get(id).ok_or(
        "unsupported canonical geography; bundled pilot contains only three areas",
    )?)?)
}
fn ring_input(s: &str) -> Result<Vec<Vec<Coordinate>>, Error> {
    let mut rings = Vec::new();
    for line in s.lines() {
        let mut fields = line.split('\t');
        if fields.next() != Some("ring") {
            return Err("drawn ring TSV lines must begin with ring followed by tab-separated longitude,latitude vertices".into());
        }
        let points = fields
            .map(|s| -> Result<Coordinate, Error> {
                let (x, y) = s
                    .split_once(',')
                    .ok_or("vertex requires longitude,latitude")?;
                Ok(Coordinate {
                    longitude: x.parse()?,
                    latitude: y.parse()?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        rings.push(points)
    }
    Ok(rings)
}
const USAGE:&str="usage: study-area init <canonical-id> <YYYY-MM-DD> <new-project.tsv> | draw <id> <name> <states-comma> <date> <rings.tsv> <new-project.tsv> | preview <project.tsv> <new-report.html> | buffer <project.tsv> <km> | scenario <project.tsv> <scenario-id> | batch <requests.tsv> <output-dir>";
/// Run the offline CLI workflow. File writes are explicit and errors propagate.
pub fn run(args: &[String]) -> Result<(), Error> {
    let catalogue = Catalogue::pilot()?;
    let mut importer = CatalogueImporter::new(&catalogue);
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["init", id, date, path] => {
            let area = canonical(&catalogue, id)?;
            let project =
                CityProject::initialize(&id.replace(':', "-"), area, date, &mut importer)?;
            create(Path::new(path), project.to_tsv().as_bytes())?;
            println!("Created project; required data gaps are recorded");
        }
        ["draw", id, name, states, date, rings, path] => {
            let area = StudyArea::drawn(
                id,
                name,
                Region {
                    country: "US".into(),
                    states: states.split(',').map(str::to_owned).collect(),
                },
                "EPSG:4326",
                ring_input(&read(Path::new(rings))?)?,
            )?;
            let project = CityProject::initialize(id, area, date, &mut importer)?;
            create(Path::new(path), project.to_tsv().as_bytes())?;
            println!("Created drawn study project; required data gaps are recorded");
        }
        ["preview", path, output] => {
            let (project, _) = load(Path::new(path))?;
            create(Path::new(output), render_project(&project).as_bytes())?;
            println!("Created study-area report");
        }
        ["buffer", path, km] => {
            let path = Path::new(path);
            let (mut p, old) = load(path)?;
            p.expand_buffer(km.parse()?, &mut importer)?;
            replace(path, &old, &p.to_tsv())?;
            println!("Updated envelope, ingestion results and scenario baseline status");
        }
        ["scenario", path, id] => {
            let path = Path::new(path);
            let (mut p, old) = load(path)?;
            p.add_scenario(id)?;
            replace(path, &old, &p.to_tsv())?;
            println!("Recorded scenario baseline reference (no simulation performed)");
        }
        ["batch", requests, dir] => {
            let requests = read(Path::new(requests))?;
            let dir = Path::new(dir);
            if !dir.exists() {
                std::fs::create_dir_all(dir)?
            }
            if !std::fs::symlink_metadata(dir)?.file_type().is_dir() {
                return Err("batch output must be a directory, not a symlink".into());
            }
            let mut failed = false;
            for (index, line) in requests.lines().enumerate() {
                let fields: Vec<_> = line.split('\t').collect();
                let label = fields.first().copied().unwrap_or("");
                let result = (|| -> Result<&str, Error> {
                    let [id, geography, date] = fields.as_slice() else {
                        return Err(
                            "expected project-id, canonical-id, snapshot-date TSV fields".into(),
                        );
                    };
                    validate_project_id(id)?;
                    let expected = CityProject::initialize(
                        id,
                        canonical(&catalogue, geography)?,
                        date,
                        &mut importer,
                    )?;
                    let path = dir.join(format!("{id}.tsv"));
                    match std::fs::symlink_metadata(&path) {
                        Ok(_) => {
                            let (existing, _) = load(&path)?;
                            if existing.to_tsv() == expected.to_tsv() {
                                Ok("Unchanged")
                            } else {
                                Err("project ID conflicts with an existing project; no file changed".into())
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            create(&path, expected.to_tsv().as_bytes())?;
                            Ok("Created")
                        }
                        Err(e) => Err(e.into()),
                    }
                })();
                match result {
                    Ok(status) => println!("{}\t{}\t{}", index + 1, label, status),
                    Err(e) => {
                        failed = true;
                        println!("{}\t{}\tFailed\t{}", index + 1, label.escape_default(), e)
                    }
                }
            }
            if failed {
                return Err("batch completed with individual failures; see per-row results".into());
            }
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}
