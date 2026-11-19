//! E12 deterministic MIT synthetic fixture, 2026-01-05, abstract corridor, seed 42.
#![allow(dead_code)]
#[path = "e10.rs"]
pub mod base;
use interventions::{
    feasibility::*,
    scenario::{Batch, History, Snapshot},
    transit::Operations,
};
use mobility_graph::{EdgeId, Mode};
use std::collections::BTreeMap;

pub fn inputs(h: &History, batch: &Batch) -> Inputs {
    let s = h.preview(batch).unwrap();
    for_snapshot(h.current(), &s)
}
pub fn for_snapshot(baseline: &Snapshot, s: &Snapshot) -> Inputs {
    let mut segments = vec![];
    for r in s.roads.values() {
        let lanes = [
            (LaneUse::General, r.general_lanes),
            (LaneUse::Bike, r.bike_lanes),
            (LaneUse::Bus, r.bus_lanes),
            (LaneUse::Retired, r.retired_lanes),
        ]
        .into_iter()
        .flat_map(|(kind, n)| {
            (0..n).map(move |_| Lane {
                kind,
                width_mm: 3000,
                minimum_mm: 2500,
            })
        })
        .collect();
        let mut edges = vec![r.road];
        edges.extend(
            s.allocations
                .iter()
                .filter_map(|(e, owner)| (*owner == r.road).then_some(*e)),
        );
        segments.push(Segment {
            id: r.segment.clone(),
            edges,
            road: Some(r.road),
            row: r.segment.clone(),
            lanes,
            other_width_mm: 1000,
            restrictions: restrictions(),
            provenance: base::synthetic(),
        });
    }
    for l in &s.network.data().links {
        if baseline.network.link(l.id) != Some(l)
            && !segments.iter().any(|s| s.edges.contains(&l.id))
        {
            segments.push(Segment {
                id: format!("facility-{}", l.id.0),
                edges: vec![l.id],
                road: None,
                row: format!("facility-{}", l.id.0),
                lanes: vec![],
                other_width_mm: 2000,
                restrictions: restrictions(),
                provenance: base::synthetic(),
            });
        }
    }
    let rows = segments
        .iter()
        .map(|s| RightOfWay {
            id: s.row.clone(),
            available_mm: 20000,
            provenance: base::synthetic(),
        })
        .collect();
    let mut access = vec![];
    for kind in AccessKind::ALL {
        let mode = if kind == AccessKind::Disability || kind == AccessKind::Property {
            Mode::Walk
        } else {
            Mode::Car
        };
        let req = |from, to| base::request(from, to, mode);
        access.push(AccessSample {
            id: format!("{kind:?}"),
            kind,
            outbound: req(1, 2),
            returning: req(2, 1),
            permitted_car_edges: (mode == Mode::Car)
                .then(|| [7, 8, 100, 101, 102, 103].map(EdgeId).into_iter().collect()),
            provenance: base::synthetic(),
        });
    }
    Inputs {
        budget: Budget {
            currency: "USD".into(),
            price_year: 2026,
            minor_units_per_major: 100,
            capital: 1000.0,
            annual: 1000.0,
            years: 10,
            real_discount_rate: 0.0,
        },
        costs: [
            Category::Construction,
            Category::Maintenance,
            Category::Disruption,
        ]
        .into_iter()
        .map(|category| Cost {
            id: format!("{category:?}"),
            category,
            quantity: 2.0,
            unit: "synthetic metre".into(),
            rate: Bounds {
                lower: 8.0,
                central: 10.0,
                upper: 12.0,
            },
            local_factor: 1.5,
            currency: "USD".into(),
            price_year: 2026,
            provenance: base::synthetic(),
        })
        .collect(),
        transit: vec![TransitCost {
            trip: "outbound".into(),
            operations: Operations {
                currency: "USD".into(),
                turnaround_seconds: 0,
                deadhead_seconds_per_run: 0,
                deadhead_metres_per_run: 0,
                operators_per_vehicle: 1,
                vehicle_minor_per_hour: 3600,
                operator_minor_per_hour: 0,
                minor_per_km: 0,
                max_run_instances: 1000,
            },
            coverage_repetitions_per_year: 10.0,
            annualization: "synthetic ten repeats of explicitly covered runs; not a real year"
                .into(),
            local_factor: 1.0,
            price_year: 2026,
            provenance: base::synthetic(),
        }],
        segments,
        rows,
        access,
    }
}
pub fn restrictions() -> BTreeMap<Restriction, Evidence> {
    Restriction::ALL
        .into_iter()
        .map(|k| (k, Evidence::Clear))
        .collect()
}
pub fn proposal(h: &History, id: &str) -> Batch {
    base::batch(h, id, vec![base::bike(h, 7, 3)])
}
