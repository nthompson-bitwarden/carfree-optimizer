//! Synthetic acceptance scenarios for E05; no observed travel inputs.
use mobility_graph::network::*;
use mobility_graph::{EdgeId, Mode, NodeId};
use mobility_routing::*;

fn source() -> Provenance {
    Provenance {
        source: "E05 synthetic fixture".into(),
        source_version: "v1".into(),
        license: "MIT (repository-authored synthetic fixture)".into(),
        geography: "abstract".into(),
        date: "2026-11-17".into(),
        basis: Basis::Modeled,
        uncertainty: "not calibrated".into(),
    }
}
fn data() -> NetworkData {
    NetworkData {
        revision: "fixture-v1".into(),
        provenance: source(),
        timezone: "UTC".into(),
        tzdb: "caller-fixture".into(),
        coverage_start: 0,
        coverage_end: 200_000,
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
            last_date: 20261118,
            weekdays: 127,
            added: vec![],
            removed: vec![],
        }],
        days: vec![
            ServiceDay {
                date: 20261117,
                weekday: 1,
                origin_utc: 0,
            },
            ServiceDay {
                date: 20261118,
                weekday: 2,
                origin_utc: 86400,
            },
        ],
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
            safety: None,
            capacity: if mode == Mode::Car {
                Some(Box::new(Capacity {
                    vehicles_per_hour: 500,
                    provenance: source(),
                }))
            } else {
                None
            },
        },
    }
}
fn transfer(
    id: u32,
    node: u32,
    from: Mode,
    to: Mode,
    action: TransferAction,
    seconds: u32,
) -> Link {
    Link {
        id: EdgeId(id),
        from: NodeId(node),
        to: NodeId(node),
        distance_m: None,
        seconds,
        access: Access::Accessible,
        kind: LinkKind::Transfer {
            from,
            to,
            action,
            windows: vec![(0, 200000)],
            bicycle_allowed: true,
        },
    }
}
fn transit(id: u32, from: u32, to: u32, mode: Mode, departures: Vec<Departure>) -> Link {
    Link {
        id: EdgeId(id),
        from: NodeId(from),
        to: NodeId(to),
        distance_m: Some(1000.0),
        seconds: 1,
        access: Access::Accessible,
        kind: LinkKind::Transit {
            mode,
            calendar: 1,
            trip: "run".into(),
            stop_index: 0,
            timetable: Timetable::Scheduled(departures),
            bicycle_allowed: true,
        },
    }
}
fn request(mode: Mode) -> Request {
    Request {
        from: NodeId(0),
        to: NodeId(2),
        departure_utc: 0,
        deadline_utc: 500,
        initial_mode: mode,
        vehicle: match mode {
            Mode::Car => Some(Mode::Car),
            Mode::Bicycle => Some(Mode::Bicycle),
            Mode::ElectricBicycle => Some(Mode::ElectricBicycle),
            _ => None,
        },
        profile: Profile {
            allowed_modes: vec![
                Mode::Walk,
                Mode::Bicycle,
                Mode::ElectricBicycle,
                Mode::Car,
                Mode::Bus,
                Mode::Rail,
                Mode::LightRail,
            ],
            step_free: false,
            mobility_aid: false,
            allow_unknown_access: false,
            protected_cycling_only: false,
            maximum_cycling_stress: None,
        },
        max_states: 20000,
    }
}
fn arrival(d: NetworkData, r: Request) -> Option<i64> {
    match earliest_arrival(&Network::new(d).unwrap(), &r).unwrap() {
        Outcome::Journey(j) => Some(j.arrival_utc),
        Outcome::Unmet(_) => None,
    }
}

