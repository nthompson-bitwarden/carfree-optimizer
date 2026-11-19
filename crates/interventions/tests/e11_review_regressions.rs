//! Independent-review regressions for E11; all inputs are modeled fixtures.
#[allow(dead_code)]
#[path = "../examples/support/e11.rs"]
mod fixture;
use fixture::*;
use interventions::{scenario::*, transit::*};
use mobility_graph::{network::*, EdgeId, Mode, NodeId};
use mobility_routing::Outcome;
#[test]
fn e11_s04_s07_fare_evaluation_requires_alighting_before_deadline() {
    let mut h = history();
    install(&mut h);
    let mut r = request();
    r.to = NodeId(2);
    r.deadline_utc = 200;
    let result = evaluate(&h.current().network, &r, &fares(&["bus"], 250), "USD", 1000).unwrap();
    assert!(matches!(result.outcome, Outcome::Unmet(_)));
    assert_eq!(result.fare_minor, None);
    r.deadline_utc = 201;
    let result = evaluate(&h.current().network, &r, &fares(&["bus"], 250), "USD", 1000).unwrap();
    let Outcome::Journey(j) = result.outcome else {
        panic!("alighting should fit")
    };
    assert_eq!(j.arrival_utc, 201);
}
#[test]
fn e11_s02_s03_duplicate_effective_writes_are_rejected() {
    let mut h = history();
    install(&mut h);
    let saved = h.clone();
    let station = facilities(Mode::Bus, 1, 2, 30);
    assert!(h
        .apply(&batch(
            &h,
            "duplicates",
            vec![Change::Station(station.clone()), Change::Station(station)]
        ))
        .is_err());
    assert_eq!(h, saved);
    let current = service();
    h.apply(&batch(
        &h,
        "allocated",
        vec![Change::Allocate {
            road: EdgeId(4),
            lanes: 1,
            target: current.rides[0].clone(),
            assessment: None,
        }],
    ))
    .unwrap();
    let saved = h.clone();
    let mut fast = service();
    fast.rides[0].seconds = 50;
    // This timing-only edit is legal by itself; rejection must be the conflict.
    assert!(h
        .preview(&batch(
            &h,
            "timing-alone",
            vec![Change::BusTiming {
                link: fast.rides[0].clone()
            }]
        ))
        .is_ok());
    assert!(h
        .apply(&batch(
            &h,
            "competing",
            vec![
                Change::BusTiming {
                    link: fast.rides[0].clone()
                },
                Change::Transit(service())
            ]
        ))
        .unwrap_err()
        .0
        .contains("conflicting effective"));
    assert_eq!(h, saved);
}
#[test]
fn e11_s01_s05_station_cannot_silently_remove_last_usable_alighting() {
    let mut h = history();
    install(&mut h);
    let saved = h.clone();
    let mut station = facilities(Mode::Bus, 1, 2, 30);
    station.links.remove(0);
    if let LinkKind::Transfer { windows, .. } = &mut station.links[0].kind {
        *windows = vec![(500, 600)];
    }
    assert!(h
        .apply(&batch(&h, "shutout", vec![Change::Station(station)]))
        .is_err());
    assert_eq!(h, saved);
}
#[test]
fn e11_s05_intermediate_station_cannot_be_silently_closed() {
    let mut h = history();
    install(&mut h);
    let mut chain = service();
    chain.rides = vec![
        ride(20, 1, 5, Mode::Bus, "bus", 100, 40),
        ride(21, 5, 2, Mode::Bus, "bus", 150, 50),
    ];
    if let LinkKind::Transit { stop_index, .. } = &mut chain.rides[1].kind {
        *stop_index = 1;
    }
    h.apply(&batch(
        &h,
        "intermediate",
        vec![
            Change::Station(facilities(Mode::Bus, 5, 5, 70)),
            Change::Transit(chain),
        ],
    ))
    .unwrap();
    let saved = h.clone();
    let mut closed = facilities(Mode::Bus, 5, 5, 70);
    closed.links.remove(0);
    if let LinkKind::Transfer { windows, .. } = &mut closed.links[0].kind {
        *windows = vec![(900, 1000)];
    }
    assert!(h
        .apply(&batch(
            &h,
            "closed-intermediate",
            vec![Change::Station(closed)]
        ))
        .is_err());
    assert_eq!(h, saved);
}
#[test]
fn e11_s01_s04_service_cannot_replace_usable_rides_with_impassable_links() {
    let mut h = history();
    let mut steps = service();
    steps.rides[0].access = Access::Steps;
    h.apply(&batch(
        &h,
        "steps",
        vec![
            Change::Station(facilities(Mode::Bus, 1, 2, 30)),
            Change::Transit(steps.clone()),
        ],
    ))
    .unwrap();
    let mut r = request();
    r.profile.step_free = false;
    r.profile.mobility_aid = false;
    assert!(matches!(
        evaluate(&h.current().network, &r, &fares(&["bus"], 250), "USD", 1000)
            .unwrap()
            .outcome,
        Outcome::Journey(_)
    ));
    let saved = h.clone();
    for edge in [20, 999] {
        let mut closed = steps.clone();
        closed.rides[0].access = Access::Impassable;
        closed.rides[0].id = EdgeId(edge);
        assert!(h
            .apply(&batch(&h, "impassable", vec![Change::Transit(closed)]))
            .is_err());
        assert_eq!(h, saved);
    }
    let mut chain = service();
    chain.rides = vec![
        ride(20, 1, 7, Mode::Bus, "bus", 100, 40),
        ride(21, 7, 2, Mode::Bus, "bus", 150, 50),
    ];
    if let LinkKind::Transit { stop_index, .. } = &mut chain.rides[1].kind {
        *stop_index = 1;
    }
    let blocked_node = Station {
        nodes: vec![Node {
            id: NodeId(7),
            access: Access::Impassable,
        }],
        links: vec![],
        assessments: Default::default(),
    };
    assert!(h
        .apply(&batch(
            &h,
            "blocked-intermediate",
            vec![Change::Station(blocked_node), Change::Transit(chain)]
        ))
        .is_err());
    assert_eq!(h, saved);
}
#[test]
fn e11_s01_s04_boarding_opportunity_must_start_inside_query_coverage() {
    let mut h = history();
    let saved = h.clone();
    let mut station = facilities(Mode::Bus, 1, 2, 30);
    if let LinkKind::Transfer { windows, .. } = &mut station.links[0].kind {
        *windows = vec![(-100, 10000)];
    }
    let mut early = service();
    early.rides[0] = ride(20, 1, 2, Mode::Bus, "bus", 0, 100);
    assert!(h
        .apply(&batch(
            &h,
            "too-early",
            vec![Change::Station(station), Change::Transit(early)]
        ))
        .is_err());
    assert_eq!(h, saved);
}
#[test]
fn e11_s05_intermediate_boarding_duration_must_fit_coverage() {
    let mut initial = history();
    install(&mut initial);
    let mut chain = service();
    chain.rides = vec![
        ride(20, 1, 5, Mode::Bus, "bus", 100, 40),
        ride(21, 5, 2, Mode::Bus, "bus", 150, 50),
    ];
    if let LinkKind::Transit { stop_index, .. } = &mut chain.rides[1].kind {
        *stop_index = 1;
    }
    initial
        .apply(&batch(
            &initial,
            "chain",
            vec![
                Change::Station(facilities(Mode::Bus, 5, 5, 70)),
                Change::Transit(chain),
            ],
        ))
        .unwrap();
    let mut data = initial.current().network.data().clone();
    data.coverage_start = 140;
    let mut h = History::new(
        "partial-baseline".into(),
        Network::new(data).unwrap(),
        vec![],
        vec![],
    )
    .unwrap();
    let saved = h.clone();
    let mut station = facilities(Mode::Bus, 5, 5, 70);
    station.links.truncate(1);
    station.links[0].seconds = 11;
    assert!(h
        .apply(&batch(&h, "too-slow-board", vec![Change::Station(station)]))
        .unwrap_err()
        .0
        .contains("last usable"));
    assert_eq!(h, saved);
}
#[test]
fn e11_s05_station_opportunity_requires_both_ride_endpoints_inside_coverage() {
    let mut original = history();
    install(&mut original);
    let mut chain = service();
    chain.rides = vec![
        ride(20, 1, 5, Mode::Bus, "bus", 100, 40),
        ride(21, 5, 2, Mode::Bus, "bus", 150, 50),
    ];
    for (i, link) in chain.rides.iter_mut().enumerate() {
        if let LinkKind::Transit {
            stop_index,
            timetable: Timetable::Scheduled(events),
            ..
        } = &mut link.kind
        {
            *stop_index = i as u32;
            let (depart, arrive) = if i == 0 { (250, 290) } else { (295, 345) };
            events.push(Departure {
                depart,
                arrive,
                run: 1,
            });
        }
    }
    original
        .apply(&batch(
            &original,
            "chain",
            vec![
                Change::Station(facilities(Mode::Bus, 5, 5, 70)),
                Change::Transit(chain),
            ],
        ))
        .unwrap();
    for board in [true, false] {
        let mut data = original.current().network.data().clone();
        if board {
            data.coverage_end = 310;
        } else {
            data.coverage_start = 120;
        }
        let mut h = History::new(
            "boundary".into(),
            Network::new(data).unwrap(),
            vec![],
            vec![],
        )
        .unwrap();
        let mut r = request();
        r.deadline_utc = 310;
        if board {
            r.from = NodeId(5);
        } else {
            r.from = NodeId(1);
            r.to = NodeId(5);
            r.departure_utc = 120;
        }
        assert!(matches!(
            evaluate(&h.current().network, &r, &fares(&["bus"], 250), "USD", 1000)
                .unwrap()
                .outcome,
            Outcome::Journey(_)
        ));
        let saved = h.clone();
        let mut station = facilities(Mode::Bus, 5, 5, 70);
        let mut link = station.links.remove(if board { 0 } else { 1 });
        if let LinkKind::Transfer { windows, .. } = &mut link.kind {
            *windows = if board {
                vec![(290, 310)]
            } else {
                vec![(130, 145)]
            };
        }
        station.links = vec![link];
        assert!(h
            .apply(&batch(&h, "remove-last", vec![Change::Station(station)]))
            .is_err());
        assert_eq!(h, saved);
    }
}
#[test]
fn e11_s01_s04_s05_fresh_edge_id_cannot_downgrade_retained_step_free_service() {
    let mut h = history();
    install(&mut h);
    assert!(matches!(
        evaluate(
            &h.current().network,
            &request(),
            &fares(&["bus"], 250),
            "USD",
            1000
        )
        .unwrap()
        .outcome,
        Outcome::Journey(_)
    ));
    let saved = h.clone();
    let mut downgraded = service();
    downgraded.rides[0].id = EdgeId(999);
    downgraded.rides[0].access = Access::Steps;
    assert!(h
        .apply(&batch(
            &h,
            "fresh-id-downgrade",
            vec![Change::Transit(downgraded)]
        ))
        .unwrap_err()
        .0
        .contains("accessible"));
    assert_eq!(h, saved);
}
#[test]
fn e11_s06_run_limit_also_bounds_out_of_coverage_departures_examined() {
    let mut h = history();
    install(&mut h);
    let mut d = h.current().network.data().clone();
    d.coverage_start = 300;
    let n = Network::new(d).unwrap();
    let mut limits = operations();
    limits.max_run_instances = 1;
    assert!(estimate(&n, "bus", &limits).is_err()); // Three template departures must not bypass a one-event bound.
    assert_eq!(
        h.current().transit["bus"].estimate.graph_revision,
        h.current().network.data().revision
    );
    assert_eq!(h.current().network.link(EdgeId(20)).unwrap().seconds, 100);
}
