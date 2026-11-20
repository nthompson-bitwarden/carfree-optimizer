//! E14-S01–S05/S07: indexed, staged transactions using E10, E12, E08 and E13.

use crate::{catalog, observation, reward, wire};
use accessibility::{scoring, upstream};
use interventions::{
    evaluation::{self, DemandBinding},
    feasibility::{self, Bounds, Cost, Inputs, Report, RightOfWay, Screened, Segment},
    scenario::{Batch, Change, History, Snapshot},
};
use mobility_graph::{
    network::{LinkKind, Network, Provenance},
    NodeId,
};
use mobility_routing::{planner::Options, Request, Run};
use mobility_sim::{
    demand::{self, Demand, TravelMode},
    simulation,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    Configuration(String),
    Action(String),
    Evidence(String),
    Constraint(Box<reward::Audit>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Configuration(message) => write!(f, "configuration: {message}"),
            Self::Action(message) => write!(f, "action: {message}"),
            Self::Evidence(message) => write!(f, "evidence: {message}"),
            Self::Constraint(audit) => {
                write!(f, "hard constraints rejected candidate")?;
                if !audit.convergence_passed {
                    write!(f, ": E08 nonconvergence")?;
                }
                for c in audit.constraints.iter().filter(|c| !c.passed) {
                    write!(f, ": {} {} > {}", c.metric, c.increase_or_loss, c.maximum)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for Error {}

fn evidence(error: impl fmt::Display) -> Error {
    Error::Evidence(error.to_string())
}

fn configuration(error: impl fmt::Display) -> Error {
    Error::Configuration(error.to_string())
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransitPortfolio {
    /// Complete current inventory when a project changes service.
    pub costs: Vec<feasibility::TransitCost>,
    pub supply: Vec<simulation::TransitSupply>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub changes: Vec<Change>,
    pub provenance: Provenance,
    /// Incremental capital/disruption and additive permanent maintenance rows.
    pub costs: Vec<Cost>,
    /// Affected segments only. Existing bindings and earlier edge coverage persist.
    pub segments: Vec<Segment>,
    /// New groups only, or identical re-declarations of existing groups.
    pub rows: Vec<RightOfWay>,
    pub transit: Option<TransitPortfolio>,
    /// Public network topology anchors, independent of demand records.
    pub anchors: [Option<NodeId>; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoorSample {
    pub destination: String,
    pub car_free: Request,
    pub car: Request,
    pub weight: f64,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub history: History,
    /// Opening engineering, cost, and immutable protected OD inventory. Capital
    /// starts at zero. Annual commitments come from the opening costs and transit.
    pub opening: Inputs,
    pub catalog: catalog::Catalog<Project>,
    /// Explicit fixed short-run demand, rebound only to transport snapshots.
    pub demand: Demand,
    /// Fixed settings and initial passenger supply. Opening ID must be exact and
    /// road supply empty; this adapter projects the E10 ledger itself.
    pub simulation: simulation::Scenario,
    /// Exact initial evidence context; scenario/graph bindings are refreshed on
    /// transport edits. The declared destination and geography scopes are fixed.
    pub context: scoring::Context,
    pub doors: Vec<DoorSample>,
    pub reward: reward::Config,
    pub horizon: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetAudit {
    pub limit: feasibility::Budget,
    pub capital: Bounds,
    pub annual: Bounds,
    pub incremental_capital: Bounds,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DoorAudit {
    pub sample: DoorSample,
    pub baseline_car_seconds: u64,
    pub current: upstream::RoutedPair,
    pub fixed_parity: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    /// Absent at opening because E12 exposes project/phase gates.
    pub screening: Option<Report>,
    pub trips: upstream::TripBridge,
    // Matched only in Rust diagnostics; tour IDs never enter policy observations.
    pub(crate) reference_car_free: BTreeSet<String>,
    pub doors: Vec<DoorAudit>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexedTransition {
    pub version: u32,
    pub state: u64,
    pub seed: u64,
    pub action: Option<usize>,
    pub mask: catalog::Mask,
    pub stop_allowed: bool,
    pub observation: observation::Observation,
    pub reward: reward::Audit,
    pub budget: BudgetAudit,
    pub evidence: Evaluation,
    pub terminated: bool,
    pub truncated: bool,
}

impl IndexedTransition {
    pub fn wire(&self) -> wire::WireTransition {
        let mut action_mask: Vec<_> = self
            .mask
            .entries
            .iter()
            .map(catalog::MaskEntry::allowed)
            .collect();
        action_mask.push(self.stop_allowed);
        wire::WireTransition {
            version: self.version,
            state: self.state,
            observation: self.observation.flatten(),
            action_mask,
            reward: self.reward.reward,
            terminated: self.terminated,
            truncated: self.truncated,
        }
    }
}

struct Episode {
    history: History,
    inputs: Inputs,
    transit: Vec<simulation::TransitSupply>,
    applied: BTreeSet<String>,
    transition: IndexedTransition,
}

struct Candidate {
    batch: Batch,
    gate: Screened,
    transit: Option<Vec<simulation::TransitSupply>>,
    incremental_capital: Bounds,
}

pub struct Environment {
    config: Config,
    opening_annual: Bounds,
    revision: u64,
    episode: Option<Episode>,
}

impl Environment {
    pub fn new(config: Config) -> Result<Self, Error> {
        let base = config.history.current();
        let context = &config.context;
        context.validate().map_err(configuration)?;
        if base.parent.is_some()
            || config.horizon == 0
            || !config.reward.valid()
            || context.horizon != scoring::Horizon::ShortRun
            || context.scenario != *base.id()
            || context.graph_revision != base.network().data().revision
            || context.graph_snapshot != base.network().encode()
            || context.demand_revision != config.demand.revision
            || context.land_use_revision != *base.land_use_revision()
            || config.demand.network_revision != base.network().data().revision
            || config.simulation.id != *base.id()
            || !config.simulation.roads.is_empty()
            || !config.simulation.edits.is_empty()
            || config.doors.is_empty()
            || config
                .doors
                .iter()
                .any(|d| !d.weight.is_finite() || d.weight <= 0.0)
        {
            return Err(configuration(
                "invalid opening bindings, reward, door samples or horizon",
            ));
        }
        let opening_annual = crate::portfolio::opening_annual(base, &config.opening)?;
        for project in config.catalog.projects() {
            let p = &project.payload;
            if p.changes.is_empty()
                || p.changes
                    .iter()
                    .any(|c| matches!(c, Change::LandUse { .. }))
            {
                return Err(configuration(
                    "reference projects require transport edits with fixed demand",
                ));
            }
            crate::portfolio::project_costs(&p.costs, &config.opening.budget)?;
        }
        Ok(Self {
            config,
            opening_annual,
            revision: 0,
            episode: None,
        })
    }

    pub fn catalog(&self) -> &catalog::Catalog<Project> {
        &self.config.catalog
    }

    pub fn stop_index(&self) -> usize {
        self.catalog().projects().len()
    }

    pub fn current(&self) -> Option<&IndexedTransition> {
        self.episode.as_ref().map(|episode| &episode.transition)
    }

    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.episode
            .as_ref()
            .map(|episode| episode.history.current())
    }

    /// Exact current engineering, cost and protected-access inputs. These
    /// diagnostics are separate from the policy observation.
    pub fn portfolio(&self) -> Option<&Inputs> {
        self.episode.as_ref().map(|episode| &episode.inputs)
    }

    fn next_revision(&self) -> Result<u64, Error> {
        self.revision
            .checked_add(1)
            .ok_or_else(|| evidence("episode revision exhausted"))
    }

    pub fn reset(&mut self, seed: u64) -> Result<&IndexedTransition, Error> {
        let revision = self.next_revision()?;
        let history = self.config.history.clone();
        let inputs = self.config.opening.clone();
        let transit = &self.config.simulation.transit;
        let applied = BTreeSet::new();
        let (evaluation, metrics) = self.evaluate(history.current(), None, transit, seed, None)?;
        let audit = reward::Audit::evaluate(self.config.reward, metrics.clone(), metrics, None);
        if !audit.passed() {
            return Err(Error::Constraint(Box::new(audit)));
        }
        let budget = BudgetAudit {
            limit: inputs.budget.clone(),
            annual: self.opening_annual,
            capital: Bounds::default(),
            incremental_capital: Bounds::default(),
        };
        let mask = self.mask(&history, &inputs, &applied, revision, false)?;
        let observation = self.observe(history.current(), &budget, &audit.after, 0)?;
        let transition = IndexedTransition {
            version: wire::VERSION,
            state: revision,
            seed,
            action: None,
            mask,
            stop_allowed: true,
            observation,
            reward: audit,
            budget,
            evidence: evaluation,
            terminated: false,
            truncated: false,
        };
        self.episode = Some(Episode {
            history,
            inputs,
            transit: transit.clone(),
            applied,
            transition,
        });
        self.revision = revision;
        self.current()
            .ok_or_else(|| evidence("missing reset transition"))
    }

    pub fn step(
        &mut self,
        action: usize,
        expected_state: u64,
    ) -> Result<&IndexedTransition, Error> {
        let episode = self
            .episode
            .as_ref()
            .ok_or_else(|| Error::Action("reset required".into()))?;
        let current = &episode.transition;
        if expected_state != current.state || current.terminated || current.truncated {
            return Err(Error::Action("stale revision or finished episode".into()));
        }
        let revision = self.next_revision()?;
        if action == self.stop_index() {
            let mask = self.mask(
                &episode.history,
                &episode.inputs,
                &episode.applied,
                revision,
                true,
            )?;
            let episode = self
                .episode
                .as_mut()
                .ok_or_else(|| evidence("missing episode"))?;
            let transition = &mut episode.transition;
            transition.state = revision;
            transition.action = Some(action);
            transition.terminated = true;
            transition.mask = mask;
            transition.stop_allowed = false;
            transition.budget.incremental_capital = Bounds::default();
            let metrics = transition.reward.after.clone();
            transition.reward =
                reward::Audit::evaluate(self.config.reward, metrics.clone(), metrics, None);
            self.revision = revision;
            return Ok(transition);
        }

        let selected = self
            .catalog()
            .project(action)
            .map_err(|e| Error::Action(e.0))?;
        let mut candidate = None;
        // resolve checks all structural eligibility, then uses the fresh E12
        // result for the requested action. Other E12 masks are rebuilt at commit.
        let project = self
            .catalog()
            .resolve(
                action,
                &current.mask.catalog_revision,
                &expected_state.to_string(),
                &current.state.to_string(),
                &episode.applied,
                |p| {
                    if p.id != selected.id {
                        return Ok(vec![]);
                    }
                    match self.prepare(&episode.history, &episode.inputs, p) {
                        Ok(prepared) => {
                            candidate = Some(prepared);
                            Ok(vec![])
                        }
                        Err(reason) => Ok(vec![reason]),
                    }
                },
            )
            .map_err(|e| Error::Action(e.0))?;
        let Candidate {
            batch,
            gate,
            transit,
            incremental_capital,
        } = candidate.ok_or_else(|| evidence("resolved project without screened candidate"))?;
        let transit = transit.unwrap_or_else(|| episode.transit.clone());
        let service_changed = !same_service(
            episode.history.current().network(),
            gate.snapshot().network(),
        ) || episode.transit != transit;
        let service_revision = if service_changed {
            gate.snapshot().id()
        } else {
            &current.evidence.trips.evidence.context.service_revision
        };
        let (evaluation, metrics) = self.evaluate(
            gate.snapshot(),
            Some((&gate, service_revision)),
            &transit,
            current.seed,
            Some(&current.evidence.doors),
        )?;
        let loss = crate::portfolio::protected_losses(
            &current.evidence,
            &evaluation,
            &self.config.demand,
            self.config.simulation.settings.demand_multiplier,
        )?;
        let audit = reward::Audit::evaluate(
            self.config.reward,
            current.reward.after.clone(),
            metrics,
            Some(loss),
        );
        if !audit.passed() {
            return Err(Error::Constraint(Box::new(audit)));
        }
        if !audit.reward.is_finite() {
            return Err(evidence("reward overflow"));
        }

        let mut history = episode.history.clone();
        if history.apply(&batch).map_err(evidence)? != gate.snapshot() {
            return Err(evidence(
                "E10 commit preview differs from E12 screened snapshot",
            ));
        }
        let mut applied = episode.applied.clone();
        applied.insert(project.id.clone());
        let truncated = applied.len() >= self.config.horizon;
        let mut inputs = gate.inputs().clone();
        // E12 covered the removed edges against the current parent. Carry the
        // physical segments and widths forward, using surviving references.
        for segment in &mut inputs.segments {
            segment
                .edges
                .retain(|edge| gate.snapshot().network().link(*edge).is_some());
        }
        let budget = BudgetAudit {
            limit: inputs.budget.clone(),
            capital: gate.report().capital,
            annual: gate.report().annual,
            incremental_capital,
        };
        let mask = self.mask(&history, &inputs, &applied, revision, truncated)?;
        let observation = self.observe(history.current(), &budget, &audit.after, applied.len())?;
        let transition = IndexedTransition {
            version: wire::VERSION,
            state: revision,
            seed: current.seed,
            action: Some(action),
            mask,
            stop_allowed: !truncated,
            observation,
            reward: audit,
            budget,
            evidence: evaluation,
            terminated: false,
            truncated,
        };
        self.episode = Some(Episode {
            history,
            inputs,
            transit,
            applied,
            transition,
        });
        self.revision = revision;
        self.current()
            .ok_or_else(|| evidence("missing committed transition"))
    }

    fn prepare(
        &self,
        history: &History,
        inputs: &Inputs,
        project: &catalog::Project<Project>,
    ) -> Result<Candidate, String> {
        let p = &project.payload;
        let parent = history.current().id();
        let batch = Batch {
            id: format!("{parent}/e14/{}", project.id),
            expected_parent: parent.clone(),
            changes: p.changes.clone(),
            provenance: p.provenance.clone(),
        };
        let next = crate::portfolio::extend(inputs, p)?;
        let gate = feasibility::screen(history, &batch, &next).map_err(|e| e.to_string())?;
        crate::portfolio::validate_transit_costs(
            history.current(),
            gate.snapshot(),
            inputs,
            &next,
        )?;
        for segment in &next.segments {
            if !segment
                .edges
                .iter()
                .any(|edge| gate.snapshot().network().link(*edge).is_some())
            {
                return Err(format!(
                    "segment {} has no surviving physical network reference",
                    segment.id
                ));
            }
        }
        let incremental_capital = crate::portfolio::project_costs(&p.costs, &inputs.budget)
            .map_err(|e| e.to_string())?
            .0;
        Ok(Candidate {
            batch,
            gate,
            transit: p.transit.as_ref().map(|t| t.supply.clone()),
            incremental_capital,
        })
    }

    fn mask(
        &self,
        history: &History,
        inputs: &Inputs,
        applied: &BTreeSet<String>,
        revision: u64,
        finished: bool,
    ) -> Result<catalog::Mask, Error> {
        self.catalog()
            .mask(&revision.to_string(), applied, |p| {
                if finished {
                    return Ok(vec!["episode finished".into()]);
                }
                Ok(self.prepare(history, inputs, p).err().into_iter().collect())
            })
            .map_err(evidence)
    }

    fn observe(
        &self,
        snapshot: &Snapshot,
        budget: &BudgetAudit,
        metrics: &reward::Metrics,
        step: usize,
    ) -> Result<observation::Observation, Error> {
        observation::encode(
            snapshot.network(),
            self.catalog(),
            budget,
            metrics,
            step,
            self.config.horizon,
        )
    }

    fn evaluate(
        &self,
        snapshot: &Snapshot,
        gate: Option<(&Screened, &str)>,
        transit: &[simulation::TransitSupply],
        seed: u64,
        fixed_doors: Option<&[DoorAudit]>,
    ) -> Result<(Evaluation, reward::Metrics), Error> {
        let mut demand = self.config.demand.clone();
        demand.network_revision = snapshot.network().data().revision.clone();
        let mut sim = self.config.simulation.clone();
        sim.id = snapshot.id().clone();
        sim.settings.seed = seed;
        sim.roads = evaluation::road_supply(snapshot, sim.periods.len()).map_err(evidence)?;
        sim.transit = transit.to_vec();
        let network = snapshot.network();
        validate_transit(network, &sim)?;
        let mut context = self.config.context.clone();
        context.scenario = sim.id.clone();
        context.graph_revision = demand.network_revision.clone();
        context.graph_snapshot = network.encode();
        context.land_use_revision = snapshot.land_use_revision().clone();
        let (trips, screening) = if let Some((gate, service_revision)) = gate {
            context.service_revision = service_revision.into();
            let binding = DemandBinding {
                scenario_id: sim.id.clone(),
                demand_revision: demand.revision.clone(),
                land_use_revision: snapshot.land_use_revision().clone(),
            };
            let trips = upstream::screened_trip_bridge(
                &context,
                gate,
                &demand,
                &sim,
                &binding,
                &simulation::Iterative,
            )
            .map_err(evidence)?;
            (trips, Some(gate.report().clone()))
        } else {
            context
                .limitations
                .push("Opening reference execution without an E12 project gate".into());
            let trips =
                upstream::trip_bridge(&context, network, &demand, &sim, &simulation::Iterative)
                    .map_err(evidence)?;
            (trips, None)
        };
        // E13 exposes cohort totals, so repeat its deterministic E06 reference
        // query on the same inputs to match alternatives at individual tours.
        let reference_car_free = demand::evaluate(
            network,
            &demand,
            &sim.demand_scenario,
            sim.settings.max_states_per_query,
        )
        .map_err(evidence)?
        .tours
        .into_iter()
        .filter(|tour| {
            tour.choices
                .iter()
                .any(|(mode, choice)| *mode != TravelMode::Car && choice.is_ok())
        })
        .map(|tour| tour.tour_id)
        .collect();
        let (doors, metrics) = evaluate_doors(
            network,
            &trips,
            &demand,
            sim.settings.demand_multiplier,
            &self.config.doors,
            fixed_doors,
        )?;
        Ok((
            Evaluation {
                screening,
                trips,
                reference_car_free,
                doors,
            },
            metrics,
        ))
    }
}

impl wire::BatchEnvironment for Environment {
    fn reset(&mut self, seed: u64) -> Result<wire::WireTransition, String> {
        Environment::reset(self, seed)
            .map(IndexedTransition::wire)
            .map_err(|e| e.to_string())
    }

    fn step(&mut self, action: usize, expected_state: u64) -> Result<wire::WireTransition, String> {
        Environment::step(self, action, expected_state)
            .map(IndexedTransition::wire)
            .map_err(|e| e.to_string())
    }
}

fn same_service(a: &Network, b: &Network) -> bool {
    let a = a.data();
    let b = b.data();
    let is_transit =
        |l: &&mobility_graph::network::Link| matches!(l.kind, LinkKind::Transit { .. });
    a.days == b.days
        && a.calendars == b.calendars
        && a.links
            .iter()
            .filter(is_transit)
            .eq(b.links.iter().filter(is_transit))
}

fn validate_transit(network: &Network, scenario: &simulation::Scenario) -> Result<(), Error> {
    let (Some(first), Some(last)) = (scenario.periods.first(), scenario.periods.last()) else {
        return Err(evidence("missing simulation periods"));
    };
    let mut supplied: BTreeSet<_> = scenario
        .transit
        .iter()
        .map(|t| (t.edge, t.run.clone()))
        .collect();
    if supplied.len() != scenario.transit.len() {
        return Err(evidence("duplicate transit segment/run supply"));
    }
    let data = network.data();
    let calendars: BTreeMap<_, _> = data.calendars.iter().map(|c| (c.id, c)).collect();
    for link in &data.links {
        let LinkKind::Transit {
            calendar,
            trip,
            stop_index,
            timetable,
            ..
        } = &link.kind
        else {
            continue;
        };
        let calendar = calendars
            .get(calendar)
            .ok_or_else(|| evidence("missing transit calendar"))?;
        for day in data.days.iter().filter(|d| calendar.active(d)) {
            // E05 run times are within the service day and following day.
            if day.origin_utc >= last.end || day.origin_utc.saturating_add(172800) <= first.start {
                continue;
            }
            for event in timetable.departures(link.seconds).map_err(evidence)? {
                let start = day
                    .origin_utc
                    .checked_add(i64::from(event.depart))
                    .ok_or_else(|| evidence("transit run time overflow"))?;
                let end = day
                    .origin_utc
                    .checked_add(i64::from(event.arrive))
                    .ok_or_else(|| evidence("transit run time overflow"))?;
                if start >= last.end || end <= first.start {
                    continue;
                }
                let run = Run {
                    trip: trip.clone(),
                    service_date: day.date,
                    ordinal: event.run,
                    stop_index: *stop_index,
                };
                if !supplied.remove(&(link.id, run)) {
                    return Err(evidence(
                        "missing transit segment/run capacity in simulation periods",
                    ));
                }
            }
        }
    }
    if !supplied.is_empty() {
        return Err(evidence(
            "transit segment/run capacity outside simulation periods or snapshot",
        ));
    }
    Ok(())
}

fn evaluate_doors(
    network: &Network,
    trips: &upstream::TripBridge,
    demand: &Demand,
    multiplier: f64,
    samples: &[DoorSample],
    fixed_doors: Option<&[DoorAudit]>,
) -> Result<(Vec<DoorAudit>, reward::Metrics), Error> {
    let doors: Vec<_> = samples
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let current = upstream::route_pair(
                network,
                &s.destination,
                &s.car_free,
                &s.car,
                &Options::default(),
            )
            .map_err(evidence)?;
            let baseline_car_seconds = match fixed_doors {
                Some(doors) => doors[i].baseline_car_seconds,
                None => match &current.pair().car {
                    scoring::Evidence::Feasible(journey) if journey.seconds() > 0 => {
                        journey.seconds()
                    }
                    _ => {
                        return Err(evidence(
                            "missing feasible opening door-to-door car comparator",
                        ))
                    }
                },
            };
            let fixed_parity = match &current.pair().car_free {
                scoring::Evidence::Feasible(j) => {
                    (baseline_car_seconds as f64 / j.seconds() as f64).min(1.0)
                }
                scoring::Evidence::Infeasible => 0.0,
                scoring::Evidence::Missing(_) => {
                    return Err(evidence("missing current car-free door evidence"))
                }
            };
            Ok(DoorAudit {
                sample: s.clone(),
                baseline_car_seconds,
                current,
                fixed_parity,
            })
        })
        .collect::<Result<_, Error>>()?;
    let metrics = crate::portfolio::metrics(trips, demand, multiplier, &doors)?;
    Ok((doors, metrics))
}
