//! E11 boundary and failure tests; deterministic modeled data only.
#[allow(dead_code)]
#[path = "../examples/support/e11.rs"]
mod fixture;
use fixture::*;
use interventions::{scenario::*, transit::*};
use mobility_graph::{network::*, EdgeId, Mode, NodeId};
use mobility_routing::{earliest_arrival, Outcome};
use std::collections::BTreeMap;
#[test]
fn e11_s01_s02_service_changes_preserve_declared_access_or_roll_back() {
    let mut original = history();
    install(&mut original);
    let requirement = Requirement {
        id: "essential-transit".into(),
        kind: AccessKind::Transit,
        request: request(),
        permitted_car_edges: None,
        provenance: provenance(),
    };
    let mut h = History::new(
        "protected".into(),
        original.current().network.clone(),
        vec![],
        vec![requirement],
    )
    .unwrap();
    let saved = h.clone();
    let mut changed = service();
    changed.rides[0] = ride(20, 1, 5, Mode::Bus, "bus", 100, 100);
    assert!(h
        .apply(&batch(
            &h,
            "lost-access",
            vec![
                Change::Station(facilities(Mode::Bus, 1, 5, 70)),
                Change::Transit(changed)
            ]
        ))
        .unwrap_err()
        .0
        .contains("unmet"));
    assert_eq!(h, saved);
    let mut stale = batch(&h, "stale", vec![Change::Transit(service())]);
    stale.expected_parent = "wrong".into();
    assert!(h.apply(&stale).is_err());
    assert_eq!(h, saved);
    let mut observed = batch(&h, "observed", vec![Change::Transit(service())]);
    observed.provenance.basis = Basis::Observed;
    assert!(h.apply(&observed).is_err());
    assert_eq!(h, saved);
}
#[test]
fn e11_s02_s06_calendar_exceptions_overnight_coverage_and_half_open_duties() {
    let mut d = network().data().clone();
    d.coverage_end = 180000;
    d.days.push(ServiceDay {
        date: 20260106,
        weekday: 1,
        origin_utc: 86400,
    });
    let mut h = History::new("base".into(), Network::new(d).unwrap(), vec![], vec![]).unwrap();
    let mut s = service();
    s.calendar.last_date = 20260106;
    s.calendar.removed = vec![20260105];
    s.rides[0] = ride(20, 1, 2, Mode::Bus, "bus", 100, 100);
    let mut station = facilities(Mode::Bus, 1, 2, 30);
    for link in &mut station.links {
        if let LinkKind::Transfer { windows, .. } = &mut link.kind {
            *windows = vec![(0, 180000)];
        }
    }
    h.apply(&batch(
        &h,
        "exception",
        vec![Change::Station(station), Change::Transit(s)],
    ))
    .unwrap();
    assert!(matches!(
        earliest_arrival(&h.current().network, &request()).unwrap(),
        Outcome::Unmet(_)
    ));
    let mut next = request();
    next.departure_utc = 86400;
    next.deadline_utc = 86900;
    let Outcome::Journey(j) = earliest_arrival(&h.current().network, &next).unwrap() else {
        panic!("added day missing")
    };
    assert_eq!(j.arrival_utc, 86611);
    assert_eq!(h.current().transit["bus"].estimate.runs, 1);
    let mut overnight = service();
    overnight.calendar = calendar(2);
    overnight.rides[0] = ride(20, 1, 2, Mode::Bus, "bus", 86500, 100);
    if let LinkKind::Transit { calendar, .. } = &mut overnight.rides[0].kind {
        *calendar = 2;
    }
    h.apply(&batch(&h, "overnight", vec![Change::Transit(overnight)]))
        .unwrap();
    assert_eq!(h.current().transit["bus"].estimate.runs, 1);
    let mut clipped = h.current().network.data().clone();
    clipped.coverage_end = 86550;
    assert!(
        estimate(&Network::new(clipped).unwrap(), "bus", &operations())
            .unwrap_err()
            .0
            .contains("partial")
    );
    let mut simple = history();
    let mut touching = service();
    if let LinkKind::Transit { timetable, .. } = &mut touching.rides[0].kind {
        *timetable = Timetable::Frequency {
            first: 100,
            end: 361,
            headway: 130,
            first_run: 0,
        };
    }
    simple
        .apply(&batch(
            &simple,
            "touching",
            vec![
                Change::Station(facilities(Mode::Bus, 1, 2, 30)),
                Change::Transit(touching),
            ],
        ))
        .unwrap();
    assert_eq!(simple.current().transit["bus"].estimate.peak_vehicles, 1);
}
#[test]
fn e11_s04_station_creation_and_rail_frequency_change_door_arrivals() {
    let mut h = history();
    let mut f = facilities(Mode::Rail, 1, 7, 30);
    f.nodes.push(Node {
        id: NodeId(7),
        access: Access::Accessible,
    });
    f.links.push(street(33, 7, 3, Mode::Walk, 10));
    f.assessments.insert(EdgeId(33), assessment());
    let mut s = Service {
        trip: "train".into(),
        calendar: calendar(1),
        rides: vec![ride(20, 1, 7, Mode::Rail, "train", 100, 100)],
        operations: operations(),
    };
    h.apply(&batch(
        &h,
        "station",
        vec![Change::Station(f), Change::Transit(s.clone())],
    ))
    .unwrap();
    let Outcome::Journey(j) = earliest_arrival(&h.current().network, &request()).unwrap() else {
        panic!("train missing")
    };
    assert_eq!(j.arrival_utc, 211);
    if let LinkKind::Transit { timetable, .. } = &mut s.rides[0].kind {
        *timetable = Timetable::Frequency {
            first: 20,
            end: 81,
            headway: 20,
            first_run: 0,
        };
    }
    h.apply(&batch(&h, "rail-frequency", vec![Change::Transit(s)]))
        .unwrap();
    let Outcome::Journey(j) = earliest_arrival(&h.current().network, &request()).unwrap() else {
        panic!("frequent train missing")
    };
    assert_eq!(j.arrival_utc, 131);
}
#[test]
fn e11_s05_station_walk_access_and_same_node_bicycle_retrieval_are_enforced() {
    let mut h = history();
    install(&mut h);
    let mut r = request();
    r.from = NodeId(6);
    assert!(matches!(
        earliest_arrival(&h.current().network, &r).unwrap(),
        Outcome::Unmet(_)
    ));
    let s = Station {
        nodes: vec![Node {
            id: NodeId(7),
            access: Access::Accessible,
        }],
        links: vec![
            street(60, 6, 1, Mode::Walk, 20),
            street(61, 6, 1, Mode::Bicycle, 10),
            transfer(62, 1, Mode::Bicycle, Mode::Walk, TransferAction::Park),
            transfer(63, 2, Mode::Walk, Mode::Bicycle, TransferAction::Retrieve),
            street(64, 2, 7, Mode::Bicycle, 10),
        ],
        assessments: [60, 61, 64]
            .into_iter()
            .map(|id| (EdgeId(id), assessment()))
            .collect(),
    };
    h.apply(&batch(&h, "connections", vec![Change::Station(s)]))
        .unwrap();
    let Outcome::Journey(j) = earliest_arrival(&h.current().network, &r).unwrap() else {
        panic!("step-free walk access missing")
    };
    assert_eq!(j.arrival_utc, 211);
    r.initial_mode = Mode::Bicycle;
    r.vehicle = Some(Mode::Bicycle);
    r.profile.mobility_aid = false;
    r.profile.allowed_modes.push(Mode::Bicycle);
    r.to = NodeId(7);
    assert!(matches!(
        earliest_arrival(&h.current().network, &r).unwrap(),
        Outcome::Unmet(_)
    )); // parked at 1, cannot retrieve at 2
    let saved = h.clone();
    let bad = Station {
        nodes: vec![],
        links: vec![street(1, 0, 1, Mode::Car, 1)],
        assessments: BTreeMap::new(),
    };
    assert!(h
        .apply(&batch(&h, "road-bypass", vec![Change::Station(bad)]))
        .is_err());
    assert_eq!(h, saved);
    let mut down = facilities(Mode::Bus, 1, 2, 30);
    down.links[0].access = Access::Steps;
    assert!(h
        .apply(&batch(&h, "downgrade", vec![Change::Station(down)]))
        .is_err());
    assert_eq!(h, saved);
}
#[test]
fn e11_s07_transfer_window_is_half_open_and_currencies_cannot_be_mixed() {
    let mut h = history();
    let first = Service {
        trip: "first".into(),
        calendar: calendar(1),
        rides: vec![ride(20, 1, 4, Mode::Bus, "first", 100, 40)],
        operations: operations(),
    };
    let second = Service {
        trip: "second".into(),
        calendar: calendar(1),
        rides: vec![ride(21, 4, 2, Mode::Bus, "second", 150, 30)],
        operations: operations(),
    };
    h.apply(&batch(
        &h,
        "two",
        vec![
            Change::Station(facilities(Mode::Bus, 1, 4, 30)),
            Change::Station(facilities(Mode::Bus, 4, 2, 32)),
            Change::Transit(first),
            Change::Transit(second),
        ],
    ))
    .unwrap();
    let mut policy = fares(&["first", "second"], 250);
    policy
        .get_mut("second")
        .unwrap()
        .transfer
        .as_mut()
        .unwrap()
        .window_seconds = 50;
    assert_eq!(
        evaluate(&h.current().network, &request(), &policy, "USD", 1000)
            .unwrap()
            .fare_minor,
        Some(500)
    );
    policy
        .get_mut("second")
        .unwrap()
        .transfer
        .as_mut()
        .unwrap()
        .window_seconds = 51;
    assert_eq!(
        evaluate(&h.current().network, &request(), &policy, "USD", 1000)
            .unwrap()
            .fare_minor,
        Some(250)
    );
    policy.get_mut("second").unwrap().currency = "EUR".into();
    assert!(
        evaluate(&h.current().network, &request(), &policy, "USD", 1000)
            .unwrap_err()
            .0
            .contains("currency")
    );
}
