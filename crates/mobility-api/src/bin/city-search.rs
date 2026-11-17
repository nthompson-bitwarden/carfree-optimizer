//! E02-S01: offline discovery demo; emits standalone HTML without network IO.
use mobility_api::render_search_results;
use mobility_ingest::Catalogue;
use std::io::Write;
use std::path::PathBuf;

const USAGE: &str = "usage: city-search '<query>' [--output <new-file.html>]";

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let query = args.next().ok_or(USAGE)?;
    let query = query.to_str().ok_or("query must be valid UTF-8")?;
    if query.starts_with("--") {
        return Err(USAGE.into());
    }
    let destination = match args.next() {
        None => None,
        Some(option) if option == "--output" => Some(PathBuf::from(args.next().ok_or(USAGE)?)),
        Some(_) => return Err(USAGE.into()),
    };
    if args.next().is_some() {
        return Err(USAGE.into());
    }
    let html = render_search_results(&Catalogue::pilot()?, query)?;
    if let Some(path) = destination {
        // Refuse to overwrite existing artifacts; callers choose a new path.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                format!(
                    "cannot create {} (choose a new output file): {error}",
                    path.display()
                )
            })?;
        if let Err(error) = file.write_all(html.as_bytes()) {
            // Preserve the primary write error. Cleanup failure is separately
            // reported, so a partial artifact is never silently successful.
            drop(file);
            let cleanup = std::fs::remove_file(&path);
            return Err(format!(
                "cannot write {}: {error}; partial-output cleanup: {:?}",
                path.display(),
                cleanup
            )
            .into());
        }
    } else {
        std::io::stdout().lock().write_all(html.as_bytes())?;
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("City search failed: {error}");
        std::process::exit(1);
    }
}
