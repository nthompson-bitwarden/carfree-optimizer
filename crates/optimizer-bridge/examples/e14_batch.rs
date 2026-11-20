//! E14 synthetic-only persistent server for Python integration/profiling.
//! argv: slots (1..64), topology seed (optional). Not a city model.
use optimizer_bridge::{Batch, MAX_SLOTS};
use std::io;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args.len() > 2 {
        return Err("usage: e14_batch SLOTS [TOPOLOGY]".into());
    }
    let count: usize = args[0].parse()?;
    let topology = args
        .get(1)
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    if count == 0 || count > MAX_SLOTS {
        return Err("slots outside 1..64".into());
    }
    let mut slots = Vec::with_capacity(count);
    for _ in 0..count {
        slots.push(optimizer_env::synthetic::environment(topology)?);
    }
    Batch::new(slots)?.serve(io::stdin().lock(), io::stdout().lock())?;
    Ok(())
}
