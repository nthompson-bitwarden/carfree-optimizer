//! Match a neutral trip score to the actual E12 scenario/demand/land-use inputs.
use crate::scoring::{check, nonnegative, DemandAudit, Error, TripIdentity, TripScore};
use interventions::{evaluation::DemandBinding, feasibility::Screened};
use mobility_sim::demand::Demand;
pub fn bind_demand(
    mut score: TripScore,
    gate: &Screened,
    demand: &Demand,
    binding: &DemandBinding,
    multiplier: f64,
) -> Result<TripScore, Error> {
    let c = &score.context;
    check(
        nonnegative(multiplier)
            && multiplier > 0.0
            && c.scenario == *gate.snapshot().id()
            && c.graph_snapshot == gate.snapshot().network().encode()
            && c.graph_revision == demand.network_revision
            && demand.network_revision == gate.snapshot().network().data().revision
            && c.demand_revision == demand.revision
            && binding.demand_revision == demand.revision
            && binding.scenario_id == c.scenario
            && binding.land_use_revision == *gate.snapshot().land_use_revision()
            && c.land_use_revision == binding.land_use_revision,
        "scored demand/scenario/land-use binding mismatch",
    )?;
    let mut inventory: Vec<_> = demand
        .tours
        .iter()
        .flat_map(|t| {
            t.legs.iter().enumerate().map(move |(i, l)| TripIdentity {
                id: format!("{}:{i}", t.id),
                group: t.group.clone(),
                neighborhood: t.home_zone.clone(),
                expansion_weight: l.expansion_weight * multiplier,
                essential: l.essential,
            })
        })
        .collect();
    inventory.sort_by(|a, b| a.id.cmp(&b.id));
    check(
        score.inventory == inventory,
        "trip membership/weighting does not match bound demand legs",
    )?;
    score.0.demand_audit = Some(DemandAudit {
        demand: demand.clone(),
        binding: binding.clone(),
        multiplier,
    });
    Ok(score)
}
