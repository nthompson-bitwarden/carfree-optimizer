//! Repository-authored synthetic fixture; arbitrary times and populations, no observations.
use mobility_graph::network::*;
use mobility_graph::{EdgeId, Mode, NodeId};
use mobility_sim::demand::*;
use std::collections::BTreeSet;

pub fn synthetic() -> Provenance {
    Provenance {
        source: "E06 synthetic corridor and demonstration assumptions".into(),
        source_version: "e06-corridor-v1".into(),
        license: "MIT".into(),
        geography: "Abstract west/east zones".into(),
        date: "2026-01-05".into(),
        basis: Basis::Modeled,
        uncertainty: "Arbitrary scenario inputs; not observed or calibrated".into(),
    }
}
pub fn corridor() -> Network {
    network_with_alighting(Access::Accessible)
}
pub fn network_with_alighting(alight_access: Access) -> Network {
    let mut links = Vec::new();
    let mut link = |from, to, seconds, access, kind, distance_m| {
        links.push(Link {
            id: EdgeId(links.len() as u32 + 1),
            from: NodeId(from),
            to: NodeId(to),
            seconds,
            access,
            kind,
            distance_m,
        });
    };
    for (mode, seconds) in [
        (Mode::Walk, 35),
        (Mode::Bicycle, 20),
        (Mode::ElectricBicycle, 15),
        (Mode::Car, 10),
    ] {
        for (from, to) in [(1, 2), (2, 1)] {
            link(
                from,
                to,
                seconds,
                Access::Accessible,
                LinkKind::Street {
                    mode,
                    crossing: false,
                    blocked: false,
                    cycling: CycleFacility::Protected,
                    safety: None,
                    capacity: None,
                },
                Some(100.0),
            );
        }
    }
    for (node, from, to, action, access) in [
        (
            1,
            Mode::Walk,
            Mode::Bus,
            TransferAction::Board,
            Access::Accessible,
        ),
        (
            2,
            Mode::Bus,
            Mode::Walk,
            TransferAction::Alight,
            alight_access,
        ),
    ] {
        link(
            node,
            node,
            2,
            access,
            LinkKind::Transfer {
                from,
                to,
                action,
                windows: vec![(0, 240)],
                bicycle_allowed: false,
            },
            None,
        );
    }
    link(
        1,
        2,
        20,
        Access::Accessible,
        LinkKind::Transit {
            mode: Mode::Bus,
            calendar: 1,
            trip: "outbound".into(),
            stop_index: 0,
            timetable: Timetable::Scheduled(vec![Departure {
                depart: 20,
                arrive: 40,
                run: 0,
            }]),
            bicycle_allowed: false,
        },
        Some(100.0),
    );
    Network::new(NetworkData {
        revision: "corridor".into(),
        provenance: synthetic(),
        timezone: "Etc/UTC".into(),
        tzdb: "2025b".into(),
        coverage_start: 0,
        coverage_end: 240,
        nodes: (1..=2)
            .map(|id| Node {
                id: NodeId(id),
                access: Access::Accessible,
            })
            .collect(),
        links,
        turns: vec![],
        calendars: vec![Calendar {
            id: 1,
            first_date: 20260105,
            last_date: 20260105,
            weekdays: 1,
            added: vec![],
            removed: vec![],
        }],
        days: vec![ServiceDay {
            date: 20260105,
            weekday: 0,
            origin_utc: 0,
        }],
    })
    .unwrap()
}

pub fn sample() -> Demand {
    let load = |cargo_grams| Load {
        cargo_grams,
        companions: 1,
    };
    let adult = TravelerGroup {
        id: "adult".into(),
        tags: BTreeSet::from([GroupTag::OlderAdult]),
        step_free: false,
        mobility_aid: false,
        protected_cycling_only: true,
        maximum_cycling_stress: None,
        modes: TravelMode::ALL
            .into_iter()
            .map(|mode| {
                (
                    mode,
                    load(if mode == TravelMode::Bicycle {
                        1000
                    } else {
                        6000
                    }),
                )
            })
            .collect(),
    };
    let chair = TravelerGroup {
        id: "chair".into(),
        tags: BTreeSet::from([GroupTag::WheelchairUser, GroupTag::Noncyclist]),
        step_free: true,
        mobility_aid: true,
        modes: [
            (TravelMode::Walk, load(7000)),
            (TravelMode::Transit, load(7000)),
        ]
        .into(),
        ..adult.clone()
    };
    let child = TravelerGroup {
        id: "child".into(),
        tags: BTreeSet::from([GroupTag::Child]),
        modes: [(TravelMode::Walk, load(1000))].into(),
        ..adult.clone()
    };
    let legs = [
        (1, 2, 0, 60, Purpose::School, 0, 5),
        (2, 1, 70, 120, Purpose::Shopping, 5000, 0),
    ]
    .into_iter()
    .map(
        |(from, to, departure_utc, deadline_utc, purpose, grams, dwell_seconds)| Leg {
            from: NodeId(from),
            to: NodeId(to),
            departure_utc,
            deadline_utc,
            window: Window {
                start: departure_utc,
                end: departure_utc + 15,
            },
            purpose,
            essential: from == 1,
            expansion_weight: 4.5,
            load: load(grams),
            dwell_seconds,
            costs: TravelMode::ALL
                .into_iter()
                .map(|mode| {
                    (
                        mode,
                        CostEvidence {
                            reliability_seconds: if mode == TravelMode::Transit {
                                4.0
                            } else {
                                0.0
                            },
                            expense_usd: if mode == TravelMode::Transit {
                                2.0
                            } else {
                                0.0
                            },
                            provenance: synthetic(),
                        },
                    )
                })
                .collect(),
        },
    )
    .collect();
    Demand {
        version: VERSION,
        revision: "synthetic-demand-v1".into(),
        network_revision: "corridor".into(),
        provenance: synthetic(),
        population: Population {
            provenance: synthetic(),
            tolerance_persons: 0,
            zones: vec![
                Zone {
                    id: "west".into(),
                    source_persons: 10,
                    strata: vec![
                        Stratum {
                            group: adult.id.clone(),
                            modeled_persons: 7,
                            representatives: 2,
                        },
                        Stratum {
                            group: chair.id.clone(),
                            modeled_persons: 3,
                            representatives: 1,
                        },
                    ],
                },
                Zone {
                    id: "east".into(),
                    source_persons: 6,
                    strata: vec![Stratum {
                        group: child.id.clone(),
                        modeled_persons: 6,
                        representatives: 1,
                    }],
                },
            ],
        },
        groups: vec![adult, chair, child],
        tours: vec![Tour {
            id: "school-and-shopping".into(),
            home_zone: "west".into(),
            group: "adult".into(),
            return_to_origin: true,
            legs,
        }],
    }
}

pub fn scenario() -> Scenario {
    Scenario {
        environment: Environment {
            label: "spring dry daylight".into(),
            season: "spring".into(),
            weather: "dry".into(),
            coverage: Window { start: 0, end: 241 },
            daylight: vec![Window { start: 0, end: 190 }],
            light_modes: TravelMode::ALL.to_vec(),
            dark_modes: vec![TravelMode::Walk, TravelMode::Transit, TravelMode::Car],
            provenance: synthetic(),
        },
        cost: CostModel {
            version: VERSION,
            revision: "illustrative-seconds-v1".into(),
            non_wait_multiplier: 1.0,
            wait_multiplier: 2.0,
            reliability_multiplier: 3.0,
            seconds_per_usd: 10.0,
            provenance: synthetic(),
        },
    }
}
