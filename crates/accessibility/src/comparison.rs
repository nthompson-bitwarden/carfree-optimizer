//! E13-S08/S09: deterministic constrained comparisons, never policy approval.
use crate::scoring::{check, nonnegative, text, AccessScore, Context, Error, TripScore, VERSION};
use interventions::feasibility::{RiskInput, Screened};
use std::collections::{BTreeMap, BTreeSet};
/// Common estimand and constraints, equal across compared candidates. Candidate
/// graph/scenario revisions may differ; population/demand and assumptions may not.
#[derive(Debug, Clone, PartialEq)]
pub struct Basis {
    pub baseline: Option<interventions::scenario::Snapshot>,
    pub demand: String,
    pub demand_snapshot: mobility_sim::demand::Demand,
    pub trip_inventory: Vec<crate::scoring::TripIdentity>,
    pub demand_multiplier: f64,
    pub destinations: String,
    pub geography: String,
    pub period: String,
    pub horizon: crate::scoring::Horizon,
    pub behavior: String,
    pub constraints: String,
    pub protected_access: Vec<interventions::feasibility::AccessSample>,
    pub minor_units_per_major: u32,
    pub currency: String,
    pub price_year: u16,
    pub years: u16,
    pub discount_rate: f64,
    pub capital_limit: f64,
    pub annual_limit: f64,
    /// Same severity, exposure period, model/version/applicability and unit.
    pub risk_definition: Option<RiskDefinition>,
    pub accessibility_definition: (crate::scoring::Stratum, Vec<String>),
}
#[derive(Debug, Clone, PartialEq)]
pub struct RiskDefinition {
    pub model: String,
    pub version: String,
    pub mode: mobility_graph::Mode,
    pub unit: interventions::feasibility::ExposureUnit,
    pub severity: String,
    pub applicability: String,
    pub start: i64,
    pub end: i64,
    pub calibration: Option<mobility_graph::network::Provenance>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    cost_completeness_source: Option<mobility_graph::network::Provenance>,
    context: Context,
    basis: Basis,
    /// lifecycle currency, accessibility share, expected collisions, car-required share.
    values: Option<[f64; 4]>,
    unavailable: Option<String>,
}
/// Caller-supplied complete-project quantity/rate inventory, independent of a
/// phase-final screening input. This is source evidence, not an inferred fact.
/// The caller must justify completeness, including evidenced zero quantities.
pub struct ProjectCostInventory {
    pub baseline: interventions::scenario::Snapshot,
    pub scenario: interventions::scenario::Snapshot,
    pub costs: Vec<interventions::feasibility::Cost>,
    pub transit: Vec<interventions::feasibility::TransitCost>,
    pub completeness_source: mobility_graph::network::Provenance,
}
pub struct SingleProject<'a> {
    pub cost_inventory: &'a ProjectCostInventory,
    pub history: &'a interventions::scenario::History,
    pub batch: &'a interventions::scenario::Batch,
    pub inputs: &'a interventions::feasibility::Inputs,
}
impl Candidate {
    /// A fresh E12 screen checks the caller's independently sourced complete
    /// single-batch inventory against the exact baseline/final snapshots. The
    /// caller establishes inventory completeness; the screen does not infer it.
    /// Different baselines cannot mix.
    pub fn single(
        project: &SingleProject<'_>,
        access: &AccessScore,
        trips: &TripScore,
        risk: Option<&RiskInput>,
        constraints: &str,
    ) -> Result<Self, Error> {
        let gate =
            interventions::feasibility::screen(project.history, project.batch, project.inputs)
                .map_err(|e| Error(format!("project screening failed: {e:?}")))?;
        let inventory = project.cost_inventory;
        let source = &inventory.completeness_source;
        check(inventory.baseline == *project.history.current() && inventory.scenario == *gate.snapshot() && inventory.costs == project.inputs.costs && inventory.transit == project.inputs.transit && [&source.source,&source.source_version,&source.license,&source.geography,&source.date,&source.uncertainty].iter().all(|s| text(s)), "complete project cost inventory/snapshot/source mismatch; phase-final inputs cannot omit capital")?;
        let mut result = Self::with_cost(
            &gate,
            access,
            trips,
            risk,
            constraints,
            Some(project.history.current().clone()),
        )?;
        result.cost_completeness_source = Some(source.clone());
        Ok(result)
    }
    /// A bare Screened may be an E12 plan final_state, whose lifecycle omits
    /// prior phase capital. Without complete project scope it stays UNRANKED.
    pub fn screened(
        gate: &Screened,
        access: &AccessScore,
        trips: &TripScore,
        risk: Option<&RiskInput>,
        constraints: &str,
    ) -> Result<Self, Error> {
        Self::with_cost(gate, access, trips, risk, constraints, None)
    }
    fn with_cost(
        gate: &Screened,
        access: &AccessScore,
        trips: &TripScore,
        risk: Option<&RiskInput>,
        constraints: &str,
        baseline: Option<interventions::scenario::Snapshot>,
    ) -> Result<Self, Error> {
        check(access.context == trips.context, "score contexts differ")?;
        let c = &access.context;
        c.validate()?;
        check(
            c.scenario == *gate.snapshot().id()
                && c.graph_revision == gate.snapshot().network().data().revision
                && c.graph_snapshot == gate.snapshot().network().encode()
                && text(constraints),
            "scores do not match screened snapshot or constraint registry missing",
        )?;
        let audit = trips.demand_audit.as_ref().ok_or_else(|| {
            Error("frontier requires exact demand/leg-inventory binding via bind_demand".into())
        })?;
        check(
            audit.binding.scenario_id == *gate.snapshot().id()
                && audit.binding.demand_revision == c.demand_revision
                && audit.binding.land_use_revision.as_ref()
                    == gate.snapshot().land_use_revision().as_ref()
                && c.land_use_revision == audit.binding.land_use_revision,
            "frontier land-use/demand binding mismatch",
        )?;
        let mut demand_snapshot = audit.demand.clone();
        demand_snapshot.network_revision.clear();
        let b = &gate.inputs().budget;
        let risk_report = risk
            .map(|r| {
                gate.risk(r)
                    .map_err(|e| Error(format!("risk screening: {e:?}")))
            })
            .transpose()?;
        let risk_value = risk_report
            .as_ref()
            .and_then(|r| r.expected_collisions.as_ref())
            .map(|r| r.central);
        let risk_definition = risk.map(|r| RiskDefinition {
            model: r.model.clone(),
            version: r.version.clone(),
            mode: r.mode,
            unit: r.unit,
            severity: r.severity.clone(),
            applicability: r.applicability.clone(),
            start: r.period_start,
            end: r.period_end,
            calibration: r.calibration.clone(),
        });
        let complete_cost = baseline.as_ref().map(|_| gate.report().lifecycle.central);
        let basis = Basis {
            baseline,
            demand: c.demand_revision.clone(),
            demand_snapshot,
            trip_inventory: trips.inventory.clone(),
            demand_multiplier: audit.multiplier,
            destinations: c.destination_revision.clone(),
            geography: c.geography_revision.clone(),
            period: c.period.clone(),
            horizon: c.horizon.clone(),
            behavior: c.behavior.clone(),
            constraints: constraints.into(),
            protected_access: gate.inputs().access.clone(),
            minor_units_per_major: b.minor_units_per_major,
            currency: b.currency.clone(),
            price_year: b.price_year,
            years: b.years,
            discount_rate: b.real_discount_rate,
            capital_limit: b.capital,
            annual_limit: b.annual,
            risk_definition,
            accessibility_definition: (access.stratum.clone(), access.destinations.clone()),
        };
        let values = access
            .reachable_share
            .zip(trips.all.car_required_share())
            .zip(risk_value)
            .zip(complete_cost)
            .map(|(((a, d), r), cost)| [cost, a, r, d]);
        Ok(Self {
            cost_completeness_source: None,
            context: c.clone(),
            basis,
            values,
            unavailable: values.is_none().then(|| {
                "missing complete project cost scope, accessibility, car-required or calibrated risk evidence".into()
            }),
        })
    }
    pub fn context(&self) -> &Context {
        &self.context
    }
    pub fn cost_completeness_source(&self) -> Option<&mobility_graph::network::Provenance> {
        self.cost_completeness_source.as_ref()
    }
    pub fn objectives(&self) -> Option<[f64; 4]> {
        self.values
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Frontier {
    pub cost_sources: BTreeMap<String, Option<mobility_graph::network::Provenance>>,
    pub version: u32,
    pub nondominated: Vec<String>,
    pub dominated: Vec<String>,
    pub unranked: BTreeMap<String, String>,
    pub contexts: Vec<Context>,
}
pub fn pareto(candidates: &[Candidate]) -> Result<Frontier, Error> {
    check(
        candidates.len() <= 10_000,
        "at most 10000 Pareto candidates",
    )?;
    let mut ids = BTreeSet::new();
    let mut basis = None;
    for c in candidates {
        check(
            ids.insert(&c.context.scenario),
            "duplicate Pareto candidate",
        )?;
        if c.values.is_some() {
            if let Some(b) = basis {
                check(
                    b == &c.basis,
                    "inconsistent Pareto constraints, units, estimand or horizon",
                )?;
            } else {
                basis = Some(&c.basis);
            }
        }
    }
    let mut r = Frontier {
        cost_sources: candidates
            .iter()
            .map(|c| {
                (
                    c.context.scenario.clone(),
                    c.cost_completeness_source.clone(),
                )
            })
            .collect(),
        version: VERSION,
        nondominated: Vec::new(),
        dominated: Vec::new(),
        unranked: BTreeMap::new(),
        contexts: candidates.iter().map(|c| c.context.clone()).collect(),
    };
    for c in candidates {
        let Some(v) = c.values else {
            r.unranked.insert(
                c.context.scenario.clone(),
                c.unavailable.clone().unwrap_or_default(),
            );
            continue;
        };
        let dominated = candidates
            .iter()
            .filter_map(|o| o.values)
            .any(|o| dominates(o, v));
        if dominated {
            r.dominated.push(c.context.scenario.clone());
        } else {
            r.nondominated.push(c.context.scenario.clone());
        }
    }
    r.nondominated.sort();
    r.dominated.sort();
    Ok(r)
}
fn dominates(a: [f64; 4], b: [f64; 4]) -> bool {
    // Minimize costs, collisions and dependence; maximize accessibility.
    let a = [a[0], -a[1], a[2], a[3]];
    let b = [b[0], -b[1], b[2], b[3]];
    a.iter().zip(b).all(|(x, y)| *x <= y) && a.iter().zip(b).any(|(x, y)| *x < y)
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Condition {
    pub weather: String,
    pub day_or_night: String,
    /// Explicit versioned timetable/frequency variant, not a service-presence flag.
    pub service_revision: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionScore {
    pub condition: Condition,
    pub score: crate::projection::Projection,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Rankings {
    pub version: u32,
    pub contexts: Vec<Context>,
    /// Weighted accessibility over a complete matched grid; descending, ID ties.
    pub ranked: Vec<(String, f64)>,
    pub unranked: Vec<String>,
    pub per_condition: BTreeMap<Condition, Vec<(String, f64)>>,
}
pub fn rank_conditions(
    grid: &[(Condition, f64)],
    rows: &[ConditionScore],
) -> Result<Rankings, Error> {
    check(
        !grid.is_empty() && grid.len() <= 10000,
        "condition grid required (at most 10000)",
    )?;
    let mut weights = BTreeMap::new();
    for (c, w) in grid {
        check(
            text(&c.weather)
                && matches!(c.day_or_night.as_str(), "day" | "night")
                && text(&c.service_revision)
                && nonnegative(*w)
                && *w > 0.0
                && weights.insert(c.clone(), *w).is_none(),
            "invalid/duplicate weather/day-night/service condition or weight",
        )?;
    }
    let total: f64 = weights.values().sum();
    check(total.is_finite(), "condition weights overflow")?;
    let mut scenarios: BTreeMap<String, BTreeMap<Condition, Option<f64>>> = BTreeMap::new();
    let mut common: Option<&Context> = None;
    let mut contexts = Vec::new();
    let mut estimand = None;
    let mut condition_keys = BTreeMap::new();
    let mut population_key = None;
    for row in rows {
        row.score.context.validate()?;
        let c = &row.score.context;
        let identity = (&row.score.origin, &row.score.neighborhood, &row.score.group);
        if let Some(key) = population_key {
            check(
                key == identity,
                "ranking origins/neighborhoods/cohorts differ",
            )?;
        } else {
            population_key = Some(identity);
        }
        check(
            row.condition.service_revision == c.service_revision,
            "condition must identify the bound timetable/frequency revision",
        )?;
        let mut across = row.score.estimand.clone();
        across.departures_utc.clear();
        if let Some(key) = &estimand {
            check(key == &across, "cross-condition score estimands differ")?;
        } else {
            estimand = Some(across);
        }
        if let Some(key) = condition_keys.insert(&row.condition, &row.score.estimand) {
            check(
                key == &row.score.estimand,
                "within-condition departure grids differ",
            )?;
        }
        if let Some(b) = common {
            check(
                c.demand_revision == b.demand_revision
                    && c.destination_revision == b.destination_revision
                    && c.geography_revision == b.geography_revision
                    && c.period == b.period
                    && c.horizon == b.horizon
                    && c.behavior == b.behavior,
                "condition ranking estimands/horizons differ",
            )?;
        } else {
            common = Some(c);
        }
        check(
            weights.contains_key(&row.condition)
                && row.score.share.is_none_or(|s| nonnegative(s) && s <= 1.0),
            "unknown condition or invalid accessibility",
        )?;
        contexts.push(c.clone());
        check(
            scenarios
                .entry(c.scenario.clone())
                .or_default()
                .insert(row.condition.clone(), row.score.share)
                .is_none(),
            "duplicate scenario-condition cell",
        )?;
    }
    let mut r = Rankings {
        version: VERSION,
        contexts,
        ranked: Vec::new(),
        unranked: Vec::new(),
        per_condition: BTreeMap::new(),
    };
    for (id, cells) in scenarios {
        let mut sum = 0.0;
        let mut complete = true;
        for (c, w) in &weights {
            match cells.get(c).copied().flatten() {
                Some(v) => {
                    sum += v * (*w / total);
                    r.per_condition
                        .entry(c.clone())
                        .or_default()
                        .push((id.clone(), v));
                }
                None => complete = false,
            }
        }
        if complete {
            r.ranked.push((id, sum));
        } else {
            r.unranked.push(id);
        }
    }
    fn sort(rows: &mut [(String, f64)]) {
        rows.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    }
    sort(&mut r.ranked);
    for v in r.per_condition.values_mut() {
        sort(v);
    }
    Ok(r)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn e13_s08_strict_dominance_and_ties() {
        assert!(dominates([1.0, 0.8, 2.0, 0.1], [2.0, 0.7, 2.0, 0.2]));
        assert!(!dominates([1.0, 0.8, 2.0, 0.1], [1.0, 0.8, 2.0, 0.1]));
        assert!(!dominates([1.0, 0.8, 3.0, 0.1], [2.0, 0.7, 2.0, 0.2]));
    }
}
