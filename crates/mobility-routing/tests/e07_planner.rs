//! E07 synthetic acceptance scenarios: abstract geography, modeled seconds, MIT.
use mobility_graph::network::*;
use mobility_graph::{EdgeId, Mode, NodeId};
use mobility_routing::{planner::*, *};

fn source() -> Provenance {
    Provenance {
        source: "E07 synthetic".into(),
        source_version: "v1".into(),
        license: "MIT".into(),
        geography: "abstract".into(),
        date: "2026-11-18".into(),
        basis: Basis::Modeled,
        uncertainty: "uncalibrated arbitrary test assumptions".into(),
    }
}
fn data() -> NetworkData {
    NetworkData {
        revision: "e07-v1".into(),
        provenance: source(),
        timezone: "UTC".into(),
        tzdb: "fixture".into(),
        coverage_start: 0,
        coverage_end: 2000,
        nodes: (0..8)
            .map(|id| Node {
                id: NodeId(id),
                access: Access::Accessible,
            })
            .collect(),
        links: vec![],
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
fn street(id: u32, from: u32, to: u32, mode: Mode, seconds: u32) -> Link {
    Link {
        id: EdgeId(id),
        from: NodeId(from),
        to: NodeId(to),
        distance_m: Some(100.0),
        seconds,
        access: Access::Accessible,
        kind: LinkKind::Street {
            mode,
            crossing: false,
            blocked: false,
            cycling: CycleFacility::Shared,
            safety: Some(Box::new(Safety {
                stress_level: 4,
                provenance: source(),
            })),
            capacity: None,
        },
    }
}
fn transfer(id: u32, node: u32, from: Mode, to: Mode, action: TransferAction) -> Link {
    Link {
        id: EdgeId(id),
        from: NodeId(node),
        to: NodeId(node),
        distance_m: None,
        seconds: 2,
        access: Access::Accessible,
        kind: LinkKind::Transfer {
            from,
            to,
            action,
            windows: vec![(0, 2000)],
            bicycle_allowed: true,
        },
    }
}
fn request(mode: Mode) -> Request {
    Request {
        from: NodeId(0),
        to: NodeId(3),
        departure_utc: 0,
        deadline_utc: 300,
        initial_mode: mode,
        vehicle: is_vehicle(mode).then_some(mode),
        profile: Profile {
            allowed_modes: vec![Mode::Walk, Mode::Bus, mode],
            step_free: true,
            mobility_aid: false,
            allow_unknown_access: false,
            protected_cycling_only: false,
            maximum_cycling_stress: None,
        },
        max_states: 10000,
    }
}
fn bus() -> Network {
    let mut d = data();
    d.links = vec![
        street(0, 0, 1, Mode::Walk, 10),
        transfer(1, 1, Mode::Walk, Mode::Bus, TransferAction::Board),
        Link {
            id: EdgeId(2),
            from: NodeId(1),
            to: NodeId(2),
            distance_m: Some(1000.0),
            seconds: 20,
            access: Access::Accessible,
            kind: LinkKind::Transit {
                mode: Mode::Bus,
                calendar: 1,
                trip: "bus".into(),
                stop_index: 0,
                timetable: Timetable::Scheduled(vec![Departure {
                    depart: 30,
                    arrive: 50,
                    run: 0,
                }]),
                bicycle_allowed: true,
            },
        },
        transfer(3, 2, Mode::Bus, Mode::Walk, TransferAction::Alight),
        street(4, 2, 3, Mode::Walk, 8),
    ];
    Network::new(d).unwrap()
}
fn journey(out: PlannedOutcome) -> Itinerary {
    match out {
        PlannedOutcome::Journey(j) => j,
        other => panic!("expected journey, got {other:?}"),
    }
}
#[test]
fn e07_s01_s02_given_doors_when_bus_runs_then_all_components_reconcile() {
    let n = bus();
    let r = request(Mode::Walk);
    let j = journey(
        plan(
            &n,
            &r,
            &Options {
                require_transit: true,
                ..Options::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(
        (
            j.times.access_seconds,
            j.times.wait_seconds,
            j.times.ride_seconds,
            j.times.transfer_seconds,
            j.times.egress_seconds,
            j.times.dwell_seconds
        ),
        (10, 18, 20, 4, 8, 0)
    );
    assert_eq!(j.journey.arrival_utc, 60);
    let mut late = r;
    late.departure_utc = 31;
    assert!(matches!(
        plan(
            &n,
            &late,
            &Options {
                require_transit: true,
                ..Options::default()
            }
        )
        .unwrap(),
        PlannedOutcome::Unmet(_)
    ));
}
#[test]
fn e07_s03_given_unsafe_shortcut_when_penalty_changes_then_physical_clock_does_not() {
    for mode in [Mode::Bicycle, Mode::ElectricBicycle] {
        let mut d = data();
        d.links = vec![
            street(0, 0, 3, mode, 5),
            street(1, 0, 1, mode, 4),
            street(2, 1, 3, mode, 4),
        ];
        for l in &mut d.links[1..] {
            if let LinkKind::Street {
                cycling, safety, ..
            } = &mut l.kind
            {
                *cycling = CycleFacility::Protected;
                safety.as_mut().unwrap().stress_level = 1;
            }
        }
        let n = Network::new(d).unwrap();
        let r = request(mode);
        assert_eq!(
            journey(plan(&n, &r, &Options::default()).unwrap())
                .journey
                .arrival_utc,
            5
        );
        let o = Options {
            comfort: Comfort {
                stress_seconds_per_level: 10,
                shared_seconds: 5,
                unknown_seconds: 20,
            },
            ..Options::default()
        };
        let j = journey(plan(&n, &r, &o).unwrap());
        assert_eq!(
            j.journey.steps.iter().map(|s| s.edge).collect::<Vec<_>>(),
            vec![EdgeId(1), EdgeId(2)]
        );
        assert_eq!((j.journey.arrival_utc, j.comfort_penalty_seconds), (8, 0));
    }
}
#[test]
fn e07_s04_given_immutable_network_when_matrix_runs_then_order_and_revision_are_explicit() {
    let n = bus();
    let r = request(Mode::Walk);
    let m = matrix(&n, "e07-v1", &[r.clone(), r], &Options::default()).unwrap();
    assert_eq!(m.graph_revision, "e07-v1");
    assert_eq!(m.results.len(), 2);
    assert_eq!(m.results[0], m.results[1]);
    assert!(matrix(&n, "wrong", &[], &Options::default()).is_err());
}
#[test]
fn e07_s05_given_stop_and_turn_rule_when_routing_then_dwell_does_not_erase_approach() {
    let mut d = data();
    d.links = vec![
        street(0, 0, 1, Mode::Car, 5),
        street(1, 1, 3, Mode::Car, 5),
        street(2, 1, 2, Mode::Car, 7),
        street(3, 2, 3, Mode::Car, 7),
    ];
    d.turns = vec![TurnRule {
        incoming: EdgeId(0),
        outgoing: EdgeId(1),
        only: false,
    }];
    let n = Network::new(d).unwrap();
    let o = Options {
        stops: vec![Stop {
            node: NodeId(1),
            dwell_seconds: 10,
        }],
        ..Options::default()
    };
    let j = journey(plan(&n, &request(Mode::Car), &o).unwrap());
    assert_eq!(j.journey.arrival_utc, 29);
    assert_eq!(j.times.dwell_seconds, 10);
    assert_eq!(j.visits[0].arrival_utc, 5);
    assert_eq!(j.visits[0].departure_utc, 15);
    assert!(!j.journey.steps.iter().any(|s| s.edge == EdgeId(1)));
}
#[test]
fn e07_s05_given_transfer_constraints_when_boarding_too_slow_then_no_fabricated_ride() {
    let n = bus();
    let r = request(Mode::Walk);
    for o in [
        Options {
            require_transit: true,
            max_boardings: Some(0),
            ..Options::default()
        },
        Options {
            require_transit: true,
            minimum_board_seconds: 21,
            ..Options::default()
        },
    ] {
        assert!(matches!(
            plan(&n, &r, &o).unwrap(),
            PlannedOutcome::Unmet(_)
        ));
    }
}
#[test]
fn e07_s06_given_departure_samples_when_one_misses_service_then_conditional_percentiles_and_gaps() {
    let w = departure_window(
        &bus(),
        &request(Mode::Walk),
        &Options {
            require_transit: true,
            ..Options::default()
        },
        &[0, 10, 31],
        300,
    )
    .unwrap();
    assert_eq!((w.feasible, w.unmet), (2, 1));
    assert_eq!(w.no_route_fraction, 1.0 / 3.0);
    assert_eq!((w.p50_seconds, w.p95_seconds), (Some(50), Some(60)));
    assert!(departure_window(&bus(), &request(Mode::Walk), &Options::default(), &[], 300).is_err());
}
#[test]
fn e07_s05_given_non_greedy_stop_when_first_approach_dead_ends_then_alternative_survives() {
    let mut d = data();
    d.links = vec![
        street(0, 0, 1, Mode::Car, 5),
        street(1, 0, 2, Mode::Car, 4),
        street(2, 2, 1, Mode::Car, 4),
        street(3, 1, 3, Mode::Car, 5),
    ];
    d.turns = vec![TurnRule {
        incoming: EdgeId(0),
        outgoing: EdgeId(3),
        only: false,
    }];
    let n = Network::new(d).unwrap();
    let j = journey(
        plan(
            &n,
            &request(Mode::Car),
            &Options {
                stops: vec![Stop {
                    node: NodeId(1),
                    dwell_seconds: 10,
                }],
                ..Options::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(
        j.journey.steps.iter().map(|s| s.edge).collect::<Vec<_>>(),
        vec![EdgeId(1), EdgeId(2), EdgeId(3)]
    );
    assert_eq!(j.journey.arrival_utc, 23);
}
#[test]
fn e07_s01_s05_given_stops_at_doors_when_dwelling_then_clock_components_reconcile() {
    let n = bus();
    let j = journey(
        plan(
            &n,
            &request(Mode::Walk),
            &Options {
                require_transit: true,
                stops: vec![
                    Stop {
                        node: NodeId(0),
                        dwell_seconds: 5,
                    },
                    Stop {
                        node: NodeId(3),
                        dwell_seconds: 9,
                    },
                ],
                ..Options::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(j.journey.arrival_utc, 69);
    assert_eq!(
        j.visits
            .iter()
            .map(|v| (v.arrival_utc, v.departure_utc))
            .collect::<Vec<_>>(),
        vec![(0, 5), (60, 69)]
    );
    let t = j.times;
    assert_eq!(
        t.access_seconds
            + t.wait_seconds
            + t.ride_seconds
            + t.transfer_seconds
            + t.egress_seconds
            + t.dwell_seconds
            + t.intermediate_walk_seconds,
        69
    );
}
#[test]
fn e07_s02_given_destination_aboard_when_no_alight_then_door_route_is_unmet() {
    let n = bus();
    let mut r = request(Mode::Walk);
    r.to = NodeId(2);
    let mut d = n.data().clone();
    d.links.retain(|l| l.id != EdgeId(3));
    let n = Network::new(d).unwrap();
    assert!(matches!(
        earliest_arrival(&n, &r).unwrap(),
        Outcome::Journey(_)
    ));
    assert!(matches!(
        plan(&n, &r, &Options::default()).unwrap(),
        PlannedOutcome::Unmet(_)
    ));
}
#[test]
fn e07_s03_given_unknown_stress_when_cost_requested_then_no_assessment_is_invented() {
    let mut d = data();
    let mut link = street(0, 0, 3, Mode::Bicycle, 5);
    if let LinkKind::Street { safety, .. } = &mut link.kind {
        *safety = None;
    }
    d.links = vec![link];
    let n = Network::new(d).unwrap();
    let o = Options {
        comfort: Comfort {
            stress_seconds_per_level: 100,
            shared_seconds: 7,
            unknown_seconds: 13,
        },
        ..Options::default()
    };
    let j = journey(plan(&n, &request(Mode::Bicycle), &o).unwrap());
    assert_eq!(j.comfort_penalty_seconds, 20);
    assert_eq!(j.edge_penalties, vec![(EdgeId(0), 20)]);
    assert_eq!(j.journey.arrival_utc, 5);
    let mut r = request(Mode::Bicycle);
    r.profile.maximum_cycling_stress = Some(4);
    assert!(matches!(
        plan(&n, &r, &o).unwrap(),
        PlannedOutcome::Unmet(_)
    ));
}
#[test]
fn e07_s06_given_no_feasible_samples_then_percentiles_are_absent_not_zero() {
    let w = departure_window(
        &bus(),
        &request(Mode::Walk),
        &Options {
            require_transit: true,
            ..Options::default()
        },
        &[31, 40],
        300,
    )
    .unwrap();
    assert_eq!((w.feasible, w.unmet, w.no_route_fraction), (0, 2, 1.0));
    assert_eq!((w.p50_seconds, w.p95_seconds), (None, None));
    assert!(departure_window(
        &bus(),
        &request(Mode::Walk),
        &Options::default(),
        &[10, 10],
        300
    )
    .is_err());
}
#[test]
fn e07_s02_given_every_start_and_deadline_then_small_timetable_oracle_matches() {
    let n = bus();
    for start in 0..40 {
        for deadline in [40, 59, 60, 100] {
            if start > deadline {
                continue;
            }
            let mut r = request(Mode::Walk);
            r.departure_utc = start;
            r.deadline_utc = deadline;
            // Independent fixture oracle: access 10 + preparation 2 must reach run
            // at second 30; ride arrives 50, alight 2, egress 8, total arrival 60.
            let expected = start + 12 <= 30 && deadline >= 60;
            let out = plan(
                &n,
                &r,
                &Options {
                    require_transit: true,
                    ..Options::default()
                },
            )
            .unwrap();
            assert_eq!(
                matches!(out, PlannedOutcome::Journey(_)),
                expected,
                "{start}/{deadline}"
            );
            if let PlannedOutcome::Journey(j) = out {
                assert_eq!(j.journey.arrival_utc, 60);
            }
        }
    }
}
#[test]
fn e07_s01_s05_given_closing_parking_when_stop_required_then_park_before_activity() {
    let mut d = data();
    let mut park = transfer(1, 1, Mode::Car, Mode::Walk, TransferAction::Park);
    if let LinkKind::Transfer { windows, .. } = &mut park.kind {
        *windows = vec![(0, 8)];
    }
    d.links = vec![
        street(0, 0, 1, Mode::Car, 5),
        park,
        street(2, 1, 3, Mode::Walk, 3),
    ];
    let n = Network::new(d).unwrap();
    let j = journey(
        plan(
            &n,
            &request(Mode::Car),
            &Options {
                stops: vec![Stop {
                    node: NodeId(1),
                    dwell_seconds: 10,
                }],
                ..Options::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(j.journey.arrival_utc, 20);
    assert_eq!(
        (
            j.times.ride_seconds,
            j.times.transfer_seconds,
            j.times.egress_seconds,
            j.times.dwell_seconds
        ),
        (5, 2, 3, 10)
    );
    assert_eq!(
        (j.visits[0].arrival_utc, j.visits[0].departure_utc),
        (7, 17)
    );
}
#[test]
fn e07_s05_given_overflowing_irrelevant_dwell_when_alternative_exists_then_prune_only_branch() {
    let mut d = data();
    d.coverage_end = i64::MAX;
    d.links = vec![
        street(0, 0, 1, Mode::Walk, 5),
        street(1, 0, 1, Mode::Walk, 19),
        street(2, 1, 3, Mode::Walk, 1),
    ];
    let n = Network::new(d).unwrap();
    let mut r = request(Mode::Walk);
    r.departure_utc = i64::MAX - 20;
    r.deadline_utc = i64::MAX;
    let j = journey(
        plan(
            &n,
            &r,
            &Options {
                stops: vec![Stop {
                    node: NodeId(1),
                    dwell_seconds: 2,
                }],
                ..Options::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(j.journey.arrival_utc, i64::MAX - 12);
}
#[test]
fn e07_s04_given_distinct_cells_then_order_and_standalone_results_match() {
    let n = bus();
    let early = request(Mode::Walk);
    let mut late = early.clone();
    late.departure_utc = 31;
    let o = Options {
        require_transit: true,
        ..Options::default()
    };
    let m = matrix(&n, "e07-v1", &[late.clone(), early.clone()], &o).unwrap();
    assert_eq!(
        m.results,
        vec![plan(&n, &late, &o).unwrap(), plan(&n, &early, &o).unwrap()]
    );
    assert!(matches!(m.results[0], PlannedOutcome::Unmet(_)));
    assert!(matches!(m.results[1], PlannedOutcome::Journey(_)));
    assert_eq!(n, bus());
}
#[test]
fn e07_s03_given_equal_run_state_then_lower_cost_later_approach_replaces_label() {
    let mut d = bus().data().clone();
    d.links.retain(|l| l.id != EdgeId(0));
    d.links.push(street(0, 0, 1, Mode::Bicycle, 5));
    let mut slow = street(5, 0, 1, Mode::Bicycle, 30);
    if let LinkKind::Street {
        cycling, safety, ..
    } = &mut slow.kind
    {
        *cycling = CycleFacility::Protected;
        safety.as_mut().unwrap().stress_level = 1;
    }
    d.links.push(slow);
    d.links.push(transfer(
        6,
        1,
        Mode::Bicycle,
        Mode::Walk,
        TransferAction::Dismount,
    ));
    if let LinkKind::Transit { timetable, .. } =
        &mut d.links.iter_mut().find(|l| l.id == EdgeId(2)).unwrap().kind
    {
        *timetable = Timetable::Scheduled(vec![Departure {
            depart: 100,
            arrive: 120,
            run: 0,
        }]);
    }
    let n = Network::new(d).unwrap();
    let j = journey(
        plan(
            &n,
            &request(Mode::Bicycle),
            &Options {
                comfort: Comfort {
                    stress_seconds_per_level: 5,
                    ..Comfort::default()
                },
                ..Options::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(j.journey.steps[0].edge, EdgeId(5));
    assert_eq!((j.journey.arrival_utc, j.comfort_penalty_seconds), (130, 0));
}
#[test]
fn e07_s05_given_two_rides_and_dwell_then_whole_journey_boarding_limit_is_enforced() {
    let mut d = bus().data().clone();
    d.links
        .push(transfer(5, 3, Mode::Walk, Mode::Bus, TransferAction::Board));
    d.links.push(Link {
        id: EdgeId(6),
        from: NodeId(3),
        to: NodeId(4),
        distance_m: Some(1000.0),
        seconds: 20,
        access: Access::Accessible,
        kind: LinkKind::Transit {
            mode: Mode::Bus,
            calendar: 1,
            trip: "second".into(),
            stop_index: 0,
            timetable: Timetable::Scheduled(vec![Departure {
                depart: 90,
                arrive: 110,
                run: 0,
            }]),
            bicycle_allowed: true,
        },
    });
    d.links.push(transfer(
        7,
        4,
        Mode::Bus,
        Mode::Walk,
        TransferAction::Alight,
    ));
    d.links.push(street(8, 4, 5, Mode::Walk, 8));
    let n = Network::new(d).unwrap();
    let mut r = request(Mode::Walk);
    r.to = NodeId(5);
    let mut o = Options {
        require_transit: true,
        max_boardings: Some(2),
        stops: vec![Stop {
            node: NodeId(3),
            dwell_seconds: 20,
        }],
        ..Options::default()
    };
    let j = journey(plan(&n, &r, &o).unwrap());
    assert_eq!(j.journey.arrival_utc, 120);
    o.max_boardings = Some(1);
    assert!(matches!(
        plan(&n, &r, &o).unwrap(),
        PlannedOutcome::Unmet(_)
    ));
    o.max_boardings = Some(2);
    o.stops[0].dwell_seconds = 30;
    assert!(matches!(
        plan(&n, &r, &o).unwrap(),
        PlannedOutcome::Unmet(_)
    ));
}
#[test]
fn e07_errors_are_not_service_gaps() {
    let mut r = request(Mode::Walk);
    r.max_states = 1;
    assert_eq!(
        plan(&bus(), &r, &Options::default()),
        Err(RoutingError::StateLimit)
    );
    assert_eq!(
        departure_window(&bus(), &r, &Options::default(), &[0], 300),
        Err(RoutingError::StateLimit)
    );
    assert!(departure_window(
        &bus(),
        &request(Mode::Walk),
        &Options::default(),
        &[1900],
        300
    )
    .is_err());
}
