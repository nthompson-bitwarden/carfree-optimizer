//! E08 Given/When/Then fixtures: modeled abstract corridor, MIT, 2026-01-05;
//! no observed traffic, population or policy claims.
#[path = "../examples/support/e06.rs"]
mod fixture;
use mobility_graph::network::{Access, Departure, LinkKind, Network, Node, Timetable};
use mobility_graph::{EdgeId, NodeId};
use mobility_routing::Run;
use mobility_sim::demand::{Demand, TravelMode};
use mobility_sim::simulation::*;

fn inputs() -> (Network, Demand, Scenario) {
    let n = fixture::corridor();
    let mut d = fixture::sample();
    d.groups[0].modes.retain(|m, _| *m == TravelMode::Car);
    let s = Scenario {
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
            .map(|id| RoadSupply {
                edge: EdgeId(id),
                vehicles_per_hour: vec![4.5 * 3600.0 / 61.0, 90.0],
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
    (n, d, s)
}
fn road(r: &Report, edge: u32, period: usize) -> &RoadResult {
    r.roads
        .iter()
        .find(|r| r.edge == EdgeId(edge) && r.period == period)
        .unwrap()
}

#[test]
fn e08_s01_s04_same_seed_replays_exactly_and_independent_bpr_oracle() {
    let (n, d, s) = inputs();
    let r = Iterative.evaluate(&n, &d, &s).unwrap();
    assert_eq!(r, Iterative.evaluate(&n, &d, &s).unwrap());
    assert_eq!(r.status, Status::Converged);
    assert_eq!(r.iterations, 3);
    // 4.5 vehicles / 4.5 capacity, t0=10, alpha=beta=1 => 20 seconds.
    assert_eq!(road(&r, 7, 0).vehicles, 4.5);
    assert_eq!(road(&r, 7, 0).used_seconds, 20);
    assert_eq!(road(&r, 8, 1).used_seconds, 20);
    assert_eq!(road(&r, 7, 1).vehicles, 0.0);
    assert_eq!(r.volume_residual, 0.0);
    assert_eq!(r.mode_trip_weights[&TravelMode::Car], 9.0);
    // One traveler plus one companion, 4.5 weighted trips each way.
    assert_eq!(r.person_seconds, 360.0);
    let mut high = s.clone();
    high.settings.demand_multiplier = 2.0;
    let high = Iterative.evaluate(&n, &d, &high).unwrap();
    assert_eq!(road(&high, 7, 0).used_seconds, 30);
    assert_eq!(road(&high, 7, 0).vehicles, 9.0);
}

#[test]
fn e08_s02_s03_edits_are_reversible_and_travelers_change_mode() {
    let (n, mut d, mut s) = inputs();
    let capacity = d.groups[0].modes[&TravelMode::Car];
    d.groups[0].modes.insert(TravelMode::Walk, capacity);
    s.settings.alpha = 0.0;
    let bytes = n.encode();
    let original = d.clone();
    let baseline = Iterative.evaluate(&n, &d, &s).unwrap();
    assert_eq!(baseline.assignments[0].mode, Some(TravelMode::Car));
    s.edits = vec![
        Edit::StreetSeconds {
            edge: EdgeId(7),
            seconds: 50,
        },
        Edit::StreetSeconds {
            edge: EdgeId(8),
            seconds: 50,
        },
    ];
    let changed = Iterative.evaluate(&n, &d, &s).unwrap();
    assert_eq!(changed.assignments[0].mode, Some(TravelMode::Walk));
    assert_eq!(changed.status, Status::Converged);
    s.edits.clear();
    assert_eq!(baseline, Iterative.evaluate(&n, &d, &s).unwrap());
    assert_eq!(n.encode(), bytes);
    assert_eq!(d, original);
    assert_ne!(
        baseline.effective_network_revision,
        changed.effective_network_revision
    );
    assert_eq!(
        changed.assignments[0].choice.as_ref().unwrap().legs[0]
            .journey
            .graph_revision,
        changed.effective_network_revision
    );
    assert_ne!(
        baseline.effective_network_revision,
        baseline.network_revision
    );
    s.edits = vec![Edit::RoadCapacity {
        edge: EdgeId(7),
        period: 0,
        vehicles_per_hour: 0.0,
    }];
    assert!(matches!(
        Iterative.evaluate(&n, &d, &s),
        Err(Error::Invalid(_))
    ));
    s.edits = vec![
        Edit::StreetSeconds {
            edge: EdgeId(7),
            seconds: 1
        };
        2
    ];
    assert!(Iterative.evaluate(&n, &d, &s).is_err());
    assert_eq!(n.encode(), bytes);
}

#[test]
fn e08_s03_route_diversion_and_capped_iteration_is_not_convergence() {
    let (n, d, mut s) = inputs();
    let mut data = n.data().clone();
    for (id, from, to) in [(12, 1, 2), (13, 2, 1)] {
        let mut l = data.links[6].clone();
        l.id = EdgeId(id);
        l.from = NodeId(from);
        l.to = NodeId(to);
        l.seconds = 25;
        data.links.push(l);
        s.roads.push(RoadSupply {
            edge: EdgeId(id),
            vehicles_per_hour: vec![36000.0; 2],
        });
    }
    let n = Network::new(data).unwrap();
    s.settings.max_iterations = 2;
    s.settings.alpha = 5.0;
    let r = Iterative.evaluate(&n, &d, &s).unwrap();
    assert_eq!(r.status, Status::Nonconverged);
    let choice = r.assignments[0].choice.as_ref().unwrap();
    assert_eq!(choice.legs[0].journey.steps[0].edge, EdgeId(12));
    assert_eq!(choice.legs[1].journey.steps[0].edge, EdgeId(13));
    assert_eq!(road(&r, 7, 0).vehicles, 0.0);
    assert_eq!(road(&r, 12, 0).vehicles, 4.5);
    s.settings.max_iterations = 1;
    assert_eq!(
        Iterative.evaluate(&n, &d, &s).unwrap().status,
        Status::Nonconverged
    );
}

#[test]
fn e08_s04_entry_time_not_departure_bin_for_downstream_link() {
    let (n, mut d, mut s) = inputs();
    let mut data = n.data().clone();
    data.nodes.push(Node {
        id: NodeId(3),
        access: Access::Accessible,
    });
    // Car-only path 1 -> 3 (10 sec) -> 2 (10 sec). Enters second edge at t=10.
    data.links[6].to = NodeId(3);
    let mut second = data.links[6].clone();
    second.id = EdgeId(12);
    second.from = NodeId(3);
    second.to = NodeId(2);
    data.links.push(second);
    let n = Network::new(data).unwrap();
    d.tours[0].legs.truncate(1);
    d.tours[0].return_to_origin = false;
    d.tours[0].legs[0].dwell_seconds = 0;
    s.periods = vec![
        Period { start: 0, end: 10 },
        Period {
            start: 10,
            end: 241,
        },
    ];
    s.roads.push(RoadSupply {
        edge: EdgeId(12),
        vehicles_per_hour: vec![1620.0, 4.5 * 3600.0 / 231.0],
    });
    for r in &mut s.roads {
        if r.edge != EdgeId(12) {
            r.vehicles_per_hour = vec![1620.0, 4.5 * 3600.0 / 231.0];
        }
    }
    s.settings.alpha = 0.0;
    let r = Iterative.evaluate(&n, &d, &s).unwrap();
    assert_eq!(road(&r, 7, 0).vehicles, 4.5);
    assert_eq!(road(&r, 12, 0).vehicles, 0.0);
    assert_eq!(road(&r, 12, 1).vehicles, 4.5);
    s.settings.alpha = 1.0;
    let r = Iterative.evaluate(&n, &d, &s).unwrap();
    assert_eq!(road(&r, 12, 1).used_seconds, 20);
    assert_eq!(road(&r, 12, 0).used_seconds, 10);
    // Hold alpha and demand fixed; halve only downstream period-one capacity.
    s.edits.push(Edit::RoadCapacity {
        edge: EdgeId(12),
        period: 1,
        vehicles_per_hour: 2.25 * 3600.0 / 231.0,
    });
    let reduced = Iterative.evaluate(&n, &d, &s).unwrap();
    assert_eq!(road(&reduced, 12, 1).used_seconds, 30);
    assert_eq!(road(&reduced, 12, 0).used_seconds, 10);
}

fn transit_only(d: &mut Demand) {
    d.groups[0].modes.retain(|m, _| *m == TravelMode::Car);
    let load = *d.groups[0].modes.get(&TravelMode::Car).unwrap();
    d.groups[0].modes.clear();
    d.groups[0].modes.insert(TravelMode::Transit, load);
    d.tours[0].legs.truncate(1);
    d.tours[0].return_to_origin = false;
    d.tours[0].legs[0].dwell_seconds = 0;
    d.tours[0].legs[0].load.companions = 0;
}
#[test]
fn e08_s05_transit_denial_is_bounded_and_not_infinite_capacity() {
    let (n, mut d, mut s) = inputs();
    transit_only(&mut d);
    s.transit[0].passengers = 4.5;
    let r = Iterative.evaluate(&n, &d, &s).unwrap();
    assert!(!r.assignments[0].denied_boarding);
    assert_eq!(r.transit[0].served_passengers, 4.5);
    s.transit[0].passengers = 4.0;
    let r = Iterative.evaluate(&n, &d, &s).unwrap();
    assert!(r.assignments[0].denied_boarding);
    assert_eq!(r.unserved_essential_weight, 4.5);
    assert_eq!(r.transit[0].requested_passengers, 4.5);
    assert_eq!(r.transit[0].served_passengers, 0.0);
    assert_eq!(r.person_seconds, 0.0);
    s.transit.clear();
    assert!(matches!(
        Iterative.evaluate(&n, &d, &s),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn e08_s05_atomic_reservation_releases_prior_segments_on_later_denial() {
    let (n, mut d, mut s) = inputs();
    transit_only(&mut d);
    let mut data = n.data().clone();
    data.nodes.push(Node {
        id: NodeId(3),
        access: Access::Accessible,
    });
    data.links[10].to = NodeId(3);
    let mut second = data.links[10].clone();
    second.id = EdgeId(12);
    second.from = NodeId(3);
    second.to = NodeId(2);
    if let LinkKind::Transit {
        stop_index,
        timetable,
        ..
    } = &mut second.kind
    {
        *stop_index = 1;
        *timetable = Timetable::Scheduled(vec![Departure {
            depart: 40,
            arrive: 45,
            run: 0,
        }]);
    }
    data.links.push(second);
    let n = Network::new(data).unwrap();
    let mut supply = s.transit[0].clone();
    supply.edge = EdgeId(12);
    supply.run.stop_index = 1;
    supply.passengers = 0.0;
    s.transit.push(supply);
    let r = Iterative.evaluate(&n, &d, &s).unwrap();
    assert!(r.assignments[0].denied_boarding);
    assert!(r.transit.iter().all(|x| x.served_passengers == 0.0));
    assert!(r.transit.iter().all(|x| x.requested_passengers == 4.5));
}

#[test]
fn e08_s06_s08_parallel_preserves_order_errors_and_adapter_semantics() {
    let (n, d, mut s) = inputs();
    s.settings.alpha = 0.0;
    let scenarios: Vec<_> = (0..7)
        .map(|i| {
            let mut s = s.clone();
            s.id = format!("scenario-{i}");
            s
        })
        .collect();
    let serial = batch(&Iterative, &n, &d, &scenarios, 1).unwrap();
    let parallel = batch(&Iterative, &n, &d, &scenarios, 3).unwrap();
    assert_eq!(serial.reports, parallel.reports);
    assert_eq!(parallel.workers_used, 3);
    assert!(parallel.scenarios_per_second.is_finite() && parallel.scenarios_per_second > 0.0);
    for (i, r) in parallel.reports.iter().enumerate() {
        assert_eq!(r.scenario_id, format!("scenario-{i}"));
    }
    let fixed = FixedCost.evaluate(&n, &d, &s).unwrap();
    assert_eq!(fixed.status, Status::FixedCosts);
    let iterative = Iterative.evaluate(&n, &d, &s).unwrap();
    assert_eq!(fixed.assignments, iterative.assignments);
    assert_eq!(
        fixed.effective_network_revision,
        iterative.effective_network_revision
    );
    assert_eq!(fixed.person_seconds, iterative.person_seconds);
    s.settings.alpha = 1.0;
    assert!(
        Iterative.evaluate(&n, &d, &s).unwrap().person_seconds
            > FixedCost.evaluate(&n, &d, &s).unwrap().person_seconds
    );
    let mut invalid = scenarios;
    invalid[3].settings.max_iterations = 0;
    assert!(batch(&Iterative, &n, &d, &invalid, 3).is_err());
    assert!(batch(&Iterative, &n, &d, &[], 1).is_err());
    assert!(batch(&Iterative, &n, &d, &[s], 0).is_err());
}

#[test]
fn e08_s07_crossed_grid_has_independent_distribution_oracle() {
    let (n, d, s) = inputs();
    let parameters = [1.0, 2.0].map(|multiplier| Parameters {
        label: format!("demand-{multiplier}"),
        alpha: 0.0,
        demand_multiplier: multiplier,
        preference_seconds: 10.0,
    });
    let r = sensitivity(&Iterative, &n, &d, &s, &[7, 8], &parameters, 2).unwrap();
    // Fixed car-only duration, weights give [180,180,360,360] person-seconds.
    assert_eq!(r.person_seconds.count, 4);
    assert_eq!(r.person_seconds.mean, 270.0);
    assert_eq!(r.person_seconds.sample_variance, Some(10800.0));
    assert_eq!(r.person_seconds.p50, 180.0);
    assert_eq!(r.person_seconds.p95, 360.0);
    assert_eq!(r.nonconverged, 0);
    assert_eq!(r.labels[2], ("demand-2".into(), 7));
    assert_eq!(r.runs.reports[1].seed, 8);
    assert!(sensitivity(&Iterative, &n, &d, &s, &[7, 7], &parameters, 1).is_err());
}

#[test]
fn e08_s01_failure_is_not_unmet_and_noncyclists_remain_constrained() {
    let (n, mut d, mut s) = inputs();
    s.settings.alpha = f64::NAN;
    assert!(Iterative.evaluate(&n, &d, &s).is_err());
    s.settings.alpha = 0.0;
    d.network_revision = "stale".into();
    assert!(Iterative.evaluate(&n, &d, &s).is_err());
    d.network_revision = n.data().revision.clone();
    s.settings.max_states_per_query = 1;
    assert!(matches!(
        Iterative.evaluate(&n, &d, &s),
        Err(Error::Demand(_))
    ));
    s.settings.max_states_per_query = 10000;
    d.tours[0].group = "chair".into();
    let r = Iterative.evaluate(&n, &d, &s).unwrap();
    assert_eq!(r.assignments[0].mode, Some(TravelMode::Walk));
    assert_eq!(r.essential_trip_weight, 4.5);
    // Missing prices cannot silently become free trips or disappear as infeasible.
    d.tours[0].legs[0].costs.clear();
    assert!(matches!(
        Iterative.evaluate(&n, &d, &s),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn e08_s04_replay_missed_intermediate_deadline_does_not_abort_feasible_walk() {
    let (n, mut d, mut s) = inputs();
    let capacity = d.groups[0].modes[&TravelMode::Car];
    d.groups[0].modes.insert(TravelMode::Walk, capacity);
    d.tours[0].legs.truncate(1);
    d.tours[0].return_to_origin = false;
    d.tours[0].legs[0].dwell_seconds = 0;
    let mut data = n.data().clone();
    data.nodes.push(Node {
        id: NodeId(3),
        access: Access::Accessible,
    });
    data.links[6].to = NodeId(3);
    let mut second = data.links[6].clone();
    second.id = EdgeId(12);
    second.from = NodeId(3);
    second.to = NodeId(2);
    data.links.push(second);
    let n = Network::new(data).unwrap();
    s.roads.push(RoadSupply {
        edge: EdgeId(12),
        vehicles_per_hour: s.roads[0].vehicles_per_hour.clone(),
    });
    s.settings.alpha = 100.0;
    s.settings.max_iterations = 2;
    let r = Iterative.evaluate(&n, &d, &s).unwrap();
    assert_eq!(r.assignments[0].mode, Some(TravelMode::Walk));
    assert_eq!(r.unserved_trip_weight, 0.0);
    assert_eq!(r.status, Status::Nonconverged);
    // Same feasible fallback near the maximum UTC integer: missed deadlines
    // are rejected before congested arithmetic can overflow.
    let offset = i64::MAX - 241;
    let mut data = n.data().clone();
    data.coverage_start += offset;
    data.coverage_end += offset;
    // This road-only extreme clock fixture must not assert overflowing service-day coverage.
    data.days.clear();
    data.calendars.clear();
    data.links
        .retain(|link| !matches!(link.kind, LinkKind::Transit { .. }));
    s.transit.clear();
    for link in &mut data.links {
        if let LinkKind::Transfer { windows, .. } = &mut link.kind {
            for (start, end) in windows {
                *start += offset;
                *end += offset;
            }
        }
    }
    let n = Network::new(data).unwrap();
    for tour in &mut d.tours {
        for leg in &mut tour.legs {
            leg.departure_utc += offset;
            leg.deadline_utc += offset;
            leg.window.start += offset;
            leg.window.end += offset;
        }
    }
    for p in &mut s.periods {
        p.start += offset;
        p.end += offset;
    }
    let env = &mut s.demand_scenario.environment;
    env.coverage.start += offset;
    env.coverage.end += offset;
    for window in &mut env.daylight {
        window.start += offset;
        window.end += offset;
    }
    let r = Iterative.evaluate(&n, &d, &s).unwrap();
    assert_eq!(r.assignments[0].mode, Some(TravelMode::Walk));
    assert_eq!(r.unserved_trip_weight, 0.0);
}

#[test]
fn e08_s03_extreme_finite_preference_overflow_is_explicit() {
    let (n, d, mut s) = inputs();
    s.settings.alpha = 0.0;
    s.demand_scenario.cost.non_wait_multiplier = f64::MAX / 25.0;
    s.settings.preference_seconds = f64::MAX;
    let outcomes: Vec<_> = (0..16)
        .map(|seed| {
            let mut s = s.clone();
            s.settings.seed = seed;
            FixedCost.evaluate(&n, &d, &s)
        })
        .collect();
    assert!(outcomes.iter().any(|r| matches!(r, Err(Error::Arithmetic))));
    assert!(outcomes
        .iter()
        .all(|r| matches!(r, Ok(_) | Err(Error::Arithmetic))));
}

#[test]
fn e08_s05_seats_are_reusable_after_alighting_on_a_different_segment() {
    let (n, mut d, mut s) = inputs();
    transit_only(&mut d);
    let mut data = n.data().clone();
    data.nodes.push(Node {
        id: NodeId(3),
        access: Access::Accessible,
    });
    data.links[10].to = NodeId(3);
    let mut second = data.links[10].clone();
    second.id = EdgeId(12);
    second.from = NodeId(3);
    second.to = NodeId(2);
    if let LinkKind::Transit {
        stop_index,
        timetable,
        ..
    } = &mut second.kind
    {
        *stop_index = 1;
        *timetable = Timetable::Scheduled(vec![Departure {
            depart: 40,
            arrive: 45,
            run: 0,
        }]);
    }
    data.links.push(second);
    for (template, id) in [(8, 13), (9, 14)] {
        let mut transfer = data.links[template].clone();
        transfer.id = EdgeId(id);
        transfer.from = NodeId(3);
        transfer.to = NodeId(3);
        data.links.push(transfer);
    }
    let n = Network::new(data).unwrap();
    s.transit[0].passengers = 4.5;
    let mut supply = s.transit[0].clone();
    supply.edge = EdgeId(12);
    supply.run.stop_index = 1;
    s.transit.push(supply);
    d.tours[0].legs[0].to = NodeId(3);
    let mut next = d.tours[0].clone();
    next.id = "downstream-passenger".into();
    next.legs[0].from = NodeId(3);
    next.legs[0].to = NodeId(2);
    next.legs[0].departure_utc = 22;
    next.legs[0].window = mobility_sim::demand::Window { start: 22, end: 23 };
    d.tours.push(next);
    let r = Iterative.evaluate(&n, &d, &s).unwrap();
    assert_eq!(r.unserved_trip_weight, 0.0);
    assert_eq!(r.transit.len(), 2);
    assert!(r.transit.iter().all(|x| x.served_passengers == 4.5));
    assert!(r.assignments.iter().all(|a| !a.denied_boarding));
}

#[test]
fn e08_s01_s07_seeded_preferences_change_choices_but_replay_exactly() {
    let (n, mut d, mut s) = inputs();
    let capacity = d.groups[0].modes[&TravelMode::Car];
    d.groups[0].modes.insert(TravelMode::Walk, capacity);
    s.settings.alpha = 0.0;
    s.settings.preference_seconds = 200.0;
    let mut modes = std::collections::BTreeSet::new();
    for seed in 0..16 {
        s.settings.seed = seed;
        let r = Iterative.evaluate(&n, &d, &s).unwrap();
        assert_eq!(r, Iterative.evaluate(&n, &d, &s).unwrap());
        modes.insert(r.assignments[0].mode);
    }
    assert_eq!(
        modes,
        [Some(TravelMode::Walk), Some(TravelMode::Car)].into()
    );
}

#[test]
fn e08_s06_worker_panic_is_a_typed_failure_not_partial_success() {
    struct Panics;
    impl Adapter for Panics {
        fn name(&self) -> &'static str {
            "test-panic"
        }
        fn evaluate(&self, _: &Network, _: &Demand, _: &Scenario) -> Result<Report, Error> {
            panic!("intentional test adapter panic")
        }
    }
    let (n, d, s) = inputs();
    assert!(matches!(
        batch(&Panics, &n, &d, &[s.clone(), s], 2),
        Err(Error::WorkerPanic)
    ));
}

#[test]
fn e08_s02_effective_revision_includes_period_utc_boundaries() {
    let (n, d, mut s) = inputs();
    s.settings.alpha = 0.0;
    let a = FixedCost.evaluate(&n, &d, &s).unwrap();
    s.periods[0].end = 60;
    s.periods[1].start = 60;
    let b = FixedCost.evaluate(&n, &d, &s).unwrap();
    assert_eq!(a.person_seconds, b.person_seconds);
    assert_eq!(
        a.roads.iter().map(|r| r.used_seconds).collect::<Vec<_>>(),
        b.roads.iter().map(|r| r.used_seconds).collect::<Vec<_>>()
    );
    assert_ne!(a.effective_network_revision, b.effective_network_revision);
}
