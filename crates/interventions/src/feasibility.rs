//! E12 conservative, in-memory pre-assignment screening. All engineering limits,
//! protected endpoints and cost/risk assumptions are explicit caller evidence.
//! This is neither engineering approval nor a calibrated impact prediction.
use crate::{
    evaluation,
    scenario::{self, Batch, History, Snapshot},
    transit,
};
use mobility_graph::{network::*, EdgeId, Mode};
use mobility_routing::{
    planner::{plan, Options, PlannedOutcome},
    Request,
};
use std::collections::{BTreeMap, BTreeSet};
mod phases;
pub use phases::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection(pub Vec<String>);
impl Rejection {
    pub fn contains(&self, text: &str) -> bool {
        self.0.iter().any(|s| s.contains(text))
    }
}
impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.join("; "))
    }
}
impl std::error::Error for Rejection {}
fn check(ok: bool, reason: impl Into<String>) -> Result<(), Rejection> {
    if ok {
        Ok(())
    } else {
        Err(Rejection(vec![reason.into()]))
    }
}
fn err(e: impl std::fmt::Display) -> Rejection {
    Rejection(vec![e.to_string()])
}
fn text(s: &str) -> bool {
    !s.trim().is_empty()
}
fn nonnegative(v: f64) -> bool {
    v.is_finite() && v >= 0.0
}
fn provenance(n: &Network, p: &Provenance) -> Result<(), Rejection> {
    scenario::validate_provenance(n, p, false).map_err(err)
}

