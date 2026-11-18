//! Synthetic whole-job benchmark, including E08 simulation; not metro-scale evidence.
#[path = "support/e09.rs"]
mod fixture;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (n, d, s, mut request) = fixture::inputs();
    println!("synthetic calibration; candidates,seeds,targets,repetitions,elapsed_ms");
    for size in [2, 8, 32] {
        let original = request.candidates[0].clone();
        request.candidates = (0..size)
            .map(|i| {
                let mut p = original.clone();
                p.label = format!("grid-{i}");
                p.demand_multiplier = 1.0 + i as f64 / 10.0;
                p
            })
            .collect();
        let start = std::time::Instant::now();
        for _ in 0..20 {
            std::hint::black_box(mobility_sim::calibration::run(
                &mobility_sim::simulation::Iterative,
                &n,
                &d,
                &s,
                &request,
            )?);
        }
        println!(
            "{},{},{},20,{:.3}",
            size,
            request.seeds.len(),
            request.observations.len(),
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
    Ok(())
}
