//! E07 human-reviewable demo and per-process synthetic routing benchmark.
#[allow(dead_code)]
#[path = "e05_network.rs"]
mod corridor;
use mobility_graph::network::Network;
use mobility_routing::planner::*;
use std::hint::black_box;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let count = match args.as_slice() {
        [] => 64,
        [flag, size] if flag == "--benchmark" => {
            let n: u32 = size.parse()?;
            if ![64, 128, 256].contains(&n) {
                return Err("benchmark size must be 64, 128 or 256".into());
            }
            n
        }
        _ => return Err("usage: e07_routing [--benchmark 64|128|256]".into()),
    };
    let start = Instant::now();
    let network = Network::new(corridor::fixture(count))?;
    let build_us = start.elapsed().as_micros();
    let options = Options {
        require_transit: true,
        ..Options::default()
    };
    if args.is_empty() {
        println!("SYNTHETIC E07 — modeled, uncalibrated; actual routable door nodes supplied");
        let result = plan(&network, &corridor::request(count, 0), &options)?;
        if let PlannedOutcome::Journey(j) = result {
            println!(
                "revision={} arrival={} UTC seconds",
                j.journey.graph_revision, j.journey.arrival_utc
            );
            println!("component durations in seconds: {:?}", j.times);
            println!(
                "comfort cost={} equivalent seconds; not elapsed time",
                j.comfort_penalty_seconds
            );
        } else {
            return Err("demo unexpectedly infeasible".into());
        }
        let w = departure_window(
            &network,
            &corridor::request(count, 0),
            &options,
            &[0, 300, 600, 2100],
            10000,
        )?;
        println!("conditional nearest-rank departure-window results: {w:?}");
    } else {
        let mut latencies = Vec::new();
        let mut paths = 0;
        let total = Instant::now();
        for i in 0..100 {
            let start = Instant::now();
            let result = plan(&network, &corridor::request(count, i * 5), &options)?;
            latencies.push(start.elapsed().as_nanos());
            if matches!(result, PlannedOutcome::Journey(_)) {
                paths += 1;
            }
            black_box(result);
        }
        let elapsed = total.elapsed().as_secs_f64();
        if paths != 100 {
            return Err("benchmark contains unmet queries".into());
        }
        latencies.sort_unstable();
        println!("fixture,nodes,links,expanded_service_events,build_us,queries,paths,queries_per_second,paths_per_second,p50_us,p95_us");
        let events = network
            .data()
            .links
            .iter()
            .filter_map(|l| match &l.kind {
                mobility_graph::network::LinkKind::Transit { timetable, .. } => {
                    Some(timetable.departures(l.seconds).map(|i| i.count()))
                }
                _ => None,
            })
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .sum::<usize>();
        println!(
            "{},{},{},{},{},100,{},{:.2},{:.2},{:.3},{:.3}",
            network.data().revision,
            count,
            network.data().links.len(),
            events,
            build_us,
            paths,
            100.0 / elapsed,
            paths as f64 / elapsed,
            latencies[49] as f64 / 1000.0,
            latencies[94] as f64 / 1000.0
        );
        // Peak RSS is measured by /usr/bin/time -l (macOS) or -v (Linux)
        // around this single-size process, not invented allocator estimates.
    }
    Ok(())
}
