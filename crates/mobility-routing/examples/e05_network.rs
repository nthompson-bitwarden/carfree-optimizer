//! Versioned synthetic corridor fixture; not observed traffic or service.
use mobility_graph::network::*;
use mobility_graph::{EdgeId, Mode, NodeId};
use mobility_routing::*;
use std::hint::black_box;
use std::time::Instant;

fn provenance() -> Provenance {
    Provenance {
        source: "e05-corridor-v1 synthetic fixture".into(),
        source_version: "v1".into(),
        license: "MIT (repository-authored synthetic fixture)".into(),
        geography: "abstract corridor, no real city".into(),
        date: "2026-11-17".into(),
        basis: Basis::Modeled,
        uncertainty: "arbitrary deterministic demonstration inputs; uncalibrated".into(),
    }
}
fn fixture(count: u32) -> NetworkData {
    let mut links = Vec::new();
    for mode in [Mode::Walk, Mode::Bicycle, Mode::Car] {
        for i in 0..count - 1 {
            links.push(Link {
                id: EdgeId(links.len() as u32),
                from: NodeId(i),
                to: NodeId(i + 1),
                distance_m: Some(100.0),
                seconds: if mode == Mode::Walk { 30 } else { 10 },
                access: Access::Accessible,
                kind: LinkKind::Street {
                    mode,
                    crossing: i % 8 == 0,
                    blocked: false,
                    cycling: CycleFacility::Protected,
                    safety: None,
                    capacity: if mode == Mode::Car {
                        Some(Box::new(Capacity {
                            vehicles_per_hour: 600,
                            provenance: provenance(),
                        }))
                    } else {
                        None
                    },
                },
            });
        }
    }
    let stops: Vec<u32> = (0..count).step_by(8).collect();
    for (j, node) in stops.iter().enumerate() {
        for (from, to, action) in [
            (Mode::Walk, Mode::Bus, TransferAction::Board),
            (Mode::Bus, Mode::Walk, TransferAction::Alight),
        ] {
            links.push(Link {
                id: EdgeId(links.len() as u32),
                from: NodeId(*node),
                to: NodeId(*node),
                distance_m: None,
                seconds: 10,
                access: Access::Accessible,
                kind: LinkKind::Transfer {
                    from,
                    to,
                    action,
                    windows: vec![(0, 86400)],
                    bicycle_allowed: true,
                },
            });
        }
        if let Some(next) = stops.get(j + 1) {
            let first = 60 + j as u32 * 120;
            links.push(Link {
                id: EdgeId(links.len() as u32),
                from: NodeId(*node),
                to: NodeId(*next),
                distance_m: Some(800.0),
                seconds: 120,
                access: Access::Accessible,
                kind: LinkKind::Transit {
                    mode: Mode::Bus,
                    calendar: 1,
                    trip: "corridor".into(),
                    stop_index: j as u32,
                    timetable: Timetable::Frequency {
                        first,
                        end: first + 1800,
                        headway: 300,
                        first_run: 0,
                    },
                    bicycle_allowed: true,
                },
            });
        }
    }
    NetworkData {
        revision: format!("e05-corridor-v1-{count}"),
        provenance: provenance(),
        timezone: "UTC".into(),
        tzdb: "synthetic-utc-no-civil-conversion".into(),
        coverage_start: 0,
        coverage_end: 86400,
        nodes: (0..count)
            .map(|id| Node {
                id: NodeId(id),
                access: Access::Accessible,
            })
            .collect(),
        links,
        turns: vec![],
        calendars: vec![Calendar {
            id: 1,
            first_date: 20261117,
            last_date: 20261117,
            weekdays: 127,
            added: vec![],
            removed: vec![],
        }],
        days: vec![ServiceDay {
            date: 20261117,
            weekday: 1,
            origin_utc: 0,
        }],
    }
}
fn request(count: u32, departure: i64) -> Request {
    Request {
        from: NodeId(0),
        to: NodeId(count - 1),
        departure_utc: departure,
        deadline_utc: 10000,
        initial_mode: Mode::Walk,
        vehicle: None,
        profile: Profile {
            allowed_modes: vec![Mode::Walk, Mode::Bus],
            step_free: true,
            mobility_aid: true,
            allow_unknown_access: false,
            protected_cycling_only: false,
            maximum_cycling_stress: None,
        },
        max_states: 200000,
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--benchmark") {
        println!("fixture,nodes,links,build_us,decode_us,queries,p50_us,p95_us");
        for count in [64, 128, 256] {
            let data = fixture(count);
            let start = Instant::now();
            let network = Network::new(data)?;
            let build = start.elapsed().as_micros();
            let bytes = network.encode();
            let start = Instant::now();
            assert_eq!(Network::decode(&bytes)?, network);
            let decode = start.elapsed().as_micros();
            let mut times = Vec::new();
            for i in 0..100 {
                let start = Instant::now();
                let result = earliest_arrival(&network, &request(count, i * 5))?;
                assert!(matches!(result, Outcome::Journey(_)));
                black_box(result);
                times.push(start.elapsed().as_micros());
            }
            times.sort_unstable();
            println!(
                "{},{},{},{},{},100,{},{}",
                network.data().revision,
                count,
                network.data().links.len(),
                build,
                decode,
                times[49],
                times[94]
            );
        }
    } else {
        let network = Network::new(fixture(64))?;
        println!("SYNTHETIC E05 DEMO — uncalibrated, no car-independence claim");
        println!("Step-free mobility-aid traveler, walking and bus only:");
        match earliest_arrival(&network, &request(64, 0))? {
            Outcome::Journey(j) => {
                println!(
                    "arrival UTC second {} ({} steps)",
                    j.arrival_utc,
                    j.steps.len()
                );
                for s in j.steps {
                    println!(
                        "edge {} {:?}: {}..{}, wait {} s",
                        s.edge.0, s.mode, s.departure_utc, s.arrival_utc, s.waiting_seconds
                    );
                }
            }
            Outcome::Unmet(u) => println!("unmet trip: {u:?}"),
        }
        let mut constrained = request(64, 0);
        constrained.deadline_utc = 50;
        println!(
            "Deadline 50 s: {:?}",
            earliest_arrival(&network, &constrained)?
        );
    }
    Ok(())
}
