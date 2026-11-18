//! E08 bounded reference assignment; see docs/SIMULATION.md for assumptions.
use crate::demand::{self, Choice, Demand, TravelMode};
use mobility_graph::network::{LinkKind, Network, Provenance};
use mobility_graph::{EdgeId, Mode};
use mobility_routing::Run;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    Invalid(String),
    Demand(demand::Error),
    Arithmetic,
    WorkerPanic,
    WorkerSpawn(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
fn require(ok: bool, message: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid(message.into()))
    }
}
fn finite(x: f64) -> bool {
    x.is_finite() && x >= 0.0
}
fn checked(x: f64) -> Result<f64, Error> {
    if finite(x) {
        Ok(x)
    } else {
        Err(Error::Arithmetic)
    }
}

/// Nonoverlapping, contiguous, half-open UTC link-entry periods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Period {
    pub start: i64,
    pub end: i64,
}
/// One positive road capacity per edge and period; no inferred defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadSupply {
    pub edge: EdgeId,
    pub vehicles_per_hour: Vec<f64>,
}
/// Capacity per run and occupied link segment, shared across departure periods.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitSupply {
    pub edge: EdgeId,
    pub run: Run,
    pub passengers: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    StreetSeconds {
        edge: EdgeId,
        seconds: u32,
    },
    RoadCapacity {
        edge: EdgeId,
        period: usize,
        vehicles_per_hour: f64,
    },
    TransitCapacity {
        edge: EdgeId,
        run: Run,
        passengers: f64,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub seed: u64,
    pub max_iterations: usize,
    pub max_states_per_query: usize,
    /// Relative volume residual, divided by max(1, old, new).
    pub tolerance: f64,
    /// Explicit modeled BPR t = ceil(t0 * (1 + alpha * (v/c)^beta)).
    pub alpha: f64,
    pub beta: f64,
    pub vehicles_per_trip: f64,
    pub demand_multiplier: f64,
    /// Stable uniform [0, scale) additive tour/mode preference, equivalent seconds.
    pub preference_seconds: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    pub version: u32,
    pub id: String,
    pub provenance: Provenance,
    pub demand_scenario: demand::Scenario,
    pub periods: Vec<Period>,
    pub roads: Vec<RoadSupply>,
    pub transit: Vec<TransitSupply>,
    pub edits: Vec<Edit>,
    pub settings: Settings,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Converged,
    Nonconverged,
    FixedCosts,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub tour_id: String,
    pub mode: Option<TravelMode>,
    pub choice: Option<Choice>,
    /// Atomic whole-tour denial in the evaluated finite candidate set; no retry.
    pub denied_boarding: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct RoadResult {
    pub edge: EdgeId,
    pub period: usize,
    pub vehicles: f64,
    pub capacity_vehicles: f64,
    /// Duration used to route/replay the final assignment.
    pub used_seconds: u32,
    /// BPR duration implied by final assigned volume, possibly not yet equilibrated.
    pub implied_seconds: u32,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TransitResult {
    pub edge: EdgeId,
    pub run: Run,
    pub requested_passengers: f64,
    pub served_passengers: f64,
    pub capacity_passengers: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub version: u32,
    pub scenario_id: String,
    pub network_revision: String,
    /// Scenario-specific identity of edited network and final entry-period costs.
    pub effective_network_revision: String,
    pub demand_revision: String,
    pub engine: String,
    pub seed: u64,
    pub status: Status,
    pub iterations: usize,
    pub volume_residual: f64,
    pub assignments: Vec<Assignment>,
    pub roads: Vec<RoadResult>,
    pub transit: Vec<TransitResult>,
    pub mode_trip_weights: BTreeMap<TravelMode, f64>,
    pub person_seconds: f64,
    pub unserved_trip_weight: f64,
    pub essential_trip_weight: f64,
    pub unserved_essential_weight: f64,
    /// Essential weight whose tour has no feasible car-free candidate, before capacity.
    pub essential_without_car_free_candidate: f64,
}

pub trait Adapter: Sync {
    fn name(&self) -> &'static str;
    fn evaluate(
        &self,
        baseline: &Network,
        demand: &Demand,
        scenario: &Scenario,
    ) -> Result<Report, Error>;
}
pub struct Iterative;
pub struct FixedCost;
impl Adapter for Iterative {
    fn name(&self) -> &'static str {
        "iterative-bpr-v1"
    }
    fn evaluate(&self, n: &Network, d: &Demand, s: &Scenario) -> Result<Report, Error> {
        execute(n, d, s, true, self.name())
    }
}
impl Adapter for FixedCost {
    fn name(&self) -> &'static str {
        "fixed-cost-v1"
    }
    fn evaluate(&self, n: &Network, d: &Demand, s: &Scenario) -> Result<Report, Error> {
        execute(n, d, s, false, self.name())
    }
}

