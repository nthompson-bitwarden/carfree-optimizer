//! E08 demo/benchmark: repository-authored modeled inputs, no calibration.
#[path = "support/e06.rs"]
mod fixture;
use mobility_graph::EdgeId;
use mobility_routing::Run;
use mobility_sim::demand::TravelMode;
use mobility_sim::simulation::*;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let independent_reference = args == ["--independent-reference"];
    let (tours, workers, repetitions) = match args.as_slice() {
        [] => (1, 1, 1),
        [_] if independent_reference => (1, 1, 1),
        [flag, tours, workers, repetitions] if flag == "--benchmark" => (
            tours.parse::<usize>()?,
            workers.parse::<usize>()?,
            repetitions.parse::<usize>()?,
        ),
        _ => return Err("usage: e08_simulation [--benchmark TOURS WORKERS REPETITIONS | --independent-reference]".into()),
    };
    if tours == 0 || tours > 100_000 || repetitions == 0 || repetitions > 100 {
        return Err("invalid benchmark bounds".into());
    }
    let network = fixture::corridor();
    let mut demand = fixture::sample();
    demand.groups[0]
        .modes
        .retain(|m, _| matches!(m, TravelMode::Car | TravelMode::Walk));
    let template = demand.tours[0].clone();
    demand.tours = (0..tours)
        .map(|i| {
            let mut t = template.clone();
            t.id = format!("tour-{i:06}");
            t
        })
        .collect();
    let scenario = Scenario {
        version: VERSION,
        id: "e08-corridor-v1".into(),
        provenance: fixture::synthetic(),
        demand_scenario: fixture::scenario(),
        periods: vec![
            Period { start: 0, end: 61 },
            Period {
                start: 61,
                end: 241,
            },
        ],
        roads: [7, 8]
            .map(|edge| RoadSupply {
                edge: EdgeId(edge),
                vehicles_per_hour: vec![4.5 * tours as f64 * 3600.0 / 61.0, 90.0 * tours as f64],
            })
            .to_vec(),
        transit: vec![TransitSupply {
            edge: EdgeId(11),
            run: Run {
                trip: "outbound".into(),
                service_date: 20260105,
                ordinal: 0,
                stop_index: 0,
            },
            passengers: 10.0,
        }],
        edits: vec![],
        settings: Settings {
            seed: 42,
            max_iterations: 10,
            max_states_per_query: 10_000,
            tolerance: 1e-12,
            alpha: 1.0,
            beta: 1.0,
            vehicles_per_trip: 1.0,
            demand_multiplier: 1.0,
            preference_seconds: 0.0,
        },
    };
    if independent_reference {
        compare_free_flow(&network, &mut demand, scenario)?;
        return Ok(());
    }
    let scenarios = vec![scenario; 8];
    let oracle = batch(&Iterative, &network, &demand, &scenarios, 1)?.reports;
    let mut latencies = Vec::new();
    let mut elapsed = 0.0;
    for _ in 0..repetitions {
        let result = batch(&Iterative, &network, &demand, &scenarios, workers)?;
        if result.reports != oracle || result.reports.iter().any(|r| r.status != Status::Converged)
        {
            return Err("benchmark correctness failure".into());
        }
        elapsed += result.elapsed_seconds;
        for s in &scenarios {
            let start = Instant::now();
            let r = Iterative.evaluate(&network, &demand, s)?;
            if r != oracle[0] {
                return Err("latency correctness failure".into());
            }
            latencies.push(start.elapsed().as_secs_f64());
        }
    }
    latencies.sort_by(f64::total_cmp);
    let r = &oracle[0];
    println!("MODELED uncalibrated e08-corridor-v1; network={} demand={} seed={} tours={} nodes={} links={} periods=2 scenarios=8 workers={} repeats={}",
        r.network_revision, r.demand_revision, r.seed, tours, network.data().nodes.len(), network.data().links.len(), workers, repetitions);
    println!("engine={} status={:?} iterations={} residual={} person_seconds={} unserved_essential_weight={} essential_without_car_free_candidate={}",
        r.engine, r.status, r.iterations, r.volume_residual, r.person_seconds, r.unserved_essential_weight, r.essential_without_car_free_candidate);
    println!("batch_elapsed_seconds={elapsed:.6} scenarios_per_second={:.3} single_scenario_p50_ms={:.3} single_scenario_p95_ms={:.3}",
        (8 * repetitions) as f64 / elapsed, latencies[(latencies.len() * 50).div_ceil(100) - 1] * 1000.0, latencies[(latencies.len() * 95).div_ceil(100) - 1] * 1000.0);
    Ok(())
}

// Match a single car on each 100 m leg in the E20 SUMO free-flow fixture.
fn compare_free_flow(
    network: &mobility_graph::network::Network,
    demand: &mut mobility_sim::demand::Demand,
    mut scenario: Scenario,
) -> Result<(), Box<dyn std::error::Error>> {
    demand.groups[0]
        .modes
        .retain(|mode, _| *mode == TravelMode::Car);
    scenario.settings.alpha = 0.0;
    for speed_mps in [10, 5] {
        scenario.edits = [7, 8]
            .map(|edge| Edit::StreetSeconds {
                edge: EdgeId(edge),
                seconds: 100 / speed_mps,
            })
            .to_vec();
        let report = Iterative.evaluate(network, demand, &scenario)?;
        let weight = report.mode_trip_weights[&TravelMode::Car];
        if weight != 9.0 || report.unserved_trip_weight != 0.0 || report.status != Status::Converged
        {
            return Err("free-flow comparator expected both car trip legs to complete".into());
        }
        println!("{speed_mps},{}", report.person_seconds / (2.0 * weight));
    }
    Ok(())
}
