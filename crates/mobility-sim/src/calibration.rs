//! E09 deterministic, bounded calibration against caller-supplied evidence.
//! No observations or geographic crosswalks are inferred. See docs/CALIBRATION.md.
use crate::demand::{Demand, Purpose, TravelMode};
use crate::simulation::{self, Adapter, Parameters, Scenario, Settings, Status};
pub use mobility_graph::network::Provenance as SourceProvenance;
use mobility_graph::network::{Basis, LinkKind, Network, Provenance};
use mobility_graph::EdgeId;
use std::collections::{BTreeMap, BTreeSet};

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
fn require(ok: bool, message: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error(message.into()))
    }
}
fn number(x: f64) -> bool {
    x.is_finite() && x >= 0.0
}
fn checked(x: f64) -> Result<f64, Error> {
    if number(x) {
        Ok(x)
    } else {
        Err(Error("nonfinite calibration arithmetic".into()))
    }
}
fn text(s: &str) -> bool {
    !s.trim().is_empty()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Split {
    Fit,
    Holdout,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Metric {
    RoadVehicles,
    TransitBoardings,
    ModeShare,
    MeanCommuteSeconds,
}
impl Metric {
    pub fn unit(self) -> &'static str {
        match self {
            Self::RoadVehicles => "vehicles",
            Self::TransitBoardings => "person-boardings",
            Self::ModeShare => "fraction",
            Self::MeanCommuteSeconds => "seconds",
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A directed station cutline, NOT consecutive links along one road.
    Road {
        station: String,
        direction: String,
        edges: Vec<EdgeId>,
    },
    /// Explicit trip-to-route/service-area crosswalk. Boardings, not segment loads.
    Transit {
        route_or_area: String,
        trips: Vec<String>,
    },
    /// Expansion-weighted person trips, including companions; all eligible trips
    /// form the mode-share denominator, including unserved trips.
    Survey {
        area: String,
        groups: Vec<String>,
        purpose: Option<Purpose>,
        measure: SurveyMeasure,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub enum SurveyMeasure {
    ModeShare(TravelMode),
    MeanCommuteSeconds,
}
impl Target {
    pub fn metric(&self) -> Metric {
        match self {
            Self::Road { .. } => Metric::RoadVehicles,
            Self::Transit { .. } => Metric::TransitBoardings,
            Self::Survey {
                measure: SurveyMeasure::ModeShare(_),
                ..
            } => Metric::ModeShare,
            Self::Survey {
                measure: SurveyMeasure::MeanCommuteSeconds,
                ..
            } => Metric::MeanCommuteSeconds,
        }
    }
    pub fn location(&self) -> &str {
        match self {
            Self::Road { station, .. } => station,
            Self::Transit { route_or_area, .. } => route_or_area,
            Self::Survey { area, .. } => area,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub id: String,
    /// Stable original source record identity, independent of this row's ID.
    pub record: String,
    pub source: Provenance,
    pub split: Split,
    pub target: Target,
    /// Explicit, versioned spatial/cohort mapping provenance.
    pub crosswalk: Provenance,
    /// Bind reused edge, trip and group IDs to these exact input revisions.
    pub network_revision: String,
    pub demand_revision: String,
    /// Half-open UTC interval on the network epoch. No prorating or date matching.
    pub start: i64,
    pub end: i64,
    /// None is missing, never zero. A missing row requires a reason.
    pub value: Option<f64>,
    pub missing_reason: Option<String>,
    /// Positive declared error scale in metric units; no mixed-unit raw averaging.
    pub scale: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub project_id: String,
    pub observations: Vec<Observation>,
    pub candidates: Vec<Parameters>,
    /// Common, distinct seeds for every candidate; input order is the tie-break.
    pub seeds: Vec<u64>,
    /// Each listed family must have usable evidence in BOTH splits.
    pub required_metrics: BTreeSet<Metric>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub observation: String,
    pub modeled: Option<f64>,
    /// Model minus observed, in original units. Undefined when either is missing.
    pub residual: Option<f64>,
    pub normalized_error: Option<f64>,
    pub eligible_person_trips: Option<f64>,
    pub unserved_person_trips: Option<f64>,
    pub flags: Vec<String>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub expected: usize,
    pub compared: usize,
    /// None unless EVERY expected comparison is usable. Missingness cannot help fit.
    pub normalized_rmse: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub seed: u64,
    pub status: Option<Status>,
    pub effective_network_revision: Option<String>,
    pub error: Option<String>,
    pub comparisons: Vec<Comparison>,
    pub score: Score,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub parameters: Parameters,
    pub runs: Vec<Run>,
    pub score: Option<f64>,
    /// Population variance across the declared seeds, not a confidence interval.
    pub score_variance: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fitness {
    ChecksPassed,
    Limited,
}
/// Immutable result envelope: presentation/export share this fitness decision.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    request: Request,
    base_settings: Settings,
    scenario_snapshot: Scenario,
    network_source: Provenance,
    demand_source: Provenance,
    scenario_id: String,
    scenario_source: Provenance,
    network_revision: String,
    demand_revision: String,
    engine: String,
    candidates: Vec<Candidate>,
    selected: Option<usize>,
    holdout: Vec<Run>,
    fitness: Fitness,
    limitations: Vec<String>,
}
impl Report {
    pub fn request(&self) -> &Request {
        &self.request
    }
    pub fn base_settings(&self) -> &Settings {
        &self.base_settings
    }
    pub fn scenario_snapshot(&self) -> &Scenario {
        &self.scenario_snapshot
    }
    pub fn network_source(&self) -> &Provenance {
        &self.network_source
    }
    pub fn demand_source(&self) -> &Provenance {
        &self.demand_source
    }
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }
    pub fn scenario_source(&self) -> &Provenance {
        &self.scenario_source
    }
    pub fn network_revision(&self) -> &str {
        &self.network_revision
    }
    pub fn demand_revision(&self) -> &str {
        &self.demand_revision
    }
    pub fn engine(&self) -> &str {
        &self.engine
    }
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }
    pub fn holdout(&self) -> &[Run] {
        &self.holdout
    }
    pub fn fitness(&self) -> &Fitness {
        &self.fitness
    }
    pub fn limitations(&self) -> &[String] {
        &self.limitations
    }
    pub fn banner(&self) -> &'static str {
        match self.fitness {
            Fitness::ChecksPassed => "Declared calibration checks passed; not independent validation or policy certification.",
            Fitness::Limited => "LIMITED FITNESS: calibration evidence is failing, incomplete, synthetic, or unvalidated. Do not use for policy recommendations.",
        }
    }
}

fn provenance(p: &Provenance) -> bool {
    // Use the same Gregorian civil-date validation as the network contract.
    let parts: Vec<_> = p.date.split('-').collect();
    let date_ok = if parts.len() == 3
        && parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts.iter().all(|s| s.bytes().all(|b| b.is_ascii_digit()))
    {
        match (
            parts[0].parse::<u32>(),
            parts[1].parse::<u32>(),
            parts[2].parse::<u32>(),
        ) {
            (Ok(y), Ok(m), Ok(d)) if y > 0 && (1..=12).contains(&m) => {
                let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
                let days = [
                    31,
                    if leap { 29 } else { 28 },
                    31,
                    30,
                    31,
                    30,
                    31,
                    31,
                    30,
                    31,
                    30,
                    31,
                ];
                d > 0 && d <= days[m as usize - 1]
            }
            _ => false,
        }
    } else {
        false
    };
    date_ok
        && [
            &p.source,
            &p.source_version,
            &p.license,
            &p.geography,
            &p.uncertainty,
        ]
        .iter()
        .all(|s| text(s))
}
fn validate(r: &Request, s: &Scenario) -> Result<(), Error> {
    let p = &s.settings;
    require(
        number(p.alpha)
            && number(p.demand_multiplier)
            && p.demand_multiplier > 0.0
            && number(p.preference_seconds)
            && number(p.beta)
            && p.beta > 0.0
            && number(p.vehicles_per_trip)
            && p.vehicles_per_trip > 0.0
            && number(p.tolerance)
            && p.tolerance <= 1.0
            && p.max_iterations > 0
            && p.max_states_per_query > 0,
        "invalid base settings",
    )?;
    require(
        text(&r.project_id) && !r.observations.is_empty() && r.observations.len() <= 100_000,
        "project/evidence size",
    )?;
    require(
        !r.required_metrics.is_empty(),
        "required metrics must be declared",
    )?;
    require(
        !r.candidates.is_empty()
            && !r.seeds.is_empty()
            && r.candidates
                .len()
                .checked_mul(r.seeds.len())
                .is_some_and(|n| n <= 10_000),
        "candidate/seed grid size",
    )?;
    require(
        r.seeds.iter().collect::<BTreeSet<_>>().len() == r.seeds.len(),
        "duplicate seeds",
    )?;
    require(
        r.candidates.iter().all(|p| {
            text(&p.label)
                && number(p.alpha)
                && number(p.demand_multiplier)
                && p.demand_multiplier > 0.0
                && number(p.preference_seconds)
        }) && r
            .candidates
            .iter()
            .map(|p| &p.label)
            .collect::<BTreeSet<_>>()
            .len()
            == r.candidates.len(),
        "invalid candidate parameters/labels",
    )?;
    require(
        !s.periods.is_empty()
            && s.periods.iter().all(|p| p.start < p.end)
            && s.periods.windows(2).all(|p| p[0].end == p[1].start),
        "scenario periods must be contiguous",
    )?;
    let mut ids = BTreeSet::new();
    let mut source_splits = BTreeMap::new();
    let mut target_splits = BTreeMap::new();
    for o in &r.observations {
        require(
            text(&o.id) && ids.insert(&o.id) && text(&o.record),
            "duplicate/empty evidence identity",
        )?;
        require(
            provenance(&o.source)
                && provenance(&o.crosswalk)
                && text(&o.network_revision)
                && text(&o.demand_revision),
            "invalid evidence/crosswalk provenance",
        )?;
        require(
            o.start < o.end && (o.end as i128 - o.start as i128) <= i64::MAX as i128,
            "invalid observation interval",
        )?;
        require(
            o.scale.is_finite() && o.scale > 0.0 && o.value.is_none_or(number),
            "invalid target/scale",
        )?;
        require(
            match (&o.value, &o.missing_reason) {
                (None, Some(s)) => text(s),
                (Some(_), None) => true,
                _ => false,
            },
            "missing target needs reason; present target cannot have missing reason",
        )?;
        if o.target.metric() == Metric::ModeShare {
            require(o.value.is_none_or(|v| v <= 1.0), "mode share outside [0,1]")?;
        }
        let key = (&o.source.source, &o.record);
        require(
            source_splits
                .insert(key, o.split)
                .is_none_or(|old| old == o.split),
            "source record leaks across fit/holdout",
        )?;
        // Same mapped quantity/time cannot be split by changing row/source IDs.
        let key = (o.start, o.end, target_identity(&o.target));
        require(
            target_splits.insert(key, o.split).is_none(),
            "duplicate mapped evidence (including cross-split leakage)",
        )?;
        match &o.target {
            Target::Road {
                station,
                direction,
                edges,
            } => require(
                text(station)
                    && text(direction)
                    && !edges.is_empty()
                    && edges.iter().collect::<BTreeSet<_>>().len() == edges.len(),
                "invalid road crosswalk",
            )?,
            Target::Transit {
                route_or_area,
                trips,
            } => require(
                text(route_or_area)
                    && !trips.is_empty()
                    && trips.iter().all(|t| text(t))
                    && trips.iter().collect::<BTreeSet<_>>().len() == trips.len(),
                "invalid transit crosswalk",
            )?,
            Target::Survey {
                area,
                groups,
                purpose,
                measure,
            } => {
                require(
                    text(area)
                        && !groups.is_empty()
                        && groups.iter().all(|g| text(g))
                        && groups.iter().collect::<BTreeSet<_>>().len() == groups.len(),
                    "invalid survey cohort",
                )?;
                require(
                    !matches!(measure, SurveyMeasure::MeanCommuteSeconds)
                        || *purpose == Some(Purpose::Commute),
                    "commute target must explicitly select commute purpose",
                )?;
            }
        }
    }
    Ok(())
}
fn target_identity(t: &Target) -> String {
    let mut t = t.clone();
    match &mut t {
        Target::Road { edges, .. } => edges.sort_by_key(|e| e.0),
        Target::Transit { trips, .. } => trips.sort(),
        Target::Survey { groups, .. } => groups.sort(),
    }
    match t {
        Target::Road { edges, .. } => format!("road:{edges:?}"),
        Target::Transit { trips, .. } => format!("transit:{trips:?}"),
        Target::Survey {
            groups,
            purpose,
            measure,
            ..
        } => format!("survey:{groups:?}:{purpose:?}:{measure:?}"),
    }
}

/// A covered zero requires explicit supply/output cells for every mapped scheduled
/// segment/run departing in the interval, including runs unused by any traveler.
fn transit_coverage(
    o: &Observation,
    trips: &[String],
    n: &Network,
    s: &Scenario,
    report: &simulation::Report,
) -> Result<bool, Error> {
    for link in &n.data().links {
        let LinkKind::Transit {
            trip,
            stop_index,
            calendar,
            timetable,
            ..
        } = &link.kind
        else {
            continue;
        };
        if !trips.contains(trip) {
            continue;
        }
        for day in &n.data().days {
            if !n
                .data()
                .calendars
                .iter()
                .any(|c| c.id == *calendar && c.active(day))
            {
                continue;
            }
            for event in timetable
                .departures(link.seconds)
                .map_err(|e| Error(e.to_string()))?
            {
                let departure = day
                    .origin_utc
                    .checked_add(i64::from(event.depart))
                    .ok_or_else(|| Error("scheduled UTC overflow".into()))?;
                if departure < o.start || departure >= o.end {
                    continue;
                }
                let run = mobility_routing::Run {
                    trip: trip.clone(),
                    service_date: day.date,
                    ordinal: event.run,
                    stop_index: *stop_index,
                };
                if !s.transit.iter().any(|t| t.edge == link.id && t.run == run)
                    || report
                        .transit
                        .iter()
                        .filter(|t| t.edge == link.id && t.run == run)
                        .count()
                        != 1
                {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

fn compare(
    o: &Observation,
    n: &Network,
    d: &Demand,
    s: &Scenario,
    report: &simulation::Report,
) -> Result<Comparison, Error> {
    let mut c = Comparison {
        observation: o.id.clone(),
        modeled: None,
        residual: None,
        normalized_error: None,
        eligible_person_trips: None,
        unserved_person_trips: None,
        flags: vec![],
    };
    if let Some(reason) = &o.missing_reason {
        c.flags.push(format!("missing observation: {reason}"));
    }
    if o.network_revision != n.data().revision || o.demand_revision != d.revision {
        c.flags
            .push("stale crosswalk input revision binding".into());
        return Ok(c);
    }
    if o.start < s.periods[0].start
        || o.end
            > s.periods
                .last()
                .ok_or_else(|| Error("missing periods".into()))?
                .end
        || o.start < n.data().coverage_start
        || o.end as i128 > n.data().coverage_end as i128 + 1
    {
        c.flags.push("incomplete model time coverage".into());
        return Ok(c);
    }
    let prediction = match &o.target {
        Target::Road { edges, .. } => {
            let periods: Vec<_> = s
                .periods
                .iter()
                .enumerate()
                .filter(|(_, p)| p.start >= o.start && p.end <= o.end)
                .collect();
            if periods.first().is_none_or(|(_, p)| p.start != o.start)
                || periods.last().is_none_or(|(_, p)| p.end != o.end)
            {
                c.flags
                    .push("road interval does not match whole model entry periods".into());
                None
            } else if edges.iter().any(|edge| {
                !n.data().links.iter().any(|l| {
                    l.id == *edge
                        && matches!(
                            l.kind,
                            LinkKind::Street {
                                mode: mobility_graph::Mode::Car,
                                ..
                            }
                        )
                })
            }) {
                c.flags.push("missing directed road crosswalk".into());
                None
            } else {
                let mut sum = 0.0;
                let mut complete = true;
                for edge in edges {
                    for (period, _) in &periods {
                        let rows: Vec<_> = report
                            .roads
                            .iter()
                            .filter(|r| r.edge == *edge && r.period == *period)
                            .collect();
                        if rows.len() != 1 {
                            complete = false;
                        } else {
                            sum = checked(sum + rows[0].vehicles)?;
                        }
                    }
                }
                if complete {
                    Some(sum)
                } else {
                    c.flags.push("missing road model coverage".into());
                    None
                }
            }
        }
        Target::Transit { trips, .. } => {
            if trips.iter().any(|trip| {
                !n.data()
                    .links
                    .iter()
                    .any(|l| matches!(&l.kind, LinkKind::Transit { trip: t, .. } if t == trip))
            }) {
                c.flags.push("missing trip-to-route/area crosswalk".into());
                None
            } else if !transit_coverage(o, trips, n, s, report)? {
                c.flags
                    .push("incomplete scheduled transit model supply coverage".into());
                None
            } else {
                let mut total = 0.0;
                for (tour, a) in d.tours.iter().zip(&report.assignments) {
                    if a.denied_boarding || a.mode != Some(TravelMode::Transit) {
                        continue;
                    }
                    let Some(choice) = &a.choice else {
                        return Err(Error("served transit assignment lacks journey".into()));
                    };
                    for (leg, journey) in tour.legs.iter().zip(&choice.legs) {
                        let weight = checked(
                            leg.expansion_weight
                                * s.settings.demand_multiplier
                                * (1.0 + f64::from(leg.load.companions)),
                        )?;
                        let mut previous = None;
                        for step in &journey.journey.steps {
                            if let Some(run) = &step.run {
                                let key = (&run.trip, run.service_date, run.ordinal);
                                if previous != Some(key)
                                    && trips.contains(&run.trip)
                                    && step.departure_utc >= o.start
                                    && step.departure_utc < o.end
                                {
                                    total = checked(total + weight)?;
                                }
                                previous = Some(key);
                            } else {
                                previous = None;
                            }
                        }
                    }
                }
                Some(total)
            }
        }
        Target::Survey {
            groups,
            purpose,
            measure,
            ..
        } => {
            if o.start < s.demand_scenario.environment.coverage.start
                || o.end > s.demand_scenario.environment.coverage.end
            {
                c.flags
                    .push("incomplete survey demand-environment time coverage".into());
                None
            } else if groups
                .iter()
                .any(|g| !d.groups.iter().any(|group| &group.id == g))
            {
                c.flags.push("missing survey group crosswalk".into());
                None
            } else {
                let mut eligible = 0.0;
                let mut unserved = 0.0;
                let mut numerator = 0.0;
                let mut served = 0.0;
                for (tour, a) in d.tours.iter().zip(&report.assignments) {
                    if !groups.contains(&tour.group) {
                        continue;
                    }
                    for (index, leg) in tour.legs.iter().enumerate() {
                        // Cohort selected by requested departure, independent of fitted travel times.
                        if leg.departure_utc < o.start
                            || leg.departure_utc >= o.end
                            || purpose.is_some_and(|p| p != leg.purpose)
                        {
                            continue;
                        }
                        let weight = checked(
                            leg.expansion_weight
                                * s.settings.demand_multiplier
                                * (1.0 + f64::from(leg.load.companions)),
                        )?;
                        eligible = checked(eligible + weight)?;
                        if a.denied_boarding || a.mode.is_none() {
                            unserved = checked(unserved + weight)?;
                            continue;
                        }
                        served = checked(served + weight)?;
                        match measure {
                            SurveyMeasure::ModeShare(mode) => {
                                if a.mode == Some(*mode) {
                                    numerator = checked(numerator + weight)?;
                                }
                            }
                            SurveyMeasure::MeanCommuteSeconds => {
                                let journey = &a
                                    .choice
                                    .as_ref()
                                    .ok_or_else(|| Error("served assignment lacks journey".into()))?
                                    .legs[index]
                                    .journey;
                                let seconds = journey
                                    .arrival_utc
                                    .checked_sub(journey.departure_utc)
                                    .ok_or_else(|| Error("journey duration overflow".into()))?;
                                numerator = checked(numerator + weight * seconds as f64)?;
                            }
                        }
                    }
                }
                c.eligible_person_trips = Some(eligible);
                c.unserved_person_trips = Some(unserved);
                if unserved > 0.0 {
                    c.flags.push("survey contains unserved person trips; commute mean is conditional on service".into());
                }
                let denominator = if matches!(measure, SurveyMeasure::MeanCommuteSeconds) {
                    served
                } else {
                    eligible
                };
                if denominator > 0.0 {
                    Some(checked(numerator / denominator)?)
                } else {
                    c.flags.push("empty survey denominator".into());
                    None
                }
            }
        }
    };
    c.modeled = prediction;
    if let (Some(predicted), Some(observed)) = (prediction, o.value) {
        let residual = predicted - observed;
        require(residual.is_finite(), "residual overflow")?;
        c.residual = Some(residual);
        c.normalized_error = Some(checked(residual.abs() / o.scale)?);
    }
    Ok(c)
}
fn score(comparisons: &[Comparison]) -> Result<Score, Error> {
    let values: Vec<_> = comparisons
        .iter()
        .filter_map(|c| c.normalized_error.filter(|_| c.flags.is_empty()))
        .collect();
    let rmse = if !values.is_empty() && values.len() == comparisons.len() {
        let sum = values
            .iter()
            .try_fold(0.0, |sum, e| checked(sum + e * e / values.len() as f64))?;
        Some(sum.sqrt())
    } else {
        None
    };
    Ok(Score {
        expected: comparisons.len(),
        compared: values.len(),
        normalized_rmse: rmse,
    })
}
fn evaluate(
    adapter: &dyn Adapter,
    n: &Network,
    d: &Demand,
    template: &Scenario,
    p: &Parameters,
    seed: u64,
    rows: &[&Observation],
) -> Result<Run, Error> {
    let mut s = template.clone();
    s.settings.alpha = p.alpha;
    s.settings.demand_multiplier = p.demand_multiplier;
    s.settings.preference_seconds = p.preference_seconds;
    s.settings.seed = seed;
    let result = adapter.evaluate(n, d, &s);
    let failed = |message: String| Run {
        seed,
        status: None,
        effective_network_revision: None,
        error: Some(message),
        comparisons: vec![],
        score: Score {
            expected: rows.len(),
            compared: 0,
            normalized_rmse: None,
        },
    };
    let report = match result {
        Ok(r) => r,
        Err(e) => return Ok(failed(e.to_string())),
    };
    // Never silently zip truncated or reordered assignment rows from alternate adapters.
    if report.version != simulation::VERSION
        || report.seed != seed
        || report.scenario_id != s.id
        || report.network_revision != n.data().revision
        || report.demand_revision != d.revision
        || report.engine != adapter.name()
        || report.assignments.len() != d.tours.len()
        || !report.assignments.iter().zip(&d.tours).all(|(a, t)| {
            a.tour_id == t.id
                && a.choice
                    .as_ref()
                    .is_none_or(|c| c.legs.len() == t.legs.len())
                && (a.mode.is_none() || a.choice.is_some())
        })
    {
        return Ok(failed(
            "adapter output identity/assignment contract mismatch".into(),
        ));
    }
    let comparisons: Vec<_> = rows
        .iter()
        .map(|o| {
            compare(o, n, d, &s, &report).unwrap_or_else(|error| Comparison {
                observation: o.id.clone(),
                modeled: None,
                residual: None,
                normalized_error: None,
                eligible_person_trips: None,
                unserved_person_trips: None,
                flags: vec![format!("comparison calculation failed: {error}")],
            })
        })
        .collect();
    let (mut score, error) = match score(&comparisons) {
        Ok(score) => (score, None),
        Err(error) => (
            Score {
                expected: comparisons.len(),
                compared: comparisons
                    .iter()
                    .filter(|c| c.normalized_error.is_some() && c.flags.is_empty())
                    .count(),
                normalized_rmse: None,
            },
            Some(error.to_string()),
        ),
    };
    if report.status != Status::Converged {
        score.normalized_rmse = None;
    }
    Ok(Run {
        seed,
        status: Some(report.status),
        effective_network_revision: Some(report.effective_network_revision),
        error,
        comparisons,
        score,
    })
}

/// Ordered grid selection uses ONLY fit scores. The selected settings are then
/// independently replayed on the same declared seeds for separate holdout scores.
/// Adapter errors are retained, never dropped. Structural/arithmetic errors fail closed.
pub fn run(
    adapter: &dyn Adapter,
    n: &Network,
    d: &Demand,
    s: &Scenario,
    request: &Request,
) -> Result<Report, Error> {
    validate(request, s)?;
    let fit: Vec<_> = request
        .observations
        .iter()
        .filter(|o| o.split == Split::Fit)
        .collect();
    let holdout_rows: Vec<_> = request
        .observations
        .iter()
        .filter(|o| o.split == Split::Holdout)
        .collect();
    let mut candidates = vec![];
    for parameters in &request.candidates {
        let runs = request
            .seeds
            .iter()
            .map(|&seed| evaluate(adapter, n, d, s, parameters, seed, &fit))
            .collect::<Result<Vec<_>, _>>()?;
        let values: Option<Vec<_>> = runs.iter().map(|r| r.score.normalized_rmse).collect();
        let (mean, variance) = if let Some(values) = values {
            let mean = checked(values.iter().map(|v| v / values.len() as f64).sum())?;
            let variance = checked(
                values
                    .iter()
                    .map(|v| (v - mean).powi(2) / values.len() as f64)
                    .sum(),
            )?;
            (Some(mean), Some(variance))
        } else {
            (None, None)
        };
        candidates.push(Candidate {
            parameters: parameters.clone(),
            runs,
            score: mean,
            score_variance: variance,
        });
    }
    let selected = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, c)| c.score.map(|score| (i, score)))
        .min_by(|(i, a), (j, b)| a.total_cmp(b).then(i.cmp(j)))
        .map(|(i, _)| i);
    let holdout = if let Some(i) = selected {
        request
            .seeds
            .iter()
            .map(|&seed| {
                evaluate(
                    adapter,
                    n,
                    d,
                    s,
                    &candidates[i].parameters,
                    seed,
                    &holdout_rows,
                )
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![]
    };
    let mut limitations = vec![];
    if selected.is_none() {
        limitations.push("no candidate has complete, converged fitting evidence".into());
    }
    for metric in &request.required_metrics {
        for split in [Split::Fit, Split::Holdout] {
            if !request.observations.iter().any(|o| {
                o.split == split
                    && o.target.metric() == *metric
                    && o.value.is_some()
                    && o.source.basis == Basis::Observed
            }) {
                limitations.push(format!("missing observed {metric:?} evidence in {split:?}"));
            }
        }
    }
    if request
        .observations
        .iter()
        .any(|o| o.source.basis != Basis::Observed)
    {
        limitations.push(
            "synthetic, modeled or imputed targets are not empirical calibration evidence".into(),
        );
    }
    if n.data().provenance.basis != Basis::Observed
        || s.provenance.basis != Basis::Observed
        || d.provenance.basis != Basis::Observed
    {
        limitations.push("network/scenario includes modeled or imputed inputs; geographic and model assumptions require independent validation".into());
    }
    for (split, runs) in [
        (
            Split::Fit,
            selected
                .map(|i| candidates[i].runs.as_slice())
                .unwrap_or(&[]),
        ),
        (Split::Holdout, holdout.as_slice()),
    ] {
        if runs.is_empty() {
            limitations.push(format!("no usable {split:?} runs"));
        }
        for r in runs {
            if let Some(e) = &r.error {
                limitations.push(format!("{split:?} seed {} failed: {e}", r.seed));
            }
            if r.status != Some(Status::Converged) {
                limitations.push(format!("{split:?} seed {} is not converged", r.seed));
            }
            if r.score.normalized_rmse.is_none() {
                limitations.push(format!(
                    "{split:?} seed {} has incomplete evidence coverage",
                    r.seed
                ));
            }
            for c in &r.comparisons {
                for flag in &c.flags {
                    limitations.push(format!("{split:?} {}: {flag}", c.observation));
                }
                if c.normalized_error.is_some_and(|e| e > 1.0) {
                    limitations.push(format!(
                        "{split:?} {} exceeds its declared error scale",
                        c.observation
                    ));
                }
            }
        }
    }
    limitations.sort();
    limitations.dedup();
    Ok(Report {
        request: request.clone(),
        base_settings: s.settings.clone(),
        scenario_snapshot: s.clone(),
        network_source: n.data().provenance.clone(),
        demand_source: d.provenance.clone(),
        scenario_id: s.id.clone(),
        scenario_source: s.provenance.clone(),
        network_revision: n.data().revision.clone(),
        demand_revision: d.revision.clone(),
        engine: adapter.name().into(),
        candidates,
        selected,
        holdout,
        fitness: if limitations.is_empty() {
            Fitness::ChecksPassed
        } else {
            Fitness::Limited
        },
        limitations,
    })
}
