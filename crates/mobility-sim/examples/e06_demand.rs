#[path = "support/e06.rs"]
mod fixture;

use fixture::*;
use mobility_sim::demand::*;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let network = corridor();
    let mut demand = sample();
    let mut assumptions = scenario();
    match std::env::args().nth(1).as_deref() {
        Some("--benchmark") => {
            for size in [100, 1_000, 10_000] {
                let template = sample().tours.remove(0);
                demand.tours = (0..size)
                    .map(|i| {
                        let mut tour = template.clone();
                        tour.id = format!("tour-{i}");
                        tour.legs[0].departure_utc = i % 15;
                        if i % 2 == 1 {
                            tour.group = "chair".into();
                        }
                        tour
                    })
                    .collect();
                let mut samples = Vec::new();
                for _ in 0..5 {
                    let start = Instant::now();
                    let report = evaluate(&network, &demand, &assumptions, 500)?;
                    samples.push(start.elapsed().as_micros());
                    assert_eq!(report.tours.len(), size as usize);
                    assert_eq!(report.essential.total_weight, size as f64 * 4.5);
                    assert_eq!(report.essential.no_car_free_share(), Some(0.0));
                }
                samples.sort_unstable();
                println!(
                    "{size:>5} two-leg tours: median {} µs; samples {:?}",
                    samples[2], samples
                );
            }
        }
        None => {
            println!("Synthetic only; not observed or calibrated.");
            println!(
                "Represented residents: {}",
                demand
                    .population
                    .generate()?
                    .map(|r| r.persons)
                    .sum::<u64>()
            );
            let report = evaluate(&network, &demand, &assumptions, 500)?;
            println!("{} (including the return):", report.environment_label);
            for (mode, choice) in &report.tours[0].choices {
                match choice {
                    Ok(choice) => println!(
                        "  {mode:?}: feasible, {:?} equivalent seconds",
                        choice.generalized_seconds
                    ),
                    Err(reason) => println!("  {mode:?}: {reason:?}"),
                }
            }
            assumptions.environment.label = "winter storm before dawn".into();
            assumptions.environment.season = "winter".into();
            assumptions.environment.weather = "storm".into();
            assumptions.environment.dark_modes = vec![TravelMode::Car];
            assumptions.environment.daylight.clear();
            let storm = evaluate(&network, &demand, &assumptions, 500)?;
            for run in [report, storm] {
                println!(
                    "{}: {:.1}/{:.1} essential leg weight lacks a complete car-free tour",
                    run.environment_label,
                    run.essential.no_car_free_weight,
                    run.essential.total_weight
                );
            }
            let sensitivity = long_horizon(
                &demand.population,
                &Sensitivity {
                    revision: "illustrative-high".into(),
                    years: 10,
                    provenance: synthetic(),
                    moves: vec![Move {
                        from: "west".into(),
                        to: "east".into(),
                        persons: 2,
                    }],
                    rates: vec![Rate {
                        zone: "east".into(),
                        purpose: Purpose::Shopping,
                        trips_per_person: 0.5,
                        multiplier: 1.25,
                    }],
                },
            )?;
            let trips = &sensitivity.trips[0];
            println!("Separate {}-year sensitivity, east shopping trips/day: {} → {} with relocation → {} with induced demand", sensitivity.years, trips.baseline, trips.relocation_only, trips.with_induced_demand);
        }
        Some(_) => return Err("usage: e06_demand [--benchmark]".into()),
    }
    Ok(())
}