#[test]
fn e05_s02_explicit_stress_limit_excludes_missing_or_high_assessments() {
    let mut d = data();
    d.links = vec![
        street(0, 0, 2, Mode::Bicycle, 10),
        street(1, 0, 2, Mode::Bicycle, 20),
        street(2, 0, 2, Mode::Bicycle, 30),
    ];
    for (index, stress) in [(1, 4), (2, 2)] {
        if let LinkKind::Street { safety, .. } = &mut d.links[index].kind {
            *safety = Some(Box::new(Safety {
                stress_level: stress,
                provenance: source(),
            }));
        }
    }
    let mut r = request(Mode::Bicycle);
    assert_eq!(arrival(d.clone(), r.clone()), Some(10));
    r.profile.maximum_cycling_stress = Some(2);
    assert_eq!(arrival(d.clone(), r.clone()), Some(30));
    r.profile.maximum_cycling_stress = Some(1);
    assert_eq!(arrival(d, r), None);
}

#[test]
fn e05_s01_directed_crossing_barrier_and_disconnected_segments() {
    let mut d = data();
    d.links = vec![
        street(0, 0, 1, Mode::Walk, 10),
        street(1, 1, 2, Mode::Walk, 20),
    ];
    if let LinkKind::Street { crossing, .. } = &mut d.links[1].kind {
        *crossing = true;
    }
    assert_eq!(arrival(d.clone(), request(Mode::Walk)), Some(30));
    let mut reverse = request(Mode::Walk);
    reverse.from = NodeId(2);
    reverse.to = NodeId(0);
    assert_eq!(arrival(d.clone(), reverse), None);
    if let LinkKind::Street { blocked, .. } = &mut d.links[1].kind {
        *blocked = true;
    }
    assert_eq!(arrival(d.clone(), request(Mode::Walk)), None);
    d.links.pop();
    assert_eq!(arrival(d, request(Mode::Walk)), None);
}
#[test]
fn e05_s02_protection_and_mode_specific_forbidden_links() {
    let mut d = data();
    d.links = vec![
        street(0, 0, 2, Mode::Bicycle, 10),
        street(1, 0, 2, Mode::Bicycle, 30),
    ];
    if let LinkKind::Street { cycling, .. } = &mut d.links[1].kind {
        *cycling = CycleFacility::Protected;
    }
    assert_eq!(arrival(d.clone(), request(Mode::Bicycle)), Some(10));
    let mut r = request(Mode::Bicycle);
    r.profile.protected_cycling_only = true;
    assert_eq!(arrival(d.clone(), r), Some(30));
    assert_eq!(arrival(d.clone(), request(Mode::ElectricBicycle)), None);
    if let LinkKind::Street { mode, .. } = &mut d.links[1].kind {
        *mode = Mode::ElectricBicycle;
    }
    assert_eq!(arrival(d, request(Mode::ElectricBicycle)), Some(30));
}
#[test]
fn e05_s03_wait_calendars_and_overtaking_for_all_transit_modes() {
    for mode in [Mode::Bus, Mode::Rail, Mode::LightRail] {
        let mut d = data();
        d.links = vec![
            transfer(0, 0, Mode::Walk, mode, TransferAction::Board, 5),
            transit(
                1,
                0,
                2,
                mode,
                vec![
                    Departure {
                        depart: 10,
                        arrive: 100,
                        run: 0,
                    },
                    Departure {
                        depart: 20,
                        arrive: 40,
                        run: 1,
                    },
                ],
            ),
        ];
        assert_eq!(arrival(d.clone(), request(Mode::Walk)), Some(40));
        d.calendars[0].removed = vec![20261117];
        assert_eq!(arrival(d, request(Mode::Walk)), None);
    }
}
#[test]
fn e05_s03_same_run_continuity_and_missed_transfer() {
    let mut d = data();
    d.links = vec![
        transfer(0, 0, Mode::Walk, Mode::Bus, TransferAction::Board, 1),
        transit(
            1,
            0,
            1,
            Mode::Bus,
            vec![Departure {
                depart: 10,
                arrive: 20,
                run: 0,
            }],
        ),
        transit(
            2,
            1,
            2,
            Mode::Bus,
            vec![Departure {
                depart: 22,
                arrive: 30,
                run: 0,
            }],
        ),
    ];
    if let LinkKind::Transit { stop_index, .. } = &mut d.links[2].kind {
        *stop_index = 1;
    }
    assert_eq!(arrival(d.clone(), request(Mode::Walk)), Some(30));
    if let LinkKind::Transit { trip, .. } = &mut d.links[2].kind {
        *trip = "different".into();
    }
    assert_eq!(arrival(d.clone(), request(Mode::Walk)), None);
    d.links.push(transfer(
        3,
        1,
        Mode::Bus,
        Mode::Walk,
        TransferAction::Alight,
        2,
    ));
    d.links.push(transfer(
        4,
        1,
        Mode::Walk,
        Mode::Bus,
        TransferAction::Board,
        2,
    ));
    assert_eq!(arrival(d, request(Mode::Walk)), None); // cannot catch 22 after 24
}
#[test]
fn e05_s03_frequency_instances_and_exclusive_window_end() {
    let mut d = data();
    d.links = vec![
        transfer(0, 0, Mode::Walk, Mode::Bus, TransferAction::Board, 1),
        transit(1, 0, 1, Mode::Bus, vec![]),
        transit(2, 1, 2, Mode::Bus, vec![]),
    ];
    for (i, first) in [(1, 10), (2, 25)] {
        d.links[i].seconds = 10;
        if let LinkKind::Transit {
            stop_index,
            timetable,
            ..
        } = &mut d.links[i].kind
        {
            *stop_index = (i - 1) as u32;
            *timetable = Timetable::Frequency {
                first,
                end: first + 20,
                headway: 10,
                first_run: 0,
            };
        }
    }
    assert_eq!(arrival(d.clone(), request(Mode::Walk)), Some(35));
    let mut r = request(Mode::Walk);
    r.departure_utc = 29;
    assert_eq!(arrival(d, r), None); // first-leg departure 30 is excluded
}
#[test]
fn e05_s04_turn_rules_and_capacity_roundtrip() {
    let mut d = data();
    d.links = vec![
        street(0, 0, 1, Mode::Car, 10),
        street(1, 1, 2, Mode::Car, 10),
        street(2, 1, 3, Mode::Car, 10),
        street(3, 3, 2, Mode::Car, 20),
    ];
    d.turns = vec![TurnRule {
        incoming: EdgeId(0),
        outgoing: EdgeId(1),
        only: false,
    }];
    assert_eq!(arrival(d.clone(), request(Mode::Car)), Some(40));
    d.turns[0] = TurnRule {
        incoming: EdgeId(0),
        outgoing: EdgeId(2),
        only: true,
    };
    assert_eq!(arrival(d.clone(), request(Mode::Car)), Some(40));
    if let LinkKind::Street { capacity, .. } = &mut d.links[0].kind {
        capacity.as_mut().unwrap().vehicles_per_hour = 900;
    }
    let n = Network::new(d).unwrap();
    assert_eq!(Network::decode(&n.encode()).unwrap(), n);
}
#[test]
fn e05_s05_canonical_roundtrip_and_malformed_rejection() {
    let mut d = data();
    d.links = vec![
        street(0, 0, 1, Mode::Walk, 10),
        street(1, 1, 2, Mode::Walk, 10),
    ];
    let n = Network::new(d.clone()).unwrap();
    d.nodes.reverse();
    d.links.reverse();
    d.days.reverse();
    assert_eq!(n.encode(), Network::new(d).unwrap().encode());
    assert_eq!(Network::decode(&n.encode()).unwrap(), n);
    let bytes = n.encode();
    for end in 0..bytes.len() {
        assert!(Network::decode(&bytes[..end]).is_err());
    }
    let mut bytes = n.encode();
    bytes.push(0);
    assert!(Network::decode(&bytes).is_err());
    let mut bytes = n.encode();
    bytes[0] = 255;
    assert!(Network::decode(&bytes).is_err());
}
#[test]
fn e05_s06_park_ride_and_no_remote_vehicle_retrieval() {
    let mut d = data();
    d.links = vec![
        street(0, 0, 1, Mode::Car, 10),
        transfer(1, 1, Mode::Car, Mode::Walk, TransferAction::Park, 5),
        transfer(2, 1, Mode::Walk, Mode::Bus, TransferAction::Board, 5),
        transit(
            3,
            1,
            2,
            Mode::Bus,
            vec![Departure {
                depart: 30,
                arrive: 60,
                run: 0,
            }],
        ),
        transfer(4, 2, Mode::Bus, Mode::Walk, TransferAction::Alight, 1),
        transfer(5, 2, Mode::Walk, Mode::Car, TransferAction::Retrieve, 1),
        street(6, 2, 3, Mode::Car, 10),
    ];
    assert_eq!(arrival(d.clone(), request(Mode::Car)), Some(60));
    let mut r = request(Mode::Car);
    r.to = NodeId(3);
    assert_eq!(arrival(d.clone(), r), None);
    if let LinkKind::Transfer { windows, .. } = &mut d.links[1].kind {
        windows.clear();
    }
    assert_eq!(arrival(d, request(Mode::Car)), None);
}
#[test]
fn e05_s06_bike_carriage_restriction_and_parking() {
    let mut d = data();
    d.links = vec![
        transfer(0, 0, Mode::Bicycle, Mode::Walk, TransferAction::Dismount, 1),
        transfer(1, 0, Mode::Walk, Mode::Rail, TransferAction::Board, 1),
        transit(
            2,
            0,
            2,
            Mode::Rail,
            vec![Departure {
                depart: 10,
                arrive: 20,
                run: 0,
            }],
        ),
    ];
    assert_eq!(arrival(d.clone(), request(Mode::Bicycle)), Some(20));
    if let LinkKind::Transit {
        bicycle_allowed, ..
    } = &mut d.links[2].kind
    {
        *bicycle_allowed = false;
    }
    assert_eq!(arrival(d.clone(), request(Mode::Bicycle)), None);
    if let LinkKind::Transfer { action, .. } = &mut d.links[0].kind {
        *action = TransferAction::Park;
    }
    assert_eq!(arrival(d, request(Mode::Bicycle)), Some(20));
}
#[test]
fn e05_s07_step_free_unmet_and_request_errors_are_distinct() {
    let mut d = data();
    d.links = vec![
        street(0, 0, 2, Mode::Walk, 10),
        street(1, 0, 1, Mode::Walk, 20),
        street(2, 1, 2, Mode::Walk, 20),
    ];
    d.links[0].access = Access::Steps;
    let mut r = request(Mode::Walk);
    r.profile.step_free = true;
    r.profile.mobility_aid = true;
    r.profile.allowed_modes = vec![Mode::Walk, Mode::Bus];
    assert_eq!(arrival(d.clone(), r.clone()), Some(40));
    d.links[1].access = Access::Unknown;
    let n = Network::new(d).unwrap();
    assert!(matches!(earliest_arrival(&n, &r), Ok(Outcome::Unmet(_))));
    r.from = NodeId(99);
    assert!(earliest_arrival(&n, &r).is_err());
    r.from = NodeId(0);
    r.profile.allow_unknown_access = true; // a legal successor must exceed the bound
    r.max_states = 1;
    assert_eq!(earliest_arrival(&n, &r), Err(RoutingError::StateLimit));
}