type RoadKey = (EdgeId, usize);
type TransitKey = (EdgeId, Run);
struct Prepared {
    network: Network,
    roads: BTreeMap<RoadKey, f64>,
    transit: BTreeMap<TransitKey, f64>,
}
fn prepare(n: &Network, d: &Demand, s: &Scenario) -> Result<Prepared, Error> {
    let p = &s.settings;
    require(
        s.version == VERSION && !s.id.trim().is_empty(),
        "scenario version/id",
    )?;
    require(
        d.network_revision == n.data().revision,
        "stale demand graph binding",
    )?;
    require(
        p.max_iterations > 0
            && p.max_states_per_query > 0
            && finite(p.tolerance)
            && p.tolerance <= 1.0
            && finite(p.alpha)
            && p.beta.is_finite()
            && p.beta > 0.0
            && p.vehicles_per_trip.is_finite()
            && p.vehicles_per_trip > 0.0
            && p.demand_multiplier.is_finite()
            && p.demand_multiplier > 0.0
            && finite(p.preference_seconds),
        "invalid settings",
    )?;
    require(
        !s.periods.is_empty()
            && s.periods.len() <= 128
            && s.periods
                .iter()
                .all(|b| b.start < b.end && b.end.checked_sub(b.start).is_some())
            && s.periods.windows(2).all(|b| b[0].end == b[1].start)
            && s.periods[0].start <= n.data().coverage_start
            && s.periods
                .last()
                .is_some_and(|b| b.end > n.data().coverage_end),
        "periods must cover graph coverage",
    )?;
    // Network validation also validates the scenario's complete provenance fields.
    let mut data = n.data().clone();
    data.provenance = s.provenance.clone();
    Network::new(data.clone()).map_err(|e| Error::Invalid(e.to_string()))?;
    data.provenance = n.data().provenance.clone();
    let mut roads = BTreeMap::new();
    for r in &s.roads {
        require(
            n.link(r.edge).is_some_and(|l| {
                matches!(
                    l.kind,
                    LinkKind::Street {
                        mode: Mode::Car,
                        ..
                    }
                )
            }) && r.vehicles_per_hour.len() == s.periods.len(),
            "invalid road supply",
        )?;
        for (i, &capacity) in r.vehicles_per_hour.iter().enumerate() {
            require(
                capacity.is_finite()
                    && capacity > 0.0
                    && roads.insert((r.edge, i), capacity).is_none(),
                "invalid/duplicate road capacity",
            )?;
        }
    }
    for l in &data.links {
        if matches!(
            l.kind,
            LinkKind::Street {
                mode: Mode::Car,
                blocked: false,
                ..
            }
        ) {
            require(roads.contains_key(&(l.id, 0)), "missing road capacity")?;
        }
    }
    let mut transit = BTreeMap::new();
    for t in &s.transit {
        let valid_run = n.link(t.edge).is_some_and(|l| {
            if let LinkKind::Transit {
                trip,
                stop_index,
                calendar,
                timetable,
                ..
            } = &l.kind
            {
                trip == &t.run.trip
                    && stop_index == &t.run.stop_index
                    && data.days.iter().any(|day| {
                        day.date == t.run.service_date
                            && data
                                .calendars
                                .iter()
                                .any(|c| c.id == *calendar && c.active(day))
                    })
                    && timetable
                        .departures(l.seconds)
                        .is_ok_and(|mut events| events.any(|e| e.run == t.run.ordinal))
            } else {
                false
            }
        });
        require(
            valid_run
                && finite(t.passengers)
                && transit
                    .insert((t.edge, t.run.clone()), t.passengers)
                    .is_none(),
            "invalid/duplicate transit supply",
        )?;
    }
    // No last-write-wins: reject overlapping/conflicting edits atomically.
    let mut keys = BTreeSet::new();
    for e in &s.edits {
        match e {
            Edit::StreetSeconds { edge, seconds } => {
                require(
                    keys.insert(format!("street:{}", edge.0)) && *seconds > 0,
                    "duplicate/invalid street edit",
                )?;
                let link = data
                    .links
                    .iter_mut()
                    .find(|l| l.id == *edge)
                    .ok_or_else(|| Error::Invalid("unknown edited edge".into()))?;
                require(
                    matches!(link.kind, LinkKind::Street { .. }),
                    "duration edit requires street",
                )?;
                link.seconds = *seconds;
            }
            Edit::RoadCapacity {
                edge,
                period,
                vehicles_per_hour,
            } => {
                require(
                    keys.insert(format!("road:{}:{period}", edge.0))
                        && vehicles_per_hour.is_finite()
                        && *vehicles_per_hour > 0.0,
                    "duplicate/invalid road edit",
                )?;
                *roads
                    .get_mut(&(*edge, *period))
                    .ok_or_else(|| Error::Invalid("unknown road/period edit".into()))? =
                    *vehicles_per_hour;
            }
            Edit::TransitCapacity {
                edge,
                run,
                passengers,
            } => {
                require(
                    keys.insert(format!("transit:{}:{run:?}", edge.0)) && finite(*passengers),
                    "duplicate/invalid transit edit",
                )?;
                *transit
                    .get_mut(&(*edge, run.clone()))
                    .ok_or_else(|| Error::Invalid("unknown transit run edit".into()))? =
                    *passengers;
            }
        }
    }
    let network = Network::new(data).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(Prepared {
        network,
        roads,
        transit,
    })
}
fn period(s: &Scenario, utc: i64) -> Result<usize, Error> {
    let i = s.periods.partition_point(|p| p.end <= utc);
    if s.periods.get(i).is_some_and(|p| p.start <= utc) {
        Ok(i)
    } else {
        Err(Error::Invalid("link entry outside periods".into()))
    }
}
fn duration(base: u32, volume: f64, capacity: f64, s: &Settings) -> Result<u32, Error> {
    let raw = checked(f64::from(base) * (1.0 + s.alpha * (volume / capacity).powf(s.beta)))?;
    // Suppress sub-nanosecond roundoff at an integer before conservative rounding.
    let seconds = if (raw - raw.round()).abs() <= 1e-9 {
        raw.round()
    } else {
        raw.ceil()
    };
    if seconds > f64::from(u32::MAX) {
        Err(Error::Arithmetic)
    } else {
        Ok(seconds as u32)
    }
}
fn durations(
    p: &Prepared,
    s: &Scenario,
    volumes: &BTreeMap<RoadKey, f64>,
    feedback: bool,
) -> Result<BTreeMap<RoadKey, u32>, Error> {
    p.roads
        .iter()
        .map(|(&(edge, i), &hourly)| {
            let capacity =
                checked(hourly * (s.periods[i].end - s.periods[i].start) as f64 / 3600.0)?;
            require(capacity > 0.0, "capacity underflow")?;
            let base = p.network.link(edge).ok_or(Error::Arithmetic)?.seconds;
            let seconds = if feedback {
                duration(
                    base,
                    *volumes.get(&(edge, i)).unwrap_or(&0.0),
                    capacity,
                    &s.settings,
                )?
            } else {
                base
            };
            Ok(((edge, i), seconds))
        })
        .collect()
}
fn score(
    choice: &mut Choice,
    tour: &demand::Tour,
    cost: &demand::CostModel,
    mode: TravelMode,
) -> Result<f64, Error> {
    let mut sum = 0.0;
    for (leg, original) in choice.legs.iter_mut().zip(&tour.legs) {
        let e = original
            .costs
            .get(&mode)
            .ok_or_else(|| Error::Invalid("unpriced feasible alternative".into()))?;
        let elapsed = leg
            .journey
            .arrival_utc
            .checked_sub(leg.journey.departure_utc)
            .ok_or(Error::Arithmetic)?;
        let value = checked(
            (elapsed - leg.waiting_seconds) as f64 * cost.non_wait_multiplier
                + leg.waiting_seconds as f64 * cost.wait_multiplier
                + e.reliability_seconds * cost.reliability_multiplier
                + e.expense_usd * cost.seconds_per_usd,
        )?;
        leg.generalized_seconds = Some(value);
        sum = checked(sum + value)?;
    }
    choice.generalized_seconds = Some(sum);
    Ok(sum)
}
fn replay(
    choice: &mut Choice,
    tour: &demand::Tour,
    s: &Scenario,
    times: &BTreeMap<RoadKey, u32>,
) -> Result<bool, Error> {
    for (i, (leg, original)) in choice.legs.iter_mut().zip(&tour.legs).enumerate() {
        let mut time = original.departure_utc;
        for step in &mut leg.journey.steps {
            step.departure_utc = time;
            let seconds = i64::from(
                *times
                    .get(&(step.edge, period(s, time)?))
                    .ok_or(Error::Arithmetic)?,
            );
            let remaining = original
                .deadline_utc
                .checked_sub(time)
                .ok_or(Error::Arithmetic)?;
            if seconds > remaining {
                return Ok(false);
            }
            time = time.checked_add(seconds).ok_or(Error::Arithmetic)?;
            step.arrival_utc = time;
            step.waiting_seconds = 0;
        }
        leg.journey.arrival_utc = time;
        leg.waiting_seconds = 0;
        if time > original.deadline_utc
            || tour.legs.get(i + 1).is_some_and(|next| {
                time.checked_add(i64::from(original.dwell_seconds))
                    .is_none_or(|t| t > next.departure_utc)
            })
        {
            return Ok(false);
        }
    }
    Ok(true)
}
fn shock(seed: u64, id: &str, mode: TravelMode, scale: f64) -> f64 {
    // Versioned FNV-1a + SplitMix64, independent of scheduling and std Hash randomization.
    let mut x = seed ^ 0xcbf29ce484222325;
    for b in id.bytes().chain([mode as u8]) {
        x = (x ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((x ^ (x >> 31)) >> 11) as f64 / ((1_u64 << 53) as f64) * scale
}
fn view_revision(n: &Network, s: &Scenario, times: &BTreeMap<RoadKey, u32>) -> String {
    // Non-cryptographic deterministic fingerprint; retain full inputs for replay.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in n
        .encode()
        .into_iter()
        .chain(s.id.bytes())
        .chain(
            s.periods
                .iter()
                .flat_map(|p| p.start.to_le_bytes().into_iter().chain(p.end.to_le_bytes())),
        )
        .chain(times.iter().flat_map(|(&(edge, i), &seconds)| {
            edge.0
                .to_le_bytes()
                .into_iter()
                .chain((i as u64).to_le_bytes())
                .chain(seconds.to_le_bytes())
        }))
    {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    format!("{}:e08:{hash:016x}", n.data().revision)
}
fn candidates(
    p: &Prepared,
    d: &Demand,
    s: &Scenario,
    times: &BTreeMap<RoadKey, u32>,
    effective_revision: &str,
) -> Result<Vec<BTreeMap<TravelMode, Choice>>, Error> {
    let mut result: Vec<BTreeMap<TravelMode, Choice>> = vec![BTreeMap::new(); d.tours.len()];
    // Per-period views plus a worst-period envelope produce a bounded legal
    // candidate set. The envelope also finds detours needed in several periods.
    for i in 0..=s.periods.len() {
        let mut data = p.network.data().clone();
        for link in &mut data.links {
            let seconds = if i == s.periods.len() {
                (0..i).filter_map(|j| times.get(&(link.id, j))).max()
            } else {
                times.get(&(link.id, i))
            };
            if let Some(seconds) = seconds {
                link.seconds = *seconds;
            }
        }
        data.revision = format!("{effective_revision}:candidate-{i}");
        let mut local_demand = d.clone();
        local_demand.network_revision = data.revision.clone();
        let network = Network::new(data).map_err(|e| Error::Invalid(e.to_string()))?;
        let report = demand::evaluate(
            &network,
            &local_demand,
            &s.demand_scenario,
            s.settings.max_states_per_query,
        )
        .map_err(Error::Demand)?;
        for (index, alternatives) in report.tours.into_iter().enumerate() {
            for (mode, candidate) in alternatives.choices {
                if i > 0 && mode != TravelMode::Car {
                    continue;
                }
                let Ok(mut choice) = candidate else {
                    continue;
                };
                if mode == TravelMode::Car && !replay(&mut choice, &d.tours[index], s, times)? {
                    continue;
                }
                let value = score(&mut choice, &d.tours[index], &s.demand_scenario.cost, mode)?;
                checked(
                    value
                        + shock(
                            s.settings.seed,
                            &d.tours[index].id,
                            mode,
                            s.settings.preference_seconds,
                        ),
                )?;
                for leg in &mut choice.legs {
                    leg.journey.graph_revision = effective_revision.into();
                }
                if result[index]
                    .get(&mode)
                    .is_none_or(|old| value < old.generalized_seconds.unwrap_or(f64::INFINITY))
                {
                    result[index].insert(mode, choice);
                }
            }
        }
    }
    // Missing capacities are errors for every feasible transit candidate, even unchosen ones.
    for choices in &result {
        if let Some(c) = choices.get(&TravelMode::Transit) {
            for leg in &c.legs {
                for step in &leg.journey.steps {
                    if let Some(run) = &step.run {
                        require(
                            p.transit.contains_key(&(step.edge, run.clone())),
                            "missing transit segment/run capacity",
                        )?;
                    }
                }
            }
        }
    }
    Ok(result)
}
fn execute(
    n: &Network,
    d: &Demand,
    s: &Scenario,
    feedback: bool,
    engine: &str,
) -> Result<Report, Error> {
    let p = prepare(n, d, s)?;
    let mut volumes = BTreeMap::new();
    let mut previous: Option<Vec<Assignment>> = None;
    let limit = if feedback {
        s.settings.max_iterations
    } else {
        1
    };
    for iteration in 1..=limit {
        let times = durations(&p, s, &volumes, feedback)?;
        let effective_revision = view_revision(&p.network, s, &times);
        let choices = candidates(&p, d, s, &times, &effective_revision)?;
        let mut next = BTreeMap::<RoadKey, f64>::new();
        let mut requested = BTreeMap::<TransitKey, f64>::new();
        let mut served = BTreeMap::<TransitKey, f64>::new();
        let mut report = Report {
            version: VERSION,
            scenario_id: s.id.clone(),
            network_revision: n.data().revision.clone(),
            effective_network_revision: effective_revision,
            demand_revision: d.revision.clone(),
            engine: engine.into(),
            seed: s.settings.seed,
            status: if feedback {
                Status::Nonconverged
            } else {
                Status::FixedCosts
            },
            iterations: iteration,
            volume_residual: 0.0,
            assignments: Vec::new(),
            roads: Vec::new(),
            transit: Vec::new(),
            mode_trip_weights: BTreeMap::new(),
            person_seconds: 0.0,
            unserved_trip_weight: 0.0,
            essential_trip_weight: 0.0,
            unserved_essential_weight: 0.0,
            essential_without_car_free_candidate: 0.0,
        };
        for (tour, alternatives) in d.tours.iter().zip(choices) {
            let chosen = alternatives.iter().min_by(|(ma, a), (mb, b)| {
                let cost = |m: TravelMode, c: &Choice| {
                    c.generalized_seconds.unwrap_or(f64::INFINITY)
                        + shock(s.settings.seed, &tour.id, m, s.settings.preference_seconds)
                };
                cost(**ma, a).total_cmp(&cost(**mb, b)).then(ma.cmp(mb))
            });
            let mode = chosen.map(|(&m, _)| m);
            let choice = chosen.map(|(_, c)| c.clone());
            let mut reservations = BTreeMap::<TransitKey, f64>::new();
            if mode == Some(TravelMode::Transit) {
                for (leg, route) in tour
                    .legs
                    .iter()
                    .zip(&choice.as_ref().ok_or(Error::Arithmetic)?.legs)
                {
                    let passengers = checked(
                        leg.expansion_weight
                            * s.settings.demand_multiplier
                            * (1.0 + f64::from(leg.load.companions)),
                    )?;
                    for step in &route.journey.steps {
                        if let Some(run) = &step.run {
                            let key = (step.edge, run.clone());
                            let value = reservations.entry(key).or_default();
                            *value = checked(*value + passengers)?;
                        }
                    }
                }
            }
            let mut denied = false;
            for (key, &count) in &reservations {
                let value = requested.entry(key.clone()).or_default();
                *value = checked(*value + count)?;
                if served.get(key).unwrap_or(&0.0) + count
                    > *p.transit.get(key).ok_or(Error::Arithmetic)?
                {
                    denied = true;
                }
            }
            if !denied {
                for (key, count) in reservations {
                    let value = served.entry(key).or_default();
                    *value = checked(*value + count)?;
                }
            }
            let car_free = alternatives.keys().any(|m| *m != TravelMode::Car);
            for (i, leg) in tour.legs.iter().enumerate() {
                let weight = checked(leg.expansion_weight * s.settings.demand_multiplier)?;
                if leg.essential {
                    report.essential_trip_weight = checked(report.essential_trip_weight + weight)?;
                    if !car_free {
                        report.essential_without_car_free_candidate =
                            checked(report.essential_without_car_free_candidate + weight)?;
                    }
                }
                if mode.is_none() || denied {
                    report.unserved_trip_weight = checked(report.unserved_trip_weight + weight)?;
                    if leg.essential {
                        report.unserved_essential_weight =
                            checked(report.unserved_essential_weight + weight)?;
                    }
                    continue;
                }
                let m = mode.ok_or(Error::Arithmetic)?;
                let value = report.mode_trip_weights.entry(m).or_default();
                *value = checked(*value + weight)?;
                let route = &choice.as_ref().ok_or(Error::Arithmetic)?.legs[i].journey;
                report.person_seconds = checked(
                    report.person_seconds
                        + weight
                            * (1.0 + f64::from(leg.load.companions))
                            * (route.arrival_utc - route.departure_utc) as f64,
                )?;
                if m == TravelMode::Car {
                    for step in &route.steps {
                        let value = next
                            .entry((step.edge, period(s, step.departure_utc)?))
                            .or_default();
                        *value = checked(*value + weight * s.settings.vehicles_per_trip)?;
                    }
                }
            }
            report.assignments.push(Assignment {
                tour_id: tour.id.clone(),
                mode,
                choice,
                denied_boarding: denied,
            });
        }
        let implied = durations(&p, s, &next, feedback)?;
        for (&(edge, i), &hourly) in &p.roads {
            let v = *next.get(&(edge, i)).unwrap_or(&0.0);
            let old = *volumes.get(&(edge, i)).unwrap_or(&0.0);
            report.volume_residual = report
                .volume_residual
                .max((v - old).abs() / v.max(old).max(1.0));
            report.roads.push(RoadResult {
                edge,
                period: i,
                vehicles: v,
                capacity_vehicles: hourly * (s.periods[i].end - s.periods[i].start) as f64 / 3600.0,
                used_seconds: times[&(edge, i)],
                implied_seconds: implied[&(edge, i)],
            });
        }
        for ((edge, run), &capacity) in &p.transit {
            let key = (*edge, run.clone());
            report.transit.push(TransitResult {
                edge: *edge,
                run: run.clone(),
                requested_passengers: *requested.get(&key).unwrap_or(&0.0),
                served_passengers: *served.get(&key).unwrap_or(&0.0),
                capacity_passengers: capacity,
            });
        }
        if feedback
            && report.volume_residual <= s.settings.tolerance
            && previous.as_ref() == Some(&report.assignments)
        {
            report.status = Status::Converged;
        }
        if !feedback || report.status == Status::Converged || iteration == limit {
            return Ok(report);
        }
        previous = Some(report.assignments);
        volumes = next;
    }
    Err(Error::Arithmetic)
}

#[derive(Debug)]
pub struct Batch {
    pub reports: Vec<Report>,
    pub workers_used: usize,
    pub elapsed_seconds: f64,
    pub scenarios_per_second: f64,
}
/// Complete ordered batch, no partial success on any invalid scenario or worker panic.
pub fn batch(
    adapter: &dyn Adapter,
    n: &Network,
    d: &Demand,
    scenarios: &[Scenario],
    workers: usize,
) -> Result<Batch, Error> {
    require(
        workers > 0 && workers <= 256 && !scenarios.is_empty(),
        "invalid batch/workers",
    )?;
    let used = workers.min(scenarios.len());
    let start = Instant::now();
    let reports = std::thread::scope(|scope| {
        let chunk = scenarios.len().div_ceil(used);
        let mut handles = Vec::new();
        let mut error = None;
        for part in scenarios.chunks(chunk) {
            match std::thread::Builder::new().spawn_scoped(scope, move || {
                part.iter()
                    .map(|s| adapter.evaluate(n, d, s))
                    .collect::<Result<Vec<_>, _>>()
            }) {
                Ok(handle) => handles.push(handle),
                Err(e) => {
                    error = Some(Error::WorkerSpawn(e.to_string()));
                    break;
                }
            }
        }
        let mut result = Vec::new();
        for handle in handles {
            match handle.join() {
                Ok(Ok(part)) => result.extend(part),
                Ok(Err(e)) => {
                    error.get_or_insert(e);
                }
                Err(_) => {
                    error.get_or_insert(Error::WorkerPanic);
                }
            }
        }
        error.map_or(Ok(result), Err)
    })?;
    let elapsed = start.elapsed().as_secs_f64();
    Ok(Batch {
        reports,
        workers_used: scenarios.len().div_ceil(scenarios.len().div_ceil(used)),
        elapsed_seconds: elapsed,
        scenarios_per_second: scenarios.len() as f64 / elapsed.max(f64::MIN_POSITIVE),
    })
}
#[derive(Debug, Clone, PartialEq)]
pub struct Parameters {
    pub label: String,
    pub alpha: f64,
    pub demand_multiplier: f64,
    pub preference_seconds: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Distribution {
    pub count: usize,
    pub mean: f64,
    pub sample_variance: Option<f64>,
    pub minimum: f64,
    pub p50: f64,
    pub p95: f64,
    pub maximum: f64,
}
#[derive(Debug)]
pub struct Sensitivity {
    pub runs: Batch,
    pub labels: Vec<(String, u64)>,
    pub nonconverged: usize,
    pub person_seconds: Distribution,
    pub unserved_essential_weight: Distribution,
}
fn distribution(mut values: Vec<f64>) -> Result<Distribution, Error> {
    values.sort_by(f64::total_cmp);
    let count = values.len();
    let mean = checked(values.iter().map(|x| x / count as f64).sum())?;
    let variance = if count > 1 {
        Some(checked(
            values
                .iter()
                .map(|x| (x - mean).powi(2) / (count - 1) as f64)
                .sum(),
        )?)
    } else {
        None
    };
    Ok(Distribution {
        count,
        mean,
        sample_variance: variance,
        minimum: values[0],
        p50: values[(count * 50).div_ceil(100) - 1],
        p95: values[(count * 95).div_ceil(100) - 1],
        maximum: values[count - 1],
    })
}
/// Parameter-major crossed grid; all capped runs retained and counted, errors fail the job.
pub fn sensitivity(
    adapter: &dyn Adapter,
    n: &Network,
    d: &Demand,
    template: &Scenario,
    seeds: &[u64],
    parameters: &[Parameters],
    workers: usize,
) -> Result<Sensitivity, Error> {
    require(
        !seeds.is_empty()
            && !parameters.is_empty()
            && seeds
                .len()
                .checked_mul(parameters.len())
                .is_some_and(|n| n <= 100_000)
            && seeds.iter().collect::<BTreeSet<_>>().len() == seeds.len()
            && parameters
                .iter()
                .map(|p| &p.label)
                .collect::<BTreeSet<_>>()
                .len()
                == parameters.len()
            && parameters.iter().all(|p| !p.label.trim().is_empty()),
        "invalid sensitivity grid",
    )?;
    let mut scenarios = Vec::new();
    let mut labels = Vec::new();
    for p in parameters {
        for &seed in seeds {
            let mut s = template.clone();
            s.settings.seed = seed;
            s.settings.alpha = p.alpha;
            s.settings.demand_multiplier = p.demand_multiplier;
            s.settings.preference_seconds = p.preference_seconds;
            scenarios.push(s);
            labels.push((p.label.clone(), seed));
        }
    }
    let runs = batch(adapter, n, d, &scenarios, workers)?;
    Ok(Sensitivity {
        nonconverged: runs
            .reports
            .iter()
            .filter(|r| r.status == Status::Nonconverged)
            .count(),
        person_seconds: distribution(runs.reports.iter().map(|r| r.person_seconds).collect())?,
        unserved_essential_weight: distribution(
            runs.reports
                .iter()
                .map(|r| r.unserved_essential_weight)
                .collect(),
        )?,
        runs,
        labels,
    })
}
