//! E14-S03: versioned policy inputs, current snapshot and declared project inputs.
//! Graph indices are local positions. Source identifiers and trip rows are
//! excluded. The vector pools network features independently of node ordering.

use crate::{
    catalog::Catalog,
    portfolio::project_costs,
    reference::{BudgetAudit, Error, Project},
    reward::Metrics,
};
use interventions::scenario::Change;
use mobility_graph::{
    network::{Access, LinkKind, Network},
    Mode,
};
use std::collections::BTreeMap;

pub const VERSION: u32 = 1;
pub const GLOBAL_FEATURES: [&str; 24] = [
    "accepted_project_count",
    "remaining_project_steps",
    "node_count",
    "edge_count",
    "walk_edge_count",
    "bike_and_ebike_edge_count",
    "transit_edge_count",
    "open_car_edge_count",
    "transfer_edge_count",
    "step_free_node_count",
    "mean_total_degree",
    "max_in_degree",
    "max_out_degree",
    "trip_weight",
    "essential_trip_weight",
    "noncycling_trip_weight",
    "step_free_trip_weight",
    "remaining_capital_major",
    "remaining_annual_major_per_year",
    "annual_commitment_upper_major_per_year",
    "reference_car_free_share",
    "fixed_door_parity",
    "unserved_essential_trip_weight",
    "denied_trip_weight",
];
pub const ACTION_FEATURES: [&str; 10] = [
    "is_stop",
    "capital_upper_major",
    "added_maintenance_upper_major_per_year",
    "change_count",
    "proposed_walk_link_mean_seconds",
    "proposed_walk_link_seconds_present",
    "source_anchor_out_degree",
    "target_anchor_in_degree",
    "source_anchor_present",
    "target_anchor_present",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub access: Access,
    pub in_degree: usize,
    pub out_degree: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EdgeKind {
    Street { mode: Mode, blocked: bool },
    Transit { mode: Mode },
    Transfer { from: Mode, to: Mode },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub kind: EdgeKind,
    pub access: Access,
    pub seconds: u32,
    /// Absence represents a transfer without a physical length.
    pub distance_metres: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub features: [f64; ACTION_FEATURES.len()],
    /// Absent at stop or where the public anchor is not in the current graph.
    pub anchors: [Option<usize>; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub version: u32,
    pub global: [f64; GLOBAL_FEATURES.len()],
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Catalog order followed by stop. A separate mask governs eligibility.
    pub actions: Vec<Action>,
}

impl Observation {
    /// Fixed width within a catalog: 24 global scalars followed by ten scalars
    /// for each action, including stop. Graph order and size do not pad the vector.
    pub fn flatten(&self) -> Vec<f64> {
        let mut result =
            Vec::with_capacity(GLOBAL_FEATURES.len() + ACTION_FEATURES.len() * self.actions.len());
        result.extend(self.global);
        result.extend(self.actions.iter().flat_map(|a| a.features));
        result
    }
}

pub(crate) fn encode(
    network: &Network,
    catalog: &Catalog<Project>,
    budget: &BudgetAudit,
    metrics: &Metrics,
    step: usize,
    horizon: usize,
) -> Result<Observation, Error> {
    let data = network.data();
    let indices: BTreeMap<_, _> = data
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, i))
        .collect();
    let mut nodes: Vec<_> = data
        .nodes
        .iter()
        .map(|n| Node {
            access: n.access,
            in_degree: 0,
            out_degree: 0,
        })
        .collect();
    let mut counts = [0usize; 5];
    let mut edges = Vec::with_capacity(data.links.len());
    for link in &data.links {
        // Network construction validates that both endpoint nodes exist.
        let from = indices[&link.from];
        let to = indices[&link.to];
        nodes[from].out_degree += 1;
        nodes[to].in_degree += 1;
        let kind = match link.kind {
            LinkKind::Street { mode, blocked, .. } => {
                match mode {
                    Mode::Walk => counts[0] += 1,
                    Mode::Bicycle | Mode::ElectricBicycle => counts[1] += 1,
                    Mode::Car if !blocked => counts[3] += 1,
                    _ => {}
                }
                EdgeKind::Street { mode, blocked }
            }
            LinkKind::Transit { mode, .. } => {
                counts[2] += 1;
                EdgeKind::Transit { mode }
            }
            LinkKind::Transfer { from, to, .. } => {
                counts[4] += 1;
                EdgeKind::Transfer { from, to }
            }
        };
        edges.push(Edge {
            from,
            to,
            kind,
            access: link.access,
            seconds: link.seconds,
            distance_metres: link.distance_m,
        });
    }
    let max_in = nodes.iter().map(|n| n.in_degree).max().unwrap_or(0);
    let max_out = nodes.iter().map(|n| n.out_degree).max().unwrap_or(0);
    let global = [
        step as f64,
        (horizon - step) as f64,
        nodes.len() as f64,
        edges.len() as f64,
        counts[0] as f64,
        counts[1] as f64,
        counts[2] as f64,
        counts[3] as f64,
        counts[4] as f64,
        nodes
            .iter()
            .filter(|n| n.access == Access::Accessible)
            .count() as f64,
        (2.0 * edges.len() as f64) / nodes.len().max(1) as f64,
        max_in as f64,
        max_out as f64,
        metrics.all.total,
        metrics.essential.total,
        metrics.noncycling.total,
        metrics.step_free.total,
        budget.limit.capital - budget.capital.upper,
        budget.limit.annual - budget.annual.upper,
        budget.annual.upper,
        metrics.all.car_free / metrics.all.total,
        metrics.fixed_door_parity,
        metrics.essential.unserved,
        metrics.denied,
    ];
    let mut actions = Vec::with_capacity(catalog.projects().len() + 1);
    for p in catalog.projects() {
        let project = &p.payload;
        let (capital, maintenance) = project_costs(&project.costs, &budget.limit)?;
        let anchors = project
            .anchors
            .map(|n| n.and_then(|id| indices.get(&id).copied()));
        let (walk_seconds, walk_links) = project
            .changes
            .iter()
            .filter_map(|change| match change {
                Change::Pedestrian { link, .. } => Some(link.seconds),
                _ => None,
            })
            .fold((0.0, 0usize), |(sum, n), seconds| {
                (sum + f64::from(seconds), n + 1)
            });
        let features = [
            0.0,
            capital.upper,
            maintenance.upper,
            project.changes.len() as f64,
            walk_seconds / walk_links.max(1) as f64,
            f64::from(walk_links > 0),
            anchors[0].map_or(0.0, |i| nodes[i].out_degree as f64),
            anchors[1].map_or(0.0, |i| nodes[i].in_degree as f64),
            f64::from(anchors[0].is_some()),
            f64::from(anchors[1].is_some()),
        ];
        actions.push(Action { features, anchors });
    }
    let mut stop = [0.0; ACTION_FEATURES.len()];
    stop[0] = 1.0;
    actions.push(Action {
        features: stop,
        anchors: [None, None],
    });
    Ok(Observation {
        version: VERSION,
        global,
        nodes,
        edges,
        actions,
    })
}