#[test]
fn e05_s03_after_midnight_calendar_addition_and_dst_origins() {
    for (date, weekday, origin, next_date, next_weekday, next_origin) in [
        (20260308, 6, 0, 20260309, 0, 82800), // 23-hour local day
        (20261101, 6, 0, 20261102, 0, 90000), // 25-hour local day
    ] {
        let mut d = data();
        d.timezone = "America/New_York".into();
        d.tzdb = "synthetic-origin-fixture-not-a-timezone-resolver".into();
        d.days = vec![
            ServiceDay {
                date,
                weekday,
                origin_utc: origin,
            },
            ServiceDay {
                date: next_date,
                weekday: next_weekday,
                origin_utc: next_origin,
            },
        ];
        d.calendars[0] = Calendar {
            id: 1,
            first_date: date,
            last_date: next_date,
            weekdays: 0,
            added: vec![date],
            removed: vec![],
        };
        d.links = vec![
            transfer(0, 0, Mode::Walk, Mode::Rail, TransferAction::Board, 1),
            transit(
                1,
                0,
                2,
                Mode::Rail,
                vec![Departure {
                    depart: 90010,
                    arrive: 90020,
                    run: 0,
                }],
            ),
        ];
        let mut r = request(Mode::Walk);
        r.departure_utc = 90000;
        r.deadline_utc = 90100;
        assert_eq!(arrival(d.clone(), r.clone()), Some(90020)); // previous service day's 25:00:10
        d.calendars[0].added = vec![next_date];
        assert_eq!(arrival(d.clone(), r.clone()), None); // not a civil-date-only match
                                                         // Now use the second origin itself. Normalizing it to 86400 must change
                                                         // the result, unlike the preceding service-date-only check.
        if let LinkKind::Transit { timetable, .. } = &mut d.links[1].kind {
            *timetable = Timetable::Scheduled(vec![Departure {
                depart: 10,
                arrive: 20,
                run: 0,
            }]);
        }
        r.departure_utc = next_origin;
        r.deadline_utc = next_origin + 100;
        assert_eq!(arrival(d.clone(), r.clone()), Some(next_origin + 20));
        d.days[1].origin_utc = 86400;
        assert_ne!(arrival(d, r), Some(next_origin + 20));
    }
}

