//! E01-S05 synthetic primitive microbenchmark, not simulation validation.
use std::{env, fs, hint::black_box, process, time::Instant};

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        return Err("usage: foundations-benchmark FIXTURE SAMPLES ITERATIONS".into());
    }
    let samples = args[2].parse::<usize>().map_err(|e| e.to_string())?;
    let iterations = args[3].parse::<usize>().map_err(|e| e.to_string())?;
    if samples == 0 || iterations == 0 {
        return Err("samples and iterations must be positive".into());
    }
    let input = fs::read_to_string(&args[1]).map_err(|e| e.to_string())?;
    let mut fixture = Vec::new();
    for line in input
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
    {
        let (reachable, total) = line.split_once(',').ok_or("invalid fixture row")?;
        fixture.push((
            reachable.parse::<usize>().map_err(|e| e.to_string())?,
            total.parse::<usize>().map_err(|e| e.to_string())?,
        ));
    }
    if fixture.is_empty() {
        return Err("fixture must contain counts".into());
    }
    // O(1) per metric, O(iterations * fixture rows) per sample, O(rows) memory.
    // Inputs and outputs pass through black_box so release cannot fold the work.
    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..iterations {
            for &(reachable, total) in &fixture {
                black_box(accessibility::reachable_share(
                    black_box(reachable),
                    black_box(total),
                ));
            }
        }
        println!("{}", start.elapsed().as_nanos());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("benchmark: {error}");
        process::exit(1);
    }
}
