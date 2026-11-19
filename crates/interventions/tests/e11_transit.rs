//! E11-S01..07 deterministic acceptance; arbitrary modeled inputs, not calibration.
#[allow(dead_code)]
#[path = "../examples/support/e11.rs"]
mod fixture;
use fixture::*;
use interventions::{evaluation::road_supply, scenario::*, transit::*};
use mobility_graph::{network::*, EdgeId, Mode, NodeId};
use mobility_routing::{earliest_arrival, Outcome};
use std::collections::BTreeMap;
fn arrival(h: &History) -> i64 {
    match earliest_arrival(&h.current().network, &request()).unwrap() {
        Outcome::Journey(j) => j.arrival_utc,
        Outcome::Unmet(_) => -1,
    }
}
#[test]
fn e11_s01_s02_add_reroute_frequency_hours_calendar_and_atomic_failures() {
    let mut h = history();
    assert_eq!(arrival(&h), -1);
    install(&mut h);
    assert_eq!(arrival(&h), 211);
    let Outcome::Journey(before) = earliest_arrival(&h.current().network, &request()).unwrap()
    else {
        panic!("bus missing")
    };
    assert_eq!(
        before
            .steps
            .iter()
            .find(|s| s.run.is_some())
            .unwrap()
            .waiting_seconds,
        89
    );
    let mut changed = service();
    if let LinkKind::Transit { timetable, .. } = &mut changed.rides[0].kind {
        *timetable = Timetable::Frequency {
            first: 20,
            end: 61,
            headway: 20,
            first_run: 0,
        };
    }
    h.apply(&batch(
        &h,
        "frequent",
        vec![Change::Transit(changed.clone())],
    ))
    .unwrap();
    assert_eq!(arrival(&h), 131);
    let Outcome::Journey(after) = earliest_arrival(&h.current().network, &request()).unwrap()
    else {
        panic!("frequent bus missing")
    };
    assert_eq!(
        after
            .steps
            .iter()
            .find(|s| s.run.is_some())
            .unwrap()
            .waiting_seconds,
        9
    );
    let mut last = request();
    last.departure_utc = 49;
    let Outcome::Journey(last) = earliest_arrival(&h.current().network, &last).unwrap() else {
        panic!("last run missing")
    };
    assert_eq!(last.arrival_utc, 171);
    let mut late = request();
    late.departure_utc = 50;
    assert!(matches!(
        earliest_arrival(&h.current().network, &late).unwrap(),
        Outcome::Unmet(_)
    ));
    let saved = h.clone();
    changed.rides[0].to = NodeId(999);
    assert!(h
        .apply(&batch(&h, "bad-stop", vec![Change::Transit(changed)]))
        .unwrap_err()
        .0
        .contains("stop"));
    assert_eq!(h, saved);
    let mut reroute = service();
    reroute.rides = vec![
        ride(21, 1, 5, Mode::Bus, "bus", 20, 30),
        ride(22, 5, 2, Mode::Bus, "bus", 55, 30),
    ];
    if let LinkKind::Transit { stop_index, .. } = &mut reroute.rides[1].kind {
        *stop_index = 1;
    }
    h.apply(&batch(
        &h,
        "reroute",
        vec![Change::Transit(reroute.clone())],
    ))
    .unwrap();
    assert!(h.current().network.link(EdgeId(20)).is_none());
    assert_eq!(arrival(&h), 96);
    let encoded = h.current().network.encode();
    assert!(h.undo());
    assert_eq!(arrival(&h), 131);
    assert!(h.redo());
    assert_eq!(encoded, h.current().network.encode());
    let saved = h.clone();
    if let LinkKind::Transit { timetable, .. } = &mut reroute.rides[1].kind {
        *timetable = Timetable::Scheduled(vec![Departure {
            depart: 25,
            arrive: 30,
            run: 0,
        }]);
    }
    assert!(h
        .apply(&batch(&h, "bad-dwell", vec![Change::Transit(reroute)]))
        .unwrap_err()
        .0
        .contains("dwell"));
    assert_eq!(h, saved);
    let mut cancelled = service();
    cancelled.calendar = calendar(2);
    cancelled.calendar.removed.push(20260105);
    if let LinkKind::Transit { calendar, .. } = &mut cancelled.rides[0].kind {
        *calendar = 2;
    }
    assert!(h
        .apply(&batch(
            &h,
            "no-active-service",
            vec![Change::Transit(cancelled)]
        ))
        .unwrap_err()
        .0
        .contains("no active"));
    let mut bad_calendar = service();
    bad_calendar.calendar.first_date = 20260230;
    assert!(h
        .apply(&batch(
            &h,
            "bad-calendar",
            vec![Change::Transit(bad_calendar)]
        ))
        .is_err());
    assert_eq!(h, saved);
}
#[test]
fn e11_s03_priority_uses_shared_lane_inventory_and_reroute_cannot_orphan_it() {
    let mut h = history();
    install(&mut h);
    let mut faster = service();
    faster.rides[0].seconds = 50; // Preserve all three run identities and headway.
    h.apply(&batch(
        &h,
        "brt",
        vec![
            Change::Transit(faster.clone()),
            Change::Allocate {
                road: EdgeId(4),
                lanes: 1,
                target: faster.rides[0].clone(),
                assessment: None,
            },
        ],
    ))
    .unwrap();
    assert_eq!(arrival(&h), 161);
    assert_eq!(h.current().transit["bus"].estimate.runs, 3);
    assert_eq!(h.current().roads[&EdgeId(4)].general_lanes, 2);
    assert_eq!(
        h.current().road_capacities().unwrap(),
        vec![(EdgeId(4), 200)]
    );
    assert_eq!(
        road_supply(h.current(), 2).unwrap()[0].vehicles_per_hour,
        vec![200.0, 200.0]
    );
    let mut shared = faster.clone();
    shared.trip = "second".into();
    shared.rides[0].id = EdgeId(23);
    if let LinkKind::Transit { trip, .. } = &mut shared.rides[0].kind {
        *trip = "second".into();
    }
    h.apply(&batch(
        &h,
        "shared",
        vec![
            Change::Transit(shared.clone()),
            Change::AssociateBus {
                road: EdgeId(4),
                link: shared.rides[0].clone(),
            },
        ],
    ))
    .unwrap();
    assert_eq!(h.current().roads[&EdgeId(4)].bus_lanes, 1);
    let saved = h.clone();
    faster.rides[0].to = NodeId(5);
    assert!(h
        .apply(&batch(&h, "orphan", vec![Change::Transit(faster)]))
        .unwrap_err()
        .0
        .contains("orphan"));
    assert!(h
        .apply(&batch(
            &h,
            "overallocated",
            vec![Change::Allocate {
                road: EdgeId(4),
                lanes: 3,
                target: shared.rides[0].clone(),
                assessment: None
            }]
        ))
        .is_err());
    assert_eq!(h, saved);
}
fn rail(h: &mut History, connection: u32) {
    let train = Service {
        trip: "rail".into(),
        calendar: calendar(1),
        rides: vec![ride(40, 1, 4, Mode::Rail, "rail", 100, 40)],
        operations: operations(),
    };
    let tram = Service {
        trip: "tram".into(),
        calendar: calendar(1),
        rides: vec![ride(41, 4, 2, Mode::LightRail, "tram", connection, 30)],
        operations: operations(),
    };
    h.apply(&batch(
        h,
        "rail",
        vec![
            Change::Station(facilities(Mode::Rail, 1, 4, 50)),
            Change::Station(facilities(Mode::LightRail, 4, 2, 52)),
            Change::Transit(train),
            Change::Transit(tram),
        ],
    ))
    .unwrap();
}
#[test]
fn e11_s04_s07_timed_rail_transfers_fares_and_generalized_cost_are_separate() {
    let mut h = history();
    rail(&mut h, 150);
    assert_eq!(arrival(&h), 191);
    let policy = fares(&["rail", "tram"], 250);
    let a = evaluate(&h.current().network, &request(), &policy, "USD", 1000).unwrap();
    assert_eq!(a.fare_minor, Some(250));
    assert_eq!(a.generalized_seconds, Some(1091.0));
    let mut no_discount = policy.clone();
    no_discount.get_mut("tram").unwrap().transfer = None;
    let b = evaluate(&h.current().network, &request(), &no_discount, "USD", 1000).unwrap();
    assert_eq!(a.outcome, b.outcome);
    assert_eq!(b.fare_minor, Some(500));
    assert_eq!(b.generalized_seconds, Some(1991.0));
    assert!(evaluate(
        &h.current().network,
        &request(),
        &BTreeMap::new(),
        "USD",
        1000
    )
    .is_err());
    assert!(evaluate(&h.current().network, &request(), &policy, "USD", 0).is_err());
    let accessible_network = h.current().network.clone();
    let mut facility = facilities(Mode::LightRail, 4, 2, 52);
    facility.links.truncate(1);
    facility.links[0].seconds = 20;
    h.apply(&batch(
        &h,
        "minimum-transfer",
        vec![Change::Station(facility)],
    ))
    .unwrap();
    assert_eq!(arrival(&h), -1);
    let unmet = evaluate(&h.current().network, &request(), &policy, "USD", 1000).unwrap();
    assert_eq!(unmet.fare_minor, None);
    assert_eq!(unmet.generalized_seconds, None);
    let mut missed = history();
    rail(&mut missed, 140);
    assert_eq!(arrival(&missed), -1);
    let mut d = accessible_network.data().clone();
    d.nodes
        .iter_mut()
        .find(|n| n.id == NodeId(4))
        .unwrap()
        .access = Access::Steps;
    let n = Network::new(d).unwrap();
    assert!(matches!(
        earliest_arrival(&n, &request()).unwrap(),
        Outcome::Unmet(_)
    ));
}
#[test]
fn e11_s05_bike_parking_and_station_access_change_real_door_journeys_without_replacing_noncyclist_access(
) {
    let mut h = history();
    install(&mut h);
    let mut r = request();
    r.from = NodeId(6);
    r.initial_mode = Mode::Bicycle;
    r.vehicle = Some(Mode::Bicycle);
    r.profile.mobility_aid = false;
    r.profile.allowed_modes.push(Mode::Bicycle);
    assert!(matches!(
        earliest_arrival(&h.current().network, &r).unwrap(),
        Outcome::Unmet(_)
    ));
    let station = Station {
        nodes: vec![],
        links: vec![
            street(60, 6, 1, Mode::Bicycle, 10),
            transfer(61, 1, Mode::Bicycle, Mode::Walk, TransferAction::Park),
        ],
        assessments: BTreeMap::from([(EdgeId(60), assessment())]),
    };
    h.apply(&batch(&h, "bike-access", vec![Change::Station(station)]))
        .unwrap();
    let Outcome::Journey(j) = earliest_arrival(&h.current().network, &r).unwrap() else {
        panic!("bike access journey missing")
    };
    assert_eq!(j.arrival_utc, 211);
    assert!(j.steps.iter().any(|s| s.edge == EdgeId(61)));
    assert!(j.steps.iter().any(|s| s.mode == Mode::Bus));
    assert_eq!(arrival(&h), 211); // mobility-aid user retains a walking-access transit option.
    r.to = NodeId(2);
    r.initial_mode = Mode::Walk; // bicycle still carried at origin; no implicit riding/parking.
    r.profile.mobility_aid = true;
    assert!(matches!(
        earliest_arrival(&h.current().network, &r).unwrap(),
        Outcome::Unmet(_)
    ));
    let saved = h.clone();
    let invalid = Station {
        nodes: vec![],
        links: vec![street(62, 6, 1, Mode::Bicycle, 5)],
        assessments: BTreeMap::new(),
    };
    assert!(h
        .apply(&batch(&h, "unassessed", vec![Change::Station(invalid)]))
        .is_err());
    assert_eq!(h, saved);
}
#[test]
fn e11_s06_operations_count_each_run_once_and_bound_expansion() {
    let mut h = history();
    install(&mut h);
    let e = &h.current().transit["bus"].estimate;
    assert_eq!((e.runs, e.peak_vehicles, e.peak_operators), (3, 2, 4));
    assert_eq!((e.vehicle_seconds, e.operator_seconds), (390, 780));
    assert_eq!(e.vehicle_metres, 3300.0);
    assert_eq!(e.operating_minor, 1110.0);
    let mut bound = operations();
    bound.max_run_instances = 2;
    assert!(estimate(&h.current().network, "bus", &bound)
        .unwrap_err()
        .0
        .contains("bound"));
    let mut chain = service();
    chain.rides = vec![
        ride(70, 1, 5, Mode::Bus, "bus", 100, 40),
        ride(71, 5, 2, Mode::Bus, "bus", 150, 50),
    ];
    if let LinkKind::Transit { stop_index, .. } = &mut chain.rides[1].kind {
        *stop_index = 1;
    }
    h.apply(&batch(&h, "chain", vec![Change::Transit(chain)]))
        .unwrap();
    let e = &h.current().transit["bus"].estimate;
    assert_eq!((e.runs, e.peak_vehicles, e.vehicle_seconds), (1, 1, 130));
    assert_eq!(e.vehicle_metres, 2100.0);
    let cost = evaluate(
        &h.current().network,
        &request(),
        &fares(&["bus"], 250),
        "USD",
        1000,
    )
    .unwrap();
    assert_eq!(cost.fare_minor, Some(250)); // multi-leg through-run is not charged twice.
    let saved = h.clone();
    let mut invalid = service();
    invalid.operations.operators_per_vehicle = 0;
    assert!(h
        .apply(&batch(&h, "no-staff", vec![Change::Transit(invalid)]))
        .is_err());
    assert_eq!(h, saved);
}
