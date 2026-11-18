//! E09 repository-authored synthetic targets, never observed measurements.
#[path = "e06.rs"]
mod base;
use mobility_graph::network::Network;
use mobility_graph::EdgeId;
use mobility_routing::Run;
use mobility_sim::calibration::*;
use mobility_sim::demand::{Demand, Purpose, TravelMode};
use mobility_sim::simulation::{Parameters, Period, RoadSupply, Scenario, Settings, TransitSupply};
use std::collections::BTreeSet;

pub fn inputs() -> (Network, Demand, Scenario, Request) {
    let n = base::corridor();
    let mut d = base::sample();
    d.groups[0].modes.retain(|m, _| *m == TravelMode::Car);
    d.tours[0].legs[0].purpose = Purpose::Commute;
    let s = Scenario {
        version: mobility_sim::simulation::VERSION,
        id: "e09-synthetic-baseline".into(),
        provenance: base::synthetic(),
        demand_scenario: base::scenario(),
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
                vehicles_per_hour: vec![36000.0; 2],
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
            alpha: 0.0,
            beta: 1.0,
            vehicles_per_trip: 1.0,
            demand_multiplier: 1.0,
            preference_seconds: 0.0,
        },
    };
    let r = Request {
        project_id: "e09-synthetic-project".into(),
        observations: vec![
            road("fit", Split::Fit, 7, 0, 61, 4.5),
            road("holdout", Split::Holdout, 8, 61, 241, 4.5),
        ],
        candidates: vec![
            Parameters {
                label: "base".into(),
                alpha: 0.0,
                demand_multiplier: 1.0,
                preference_seconds: 0.0,
            },
            Parameters {
                label: "double-demand".into(),
                alpha: 0.0,
                demand_multiplier: 2.0,
                preference_seconds: 0.0,
            },
        ],
        seeds: vec![7, 42],
        required_metrics: BTreeSet::from([Metric::RoadVehicles]),
    };
    (n, d, s, r)
}
pub fn observation(
    id: &str,
    split: Split,
    target: Target,
    start: i64,
    end: i64,
    value: f64,
) -> Observation {
    Observation {
        id: id.into(),
        record: format!("synthetic-{id}"),
        source: base::synthetic(),
        split,
        target,
        crosswalk: base::synthetic(),
        network_revision: "corridor".into(),
        demand_revision: "synthetic-demand-v1".into(),
        start,
        end,
        value: Some(value),
        missing_reason: None,
        scale: 1.0,
    }
}
pub fn road(id: &str, split: Split, edge: u32, start: i64, end: i64, value: f64) -> Observation {
    observation(
        id,
        split,
        Target::Road {
            station: format!("cutline-{edge}"),
            direction: "directed-link".into(),
            edges: vec![EdgeId(edge)],
        },
        start,
        end,
        value,
    )
}
