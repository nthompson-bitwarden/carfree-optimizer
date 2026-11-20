//! E16 single-process synthetic evaluator. No subprocesses, downloads or Python
//! calculation. Hard quotas are qualified on Linux only; other hosts fail closed.
use optimizer_env::{reference::Environment, synthetic};
use std::{error::Error, io::Write, time::Duration};

#[cfg(target_os = "linux")]
fn quotas(cpu: u64, memory: u64, wall: u64) -> Result<(), Box<dyn Error>> {
    #[repr(C)]
    #[derive(Default)]
    struct Limit {
        soft: u64,
        hard: u64,
    }
    // Linux ABI, 64-bit targets only. No crate dependencies or model changes.
    unsafe extern "C" {
        fn setrlimit(resource: i32, limit: *const Limit) -> i32;
        fn getrlimit(resource: i32, limit: *mut Limit) -> i32;
    }
    if !cfg!(target_pointer_width = "64") {
        return Err("quotas require 64-bit Linux".into());
    }
    for (resource, cap) in [(0, cpu), (9, memory), (1, 32768), (4, 0)] {
        let requested = Limit {
            soft: cap,
            hard: cap,
        };
        let mut actual = Limit::default();
        // SAFETY: Linux constants; valid C-layout pointers to live Limit values.
        if unsafe { setrlimit(resource, &requested) } != 0
            || unsafe { getrlimit(resource, &mut actual) } != 0
            || actual.soft != cap
            || actual.hard != cap
        {
            return Err(format!(
                "hard quota setup failed: {}",
                std::io::Error::last_os_error()
            )
            .into());
        }
    }
    // Independent of the Python supervisor: loss of that process cannot leave
    // an indefinitely running evaluator. This watchdog never spawns a process.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(wall));
        std::process::exit(124);
    });
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn quotas(_: u64, _: u64, _: u64) -> Result<(), Box<dyn Error>> {
    Err("hard AS quotas qualified only on 64-bit Linux; use a Linux worker".into())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--quota-probe") {
        if args.len() != 5 {
            return Err("--quota-probe memory|cpu|wall CPU_SECONDS AS_BYTES WALL_SECONDS".into());
        }
        let memory: u64 = args[3].parse()?;
        quotas(args[2].parse()?, memory, args[4].parse()?)?;
        match args[1].as_str() {
            "memory" => {
                let mut allocation = Vec::<u8>::new();
                if allocation
                    .try_reserve_exact((memory * 2).try_into()?)
                    .is_ok()
                {
                    return Err("over-quota allocation unexpectedly permitted".into());
                }
                println!("over-quota allocation rejected");
                return Ok(());
            }
            "cpu" => loop {
                std::hint::black_box((1_u64..1000).sum::<u64>());
            },
            "wall" => loop {
                std::thread::sleep(Duration::from_secs(1));
            },
            _ => return Err("unknown quota probe".into()),
        }
    }
    if args.len() != 8 {
        return Err(
            "SEED TOPOLOGY DEMAND simulation|random EPISODES CPU_SECONDS AS_BYTES WALL_SECONDS"
                .into(),
        );
    }
    let seed: u64 = args[0].parse()?;
    let topology: usize = args[1].parse()?;
    let demand: u32 = args[2].parse()?;
    let algorithm = args[3].as_str();
    let episodes: usize = args[4].parse()?;
    let cpu: u64 = args[5].parse()?;
    let memory: u64 = args[6].parse()?;
    let wall: u64 = args[7].parse()?;
    if !(1..=1000).contains(&episodes)
        || !(1..=3600).contains(&cpu)
        || !(67108864..=1099511627776).contains(&memory)
        || !(2..=3600).contains(&wall)
        || !["simulation", "random"].contains(&algorithm)
    {
        return Err("invalid bounded evaluator parameters".into());
    }
    quotas(cpu, memory, wall)?;
    let mut env = Environment::new(synthetic::benchmark_configuration(topology, demand)?)?;
    let mut state = seed ^ 0x9e3779b97f4a7c15;
    let mut best: Option<f64> = None;
    let mut failures = 0;
    for episode in 1..=episodes {
        let mut transition = env.reset(seed)?.clone();
        let mut reward = 0.0;
        let mut completed = false;
        // Same E15 horizon: reset + at most two steps, including explicit stop.
        for _ in 0..2 {
            let mut allowed: Vec<_> = transition
                .mask
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, entry)| entry.allowed().then_some(i))
                .collect();
            if transition.stop_allowed {
                allowed.push(env.stop_index());
            }
            if allowed.is_empty() {
                break;
            }
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let action = if algorithm == "simulation" {
                env.stop_index()
            } else {
                allowed[(state % allowed.len() as u64) as usize]
            };
            match env.step(action, transition.state) {
                Ok(t) => transition = t.clone(),
                Err(_) => break,
            }
            reward += transition.reward.reward;
            if transition.terminated || transition.truncated {
                completed = true;
                break;
            }
        }
        if completed && reward.is_finite() {
            best = Some(best.map_or(reward, |b| b.max(reward)));
        } else {
            failures += 1;
        }
        // At most 102 bounded-width lines, including the final result: <8 KiB
        // even at the 1,000-episode cap. Progress cannot crowd out the result.
        if episode == 1 || episode % 10 == 0 || episode == episodes {
            println!(
                "progress\t{episode}\t{failures}\t{}",
                best.map_or("none".into(), |b| format!("{b:.17e}"))
            );
            std::io::stdout().flush()?;
        }
    }
    println!(
        "result\t{episodes}\t{failures}\t{}",
        best.map_or("none".into(), |b| format!("{b:.17e}"))
    );
    Ok(())
}