#[test]
fn e05_s03_frequency_run_identity_cannot_jump_to_another_vehicle() {
    let mut d = data();
    d.links = vec![
        transfer(0, 0, Mode::Walk, Mode::Bus, TransferAction::Board, 1),
        transit(1, 0, 1, Mode::Bus, vec![]),
        transit(2, 1, 2, Mode::Bus, vec![]),
    ];
    for (i, first) in [(1, 10), (2, 25)] {
        d.links[i].seconds = 10;
        if let LinkKind::Transit {
            stop_index,
            timetable,
            ..
        } = &mut d.links[i].kind
        {
            *stop_index = (i - 1) as u32;
            *timetable = Timetable::Frequency {
                first,
                end: first + 20,
                headway: 10,
                first_run: 0,
            };
        }
    }
    // Boarding after 10 forces run 1. Run 0's second leg at 25 must not be used.
    let mut r = request(Mode::Walk);
    r.departure_utc = 11;
    let n = Network::new(d).unwrap();
    let Outcome::Journey(j) = earliest_arrival(&n, &r).unwrap() else {
        panic!("expected journey")
    };
    assert_eq!(j.arrival_utc, 45);
    assert_eq!(j.steps[1].run.as_ref().unwrap().ordinal, 1);
    assert_eq!(j.steps[2].run.as_ref().unwrap().ordinal, 1);
}

