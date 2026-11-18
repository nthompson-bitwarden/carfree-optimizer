//! Explicit E10 -> E08 bridge. No demand regeneration, hidden supply addition or
//! application of the same capacity edit twice. Destination effects require a
//! caller-prepared revised demand input and a matching provenance binding.
use crate::scenario::{Error, ScenarioKind, Snapshot};
use mobility_sim::{
    demand::Demand,
    simulation::{Adapter, Report, RoadSupply, Scenario},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemandBinding {
    pub scenario_id: String,
    pub demand_revision: String,
    pub land_use_revision: Option<String>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub scenario_id: String,
    pub kind: ScenarioKind,
    pub land_use_revision: Option<String>,
    pub report: Report,
}
/// Constant modeled capacity projected exactly once into caller-defined periods.
/// Time-varying observed supply needs a separately validated mapping; this helper
/// must not be used to infer it or add it to the scenario's baseline capacities.
pub fn road_supply(snapshot: &Snapshot, periods: usize) -> Result<Vec<RoadSupply>, Error> {
    if periods == 0 || periods > 128 {
        return Err(Error("invalid period count".into()));
    }
    Ok(snapshot
        .road_capacities()?
        .into_iter()
        .map(|(edge, c)| RoadSupply {
            edge,
            vehicles_per_hour: vec![f64::from(c); periods],
        })
        .collect())
}
pub fn evaluate(
    snapshot: &Snapshot,
    engine: &impl Adapter,
    demand: &Demand,
    scenario: &Scenario,
    binding: &DemandBinding,
) -> Result<Evaluation, Error> {
    if scenario.id != snapshot.id
        || binding.scenario_id != snapshot.id
        || binding.demand_revision != demand.revision
        || binding.land_use_revision != snapshot.land_use_revision
        || demand.network_revision != snapshot.network.data().revision
    {
        return Err(Error(
            "stale scenario/demand/land-use binding; prepare explicit matched inputs".into(),
        ));
    }
    if !scenario.edits.is_empty() {
        return Err(Error(
            "E10 graph already edited: overlapping E08 edits are prohibited".into(),
        ));
    }
    let expected = road_supply(snapshot, scenario.periods.len())?;
    let mut supplied = scenario.roads.clone();
    supplied.sort_by_key(|s| s.edge);
    if supplied != expected {
        return Err(Error("road supply must be the single E10 projection, without baseline addition or blocked roads".into()));
    }
    let report = engine
        .evaluate(&snapshot.network, demand, scenario)
        .map_err(|e| Error(e.to_string()))?;
    Ok(Evaluation {
        scenario_id: snapshot.id.clone(),
        kind: snapshot.kind(),
        land_use_revision: snapshot.land_use_revision.clone(),
        report,
    })
}
