//! E02 offline study-area onboarding CLI.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = mobility_api::study_area::run(&args) {
        eprintln!("Study area failed: {error}");
        std::process::exit(1);
    }
}