#[test]
fn e05_s06_equal_time_labels_preserve_vehicle_transfer_eligibility() {
    let mut d = data();
    d.links = vec![
        transfer(0, 0, Mode::Bicycle, Mode::Walk, TransferAction::Dismount, 1),
        transfer(1, 0, Mode::Bicycle, Mode::Walk, TransferAction::Park, 1),
        transfer(2, 0, Mode::Walk, Mode::Bus, TransferAction::Board, 1),
        transit(
            3,
            0,
            2,
            Mode::Bus,
            vec![Departure {
                depart: 10,
                arrive: 20,
                run: 0,
            }],
        ),
    ];
    if let LinkKind::Transit {
        bicycle_allowed, ..
    } = &mut d.links[3].kind
    {
        *bicycle_allowed = false;
    }
    let n = Network::new(d).unwrap();
    let Outcome::Journey(j) = earliest_arrival(&n, &request(Mode::Bicycle)).unwrap() else {
        panic!("parked path must survive carried-bike label")
    };
    assert_eq!(j.steps[0].edge, EdgeId(1));
    assert_eq!(j.arrival_utc, 20);
}

#[test]
fn e05_s06_bicycle_can_be_retrieved_only_at_its_parking_node() {
    let mut d = data();
    d.links = vec![
        transfer(0, 0, Mode::Bicycle, Mode::Walk, TransferAction::Park, 1),
        street(1, 0, 1, Mode::Walk, 10),
        street(2, 1, 0, Mode::Walk, 10),
        transfer(3, 0, Mode::Walk, Mode::Bicycle, TransferAction::Retrieve, 1),
        street(4, 0, 2, Mode::Bicycle, 10),
    ];
    // Force parking before the bicycle edge is available by starting on foot
    // carrying the bike, then mounting only via retrieval (not arbitrary switching).
    let mut r = request(Mode::Walk);
    r.vehicle = Some(Mode::Bicycle);
    assert_eq!(arrival(d.clone(), r), None);
    // A bicycle initial state can use its bicycle link directly.
    assert_eq!(arrival(d.clone(), request(Mode::Bicycle)), Some(10));
    // Relocate the cycling destination link to the remote node; no remote retrieval.
    d.links[4].from = NodeId(1);
    d.links[3].from = NodeId(1);
    d.links[3].to = NodeId(1);
    assert_eq!(arrival(d, request(Mode::Bicycle)), None);
}