/// Supplied parameter bounds, not automatically a confidence interval.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bounds {
    pub lower: f64,
    pub central: f64,
    pub upper: f64,
}
impl Bounds {
    fn valid(self) -> bool {
        nonnegative(self.lower)
            && self.lower <= self.central
            && self.central <= self.upper
            && self.upper.is_finite()
    }
    fn scaled(self, factor: f64) -> Result<Self, Rejection> {
        let b = Self {
            lower: self.lower * factor,
            central: self.central * factor,
            upper: self.upper * factor,
        };
        check(
            nonnegative(factor) && b.valid(),
            "invalid/overflowing cost or risk bounds",
        )?;
        Ok(b)
    }
    fn plus(self, b: Self) -> Result<Self, Rejection> {
        let v = Self {
            lower: self.lower + b.lower,
            central: self.central + b.central,
            upper: self.upper + b.upper,
        };
        check(v.valid(), "cost sum overflow")?;
        Ok(v)
    }
    fn point(v: f64) -> Self {
        Self {
            lower: v,
            central: v,
            upper: v,
        }
    }
}
/// Gross costs for the caller-declared project scope, not baseline deltas.
/// All money is major currency units in one real price year, unrounded f64.
#[derive(Debug, Clone, PartialEq)]
pub struct Budget {
    pub currency: String,
    pub price_year: u16,
    /// Explicit currency scaling (USD=100, JPY=1); no ISO registry inference.
    pub minor_units_per_major: u32,
    pub capital: f64,
    pub annual: f64,
    pub years: u16,
    pub real_discount_rate: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    Construction,
    Maintenance,
    Disruption,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Cost {
    pub id: String,
    pub category: Category,
    pub quantity: f64,
    pub unit: String,
    pub rate: Bounds,
    pub local_factor: f64,
    pub currency: String,
    pub price_year: u16,
    pub provenance: Provenance,
}
/// One row per actual trip template, including baseline services absent E11's
/// proposal ledger. Annualization is explicit, never inferred from a short day.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitCost {
    pub trip: String,
    pub operations: transit::Operations,
    pub coverage_repetitions_per_year: f64,
    pub annualization: String,
    pub local_factor: f64,
    pub price_year: u16,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LaneUse {
    General,
    Bike,
    Bus,
    Retired,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lane {
    pub kind: LaneUse,
    pub width_mm: u32,
    pub minimum_mm: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Restriction {
    Bridge,
    Slope,
    Utility,
    Terrain,
}
impl Restriction {
    pub const ALL: [Self; 4] = [Self::Bridge, Self::Slope, Self::Utility, Self::Terrain];
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    Clear,
    Restricted(String),
    Missing(String),
    NotApplicable(String),
}
/// Directional footprints are summed into a shared ROW group. The caller must
/// assess physical equivalence and avoid duplicating shared sidewalks/buffers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub id: String,
    pub edges: Vec<EdgeId>,
    pub road: Option<EdgeId>,
    pub row: String,
    pub lanes: Vec<Lane>,
    pub other_width_mm: u32,
    pub restrictions: BTreeMap<Restriction, Evidence>,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RightOfWay {
    pub id: String,
    pub available_mm: u32,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccessKind {
    Emergency,
    Disability,
    Delivery,
    Property,
    Freight,
    RegionalEssential,
}
impl AccessKind {
    pub const ALL: [Self; 6] = [
        Self::Emergency,
        Self::Disability,
        Self::Delivery,
        Self::Property,
        Self::Freight,
        Self::RegionalEssential,
    ];
}
/// Explicit directed outbound AND return samples. Property nodes are caller-
/// attested actual property access points, not inferred centroids. This registry
/// is the declared protected scope; samples cannot establish citywide continuity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessSample {
    pub id: String,
    pub kind: AccessKind,
    pub outbound: Request,
    pub returning: Request,
    pub permitted_car_edges: Option<BTreeSet<EdgeId>>,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Inputs {
    pub budget: Budget,
    pub costs: Vec<Cost>,
    pub transit: Vec<TransitCost>,
    pub segments: Vec<Segment>,
    pub rows: Vec<RightOfWay>,
    pub access: Vec<AccessSample>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct AccessResult {
    pub id: String,
    pub outbound_seconds: i64,
    pub return_seconds: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TransitAudit {
    pub input: TransitCost,
    pub covered: transit::OperatingEstimate,
    pub annual_major: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub construction: Bounds,
    pub disruption: Bounds,
    pub maintenance: Bounds,
    pub transit_annual: f64,
    pub capital: Bounds,
    pub annual: Bounds,
    pub lifecycle: Bounds,
    pub transit: Vec<TransitAudit>,
    pub access: Vec<AccessResult>,
}
/// Only screening can construct this immutable binding; caller-assigned IDs alone
/// never certify another snapshot, including an identically named history.
/// ```compile_fail
/// use interventions::feasibility::Screened;
/// fn tamper(mut s: Screened) { s.snapshot = todo!(); }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Screened {
    snapshot: Snapshot,
    inputs: Inputs,
    report: Report,
    interval: Option<(i64, i64)>,
}
impl Screened {
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn inputs(&self) -> &Inputs {
        &self.inputs
    }
    pub fn report(&self) -> &Report {
        &self.report
    }
    /// All hard gates already passed for this exact snapshot. Existing E08
    /// adapter still validates demand, passenger supply and simulation settings.
    pub fn evaluate(
        &self,
        engine: &impl mobility_sim::simulation::Adapter,
        demand: &mobility_sim::demand::Demand,
        input: &mobility_sim::simulation::Scenario,
        binding: &evaluation::DemandBinding,
    ) -> Result<evaluation::Evaluation, scenario::Error> {
        if let Some((start, end)) = self.interval {
            if demand.tours.iter().flat_map(|tour| &tour.legs).any(|leg| {
                leg.departure_utc < start
                    || leg.departure_utc >= end
                    || leg.deadline_utc > end
                    || leg.window.start < start
                    || leg.window.end > end
            }) {
                return Err(scenario::Error(
                    "demand outside screened construction phase interval".into(),
                ));
            }
        }
        evaluation::evaluate(&self.snapshot, engine, demand, input, binding)
    }
    pub fn risk(&self, input: &RiskInput) -> Result<RiskReport, Rejection> {
        if let Some((start, end)) = self.interval {
            check(
                input.period_start >= start && input.period_end <= end,
                "exposure period outside screened phase",
            )?;
        }
        risk(&self.snapshot, input)
    }
}

fn costs(s: &Snapshot, i: &Inputs) -> Result<Report, Rejection> {
    let b = &i.budget;
    check(
        b.currency.len() == 3
            && b.currency.bytes().all(|c| c.is_ascii_uppercase())
            && (1..=9999).contains(&b.price_year)
            && (1..=1_000_000).contains(&b.minor_units_per_major)
            && nonnegative(b.capital)
            && nonnegative(b.annual)
            && (1..=200).contains(&b.years)
            && nonnegative(b.real_discount_rate),
        "invalid budget/currency/price year/horizon/real discount",
    )?;
    let mut totals = BTreeMap::<Category, Bounds>::new();
    let mut ids = BTreeSet::new();
    for c in &i.costs {
        check(
            text(&c.id)
                && ids.insert(&c.id)
                && text(&c.unit)
                && nonnegative(c.quantity)
                && c.rate.valid()
                && c.local_factor.is_finite()
                && c.local_factor > 0.0,
            "invalid/duplicate unit cost assumptions",
        )?;
        check(
            c.currency == b.currency && c.price_year == b.price_year,
            "incompatible currency or price year",
        )?;
        provenance(&s.network, &c.provenance)?;
        let amount = c.rate.scaled(c.quantity)?.scaled(c.local_factor)?;
        let total = totals.entry(c.category).or_default();
        *total = total.plus(amount)?;
    }
    for category in [
        Category::Construction,
        Category::Maintenance,
        Category::Disruption,
    ] {
        check(
            totals.contains_key(&category),
            format!("missing explicit {category:?} cost category (zero must be explicit)"),
        )?;
    }
    let trips: BTreeSet<_> = s
        .network
        .data()
        .links
        .iter()
        .filter_map(|l| match &l.kind {
            LinkKind::Transit { trip, .. } => Some(trip.clone()),
            _ => None,
        })
        .collect();
    let mut seen = BTreeSet::new();
    let mut transit_annual = 0.0;
    let mut audit = vec![];
    for t in &i.transit {
        check(
            seen.insert(t.trip.clone()) && trips.contains(&t.trip),
            "duplicate/unknown transit cost coverage",
        )?;
        check(
            t.operations.currency == b.currency && t.price_year == b.price_year,
            "transit currency or price year mismatch",
        )?;
        check(
            t.coverage_repetitions_per_year.is_finite()
                && t.coverage_repetitions_per_year > 0.0
                && t.local_factor.is_finite()
                && t.local_factor > 0.0
                && text(&t.annualization),
            "invalid transit annualization/factor",
        )?;
        provenance(&s.network, &t.provenance)?;
        if let Some(record) = s.transit.get(&t.trip) {
            check(
                t.operations == record.operations,
                "transit operations differ from E11 candidate record",
            )?;
        }
        let covered = transit::estimate(&s.network, &t.trip, &t.operations).map_err(err)?;
        let annual = covered.operating_minor / f64::from(b.minor_units_per_major)
            * t.coverage_repetitions_per_year
            * t.local_factor;
        check(nonnegative(annual), "transit annual cost overflow")?;
        transit_annual += annual;
        audit.push(TransitAudit {
            input: t.clone(),
            covered,
            annual_major: annual,
        });
    }
    check(
        seen == trips,
        "missing transit cost coverage (including baseline patterns)",
    )?;
    let construction = totals[&Category::Construction];
    let maintenance = totals[&Category::Maintenance];
    let disruption = totals[&Category::Disruption];
    let capital = construction.plus(disruption)?;
    let annual = maintenance.plus(Bounds::point(transit_annual))?;
    check(
        capital.upper <= b.capital,
        format!(
            "capital upper {} exceeds budget {}",
            capital.upper, b.capital
        ),
    )?;
    check(
        annual.upper <= b.annual,
        format!("annual upper {} exceeds budget {}", annual.upper, b.annual),
    )?;
    let mut factor = 0.0;
    for year in 1..=b.years {
        factor += 1.0 / (1.0 + b.real_discount_rate).powi(i32::from(year));
    }
    let lifecycle = capital.plus(annual.scaled(factor)?)?;
    Ok(Report {
        construction,
        maintenance,
        disruption,
        transit_annual,
        capital,
        annual,
        lifecycle,
        transit: audit,
        access: vec![],
    })
}

fn physical(base: &Snapshot, s: &Snapshot, i: &Inputs) -> Result<(), Rejection> {
    let mut findings = vec![];
    let mut rows = BTreeMap::new();
    for r in &i.rows {
        provenance(&s.network, &r.provenance)?;
        check(
            text(&r.id)
                && r.available_mm > 0
                && rows.insert(r.id.clone(), (r.available_mm, 0u64)).is_none(),
            "invalid/duplicate right-of-way group",
        )?;
    }
    let mut ids = BTreeSet::new();
    let mut edges = BTreeSet::new();
    let mut roads = BTreeSet::new();
    for seg in &i.segments {
        provenance(&s.network, &seg.provenance)?;
        check(
            text(&seg.id) && ids.insert(&seg.id) && !seg.edges.is_empty(),
            "empty/duplicate segment evidence",
        )?;
        for e in &seg.edges {
            check(
                edges.insert(*e)
                    && (s.network.link(*e).is_some() || base.network.link(*e).is_some()),
                format!("segment {} duplicate/unknown edge {e:?}", seg.id),
            )?;
        }
        if let Some(road) = seg.road {
            let r = s
                .roads
                .get(&road)
                .ok_or_else(|| err(format!("{} unknown road owner", seg.id)))?;
            check(
                roads.insert(road) && seg.edges.contains(&road),
                format!("{} duplicate/missing road ownership", seg.id),
            )?;
            let count = |kind| seg.lanes.iter().filter(|l| l.kind == kind).count();
            if count(LaneUse::General) != usize::from(r.general_lanes)
                || count(LaneUse::Bike) != usize::from(r.bike_lanes)
                || count(LaneUse::Bus) != usize::from(r.bus_lanes)
                || count(LaneUse::Retired) != usize::from(r.retired_lanes)
            {
                findings.push(format!(
                    "segment {} lane inventory differs from candidate ledger",
                    seg.id
                ));
            }
            for (edge, owner) in &s.allocations {
                if *owner == road && !seg.edges.contains(edge) {
                    findings.push(format!("segment {} omits allocated edge {edge:?}", seg.id));
                }
            }
        }
        let mut width = u64::from(seg.other_width_mm);
        for l in &seg.lanes {
            if l.minimum_mm == 0 || l.width_mm < l.minimum_mm {
                findings.push(format!(
                    "segment {} {:?} width {} < {} mm",
                    seg.id, l.kind, l.width_mm, l.minimum_mm
                ));
            }
            width = width
                .checked_add(u64::from(l.width_mm))
                .ok_or_else(|| err("width overflow"))?;
        }
        check(
            width > 0,
            format!("segment {} missing physical footprint", seg.id),
        )?;
        let group = rows.get_mut(&seg.row).ok_or_else(|| {
            err(format!(
                "segment {} unknown right-of-way {}",
                seg.id, seg.row
            ))
        })?;
        group.1 = group
            .1
            .checked_add(width)
            .ok_or_else(|| err("ROW overflow"))?;
        for r in Restriction::ALL {
            match seg.restrictions.get(&r) {
                Some(Evidence::Clear) => {}
                Some(Evidence::NotApplicable(reason)) if text(reason) => {}
                Some(Evidence::Restricted(reason)) => {
                    findings.push(format!("segment {} {r:?} restricted: {reason}", seg.id))
                }
                Some(Evidence::Missing(reason)) => findings.push(format!(
                    "segment {} {r:?}: missing engineering evidence: {reason}",
                    seg.id
                )),
                _ => findings.push(format!(
                    "segment {} {r:?}: missing engineering evidence",
                    seg.id
                )),
            }
        }
    }
    for (id, (available, used)) in rows {
        if used == 0 {
            findings.push(format!("unused right-of-way group {id}"));
        }
        if used > u64::from(available) {
            findings.push(format!(
                "right-of-way {id}: proposed {used} > available {available} mm"
            ));
        }
    }
    for road in s.roads.keys() {
        if !roads.contains(road) {
            findings.push(format!("missing segment evidence for road {road:?}"));
        }
    }
    for l in base
        .network
        .data()
        .links
        .iter()
        .chain(s.network.data().links.iter())
    {
        if base.network.link(l.id) != s.network.link(l.id) && !edges.contains(&l.id) {
            findings.push(format!(
                "missing segment evidence for affected facility {:?}",
                l.id
            ));
        }
    }
    if findings.is_empty() {
        Ok(())
    } else {
        Err(Rejection(findings))
    }
}

fn access(
    s: &Snapshot,
    inputs: &[AccessSample],
    interval: Option<(i64, i64)>,
) -> Result<Vec<AccessResult>, Rejection> {
    let mut kinds = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut results = vec![];
    for a in inputs {
        check(
            text(&a.id) && ids.insert(&a.id),
            "invalid/duplicate protected OD ID",
        )?;
        provenance(&s.network, &a.provenance)?;
        kinds.insert(a.kind);
        check(
            a.outbound.from == a.returning.to
                && a.outbound.to == a.returning.from
                && a.outbound.from != a.outbound.to,
            format!(
                "{} requires explicit reversed property/critical endpoints",
                a.id
            ),
        )?;
        let mut durations = vec![];
        for (direction, r) in [("outbound", &a.outbound), ("return", &a.returning)] {
            let label = format!("{:?} {} {direction}", a.kind, a.id);
            let car = match a.kind {
                AccessKind::Disability => false,
                AccessKind::Property => r.initial_mode == Mode::Car,
                _ => true,
            };
            let mode = if car { Mode::Car } else { Mode::Walk };
            check(
                r.initial_mode == mode
                    && r.vehicle == car.then_some(Mode::Car)
                    && r.profile.allowed_modes == vec![mode]
                    && !r.profile.allow_unknown_access,
                format!("{label}: invalid protected mode/vehicle/unknown profile"),
            )?;
            if a.kind == AccessKind::Disability {
                check(
                    r.profile.step_free && r.profile.mobility_aid,
                    format!("{label}: disability requires step-free mobility-aid walking"),
                )?;
            }
            if let Some((start, end)) = interval {
                check(
                    r.departure_utc >= start && r.deadline_utc <= end,
                    format!("{label}: OD outside phase interval"),
                )?;
            }
            let filtered;
            let network = if car {
                let allowed = a
                    .permitted_car_edges
                    .as_ref()
                    .ok_or_else(|| err(format!("{label}: missing assessed vehicle permissions")))?;
                check(
                    !allowed.is_empty()
                        && allowed.iter().all(|id| {
                            s.network.link(*id).is_some_and(|l| {
                                matches!(
                                    l.kind,
                                    LinkKind::Street {
                                        mode: Mode::Car,
                                        ..
                                    }
                                )
                            })
                        }),
                    format!("{label}: invalid vehicle permission edge set"),
                )?;
                let mut d = s.network.data().clone();
                for l in &mut d.links {
                    if let LinkKind::Street {
                        mode: Mode::Car,
                        blocked,
                        ..
                    } = &mut l.kind
                    {
                        *blocked |= !allowed.contains(&l.id);
                    }
                }
                // Preserve every turn rule, especially 'only' rules targeting blocked links.
                filtered = Network::new(d).map_err(err)?;
                &filtered
            } else {
                &s.network
            };
            match plan(network, r, &Options::default())
                .map_err(|e| err(format!("{label}: routing error {e}")))?
            {
                PlannedOutcome::Journey(j) => {
                    durations.push(j.journey.arrival_utc - j.journey.departure_utc)
                }
                PlannedOutcome::Unmet(_) => {
                    return Err(err(format!(
                        "{label}: unmet designated critical/property OD"
                    )))
                }
            }
        }
        results.push(AccessResult {
            id: a.id.clone(),
            outbound_seconds: durations[0],
            return_seconds: durations[1],
        });
    }
    for kind in AccessKind::ALL {
        check(
            kinds.contains(&kind),
            format!("missing protected {kind:?} OD coverage"),
        )?;
    }
    Ok(results)
}
fn assess(
    base: &Snapshot,
    snapshot: Snapshot,
    i: &Inputs,
    interval: Option<(i64, i64)>,
) -> Result<Screened, Rejection> {
    // Cheap cost/engineering checks precede bounded routing; assignment is never invoked here.
    let mut report = costs(&snapshot, i)?;
    physical(base, &snapshot, i)?;
    report.access = access(&snapshot, &i.access, interval)?;
    Ok(Screened {
        snapshot,
        inputs: i.clone(),
        report,
        interval,
    })
}
pub fn screen(history: &History, batch: &Batch, inputs: &Inputs) -> Result<Screened, Rejection> {
    let snapshot = history.preview(batch).map_err(err)?;
    assess(history.current(), snapshot, inputs, None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExposureUnit {
    VehicleKm,
    PersonKm,
    Trips,
}
#[derive(Debug, Clone, PartialEq)]
pub struct RiskInput {
    pub exposure_scenario_id: String,
    pub exposure_graph_revision: String,
    pub model: String,
    pub version: String,
    pub mode: Mode,
    pub severity: String,
    pub unit: ExposureUnit,
    pub exposure: f64,
    pub exposure_source: Provenance,
    pub calibration: Option<Provenance>,
    pub applicability: String,
    pub period_start: i64,
    pub period_end: i64,
    pub rate: Bounds,
    pub uncertainty: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct RiskReport {
    pub scenario_id: String,
    pub graph_revision: String,
    pub basis: Basis,
    pub input: RiskInput,
    pub expected_collisions: Option<Bounds>,
    pub unavailable: Option<String>,
}
fn risk(s: &Snapshot, i: &RiskInput) -> Result<RiskReport, Rejection> {
    check(
        i.exposure_scenario_id == s.id && i.exposure_graph_revision == s.network.data().revision,
        "exposure scenario/revision does not match screened candidate",
    )?;
    check(
        text(&i.model)
            && text(&i.version)
            && text(&i.severity)
            && text(&i.applicability)
            && text(&i.uncertainty)
            && nonnegative(i.exposure)
            && i.rate.valid()
            && i.period_start < i.period_end
            && i.period_start >= s.network.data().coverage_start
            && i.period_end <= s.network.data().coverage_end,
        "invalid risk model/exposure/uncertainty/period",
    )?;
    check(
        i.unit != ExposureUnit::VehicleKm
            || matches!(i.mode, Mode::Car | Mode::Bus | Mode::Rail | Mode::LightRail),
        "vehicle-km exposure incompatible with non-motorized risk stratum",
    )?;
    provenance(&s.network, &i.exposure_source)?;
    check(
        i.exposure_source.geography == s.network.data().provenance.geography,
        "exposure geography incompatible with scenario",
    )?;
    let expected = if let Some(p) = &i.calibration {
        provenance(&s.network, p)?;
        check(
            p.geography == i.exposure_source.geography,
            "calibration/exposure geography mismatch; provide local calibration",
        )?;
        Some(i.rate.scaled(i.exposure)?)
    } else {
        None
    };
    Ok(RiskReport {
        scenario_id: s.id.clone(),
        graph_revision: s.network.data().revision.clone(),
        basis: Basis::Modeled,
        input: i.clone(),
        expected_collisions: expected,
        unavailable: i
            .calibration
            .is_none()
            .then(|| "missing calibration: risk estimate unavailable".into()),
    })
}
