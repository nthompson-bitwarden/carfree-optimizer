//! E13-S01/S02/S03/S06/S10. Version 1, in-memory only; no implicit defaults.
use std::collections::{BTreeMap, BTreeSet};

pub const VERSION: u32 = 1;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Error {}
pub(crate) fn check(ok: bool, message: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error(message.into()))
    }
}
pub(crate) fn text(s: &str) -> bool {
    !s.trim().is_empty() && !s.chars().any(char::is_control)
}
pub(crate) fn nonnegative(x: f64) -> bool {
    x.is_finite() && x >= 0.0
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Horizon {
    ShortRun,
    LongRun,
}
/// All outputs retain this exact binding. Dates/units/source details are caller
/// evidence, never inferred from identifiers. Long-run demand must be rebuilt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub scenario: String,
    pub graph_revision: String,
    /// Exact canonical network bytes for cross-module binding; neutral manual
    /// fixtures may use a documented synthetic token, never for screened gates.
    pub graph_snapshot: Vec<u8>,
    pub demand_revision: String,
    pub destination_revision: String,
    pub service_revision: String,
    pub land_use_revision: Option<String>,
    pub geography_revision: String,
    pub period: String,
    pub horizon: Horizon,
    pub behavior: String,
    pub source: String,
    pub geography: String,
    pub date: String,
    pub limitations: Vec<String>,
}
impl Context {
    pub fn validate(&self) -> Result<(), Error> {
        check(
            !self.graph_snapshot.is_empty() && text(&self.service_revision),
            "graph snapshot and service binding required",
        )?;
        check([&self.scenario, &self.graph_revision, &self.demand_revision,
            &self.destination_revision, &self.geography_revision, &self.period,
            &self.behavior, &self.source, &self.geography, &self.date].iter().all(|s| text(s)),
            "context requires scenario/revisions, period, horizon behavior and dated geographic provenance")?;
        check(
            !self.limitations.is_empty() && self.limitations.iter().all(|s| text(s)),
            "explicit limitations required",
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    Walk,
    Bicycle,
    ElectricBicycle,
    Transit,
    Car,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stratum {
    pub origin: String,
    pub neighborhood: String,
    pub group: String,
    pub destination_type: String,
    /// Canonical unique, sorted car-free eligible modes.
    pub modes: Vec<Mode>,
    pub profile: String,
    /// Explicit sample grid, not a claim about unsampled departures.
    pub departures_utc: Vec<i64>,
    pub budget_seconds: u32,
}
impl Stratum {
    fn validate(&self) -> Result<(), Error> {
        check(
            [
                &self.origin,
                &self.neighborhood,
                &self.group,
                &self.destination_type,
                &self.profile,
            ]
            .iter()
            .all(|s| text(s))
                && !self.modes.is_empty()
                && !self.modes.contains(&Mode::Car)
                && self.modes.windows(2).all(|m| m[0] < m[1])
                && !self.departures_utc.is_empty()
                && self.departures_utc.windows(2).all(|d| d[0] < d[1]),
            "invalid stratum identifiers, car-free modes or departure grid",
        )
    }
}
/// Physical door-to-door seconds. Waiting and all walks remain in the total.
#[derive(Debug, Clone, PartialEq)]
pub struct Journey {
    pub access: u32,
    pub wait: u32,
    pub ride: u32,
    pub transfer: u32,
    pub intermediate_walk: u32,
    pub egress: u32,
    pub dwell: u32,
}
impl Journey {
    pub fn seconds(&self) -> u64 {
        [
            self.access,
            self.wait,
            self.ride,
            self.transfer,
            self.intermediate_walk,
            self.egress,
            self.dwell,
        ]
        .into_iter()
        .map(u64::from)
        .sum()
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum Evidence {
    Feasible(Journey),
    /// Known failure inside the declared model/budget, not missing data.
    Infeasible,
    Missing(String),
}
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub destination: String,
    pub departure_utc: i64,
    pub car_free: Evidence,
    pub car: Evidence,
}
#[derive(Debug, Clone, PartialEq)]
pub struct AccessInput {
    pub context: Context,
    pub stratum: Stratum,
    /// Full eligible destination inventory including missing evaluations.
    pub destinations: Vec<String>,
    /// Missing cells must be explicit; duplicate or omitted cells are errors.
    pub pairs: Vec<Pair>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Reliability {
    pub feasible: usize,
    pub p50_seconds: Option<u64>,
    pub p95_seconds: Option<u64>,
}
fn reliability(mut values: Vec<u64>) -> Reliability {
    values.sort_unstable();
    let n = values.len();
    Reliability {
        feasible: n,
        p50_seconds: (n > 0).then(|| values[n.div_ceil(2) - 1]),
        p95_seconds: (n > 0).then(|| values[(n * 95).div_ceil(100) - 1]),
    }
}
/// Validated, immutable score.
/// ```compile_fail
/// fn corrupt(mut score: accessibility::scoring::AccessScore) {
///     score.reachable_share = Some(f64::NAN);
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct AccessScore(AccessResult);
impl std::ops::Deref for AccessScore {
    type Target = AccessResult;
    fn deref(&self) -> &AccessResult {
        &self.0
    }
}
/// Read-only projection: AccessScore deliberately has no DerefMut.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessResult {
    pub destinations: Vec<String>,
    pub version: u32,
    pub context: Context,
    pub stratum: Stratum,
    pub eligible: usize,
    pub evaluated: usize,
    pub reached: usize,
    pub missing: usize,
    /// None until every eligible car-free cell is known; no renormalization.
    pub reachable_share: Option<f64>,
    pub car_free: Reliability,
    pub car: Reliability,
    /// both, car-free-only, car-only, neither; unknown pairs separate.
    pub paired_feasibility: [usize; 4],
    pub unknown_pairs: usize,
    /// Conditional on joint feasibility, seconds (car-free minus car).
    pub mean_time_difference_seconds: Option<f64>,
}
pub fn access(input: &AccessInput) -> Result<AccessScore, Error> {
    input.context.validate()?;
    input.stratum.validate()?;
    let dest: BTreeSet<_> = input.destinations.iter().collect();
    check(
        dest.len() == input.destinations.len() && dest.iter().all(|s| text(s)),
        "duplicate/empty destination",
    )?;
    let eligible = dest
        .len()
        .checked_mul(input.stratum.departures_utc.len())
        .ok_or_else(|| Error("cell count overflow".into()))?;
    check(
        input.pairs.len() == eligible && eligible <= 1_000_000,
        "provide all eligible destination/departure cells, at most 1000000",
    )?;
    let mut seen = BTreeSet::new();
    let (mut cf, mut car) = (Vec::new(), Vec::new());
    let (mut evaluated, mut reached, mut unknown) = (0, 0, 0);
    let mut paired = [0; 4];
    let mut differences = Vec::new();
    for p in &input.pairs {
        check(
            dest.contains(&p.destination)
                && input
                    .stratum
                    .departures_utc
                    .binary_search(&p.departure_utc)
                    .is_ok()
                && seen.insert((&p.destination, p.departure_utc)),
            "unknown/duplicate destination or departure",
        )?;
        for evidence in [&p.car_free, &p.car] {
            if let Evidence::Missing(reason) = evidence {
                check(text(reason), "missing evidence requires reason")?;
            }
            if let Evidence::Feasible(j) = evidence {
                check(
                    j.seconds() <= u64::from(input.stratum.budget_seconds),
                    "feasible journey exceeds declared budget",
                )?;
            }
        }
        let f = match &p.car_free {
            Evidence::Feasible(j) => {
                cf.push(j.seconds());
                evaluated += 1;
                reached += 1;
                Some(true)
            }
            Evidence::Infeasible => {
                evaluated += 1;
                Some(false)
            }
            Evidence::Missing(_) => None,
        };
        let c = match &p.car {
            Evidence::Feasible(j) => {
                car.push(j.seconds());
                Some(true)
            }
            Evidence::Infeasible => Some(false),
            Evidence::Missing(_) => None,
        };
        match (f, c) {
            (Some(true), Some(true)) => {
                paired[0] += 1;
                if let (Evidence::Feasible(a), Evidence::Feasible(b)) = (&p.car_free, &p.car) {
                    differences.push(a.seconds() as f64 - b.seconds() as f64);
                }
            }
            (Some(true), Some(false)) => paired[1] += 1,
            (Some(false), Some(true)) => paired[2] += 1,
            (Some(false), Some(false)) => paired[3] += 1,
            _ => unknown += 1,
        }
    }
    Ok(AccessScore(AccessResult {
        destinations: dest.into_iter().cloned().collect(),
        version: VERSION,
        context: input.context.clone(),
        stratum: input.stratum.clone(),
        eligible,
        evaluated,
        reached,
        missing: eligible - evaluated,
        reachable_share: if evaluated == eligible {
            crate::reachable_share(reached, eligible)
        } else {
            None
        },
        car_free: reliability(cf),
        car: reliability(car),
        paired_feasibility: paired,
        unknown_pairs: unknown,
        mean_time_difference_seconds: (!differences.is_empty())
            .then(|| differences.iter().sum::<f64>() / differences.len() as f64),
    }))
}
#[derive(Debug, Clone, PartialEq)]
pub struct Trip {
    pub id: String,
    pub group: String,
    pub neighborhood: String,
    pub weight: f64,
    pub essential: bool,
    /// Complete whole-tour feasibility under declared constraints, not proximity.
    pub car_free: Option<bool>,
    pub car: Option<bool>,
    /// Actual modeled assignment; None includes denied/unserved trips.
    pub served_mode: Option<Mode>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TripIdentity {
    pub id: String,
    pub group: String,
    pub neighborhood: String,
    pub expansion_weight: f64,
    pub essential: bool,
}
pub fn trip_inventory(rows: &[Trip]) -> Vec<TripIdentity> {
    let mut result: Vec<_> = rows
        .iter()
        .map(|t| TripIdentity {
            id: t.id.clone(),
            group: t.group.clone(),
            neighborhood: t.neighborhood.clone(),
            expansion_weight: t.weight,
            essential: t.essential,
        })
        .collect();
    result.sort_by(|a, b| a.id.cmp(&b.id));
    result
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Distribution {
    pub total: f64,
    pub car_required: f64,
    pub no_mode: f64,
    pub unknown: f64,
    pub served: BTreeMap<Mode, f64>,
    pub unserved: f64,
}
impl Distribution {
    fn add(&mut self, t: &Trip) -> Result<(), Error> {
        self.total += t.weight;
        match (t.car_free, t.car) {
            (Some(false), Some(true)) => self.car_required += t.weight,
            (Some(false), Some(false)) => self.no_mode += t.weight,
            (None, _) | (_, None) => self.unknown += t.weight,
            _ => (),
        }
        if let Some(m) = &t.served_mode {
            *self.served.entry(m.clone()).or_default() += t.weight;
        } else {
            self.unserved += t.weight;
        }
        check(self.total.is_finite(), "trip weights overflow")
    }
    pub fn car_required_share(&self) -> Option<f64> {
        (self.total > 0.0 && self.unknown == 0.0).then(|| self.car_required / self.total)
    }
}
/// Confidence intervals are NOT inferred from fractional survey weights or
/// deterministic modeled trips. Caller-supplied independent replicate estimates
/// may be summarized separately; scenario ranges must not be called confidence.
#[derive(Debug, Clone, PartialEq)]
pub enum Uncertainty {
    Unavailable {
        reason: String,
    },
    Confidence {
        lower: f64,
        upper: f64,
        level: f64,
        binding: IntervalBinding,
        method: String,
        sampling_unit: String,
        source: String,
    },
    Sensitivity {
        lower: f64,
        upper: f64,
        binding: IntervalBinding,
        method: String,
        source: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareMetric {
    CarRequiredAllTrips,
    ServedModeAllTrips(Mode),
}
#[derive(Debug, Clone, PartialEq)]
pub struct IntervalBinding {
    pub inventory: Vec<TripIdentity>,
    pub context: Context,
    pub metric: ShareMetric,
    pub denominator_trip_weight: f64,
    /// Versioned survey/replicate-design evidence, not an inferred sample size.
    pub design_revision: String,
}
impl Uncertainty {
    pub(crate) fn binding_matches(
        &self,
        context: &Context,
        metric: &ShareMetric,
        denominator: f64,
        inventory: &[TripIdentity],
    ) -> Result<(), Error> {
        let b = match self {
            Self::Unavailable { .. } => return Ok(()),
            Self::Confidence { binding, .. } | Self::Sensitivity { binding, .. } => binding,
        };
        check(
            b.context == *context
                && b.inventory == inventory
                && b.metric == *metric
                && b.denominator_trip_weight == denominator
                && text(&b.design_revision),
            "uncertainty metric/context/denominator/design mismatch",
        )
    }
    pub fn validate(&self, point: Option<f64>) -> Result<(), Error> {
        match self {
            Self::Unavailable { reason } => check(text(reason), "uncertainty reason required"),
            Self::Confidence {
                lower,
                upper,
                level,
                method,
                sampling_unit,
                source,
                ..
            } => {
                check(
                    level.is_finite() && *level > 0.0 && *level < 1.0 && text(sampling_unit),
                    "invalid confidence level/unit",
                )?;
                bounds(*lower, *upper, method, source, point)
            }
            Self::Sensitivity {
                lower,
                upper,
                method,
                source,
                ..
            } => bounds(*lower, *upper, method, source, point),
        }
    }
}
fn bounds(l: f64, u: f64, method: &str, source: &str, point: Option<f64>) -> Result<(), Error> {
    check(
        nonnegative(l)
            && nonnegative(u)
            && l <= u
            && u <= 1.0
            && text(method)
            && text(source)
            && point.is_some_and(|p| p >= l && p <= u),
        "invalid/unmatched share uncertainty bounds",
    )
}
/// ```compile_fail
/// fn corrupt(mut score: accessibility::scoring::TripScore) {
///     score.all.car_required = f64::NAN;
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TripScore(pub(crate) TripResult);
impl std::ops::Deref for TripScore {
    type Target = TripResult;
    fn deref(&self) -> &TripResult {
        &self.0
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct TripResult {
    pub demand_audit: Option<DemandAudit>,
    pub inventory: Vec<TripIdentity>,
    pub version: u32,
    pub context: Context,
    pub all: Distribution,
    pub essential: Distribution,
    /// Disjoint primary mobility cohorts. Overlapping tags must be evaluated
    /// separately and never added to this unduplicated total.
    pub by_group_neighborhood: BTreeMap<(String, String), Distribution>,
    pub car_required_uncertainty: Uncertainty,
    pub served_mode_uncertainty: BTreeMap<Mode, Uncertainty>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct DemandAudit {
    pub demand: mobility_sim::demand::Demand,
    pub binding: interventions::evaluation::DemandBinding,
    pub multiplier: f64,
}
pub fn trips(
    context: &Context,
    rows: &[Trip],
    uncertainty: Uncertainty,
) -> Result<TripScore, Error> {
    context.validate()?;
    let mut ids = BTreeSet::new();
    let mut result = TripResult {
        demand_audit: None,
        inventory: trip_inventory(rows),
        version: VERSION,
        context: context.clone(),
        all: Distribution::default(),
        essential: Distribution::default(),
        by_group_neighborhood: BTreeMap::new(),
        car_required_uncertainty: uncertainty,
        served_mode_uncertainty: [
            Mode::Walk,
            Mode::Bicycle,
            Mode::ElectricBicycle,
            Mode::Transit,
            Mode::Car,
        ]
        .into_iter()
        .map(|m| {
            (
                m,
                Uncertainty::Unavailable {
                    reason: "no matched sampling-design interval supplied".into(),
                },
            )
        })
        .collect(),
    };
    for t in rows {
        check(
            text(&t.id)
                && text(&t.group)
                && text(&t.neighborhood)
                && ids.insert(&t.id)
                && nonnegative(t.weight)
                && t.weight > 0.0,
            "invalid/duplicate trip or weight",
        )?;
        check(
            match &t.served_mode {
                Some(Mode::Car) => t.car == Some(true),
                Some(_) => t.car_free == Some(true),
                None => true,
            },
            "assignment contradicts feasibility",
        )?;
        result.all.add(t)?;
        if t.essential {
            result.essential.add(t)?;
        }
        result
            .by_group_neighborhood
            .entry((t.group.clone(), t.neighborhood.clone()))
            .or_default()
            .add(t)?;
    }
    result.car_required_uncertainty.binding_matches(
        context,
        &ShareMetric::CarRequiredAllTrips,
        result.all.total,
        &result.inventory,
    )?;
    result
        .car_required_uncertainty
        .validate(result.all.car_required_share())?;
    Ok(TripScore(result))
}
