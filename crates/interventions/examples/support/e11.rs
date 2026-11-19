//! MIT repository-authored E11 abstract fixture, 2026-01-05; all values modeled.
use interventions::{scenario::*, transit::*};
use mobility_graph::{network::*, EdgeId, Mode, NodeId};
use mobility_routing::{Profile, Request};
use std::collections::BTreeMap;
pub fn provenance() -> Provenance {
    Provenance {
        source: "repository-authored synthetic E11 fixture".into(),
        source_version: "1".into(),
        license: "MIT".into(),
        geography: "abstract corridor, no real city".into(),
        date: "2026-01-05".into(),
        basis: Basis::Modeled,
        uncertainty: "arbitrary assumptions, not calibrated observations".into(),
    }
}
pub fn street(id: u32, from: u32, to: u32, mode: Mode, seconds: u32) -> Link {
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
            cycling: CycleFacility::Protected,
            safety: is_bicycle(mode).then(|| {
                Box::new(Safety {
                    stress_level: 1,
                    provenance: provenance(),
                })
            }),
            capacity: (mode == Mode::Car).then(|| {
                Box::new(Capacity {
                    vehicles_per_hour: 300,
                    provenance: provenance(),
                })
            }),
        },
    }
}
pub fn calendar(id: u32) -> Calendar {
    Calendar {
        id,
        first_date: 20260105,
        last_date: 20260105,
        weekdays: 127,
        added: vec![],
        removed: vec![],
    }
}
pub fn network() -> Network {
    Network::new(NetworkData {
        revision: "e11-base".into(),
        provenance: provenance(),
        timezone: "Etc/UTC".into(),
        tzdb: "2025b".into(),
        coverage_start: 0,
        coverage_end: 10000,
        nodes: (0..=6)
            .map(|id| Node {
                id: NodeId(id),
                access: Access::Accessible,
            })
            .collect(),
        links: vec![
            street(1, 0, 1, Mode::Walk, 10),
            street(2, 2, 3, Mode::Walk, 10),
            street(3, 1, 2, Mode::Walk, 1000),
            street(4, 1, 2, Mode::Car, 100),
        ],
        turns: vec![],
        calendars: vec![],
        days: vec![ServiceDay {
            date: 20260105,
            weekday: 0,
            origin_utc: 0,
        }],
    })
    .unwrap()
}
pub fn history() -> History {
    History::new(
        "base".into(),
        network(),
        vec![RoadSpace {
            segment: "outbound-1-2".into(),
            road: EdgeId(4),
            total_lanes: 3,
            general_lanes: 3,
            bike_lanes: 0,
            bus_lanes: 0,
            retired_lanes: 0,
            vehicles_per_lane_hour: 100,
            provenance: provenance(),
        }],
        vec![],
    )
    .unwrap()
}
pub fn batch(h: &History, id: &str, changes: Vec<Change>) -> Batch {
    Batch {
        expected_parent: h.current().id.clone(),
        id: id.into(),
        provenance: provenance(),
        changes,
    }
}
pub fn operations() -> Operations {
    Operations {
        currency: "USD".into(),
        turnaround_seconds: 20,
        deadhead_seconds_per_run: 10,
        deadhead_metres_per_run: 100,
        operators_per_vehicle: 2,
        vehicle_minor_per_hour: 3600,
        operator_minor_per_hour: 1800,
        minor_per_km: 100,
        max_run_instances: 10000,
    }
}
pub fn ride(
    id: u32,
    from: u32,
    to: u32,
    mode: Mode,
    trip: &str,
    depart: u32,
    seconds: u32,
) -> Link {
    Link {
        id: EdgeId(id),
        from: NodeId(from),
        to: NodeId(to),
        distance_m: Some(1000.0),
        seconds,
        access: Access::Accessible,
        kind: LinkKind::Transit {
            mode,
            calendar: 1,
            trip: trip.into(),
            stop_index: 0,
            timetable: Timetable::Scheduled(vec![Departure {
                depart,
                arrive: depart + seconds,
                run: 0,
            }]),
            bicycle_allowed: false,
        },
    }
}
pub fn service() -> Service {
    let mut l = ride(20, 1, 2, Mode::Bus, "bus", 100, 100);
    if let LinkKind::Transit { timetable, .. } = &mut l.kind {
        *timetable = Timetable::Frequency {
            first: 100,
            end: 301,
            headway: 100,
            first_run: 0,
        };
    }
    Service {
        trip: "bus".into(),
        calendar: calendar(1),
        rides: vec![l],
        operations: operations(),
    }
}
pub fn transfer(id: u32, node: u32, from: Mode, to: Mode, action: TransferAction) -> Link {
    Link {
        id: EdgeId(id),
        from: NodeId(node),
        to: NodeId(node),
        distance_m: None,
        seconds: 1,
        access: Access::Accessible,
        kind: LinkKind::Transfer {
            from,
            to,
            action,
            windows: vec![(0, 10000)],
            bicycle_allowed: matches!(action, TransferAction::Park | TransferAction::Retrieve),
        },
    }
}
pub fn facilities(mode: Mode, first: u32, last: u32, id: u32) -> Station {
    Station {
        nodes: vec![],
        links: vec![
            transfer(id, first, Mode::Walk, mode, TransferAction::Board),
            transfer(id + 1, last, mode, Mode::Walk, TransferAction::Alight),
        ],
        assessments: BTreeMap::new(),
    }
}
pub fn install(h: &mut History) {
    h.apply(&batch(
        h,
        "bus",
        vec![
            Change::Station(facilities(Mode::Bus, 1, 2, 30)),
            Change::Transit(service()),
        ],
    ))
    .unwrap();
}
pub fn request() -> Request {
    Request {
        from: NodeId(0),
        to: NodeId(3),
        departure_utc: 0,
        deadline_utc: 500,
        initial_mode: Mode::Walk,
        vehicle: None,
        profile: Profile {
            allowed_modes: vec![Mode::Walk, Mode::Bus, Mode::Rail, Mode::LightRail],
            step_free: true,
            mobility_aid: true,
            allow_unknown_access: false,
            protected_cycling_only: false,
            maximum_cycling_stress: None,
        },
        max_states: 10000,
    }
}
pub fn fares(trips: &[&str], amount: u32) -> BTreeMap<String, Fare> {
    trips
        .iter()
        .map(|id| {
            (
                (*id).into(),
                Fare {
                    currency: "USD".into(),
                    boarding_minor: amount,
                    transfer: Some(TransferFare {
                        group: "network".into(),
                        window_seconds: 1000,
                        minor: 0,
                    }),
                    provenance: provenance(),
                },
            )
        })
        .collect()
}
pub fn assessment() -> CrossingAssessment {
    CrossingAssessment {
        crossing: false,
        permitted: true,
        minimum_seconds: 1,
        provenance: provenance(),
    }
}
