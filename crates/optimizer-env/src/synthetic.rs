//! E14 synthetic test and demonstration fixtures. All quantities are arbitrary,
//! MIT-licensed modeled inputs in an abstract network, with no calibration.

use crate::{
    catalog::{Catalog, Project as Entry},
    reference::{Config, DoorSample, Environment, Project},
    reward,
    wire::BatchEnvironment,
};
use accessibility::scoring::{Context, Horizon};
use interventions::{
    feasibility::{
        self, AccessSample, Bounds, Budget, Category, Cost, Inputs, RightOfWay, Segment,
    },
    scenario::{self, Change, CrossingAssessment, History, Requirement, RoadSpace},
    transit::Operations,
};
use mobility_graph::{network::*, EdgeId, Mode, NodeId};
use mobility_routing::{Profile, Request, Run};
use mobility_sim::{demand as d, simulation as sim};
use std::collections::BTreeSet;

pub const TOPOLOGIES: usize = 64;
pub const ACTIONS: usize = 4;
pub const WIRE_FEATURES: usize =
    crate::observation::GLOBAL_FEATURES.len() + ACTIONS * crate::observation::ACTION_FEATURES.len();

pub fn environment(topology: usize) -> Result<impl BatchEnvironment, String> {
    Environment::new(configuration(topology)?).map_err(|e| e.to_string())
}

fn provenance() -> Provenance {
    Provenance {
        source: "E14 repository-authored synthetic network, demand and costs".into(),
        source_version: "e14-synthetic-v1".into(),
        license: "MIT".into(),
        geography: "Abstract connected topology family".into(),
        date: "2026-01-05".into(),
        basis: Basis::Modeled,
        uncertainty: "Arbitrary deterministic assumptions, no sampling or calibration".into(),
    }
}

fn street(id: u32, from: u32, to: u32, mode: Mode, seconds: u32) -> Link {
    Link {
        id: EdgeId(id),
        from: NodeId(from),
        to: NodeId(to),
        seconds,
        distance_m: Some(100.0),
        access: Access::Accessible,
        kind: LinkKind::Street {
            mode,
            crossing: false,
            blocked: false,
            cycling: CycleFacility::Unspecified,
            safety: None,
            capacity: (mode == Mode::Car).then(|| {
                Box::new(Capacity {
                    vehicles_per_hour: 900,
                    provenance: provenance(),
                })
            }),
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
            windows: vec![(0, 180)],
            bicycle_allowed: false,
        },
    }
}

