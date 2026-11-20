//! E14 synthetic reset and two-project benchmark. Model assumptions are arbitrary.
use optimizer_env::{synthetic, wire::BatchEnvironment};
use std::{hint::black_box, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let repetitions = match args.as_slice() {
        [] => 1,
        [count] => count.parse::<u32>()?,
        _ => return Err("usage: e14_reference [REPETITIONS]".into()),
    };
    if !(1..=1000).contains(&repetitions) {
        return Err("repetitions must be in 1..=1000".into());
    }
    for topology in [0, 16, 63] {
        let mut environment = synthetic::environment(topology)?;
        let mut reset = std::time::Duration::ZERO;
        let mut step = std::time::Duration::ZERO;
        for _ in 0..repetitions {
            let start = Instant::now();
            let opening = black_box(environment.reset(42)?);
            reset += start.elapsed();
            let start = Instant::now();
            let first = black_box(environment.step(0, opening.state)?);
            let final_step = black_box(environment.step(1, first.state)?);
            step += start.elapsed();
            if !final_step.truncated
                || final_step.terminated
                || opening.reward != 0.0
                || first.reward <= 0.0
                || final_step.observation.len() != synthetic::WIRE_FEATURES
            {
                return Err("unexpected E14 trace".into());
            }
        }
        println!(
            "synthetic topology={topology} seed=42 repetitions={repetitions} mean_reset_us={:.2} mean_step_us={:.2}",
            reset.as_secs_f64() * 1e6 / f64::from(repetitions),
            step.as_secs_f64() * 1e6 / (2.0 * f64::from(repetitions)),
        );
    }
    Ok(())
}
