//! Reproducible synthetic E09 report; no observed data or policy claims.
#[path = "../../mobility-sim/examples/support/e09.rs"]
mod fixture;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (n, d, s, r) = fixture::inputs();
    let report =
        mobility_sim::calibration::run(&mobility_sim::simulation::Iterative, &n, &d, &s, &r)?;
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output = match args.as_slice() {
        [] => mobility_api::calibration::render(&report),
        [format] if format == "tsv" => mobility_api::calibration::export_tsv(&report),
        _ => return Err("usage: e09_calibration [tsv]".into()),
    };
    use std::io::Write;
    std::io::stdout().lock().write_all(output.as_bytes())?;
    Ok(())
}
