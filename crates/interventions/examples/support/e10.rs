//! MIT repository-authored, arbitrary modeled E10 fixture. No observed inputs.
#[path = "../../../mobility-sim/examples/support/e06.rs"]
pub mod demand_fixture;
pub use demand_fixture::synthetic;
use interventions::scenario::*;
use mobility_graph::network::*;
use mobility_graph::{EdgeId, Mode, NodeId};
use mobility_routing::{Profile, Request};
use mobility_sim::{demand::Demand, simulation as sim};
use std::collections::BTreeSet;

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
            cycling: CycleFacility::Shared,
            safety: None,
            capacity: (mode == Mode::Car).then(|| {
                Box::new(Capacity {
                    vehicles_per_hour: 270,
                    provenance: synthetic(),
                })
            }),
        },
    }
}
pub fn network() -> Network {
    let mut d = demand_fixture::corridor().data().clone();
    d.revision = "e10-base-network".into();
    d.nodes.push(Node {
        id: NodeId(3),
        access: Access::Accessible,
    });
    for l in &mut d.links {
        if let LinkKind::Street {
            mode: Mode::Car,
            capacity,
            ..
        } = &mut l.kind
        {
            *capacity = Some(Box::new(Capacity {
                vehicles_per_hour: 270,
                provenance: synthetic(),
            }));
        }
        if let LinkKind::Street {
            mode: Mode::Bicycle,
            crossing,
            cycling,
            safety,
            ..
        } = &mut l.kind
        {
            *crossing = true;
            *cycling = CycleFacility::Shared;
            *safety = Some(Box::new(Safety {
                stress_level: 4,
                provenance: synthetic(),
            }));
        }
    }
    for (id, from, to) in [(100, 1, 3), (101, 3, 2), (102, 2, 3), (103, 3, 1)] {
        let mut detour = street(id, from, to, Mode::Car, 20);
        if let LinkKind::Street {
            capacity: Some(c), ..
        } = &mut detour.kind
        {
            c.vehicles_per_hour = 2700;
        }
        d.links.push(detour);
        d.links.push(street(id + 100, from, to, Mode::Walk, 50));
    }
    Network::new(d).unwrap()
}
pub fn request(from: u32, to: u32, mode: Mode) -> Request {
    Request {
        from: NodeId(from),
        to: NodeId(to),
        departure_utc: 0,
        deadline_utc: 60,
        initial_mode: mode,
        vehicle: (mode == Mode::Car || is_bicycle(mode)).then_some(mode),
        profile: Profile {
            allowed_modes: if mode == Mode::Bus {
                vec![Mode::Walk, Mode::Bus]
            } else {
                vec![mode]
            },
            step_free: true,
            mobility_aid: mode == Mode::Walk,
            allow_unknown_access: false,
            protected_cycling_only: false,
            maximum_cycling_stress: None,
        },
        max_states: 10_000,
    }
}
pub fn requirements() -> Vec<Requirement> {
    let mut result = vec![];
    for kind in [
        AccessKind::Emergency,
        AccessKind::Freight,
        AccessKind::Regional,
        AccessKind::Walking,
        AccessKind::Transit,
    ] {
        for (from, to) in [(1, 2), (2, 1)] {
            if kind == AccessKind::Transit && from == 2 {
                continue;
            } // explicit fixture has outbound bus only
            let mut req = request(
                from,
                to,
                if kind == AccessKind::Walking || kind == AccessKind::Transit {
                    Mode::Walk
                } else {
                    Mode::Car
                },
            );
            if kind == AccessKind::Transit {
                req.profile.allowed_modes = vec![Mode::Walk, Mode::Bus];
            }
            result.push(Requirement {
                id: format!("{kind:?}-{from}-{to}"),
                kind,
                request: req,
                permitted_car_edges: matches!(kind, AccessKind::Emergency | AccessKind::Freight)
                    .then(|| BTreeSet::from([7, 8, 100, 101, 102, 103].map(EdgeId))),
                provenance: synthetic(),
            });
        }
    }
    result
}
pub fn history() -> History {
    history_for(network(), requirements())
}
pub fn history_for(n: Network, requirements: Vec<Requirement>) -> History {
    let roads = [7, 8, 100, 101, 102, 103]
        .into_iter()
        .map(|id| RoadSpace {
            segment: format!("directional-segment-{id}"),
            road: EdgeId(id),
            total_lanes: 3,
            general_lanes: 3,
            bike_lanes: 0,
            bus_lanes: 0,
            retired_lanes: 0,
            vehicles_per_lane_hour: if id >= 100 { 900 } else { 90 },
            provenance: synthetic(),
        })
        .collect();
    History::new("baseline".into(), n, roads, requirements).unwrap()
}
pub fn batch(h: &History, id: &str, changes: Vec<Change>) -> Batch {
    Batch {
        expected_parent: h.current().id.clone(),
        id: id.into(),
        provenance: synthetic(),
        changes,
    }
}
pub fn approval(crossing: bool, minimum_seconds: u32) -> CrossingAssessment {
    CrossingAssessment {
        crossing,
        permitted: true,
        minimum_seconds,
        provenance: synthetic(),
    }
}
pub fn bike(h: &History, road: u32, target: u32) -> Change {
    let r = h.current().network.link(EdgeId(road)).unwrap();
    let mut link = street(target, r.from.0, r.to.0, Mode::Bicycle, 15);
    link.kind = LinkKind::Street {
        mode: Mode::Bicycle,
        crossing: true,
        blocked: false,
        cycling: CycleFacility::Protected,
        safety: Some(Box::new(Safety {
            stress_level: 1,
            provenance: synthetic(),
        })),
        capacity: None,
    };
    Change::Allocate {
        road: EdgeId(road),
        lanes: 1,
        target: link,
        assessment: Some(approval(true, 8)),
    }
}
pub fn bus(h: &History, seconds: u32) -> Link {
    let mut l = h.current().network.link(EdgeId(11)).unwrap().clone();
    l.seconds = seconds;
    if let LinkKind::Transit { timetable, .. } = &mut l.kind {
        *timetable = Timetable::Scheduled(vec![Departure {
            depart: 20,
            arrive: 20 + seconds,
            run: 0,
        }]);
    }
    l
}
pub fn inputs(s: &Snapshot) -> (Demand, sim::Scenario) {
    let mut d = demand_fixture::sample();
    d.network_revision = s.network.data().revision.clone(); // explicit matched transport-only replay
    d.groups[0]
        .modes
        .retain(|mode, _| *mode == mobility_sim::demand::TravelMode::Car);
    let scenario = sim::Scenario {
        version: sim::VERSION,
        id: s.id.clone(),
        provenance: synthetic(),
        demand_scenario: demand_fixture::scenario(),
        periods: vec![
            sim::Period { start: 0, end: 61 },
            sim::Period {
                start: 61,
                end: 241,
            },
        ],
        roads: interventions::evaluation::road_supply(s, 2).unwrap(),
        transit: vec![sim::TransitSupply {
            edge: EdgeId(11),
            run: mobility_routing::Run {
                trip: "outbound".into(),
                service_date: 20260105,
                ordinal: 0,
                stop_index: 0,
            },
            passengers: 10.0,
        }],
        edits: vec![],
        settings: sim::Settings {
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
    (d, scenario)
}