#[test]
fn e05_s05_full_attribute_roundtrip_and_validation_boundaries() {
    let mut d = data();
    d.links = vec![
        street(0, 0, 1, Mode::Car, 1),
        street(1, 1, 2, Mode::Car, 1),
        street(2, 0, 2, Mode::Bicycle, 1),
        transfer(3, 0, Mode::Walk, Mode::Rail, TransferAction::Board, 1),
        transit(
            4,
            0,
            2,
            Mode::Rail,
            vec![Departure {
                depart: 5,
                arrive: 6,
                run: 5,
            }],
        ),
        transit(5, 3, 4, Mode::Bus, vec![]),
    ];
    if let LinkKind::Street {
        safety,
        cycling,
        crossing,
        ..
    } = &mut d.links[2].kind
    {
        *safety = Some(Box::new(Safety {
            stress_level: 2,
            provenance: source(),
        }));
        *cycling = CycleFacility::Protected;
        *crossing = true;
    }
    if let LinkKind::Transit {
        trip, timetable, ..
    } = &mut d.links[5].kind
    {
        *trip = "frequency".into();
        *timetable = Timetable::Frequency {
            first: 10,
            end: 20,
            headway: 2,
            first_run: 10,
        };
    }
    d.turns = vec![TurnRule {
        incoming: EdgeId(0),
        outgoing: EdgeId(1),
        only: true,
    }];
    d.calendars[0].added = vec![20261119];
    d.calendars[0].removed = vec![20261118];
    let n = Network::new(d.clone()).unwrap();
    assert_eq!(Network::decode(&n.encode()).unwrap(), n);
    let mut bad = d.clone();
    bad.links[0].distance_m = Some(f64::NAN);
    assert!(Network::new(bad).is_err());
    let mut bad = d.clone();
    bad.links[0].to = NodeId(999);
    assert!(Network::new(bad).is_err());
    let mut bad = d.clone();
    bad.nodes.push(bad.nodes[0].clone());
    assert!(Network::new(bad).is_err());
    let mut bad = d.clone();
    bad.days[0].weekday = 0;
    assert!(Network::new(bad).is_err());
    let mut bad = d.clone();
    bad.provenance.date = "2026-02-30".into();
    assert!(Network::new(bad).is_err());
    let mut bad = d.clone();
    bad.links[0].seconds = 0;
    assert!(Network::new(bad).is_err());
    let mut bad = d.clone();
    bad.turns.push(TurnRule {
        incoming: EdgeId(0),
        outgoing: EdgeId(1),
        only: false,
    });
    assert!(Network::new(bad).is_err());
    let mut bad = d.clone();
    if let LinkKind::Transit { timetable, .. } = &mut bad.links[5].kind {
        *timetable = Timetable::Frequency {
            first: 1,
            end: 2,
            headway: 0,
            first_run: 0,
        };
    }
    assert!(Network::new(bad).is_err());
    let mut bytes = n.encode(); // corrupt revision length without allocating from that length
    bytes[4..12].fill(255);
    assert!(Network::decode(&bytes).is_err());
}