fn network(topology: u32) -> Result<Network, mobility_graph::network::Error> {
    let mut links = vec![
        street(1, 1, 2, Mode::Car, 12),
        street(2, 2, 1, Mode::Car, 12),
        street(14, 1, 3, Mode::Car, 20),
        street(15, 3, 2, Mode::Car, 20),
        street(3, 1, 3, Mode::Walk, 18 + topology % 9),
        street(4, 3, 1, Mode::Walk, 24),
        street(5, 3, 2, Mode::Walk, 27),
        street(6, 2, 3, Mode::Walk, 24),
        street(7, 2, 4, Mode::Walk, 8),
        street(8, 3, 4, Mode::Walk, 24),
        street(9, 4, 3, Mode::Walk, 24),
        transfer(10, 2, Mode::Car, Mode::Walk, TransferAction::Park),
        transfer(11, 1, Mode::Walk, Mode::Bus, TransferAction::Board),
        transfer(12, 3, Mode::Bus, Mode::Walk, TransferAction::Alight),
        Link {
            id: EdgeId(13),
            from: NodeId(1),
            to: NodeId(3),
            distance_m: Some(250.0),
            seconds: 8,
            access: Access::Accessible,
            kind: LinkKind::Transit {
                mode: Mode::Bus,
                calendar: 1,
                trip: "shuttle".into(),
                stop_index: 0,
                timetable: Timetable::Scheduled(vec![Departure {
                    depart: 10,
                    arrive: 18,
                    run: 0,
                }]),
                bicycle_allowed: false,
            },
        },
    ];
    // Accessible side routes enter different decision-relevant paths. At the
    // approach junction they can bring the parking trip within its deadline
    // after a project. At the destination they change the opening door parity.
    let bypass = match topology {
        4 => Some((1, 3, 8, 9)),
        5 => Some((3, 4, 5, 11)),
        7 => Some((3, 4, 6, 7)),
        8 => Some((1, 3, 10, 9)),
        _ => None,
    };
    if let Some((from, to, first, last)) = bypass {
        for (id, from, to, seconds) in [
            (200, from, 5, first),
            (201, 5, to, last),
            (202, to, 5, last),
            (203, 5, from, first),
        ] {
            links.push(street(id, from, to, Mode::Walk, seconds));
        }
    }
    // Alternating spine and branching families, all attached to the core.
    for node in 5..=5 + topology {
        let parent = if topology.is_multiple_of(2) && node > 5 {
            node - 1
        } else {
            1 + node % 3
        };
        let id = 1000 + node * 3;
        links.push(street(id, parent, node, Mode::Walk, 55));
        links.push(street(id + 1, node, parent, Mode::Walk, 55));
        if topology.is_multiple_of(3) && node > 5 {
            links.push(street(id + 2, node - 1, node, Mode::Walk, 50));
        }
    }
    Network::new(NetworkData {
        revision: format!("e14-synthetic-topology-{topology}"),
        provenance: provenance(),
        timezone: "Etc/UTC".into(),
        tzdb: "2025b".into(),
        coverage_start: 0,
        coverage_end: 180,
        nodes: (1..=5 + topology)
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
}

fn request(from: u32, to: u32, mode: Mode, deadline: i64) -> Request {
    Request {
        from: NodeId(from),
        to: NodeId(to),
        departure_utc: 0,
        deadline_utc: deadline,
        initial_mode: mode,
        vehicle: (mode == Mode::Car).then_some(Mode::Car),
        max_states: 10_000,
        profile: Profile {
            allowed_modes: vec![mode],
            step_free: true,
            mobility_aid: true,
            allow_unknown_access: false,
            protected_cycling_only: false,
            maximum_cycling_stress: None,
        },
    }
}

fn permissions() -> BTreeSet<EdgeId> {
    [1, 2, 14, 15].into_iter().map(EdgeId).collect()
}

fn requirements() -> Vec<Requirement> {
    use scenario::AccessKind;
    let mut output = Vec::new();
    for kind in [
        AccessKind::Emergency,
        AccessKind::Freight,
        AccessKind::Regional,
        AccessKind::Walking,
    ] {
        let car = kind != AccessKind::Walking;
        let mode = if car { Mode::Car } else { Mode::Walk };
        for (from, to) in [(1, 2), (2, 1)] {
            output.push(Requirement {
                id: format!("{kind:?}-{from}-{to}"),
                kind,
                request: request(from, to, mode, 130),
                permitted_car_edges: car.then(permissions),
                provenance: provenance(),
            });
        }
    }
    let mut bus = request(1, 3, Mode::Walk, 130);
    bus.profile.allowed_modes.push(Mode::Bus);
    output.push(Requirement {
        id: "outbound-protected-transit".into(),
        kind: AccessKind::Transit,
        request: bus,
        permitted_car_edges: None,
        provenance: provenance(),
    });
    output
}

fn opening() -> Inputs {
    use feasibility::AccessKind;
    let costs = [
        (Category::Construction, "opening-construction", 0.0),
        (Category::Disruption, "opening-disruption", 0.0),
        (Category::Maintenance, "opening-maintenance", 3.0),
    ]
    .into_iter()
    .map(|(category, id, rate)| cost(category, id, rate))
    .collect();
    let segments = [1, 2, 14, 15]
        .into_iter()
        .map(|edge| Segment {
            id: format!("road-{edge}"),
            edges: vec![EdgeId(edge)],
            road: Some(EdgeId(edge)),
            row: format!("road-{edge}"),
            lanes: vec![feasibility::Lane {
                kind: feasibility::LaneUse::General,
                width_mm: 3500,
                minimum_mm: 3000,
            }],
            other_width_mm: 1000,
            restrictions: restrictions(),
            provenance: provenance(),
        })
        .collect();
    Inputs {
        budget: Budget {
            currency: "USD".into(),
            price_year: 2026,
            minor_units_per_major: 100,
            capital: 100.0,
            annual: 20.0,
            years: 10,
            real_discount_rate: 0.0,
        },
        costs,
        transit: vec![feasibility::TransitCost {
            trip: "shuttle".into(),
            operations: Operations {
                currency: "USD".into(),
                turnaround_seconds: 0,
                deadhead_seconds_per_run: 0,
                deadhead_metres_per_run: 0,
                operators_per_vehicle: 1,
                vehicle_minor_per_hour: 3600,
                operator_minor_per_hour: 0,
                minor_per_km: 0,
                max_run_instances: 20,
            },
            coverage_repetitions_per_year: 25.0,
            annualization: "synthetic 25 annual runs".into(),
            local_factor: 1.0,
            price_year: 2026,
            provenance: provenance(),
        }],
        segments,
        rows: [1, 2, 14, 15]
            .into_iter()
            .map(|edge| row(&format!("road-{edge}"), 5000))
            .collect(),
        access: AccessKind::ALL
            .into_iter()
            .map(|kind| {
                let car = !matches!(kind, AccessKind::Disability | AccessKind::Property);
                let mode = if car { Mode::Car } else { Mode::Walk };
                AccessSample {
                    id: format!("protected-{kind:?}"),
                    kind,
                    outbound: request(1, 2, mode, 130),
                    returning: request(2, 1, mode, 130),
                    permitted_car_edges: car.then(permissions),
                    provenance: provenance(),
                }
            })
            .collect(),
    }
}

fn restrictions() -> std::collections::BTreeMap<feasibility::Restriction, feasibility::Evidence> {
    feasibility::Restriction::ALL
        .into_iter()
        .map(|r| (r, feasibility::Evidence::Clear))
        .collect()
}

fn row(id: &str, available_mm: u32) -> RightOfWay {
    RightOfWay {
        id: id.into(),
        available_mm,
        provenance: provenance(),
    }
}

fn cost(category: Category, id: &str, amount: f64) -> Cost {
    Cost {
        id: id.into(),
        category,
        quantity: 1.0,
        unit: "synthetic facility".into(),
        rate: Bounds {
            lower: amount,
            central: amount,
            upper: amount,
        },
        local_factor: 1.0,
        currency: "USD".into(),
        price_year: 2026,
        provenance: provenance(),
    }
}

fn catalog() -> Result<Catalog<Project>, crate::catalog::Error> {
    let projects = [
        (
            "parking-walk",
            street(100, 1, 2, Mode::Walk, 20),
            60.0,
            8.0,
            "shared-corridor",
        ),
        (
            "direct-door",
            street(101, 1, 4, Mode::Walk, 11),
            40.0,
            7.0,
            "door-corridor",
        ),
        (
            "junction-walk",
            street(102, 3, 2, Mode::Walk, 5),
            45.0,
            7.0,
            "shared-corridor",
        ),
    ];
    Catalog::new(
        "e14-synthetic-actions-v1".into(),
        projects
            .into_iter()
            .map(|(id, link, capital, annual, row_id)| {
                let segment = Segment {
                    id: id.into(),
                    edges: vec![link.id],
                    road: None,
                    row: row_id.into(),
                    lanes: vec![],
                    other_width_mm: 2500,
                    restrictions: restrictions(),
                    provenance: provenance(),
                };
                let anchors = [Some(link.from), Some(link.to)];
                Entry {
                    id: id.into(),
                    description: format!("Build synthetic {id} pedestrian link"),
                    requires: BTreeSet::new(),
                    conflicts: BTreeSet::new(),
                    payload: Project {
                        changes: vec![Change::Pedestrian {
                            link,
                            assessment: CrossingAssessment {
                                crossing: false,
                                permitted: true,
                                minimum_seconds: 5,
                                provenance: provenance(),
                            },
                        }],
                        provenance: provenance(),
                        costs: vec![
                            cost(
                                Category::Construction,
                                &format!("{id}-build"),
                                capital - 2.0,
                            ),
                            cost(Category::Disruption, &format!("{id}-disruption"), 2.0),
                            cost(Category::Maintenance, &format!("{id}-maintenance"), annual),
                        ],
                        segments: vec![segment],
                        rows: vec![row(row_id, 4000)],
                        transit: None,
                        anchors,
                    },
                }
            })
            .collect(),
    )
}

fn demand(revision: &str) -> d::Demand {
    use d::TravelMode::{Car, Transit, Walk};
    let load = d::Load {
        cargo_grams: 0,
        companions: 0,
    };
    let groups = [
        ("adult", false, [Walk, Car]),
        ("mobility-aid", true, [Walk, Transit]),
    ];
    let travelers = groups
        .iter()
        .map(|&(id, protected, modes)| d::TravelerGroup {
            id: id.into(),
            tags: protected
                .then_some(d::GroupTag::Noncyclist)
                .into_iter()
                .collect(),
            step_free: protected,
            mobility_aid: protected,
            protected_cycling_only: false,
            maximum_cycling_stress: None,
            modes: modes.into_iter().map(|m| (m, load)).collect(),
        })
        .collect();
    let tours = groups
        .iter()
        .map(|&(id, protected, modes)| d::Tour {
            id: format!("{id}-synthetic-tour"),
            home_zone: "abstract-origin".into(),
            group: id.into(),
            return_to_origin: false,
            legs: vec![d::Leg {
                from: NodeId(1),
                to: NodeId(if protected { 4 } else { 2 }),
                window: d::Window { start: 0, end: 10 },
                departure_utc: 0,
                deadline_utc: if protected { 120 } else { 25 },
                purpose: d::Purpose::Healthcare,
                essential: true,
                expansion_weight: if protected { 2.0 } else { 4.0 },
                load,
                dwell_seconds: 0,
                costs: modes
                    .into_iter()
                    .map(|m| {
                        (
                            m,
                            d::CostEvidence {
                                reliability_seconds: 0.0,
                                expense_usd: if m == Car { 0.01 } else { 0.0 },
                                provenance: provenance(),
                            },
                        )
                    })
                    .collect(),
            }],
        })
        .collect();
    d::Demand {
        version: d::VERSION,
        revision: "e14-synthetic-demand-v1".into(),
        network_revision: revision.into(),
        provenance: provenance(),
        population: d::Population {
            provenance: provenance(),
            tolerance_persons: 0,
            zones: vec![d::Zone {
                id: "abstract-origin".into(),
                source_persons: 6,
                strata: groups
                    .iter()
                    .map(|&(id, protected, _)| d::Stratum {
                        group: id.into(),
                        modeled_persons: if protected { 2 } else { 4 },
                        representatives: 1,
                    })
                    .collect(),
            }],
        },
        groups: travelers,
        tours,
    }
}

/// Rich fixture configuration for Rust diagnostics and graph experiments.
pub fn configuration(topology: usize) -> Result<Config, String> {
    if topology >= TOPOLOGIES {
        return Err(format!("synthetic topology must be in 0..{TOPOLOGIES}"));
    }
    let topology = topology as u32;
    let network = network(topology).map_err(|e| e.to_string())?;
    let opening = opening();
    let roads = [1, 2, 14, 15]
        .into_iter()
        .map(|edge| RoadSpace {
            segment: format!("road-{edge}"),
            road: EdgeId(edge),
            total_lanes: 1,
            general_lanes: 1,
            bike_lanes: 0,
            bus_lanes: 0,
            retired_lanes: 0,
            vehicles_per_lane_hour: 900,
            provenance: provenance(),
        })
        .collect();
    let demand = demand(&network.data().revision);
    let context = Context {
        scenario: "opening".into(),
        graph_revision: network.data().revision.clone(),
        graph_snapshot: network.encode(),
        demand_revision: demand.revision.clone(),
        destination_revision: "synthetic-door".into(),
        service_revision: "synthetic-shuttle-v1".into(),
        land_use_revision: None,
        geography_revision: "abstract-graph".into(),
        period: "synthetic 180 seconds".into(),
        horizon: Horizon::ShortRun,
        behavior: "fixed demand and declared modeled mode costs".into(),
        source: demand.provenance.source.clone(),
        geography: demand.provenance.geography.clone(),
        date: demand.provenance.date.clone(),
        limitations: vec![demand.provenance.uncertainty.clone()],
    };
    let history = History::new(context.scenario.clone(), network, roads, requirements())
        .map_err(|e| e.to_string())?;
    let window = d::Window { start: 0, end: 180 };
    let simulation = sim::Scenario {
        version: sim::VERSION,
        id: context.scenario.clone(),
        provenance: provenance(),
        demand_scenario: d::Scenario {
            environment: d::Environment {
                label: "synthetic dry daylight".into(),
                weather: "dry".into(),
                season: "synthetic".into(),
                coverage: window,
                daylight: vec![window],
                light_modes: d::TravelMode::ALL.to_vec(),
                dark_modes: vec![],
                provenance: provenance(),
            },
            cost: d::CostModel {
                version: d::VERSION,
                revision: "e14-synthetic-cost-v1".into(),
                non_wait_multiplier: 1.0,
                wait_multiplier: 1.0,
                reliability_multiplier: 1.0,
                seconds_per_usd: 10.0,
                provenance: provenance(),
            },
        },
        periods: vec![sim::Period { start: 0, end: 181 }],
        roads: vec![],
        transit: vec![sim::TransitSupply {
            edge: EdgeId(13),
            run: Run {
                trip: "shuttle".into(),
                service_date: 20260105,
                ordinal: 0,
                stop_index: 0,
            },
            passengers: 20.0,
        }],
        edits: vec![],
        settings: sim::Settings {
            seed: 0,
            max_iterations: 8,
            max_states_per_query: 10_000,
            tolerance: 1e-9,
            alpha: 0.0,
            beta: 1.0,
            vehicles_per_trip: 1.0,
            demand_multiplier: 1.0,
            preference_seconds: 0.0,
        },
    };
    let mut car = request(1, 4, Mode::Car, 130);
    car.profile.allowed_modes.push(Mode::Walk);
    let doors = vec![DoorSample {
        destination: "synthetic-clinic-door".into(),
        car_free: request(1, 4, Mode::Walk, 130),
        car,
        weight: 1.0,
    }];
    let reward = reward::Config {
        car_required: reward::Term {
            weight: 1.0,
            scale: 1.0,
        },
        car_free_feasible: reward::Term {
            weight: 0.5,
            scale: 1.0,
        },
        fixed_door_parity: reward::Term {
            weight: 1.0,
            scale: 1.0,
        },
        max_unserved_increase: 0.0,
        max_denied_increase: 0.0,
        max_essential_unserved_increase: 0.0,
        max_noncycling_unserved_increase: 0.0,
        max_step_free_unserved_increase: 0.0,
        max_no_mode_increase: 0.0,
        max_protected_car_free_loss: 0.0,
        require_convergence: true,
    };
    Ok(Config {
        history,
        opening,
        catalog: catalog().map_err(|e| e.to_string())?,
        demand,
        simulation,
        context,
        doors,
        reward,
        horizon: 2,
    })
}
