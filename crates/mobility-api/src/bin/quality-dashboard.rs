use mobility_ingest::connectors::normalization;
use std::{
    env,
    error::Error,
    io::{self, Write},
    path::Path,
    process::ExitCode,
};

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [store, id] = args.as_slice() else {
        return Err("Usage: quality-dashboard STORE SNAPSHOT_SHA256 > quality.html".into());
    };
    let id = id.to_str().ok_or("snapshot ID must be UTF-8")?;
    let snapshot = normalization::load(Path::new(store), id)?;
    mobility_api::quality::render(&mut io::stdout().lock(), id, &snapshot)?;
    io::stdout().flush()?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("quality-dashboard: {error}");
            ExitCode::FAILURE
        }
    }
}
