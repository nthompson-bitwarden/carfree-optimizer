//! Profile/time-aware E10 corridor preview, in addition to structural inspection.
use crate::scenario::{preview_corridor, CorridorPreview};
use mobility_graph::{
    network::{is_transit, Access, LinkKind, Network},
    EdgeId, Mode,
};
use mobility_routing::{earliest_arrival, earliest_transit_arrival, Outcome, Request};
use std::collections::BTreeSet;

/// Check the exact ordered segment selection for a real caller profile and query
/// window. Transfer edges remain available; all unselected movement is excluded.
/// This catches steps, blocked nodes, service gaps and illegal transit continuity.
pub fn preview(
    network: &Network,
    edges: &[EdgeId],
    intended_mode: Mode,
    request: &Request,
) -> CorridorPreview {
    let mut result = preview_corridor(network, edges, intended_mode);
    if !result.issues.is_empty() {
        return result;
    }
    if network
        .link(edges[0])
        .is_none_or(|l| l.from != request.from)
        || edges
            .last()
            .and_then(|e| network.link(*e))
            .is_none_or(|l| l.to != request.to)
    {
        result
            .issues
            .push("selection endpoints differ from query".into());
        return result;
    }
    let selected: BTreeSet<_> = edges.iter().copied().collect();
    let mut d = network.data().clone();
    for l in &mut d.links {
        if !selected.contains(&l.id) && !matches!(l.kind, LinkKind::Transfer { .. }) {
            l.access = Access::Impassable;
        }
    }
    let restricted = match Network::new(d) {
        Ok(n) => n,
        Err(e) => {
            result.issues.push(e.to_string());
            return result;
        }
    };
    let route = if is_transit(intended_mode) {
        earliest_transit_arrival(&restricted, request)
    } else {
        earliest_arrival(&restricted, request)
    };
    match route {
        Ok(Outcome::Journey(j)) => {
            let traversed: Vec<_> = j
                .steps
                .iter()
                .filter(|s| selected.contains(&s.edge))
                .map(|s| s.edge)
                .collect();
            if traversed != edges {
                result
                    .issues
                    .push("selection is not a feasible ordered through journey".into());
            }
        }
        Ok(Outcome::Unmet(_)) => result
            .issues
            .push("selection infeasible for supplied profile/time/service".into()),
        Err(e) => result.issues.push(format!("corridor query: {e}")),
    }
    result
}