#[test]
fn e05_s07_inaccessible_boarding_and_nodes_do_not_become_feasible() {
    let mut d = data();
    d.links = vec![
        transfer(0, 0, Mode::Walk, Mode::Bus, TransferAction::Board, 1),
        transit(
            1,
            0,
            2,
            Mode::Bus,
            vec![Departure {
                depart: 10,
                arrive: 20,
                run: 0,
            }],
        ),
    ];
    let mut r = request(Mode::Walk);
    r.profile.mobility_aid = true;
    d.links[0].access = Access::Steps;
    assert_eq!(arrival(d.clone(), r.clone()), None);
    d.links[0].access = Access::Accessible;
    d.nodes[2].access = Access::Impassable;
    assert_eq!(arrival(d.clone(), r.clone()), None);
    d.nodes[2].access = Access::Accessible;
    assert_eq!(arrival(d.clone(), r.clone()), Some(20));
    r.deadline_utc = 200001;
    assert!(earliest_arrival(&Network::new(d).unwrap(), &r).is_err());
}

#[test]
fn e05_s06_board_window_must_cover_the_actual_departure() {
    for (departure, expected) in [(4, Some(20)), (5, None), (10, None)] {
        let mut d = data();
        d.links = vec![
            transfer(0, 0, Mode::Walk, Mode::Bus, TransferAction::Board, 1),
            transit(
                1,
                0,
                2,
                Mode::Bus,
                vec![Departure {
                    depart: departure,
                    arrive: 20,
                    run: 0,
                }],
            ),
        ];
        if let LinkKind::Transfer { windows, .. } = &mut d.links[0].kind {
            *windows = vec![(0, 5)];
        }
        assert_eq!(
            arrival(d, request(Mode::Walk)),
            expected,
            "departure {departure}"
        );
    }
}

#[test]
fn e05_s06_alighting_cannot_wait_until_after_the_vehicle_leaves() {
    let mut d = data();
    d.links = vec![
        transfer(0, 0, Mode::Walk, Mode::Bus, TransferAction::Board, 1),
        transit(
            1,
            0,
            1,
            Mode::Bus,
            vec![Departure {
                depart: 10,
                arrive: 20,
                run: 0,
            }],
        ),
        transfer(2, 1, Mode::Bus, Mode::Walk, TransferAction::Alight, 1),
        street(3, 1, 2, Mode::Walk, 10),
    ];
    if let LinkKind::Transfer { windows, .. } = &mut d.links[2].kind {
        *windows = vec![(25, 100)];
    }
    assert_eq!(arrival(d.clone(), request(Mode::Walk)), None);
    if let LinkKind::Transfer { windows, .. } = &mut d.links[2].kind {
        *windows = vec![(20, 21)];
    }
    assert_eq!(arrival(d, request(Mode::Walk)), Some(31));
}

#[test]
fn e05_s04_parking_retrieval_does_not_erase_car_turn_history() {
    for only in [false, true] {
        let mut d = data();
        d.links = vec![
            street(0, 0, 1, Mode::Car, 10),
            street(1, 1, 2, Mode::Car, 10),
            street(2, 1, 3, Mode::Car, 10),
            transfer(3, 1, Mode::Car, Mode::Walk, TransferAction::Park, 1),
            transfer(4, 1, Mode::Walk, Mode::Car, TransferAction::Retrieve, 1),
        ];
        d.turns = vec![TurnRule {
            incoming: EdgeId(0),
            outgoing: EdgeId(if only { 2 } else { 1 }),
            only,
        }];
        assert_eq!(arrival(d.clone(), request(Mode::Car)), None);
        d.turns.clear();
        assert_eq!(arrival(d, request(Mode::Car)), Some(20));
    }
}

#[test]
fn e05_s03_sparse_frequency_window_at_48h_boundary_is_valid() {
    let mut d = data();
    d.days.truncate(1);
    d.links = vec![
        transfer(0, 0, Mode::Walk, Mode::Bus, TransferAction::Board, 1),
        transit(1, 0, 2, Mode::Bus, vec![]),
    ];
    d.links[1].seconds = 20;
    if let LinkKind::Transit { timetable, .. } = &mut d.links[1].kind {
        *timetable = Timetable::Frequency {
            first: 172780,
            end: 172800,
            headway: 60,
            first_run: 0,
        };
    }
    let mut r = request(Mode::Walk);
    r.departure_utc = 172779;
    r.deadline_utc = 172800;
    assert_eq!(arrival(d, r), Some(172800));
}

