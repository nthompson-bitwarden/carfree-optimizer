//! E11 reviewable demonstration and bounded proposal/routing scaling measurement.
#[allow(dead_code)]
#[path = "support/e11.rs"]
mod fixture;
use fixture::*;
use interventions::{scenario::Change, transit::evaluate};
use mobility_graph::network::{LinkKind, Timetable};
use mobility_routing::{earliest_arrival, Outcome};
use std::{hint::black_box, time::Instant};
fn main() {
    println!("E11 synthetic modeled service; abstract corridor, 2026-01-05, no calibration or policy recommendation.");
    let mut h = history();
    install(&mut h);
    let a = evaluate(
        &h.current().network,
        &request(),
        &fares(&["bus"], 250),
        "USD",
        1000,
    )
    .unwrap();
    if let Outcome::Journey(j) = a.outcome {
        println!(
            "Bus door arrival={}s; fare={} USD minor units; generalized={}s",
            j.arrival_utc,
            a.fare_minor.unwrap(),
            a.generalized_seconds.unwrap()
        );
    }
    println!(
        "Fleet/operator estimate: {:?}",
        h.current().transit["bus"].estimate
    );
    let mut priority = service();
    priority.rides[0].seconds = 50; // Same frequency/run set; only running time changes.
    h.apply(&batch(
        &h,
        "priority",
        vec![
            Change::Transit(priority.clone()),
            Change::Allocate {
                road: mobility_graph::EdgeId(4),
                lanes: 1,
                target: priority.rides[0].clone(),
                assessment: None,
            },
        ],
    ))
    .unwrap();
    println!(
        "Priority road capacity={:?}; updated operations={:?}",
        h.current().road_capacities().unwrap(),
        h.current().transit["bus"].estimate
    );
    let a = evaluate(
        &h.current().network,
        &request(),
        &fares(&["bus"], 100),
        "USD",
        1000,
    )
    .unwrap();
    println!("Priority journey and changed fare: {a:?}");
    if std::env::args().any(|arg| arg == "--benchmark") {
        println!("runs,preview_mean_us,route_mean_us,repetitions");
        for runs in [1u32, 32, 128, 512] {
            let h = history();
            let mut s = service();
            s.rides[0].seconds = 5;
            if let LinkKind::Transit { timetable, .. } = &mut s.rides[0].kind {
                *timetable = Timetable::Frequency {
                    first: 100,
                    end: 100 + runs * 10,
                    headway: 10,
                    first_run: 0,
                };
            }
            let proposal = batch(
                &h,
                "bench",
                vec![
                    Change::Station(facilities(mobility_graph::Mode::Bus, 1, 2, 30)),
                    Change::Transit(s),
                ],
            );
            let repetitions = 50;
            let start = Instant::now();
            for _ in 0..repetitions {
                black_box(h.preview(black_box(&proposal)).unwrap());
            }
            let compile = start.elapsed().as_secs_f64() * 1e6 / f64::from(repetitions);
            let candidate = h.preview(&proposal).unwrap();
            let start = Instant::now();
            for _ in 0..repetitions {
                black_box(
                    earliest_arrival(black_box(&candidate.network), black_box(&request())).unwrap(),
                );
            }
            let route = start.elapsed().as_secs_f64() * 1e6 / f64::from(repetitions);
            println!("{runs},{compile:.3},{route:.3},{repetitions}");
        }
    }
}
