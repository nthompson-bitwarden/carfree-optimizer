//! Explicit E07/E08/E09 bridges. Errors never become missing or infeasible cells.
use crate::scoring::{
    check, nonnegative, Context, Error, Evidence, Journey, Mode, Pair, Trip, TripScore, Uncertainty,
};
use mobility_graph::{
    network::{LinkKind, Network},
    Mode as GraphMode,
};
use mobility_routing::{
    planner::{self, Options, PlannedOutcome},
    Request,
};
use mobility_sim::{
    calibration,
    demand::{self, TravelMode},
    simulation,
};
use std::collections::{BTreeMap, BTreeSet};
fn itinerary(outcome: PlannedOutcome, require_car: bool) -> Result<Evidence, Error> {
    match outcome {
        PlannedOutcome::Unmet(_) => Ok(Evidence::Infeasible),
        PlannedOutcome::Journey(i) => {
            if require_car {
                check(
                    i.journey.steps.iter().any(|s| s.mode == GraphMode::Car)
                        && i.journey
                            .steps
                            .last()
                            .is_some_and(|s| s.mode == GraphMode::Walk),
                    "car comparison requires actual driving and parking/door egress",
                )?;
            }
            let t = i.times;
            let cv = |n| {
                u32::try_from(n)
                    .map_err(|_| Error("itinerary component outside u32 seconds".into()))
            };
            let j = Journey {
                access: cv(t.access_seconds)?,
                wait: cv(t.wait_seconds)?,
                ride: cv(t.ride_seconds)?,
                transfer: cv(t.transfer_seconds)?,
                intermediate_walk: cv(t.intermediate_walk_seconds)?,
                egress: cv(t.egress_seconds)?,
                dwell: cv(t.dwell_seconds)?,
            };
            check(
                i.journey
                    .arrival_utc
                    .checked_sub(i.journey.departure_utc)
                    .is_some_and(|s| s >= 0 && s as u64 == j.seconds()),
                "itinerary components do not reconcile",
            )?;
            Ok(Evidence::Feasible(j))
        }
    }
}
/// Immutable route evidence retains the actual query, profile, revision AND bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct RoutedPair {
    pair: Pair,
    network: Vec<u8>,
    car_free: Request,
    car: Request,
}
impl RoutedPair {
    pub fn pair(&self) -> &Pair {
        &self.pair
    }
    pub fn car_free_request(&self) -> &Request {
        &self.car_free
    }
    pub fn car_request(&self) -> &Request {
        &self.car
    }
}
/// Both requests name actual WALK-ONLY destination doors. The E07 search cannot
/// constrain final mode directly, so this bridge requires an inspectable door
/// topology rather than discarding an earlier driving-only goal after routing.
pub fn route_pair(
    network: &Network,
    destination: &str,
    car_free: &Request,
    car: &Request,
    options: &Options,
) -> Result<RoutedPair, Error> {
    check(crate::scoring::text(destination) && car_free.from == car.from && car_free.to == car.to && car_free.departure_utc == car.departure_utc && car_free.deadline_utc == car.deadline_utc && !car_free.profile.allowed_modes.contains(&GraphMode::Car) && car_free.vehicle != Some(GraphMode::Car) && car.initial_mode == GraphMode::Car && car.vehicle == Some(GraphMode::Car) && car.profile.step_free == car_free.profile.step_free && car.profile.mobility_aid == car_free.profile.mobility_aid && car.profile.allow_unknown_access == car_free.profile.allow_unknown_access, "comparison requires matched doors/departures/budgets/accessibility profiles and distinct modes")?;
    check(
        !options.require_transit && options.stops.is_empty(),
        "paired destination scoring requires direct options",
    )?;
    check(network.data().links.iter().filter(|l| l.to == car.to).all(|l| matches!(l.kind,LinkKind::Street {mode:GraphMode::Walk,..})), "destination must be a dedicated walk-only door node; split driving/parking arrivals upstream")?;
    let f = planner::plan(network, car_free, options)
        .map_err(|e| Error(format!("car-free routing failed: {e}")))?;
    let c = planner::plan(network, car, options)
        .map_err(|e| Error(format!("car routing failed: {e}")))?;
    Ok(RoutedPair {
        pair: Pair {
            destination: destination.into(),
            departure_utc: car.departure_utc,
            car_free: itinerary(f, false)?,
            car: itinerary(c, true)?,
        },
        network: network.encode(),
        car_free: car_free.clone(),
        car: car.clone(),
    })
}
/// Validate binding before projecting query results into the neutral scoring API.
/// Destination node IDs and the actual routing profile are explicit registries.
pub fn routed_access(
    context: &Context,
    stratum: &crate::scoring::Stratum,
    network: &Network,
    profile: &mobility_routing::Profile,
    destinations: &BTreeMap<String, mobility_graph::NodeId>,
    origins: &BTreeMap<String, mobility_graph::NodeId>,
    pairs: &[RoutedPair],
) -> Result<crate::scoring::AccessScore, Error> {
    check(
        context.graph_revision == network.data().revision
            && context.graph_snapshot == network.encode(),
        "routing graph/context mismatch",
    )?;
    let encoded = network.encode();
    let mut origin = None;
    for p in pairs {
        let r = &p.car_free;
        check(
            p.network == encoded
                && origins.get(&stratum.origin) == Some(&r.from)
                && r.profile == *profile
                && destinations.get(&p.pair.destination) == Some(&r.to)
                && r.deadline_utc.checked_sub(r.departure_utc)
                    == Some(i64::from(stratum.budget_seconds)),
            "route evidence graph/profile/destination/budget mismatch",
        )?;
        if let Some(from) = origin {
            check(r.from == from, "mixed origins")?;
        } else {
            origin = Some(r.from);
        }
        let mut modes: Vec<_> = r
            .profile
            .allowed_modes
            .iter()
            .map(|m| match m {
                GraphMode::Walk => Mode::Walk,
                GraphMode::Bicycle => Mode::Bicycle,
                GraphMode::ElectricBicycle => Mode::ElectricBicycle,
                GraphMode::Car => Mode::Car,
                _ => Mode::Transit,
            })
            .collect();
        modes.sort();
        modes.dedup();
        check(modes == stratum.modes, "route evidence mode set mismatch")?;
    }
    crate::scoring::access(&crate::scoring::AccessInput {
        context: context.clone(),
        stratum: stratum.clone(),
        destinations: destinations.keys().cloned().collect(),
        pairs: pairs.iter().map(|p| p.pair.clone()).collect(),
    })
}
fn mode(m: TravelMode) -> Mode {
    match m {
        TravelMode::Walk => Mode::Walk,
        TravelMode::Bicycle => Mode::Bicycle,
        TravelMode::ElectricBicycle => Mode::ElectricBicycle,
        TravelMode::Transit => Mode::Transit,
        TravelMode::Car => Mode::Car,
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct SimulationEvidence {
    pub context: Context,
    pub status: simulation::Status,
    pub denied_tours: usize,
    pub served_mode_trip_weights: BTreeMap<Mode, f64>,
    pub unserved_trip_weight: f64,
    pub road_vehicle_km: f64,
    /// Always limited current fitness. E09's graph bytes are not retained in its
    /// report; a matching reference report cannot certify the current run.
    pub calibration_banner: String,
    pub limitations: Vec<String>,
    run: simulation::Report,
}
impl SimulationEvidence {
    pub fn run(&self) -> &simulation::Report {
        &self.run
    }
}
/// Execute the actual adapter rather than trusting caller-relabelled reports.
/// No input graph/seed/settings/supply can be changed after execution unnoticed.
pub fn simulation_evidence(
    context: &Context,
    network: &Network,
    demand: &demand::Demand,
    scenario: &simulation::Scenario,
    engine: &impl simulation::Adapter,
    calibration: Option<&calibration::Report>,
) -> Result<SimulationEvidence, Error> {
    context.validate()?;
    let run = engine
        .evaluate(network, demand, scenario)
        .map_err(|e| Error(format!("simulation failed: {e}")))?;
    summarize_run(context, network, scenario, run, calibration)
}
/// The scoring evidence is derived from THIS exact gated execution, never a
/// second direct adapter call that could bypass phase/supply/demand checks.
pub fn screened_simulation_evidence(
    context: &Context,
    gate: &interventions::feasibility::Screened,
    demand: &demand::Demand,
    scenario: &simulation::Scenario,
    binding: &interventions::evaluation::DemandBinding,
    engine: &impl simulation::Adapter,
    calibration: Option<&calibration::Report>,
) -> Result<SimulationEvidence, Error> {
    check(
        context.land_use_revision == binding.land_use_revision,
        "scoring land-use binding mismatch",
    )?;
    let evaluation = gate
        .evaluate(engine, demand, scenario, binding)
        .map_err(|e| Error(format!("screened evaluation failed: {e}")))?;
    summarize_run(
        context,
        gate.snapshot().network(),
        scenario,
        evaluation.report,
        calibration,
    )
}
fn summarize_run(
    context: &Context,
    network: &Network,
    scenario: &simulation::Scenario,
    run: simulation::Report,
    calibration: Option<&calibration::Report>,
) -> Result<SimulationEvidence, Error> {
    context.validate()?;
    check(
        context.scenario == run.scenario_id
            && context.graph_revision == run.network_revision
            && network.data().revision == run.network_revision
            && context.graph_snapshot == network.encode()
            && context.demand_revision == run.demand_revision,
        "simulation/context/network binding mismatch",
    )?;
    let mut limitations = context.limitations.clone();
    limitations.push(format!(
        "E08 {:?}; finite candidate-set and capacity-denial bounds; effective network {}",
        run.status, run.effective_network_revision
    ));
    if let Some(c) = calibration {
        check(
            c.scenario_id() == run.scenario_id
                && c.network_revision() == run.network_revision
                && c.demand_revision() == run.demand_revision
                && c.engine() == run.engine,
            "calibration reference scope mismatch",
        )?;
        let selected = c
            .selected()
            .and_then(|i| c.candidates().get(i))
            .ok_or_else(|| Error("calibration has no selected parameters".into()))?;
        let mut expected = c.scenario_snapshot().clone();
        expected.settings.alpha = selected.parameters.alpha;
        expected.settings.preference_seconds = selected.parameters.preference_seconds;
        expected.settings.demand_multiplier = selected.parameters.demand_multiplier;
        expected.settings.seed = scenario.settings.seed;
        check(
            expected == *scenario
                && selected.runs.iter().chain(c.holdout()).any(|r| {
                    r.seed == run.seed
                        && r.error.is_none()
                        && r.effective_network_revision.as_ref()
                            == Some(&run.effective_network_revision)
                }),
            "calibration reference seed/settings/supply/effective view mismatch",
        )?;
        limitations.extend_from_slice(c.limitations());
        limitations.push(format!("E09 REFERENCE ONLY: {} Exact input-network bytes are not retained by E09; its fitness is not inherited.",c.banner()));
    }
    let mut seen = BTreeSet::new();
    let mut km = 0.0;
    for r in &run.roads {
        check(
            seen.insert((r.edge, r.period)) && nonnegative(r.vehicles),
            "duplicate/invalid road volume",
        )?;
        let link = network
            .link(r.edge)
            .ok_or_else(|| Error("unknown road edge".into()))?;
        check(
            matches!(
                link.kind,
                LinkKind::Street {
                    mode: GraphMode::Car,
                    ..
                }
            ),
            "road volume is not a car street",
        )?;
        km += link
            .distance_m
            .ok_or_else(|| Error("road length unavailable".into()))?
            / 1000.0
            * r.vehicles;
    }
    check(km.is_finite(), "vehicle-km overflow")?;
    Ok(SimulationEvidence {context:context.clone(),status:run.status.clone(),denied_tours:run.assignments.iter().filter(|a| a.denied_boarding).count(),served_mode_trip_weights:run.mode_trip_weights.iter().map(|(m,w)|(mode(*m),*w)).collect(),unserved_trip_weight:run.unserved_trip_weight,road_vehicle_km:km,calibration_banner:"LIMITED FITNESS: no exact input-bound independent calibration; do not use for policy recommendations.".into(),limitations,run})
}
#[derive(Debug, Clone, PartialEq)]
pub struct FeasibilityDistribution {
    pub total: f64,
    pub car_required: f64,
    pub no_mode: f64,
    pub unknown: f64,
}
impl FeasibilityDistribution {
    pub fn car_required_share(&self) -> Option<f64> {
        (self.total > 0.0 && self.unknown == 0.0).then(|| self.car_required / self.total)
    }
}
fn feasibility_distribution(d: &crate::scoring::Distribution) -> FeasibilityDistribution {
    FeasibilityDistribution {
        total: d.total,
        car_required: d.car_required,
        no_mode: d.no_mode,
        unknown: d.unknown,
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceFeasibility {
    pub version: u32,
    pub car_required_uncertainty: Uncertainty,
    pub context: Context,
    pub all: FeasibilityDistribution,
    pub essential: FeasibilityDistribution,
    pub by_group_neighborhood: BTreeMap<(String, String), FeasibilityDistribution>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TripBridge {
    /// Separately labeled E06 reference-network estimate, BEFORE E08 edits/costs/capacity.
    pub reference_feasibility: ReferenceFeasibility,
    /// Actual E08 served modes and unserved weight. Car-required/no-mode remain
    /// unknown where complete effective-view alternatives are not exposed by E08.
    pub assignment: TripScore,
    pub evidence: SimulationEvidence,
}
pub fn trip_bridge(
    context: &Context,
    network: &Network,
    demand: &demand::Demand,
    scenario: &simulation::Scenario,
    engine: &impl simulation::Adapter,
) -> Result<TripBridge, Error> {
    let evidence = simulation_evidence(context, network, demand, scenario, engine, None)?;
    build_trip_bridge(context, network, demand, scenario, evidence)
}
pub fn screened_trip_bridge(
    context: &Context,
    gate: &interventions::feasibility::Screened,
    demand: &demand::Demand,
    scenario: &simulation::Scenario,
    binding: &interventions::evaluation::DemandBinding,
    engine: &impl simulation::Adapter,
) -> Result<TripBridge, Error> {
    let evidence =
        screened_simulation_evidence(context, gate, demand, scenario, binding, engine, None)?;
    build_trip_bridge(
        context,
        gate.snapshot().network(),
        demand,
        scenario,
        evidence,
    )
}
fn build_trip_bridge(
    context: &Context,
    network: &Network,
    demand: &demand::Demand,
    scenario: &simulation::Scenario,
    evidence: SimulationEvidence,
) -> Result<TripBridge, Error> {
    let feasible = demand::evaluate(
        network,
        demand,
        &scenario.demand_scenario,
        scenario.settings.max_states_per_query,
    )
    .map_err(|e| Error(format!("reference feasibility: {e}")))?;
    let choices: BTreeMap<_, _> = feasible.tours.iter().map(|t| (&t.tour_id, t)).collect();
    let assignments: BTreeMap<_, _> = evidence
        .run
        .assignments
        .iter()
        .map(|t| (&t.tour_id, t))
        .collect();
    check(
        choices.len() == demand.tours.len() && assignments.len() == demand.tours.len(),
        "incomplete tour evidence",
    )?;
    let mut reference = Vec::new();
    let mut actual = Vec::new();
    for t in &demand.tours {
        let f = choices
            .get(&t.id)
            .ok_or_else(|| Error("missing reference tour".into()))?;
        let a = assignments
            .get(&t.id)
            .ok_or_else(|| Error("missing assigned tour".into()))?;
        let car = f.choices.get(&TravelMode::Car).is_some_and(|r| r.is_ok());
        let cf = f
            .choices
            .iter()
            .any(|(m, r)| *m != TravelMode::Car && r.is_ok());
        let served = if a.denied_boarding {
            None
        } else {
            a.mode.map(mode)
        };
        for (i, l) in t.legs.iter().enumerate() {
            let r = Trip {
                id: format!("{}:{i}", t.id),
                group: t.group.clone(),
                neighborhood: t.home_zone.clone(),
                weight: l.expansion_weight * scenario.settings.demand_multiplier,
                essential: l.essential,
                car_free: Some(cf),
                car: Some(car),
                served_mode: None,
            };
            reference.push(r.clone());
            actual.push(Trip {
                car_free: served.as_ref().filter(|m| **m != Mode::Car).map(|_| true),
                car: served.as_ref().filter(|m| **m == Mode::Car).map(|_| true),
                served_mode: served.clone(),
                ..r
            });
        }
    }
    let unavailable = || Uncertainty::Unavailable {
        reason: "deterministic modeled inputs have no matched sampling design".into(),
    };
    let mut reference_context = context.clone();
    reference_context.behavior.push_str("; E06 REFERENCE network before E08 edits/costs/capacity, not effective assignment feasibility");
    let reference_score = crate::scoring::trips(&reference_context, &reference, unavailable())?;
    let reference_feasibility = ReferenceFeasibility {
        version: crate::scoring::VERSION,
        car_required_uncertainty: reference_score.car_required_uncertainty.clone(),
        context: reference_context,
        all: feasibility_distribution(&reference_score.all),
        essential: feasibility_distribution(&reference_score.essential),
        by_group_neighborhood: reference_score
            .by_group_neighborhood
            .iter()
            .map(|(k, d)| (k.clone(), feasibility_distribution(d)))
            .collect(),
    };
    let assignment = crate::scoring::trips(context, &actual, unavailable())?;
    let near = |a: f64, b: f64| (a - b).abs() <= 1e-8 * a.abs().max(b.abs()).max(1.0);
    check(
        near(assignment.all.unserved, evidence.unserved_trip_weight)
            && [
                Mode::Walk,
                Mode::Bicycle,
                Mode::ElectricBicycle,
                Mode::Transit,
                Mode::Car,
            ]
            .iter()
            .all(|m| {
                near(
                    assignment.all.served.get(m).copied().unwrap_or(0.0),
                    evidence
                        .served_mode_trip_weights
                        .get(m)
                        .copied()
                        .unwrap_or(0.0),
                )
            }),
        "served/unserved weights do not reconcile",
    )?;
    Ok(TripBridge {
        reference_feasibility,
        assignment,
        evidence,
    })
}