#[test]
fn e05_s06_irrelevant_extreme_transfer_window_cannot_abort_a_valid_route() {
    let mut d = data();
    d.links = vec![
        transfer(0, 0, Mode::Walk, Mode::Bus, TransferAction::Board, 10),
        street(1, 0, 2, Mode::Walk, 10),
    ];
    if let LinkKind::Transfer { windows, .. } = &mut d.links[0].kind {
        *windows = vec![(i64::MAX - 1, i64::MAX)];
    }
    assert_eq!(arrival(d, request(Mode::Walk)), Some(10));
}

#[test]
fn e05_s01_street_beyond_horizon_does_not_overflow_valid_alternative() {
    let mut d = data();
    d.coverage_start = i64::MAX - 100;
    d.coverage_end = i64::MAX;
    d.links = vec![
        street(0, 0, 3, Mode::Walk, 20),
        street(1, 0, 2, Mode::Walk, 1),
    ];
    let mut r = request(Mode::Walk);
    r.departure_utc = i64::MAX - 10;
    r.deadline_utc = i64::MAX;
    assert_eq!(arrival(d, r), Some(i64::MAX - 9));
}

#[test]
fn e05_s03_exhaustive_small_timetable_oracle() {
    // Independent exhaustive enumeration, not another graph search. All three
    // first rides and their same-run continuations/rail transfers are examined.
    let first = vec![
        Departure {
            depart: 10,
            arrive: 20,
            run: 0,
        },
        Departure {
            depart: 15,
            arrive: 18,
            run: 1,
        },
        Departure {
            depart: 30,
            arrive: 35,
            run: 2,
        },
    ];
    let continuation = vec![
        Departure {
            depart: 21,
            arrive: 30,
            run: 0,
        },
        Departure {
            depart: 20,
            arrive: 40,
            run: 1,
        },
        Departure {
            depart: 36,
            arrive: 45,
            run: 2,
        },
    ];
    let express = vec![
        Departure {
            depart: 20,
            arrive: 25,
            run: 0,
        },
        Departure {
            depart: 40,
            arrive: 44,
            run: 1,
        },
    ];
    let mut d = data();
    d.links = vec![
        transfer(0, 0, Mode::Walk, Mode::Bus, TransferAction::Board, 1),
        transit(1, 0, 1, Mode::Bus, first.clone()),
        transit(2, 1, 2, Mode::Bus, continuation.clone()),
        transfer(3, 1, Mode::Bus, Mode::Walk, TransferAction::Alight, 1),
        transfer(4, 1, Mode::Walk, Mode::Rail, TransferAction::Board, 1),
        transit(5, 1, 2, Mode::Rail, express.clone()),
    ];
    if let LinkKind::Transit { stop_index, .. } = &mut d.links[2].kind {
        *stop_index = 1;
    }
    if let LinkKind::Transit { trip, .. } = &mut d.links[5].kind {
        *trip = "express".into();
    }
    for start in 0..=41 {
        for deadline in [24, 25, 29, 30, 44, 45, 50] {
            if start > deadline {
                continue;
            }
            let mut possibilities = vec![];
            for ride in &first {
                if i64::from(ride.depart) < start + 1 {
                    continue;
                }
                for next in &continuation {
                    if next.run == ride.run && next.depart >= ride.arrive {
                        possibilities.push(i64::from(next.arrive));
                    }
                }
                for train in &express {
                    if train.depart >= ride.arrive + 2 {
                        possibilities.push(i64::from(train.arrive));
                    }
                }
            }
            let expected = possibilities.into_iter().filter(|t| *t <= deadline).min();
            let mut r = request(Mode::Walk);
            r.departure_utc = start;
            r.deadline_utc = deadline;
            assert_eq!(
                arrival(d.clone(), r),
                expected,
                "start {start}, deadline {deadline}"
            );
        }
    }
}
