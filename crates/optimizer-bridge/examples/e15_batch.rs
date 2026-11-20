//! E15 synthetic-only evaluator; one E14 protocol slot, no city loading.
use optimizer_bridge::Batch;
use optimizer_env::{reference::Environment, synthetic::benchmark_configuration};
use std::io;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [topology, mix] = args.as_slice() else {
        return Err("usage: e15_batch TOPOLOGY DEMAND_MIX".into());
    };
    let environment = Environment::new(benchmark_configuration(topology.parse()?, mix.parse()?)?)?;
    Batch::new(vec![environment])?.serve(io::stdin().lock(), io::stdout().lock())?;
    Ok(())
}
