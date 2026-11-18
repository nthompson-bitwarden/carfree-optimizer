use mobility_ingest::connectors::{
    apply_upload, inspect, inventory, load_upload, rollback_upload, Candidate, Request,
};
use serde_json::{json, Map, Value};
use std::{
    env,
    error::Error,
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

const USAGE: &str = "Usage:
  ingest-data inventory AS_OF REQUEST.json [REQUEST.json ...]
  ingest-data preview REQUEST.json AS_OF
  ingest-data import REQUEST.json AS_OF
  ingest-data apply REQUEST.json AS_OF PROJECT.tsv
  ingest-data active PROJECT.tsv
  ingest-data rollback PROJECT.tsv

AS_OF is an explicit YYYY-MM-DD freshness assessment date.
Preview validates all records and prints up to five samples per collection.
Import prints the complete validated candidate to stdout. Validation errors exit 2.
Apply installs a custom upload sidecar. Rollback removes it.";

fn request(path: &Path) -> Result<(Request, &Path), Box<dyn Error>> {
    const LIMIT: u64 = 1024 * 1024;
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err("request must be a regular JSON file of at most 1 MiB".into());
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err("request exceeds the 1 MiB limit".into());
    }
    let request = serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let root = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok((request, root))
}

fn samples(data: Value) -> Value {
    match data {
        Value::Array(items) => {
            json!({"count":items.len(),"sample":items.into_iter().take(5).collect::<Vec<_>>() })
        }
        Value::Object(items) => {
            let groups: Map<_, _> = items.into_iter().map(|(key, value)| {
                let preview = match (key.as_str(), value) {
                    ("ways" | "nodes" | "restrictions", Value::Object(objects)) => json!({
                        "count":objects.len(),"sample":objects.into_iter().take(5).collect::<Map<_, _>>()
                    }),
                    ("schedule" | "realtime", value) => samples(value),
                    (_, value @ Value::Array(_)) => samples(value),
                    (_, value) => value,
                };
                (key, preview)
            }).collect();
            Value::Object(groups)
        }
        other => other,
    }
}

fn write(value: impl serde::Serialize) -> Result<(), Box<dyn Error>> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, &value)?;
    stdout.write_all(b"\n")?;
    Ok(())
}

fn display(mut candidate: Candidate, preview: bool) -> Result<u8, Box<dyn Error>> {
    let status = if candidate.is_valid() { 0 } else { 2 };
    if preview {
        candidate.data = samples(candidate.data);
    }
    write(candidate)?;
    Ok(status)
}

fn run() -> Result<u8, Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    match args.first().and_then(|arg| arg.to_str()) {
        Some("--help" | "-h") => {
            println!("{USAGE}");
            Ok(0)
        }
        Some("inventory") if args.len() >= 3 => {
            let date = args[1].to_str().ok_or("assessment date must be UTF-8")?;
            let requests = args[2..]
                .iter()
                .map(|path| request(Path::new(path)).map(|(request, _)| request))
                .collect::<Result<Vec<_>, _>>()?;
            write(
                json!({"schema_version":mobility_ingest::connectors::SCHEMA_VERSION, "sources":inventory(&requests, date)?}),
            )?;
            Ok(0)
        }
        Some(action @ ("preview" | "import")) if args.len() == 3 => {
            let (request, root) = request(Path::new(&args[1]))?;
            let date = args[2].to_str().ok_or("assessment date must be UTF-8")?;
            display(inspect(&request, root, date)?, action == "preview")
        }
        Some("apply") if args.len() == 4 => {
            let (request, root) = request(Path::new(&args[1]))?;
            let date = args[2].to_str().ok_or("assessment date must be UTF-8")?;
            display(
                apply_upload(Path::new(&args[3]), &request, root, date)?,
                true,
            )
        }
        Some("active") if args.len() == 2 => {
            if let Some(candidate) = load_upload(Path::new(&args[1]))? {
                display(candidate, true)
            } else {
                write(json!({"active":false}))?;
                Ok(0)
            }
        }
        Some("rollback") if args.len() == 2 => {
            write(json!({"removed":rollback_upload(Path::new(&args[1]))?}))?;
            Ok(0)
        }
        _ => Err(USAGE.into()),
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(code) => code.into(),
        Err(error) => {
            eprintln!("ingest-data: {error}");
            1.into()
        }
    }
}
