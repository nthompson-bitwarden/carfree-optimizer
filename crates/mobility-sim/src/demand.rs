//! E06 demand and traveler diversity. See docs/TRAVEL-DEMAND.md for the v1 contract.
use mobility_graph::network::{Network, Provenance};
use mobility_graph::{Mode, NodeId};
use mobility_routing::{Journey, Outcome, Profile, Request, RoutingError};
use std::collections::{BTreeMap, BTreeSet};

mod population;
pub use population::*;

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Invalid(String),
    Routing {
        tour: String,
        leg: usize,
        mode: TravelMode,
        source: RoutingError,
    },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(detail) => f.write_str(detail),
            Self::Routing {
                tour,
                leg,
                mode,
                source,
            } => write!(f, "tour {tour}, leg {leg}, {mode:?}: {source}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Routing { source, .. } => Some(source),
            Self::Invalid(_) => None,
        }
    }
}
fn require(ok: bool, detail: impl FnOnce() -> String) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid(detail()))
    }
}
fn text(s: &str) -> bool {
    !s.trim().is_empty()
}
fn nonnegative(x: f64) -> bool {
    x.is_finite() && x >= 0.0
}
fn provenance(p: &Provenance) -> bool {
    let parts: Vec<_> = p.date.split('-').collect();
    if parts.len() != 3
        || parts
            .iter()
            .zip([4, 2, 2])
            .any(|(x, n)| x.len() != n || !x.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    let Ok(year) = parts[0].parse::<u32>() else {
        return false;
    };
    let Ok(month) = parts[1].parse::<u32>() else {
        return false;
    };
    let Ok(day) = parts[2].parse::<u32>() else {
        return false;
    };
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            28 + u32::from(
                year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)),
            )
        }
        _ => 0,
    };
    year > 0
        && (1..=days).contains(&day)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TravelMode {
    Walk,
    Bicycle,
    ElectricBicycle,
    Transit,
    Car,
}
impl TravelMode {
    pub const ALL: [Self; 5] = [
        Self::Walk,
        Self::Bicycle,
        Self::ElectricBicycle,
        Self::Transit,
        Self::Car,
    ];
    fn cycling(self) -> bool {
        matches!(self, Self::Bicycle | Self::ElectricBicycle)
    }
    fn routing_modes(self) -> (Mode, Vec<Mode>) {
        let mode = match self {
            Self::Walk | Self::Transit => Mode::Walk,
            Self::Bicycle => Mode::Bicycle,
            Self::ElectricBicycle => Mode::ElectricBicycle,
            Self::Car => Mode::Car,
        };
        (
            mode,
            if self == Self::Transit {
                vec![Mode::Walk, Mode::Bus, Mode::LightRail, Mode::Rail]
            } else {
                vec![mode]
            },
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Purpose {
    Commute,
    School,
    Shopping,
    Healthcare,
    Caregiving,
    Leisure,
    Other,
}

/// Half-open integer UTC-second interval on the network epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: i64,
    pub end: i64,
}
impl Window {
    fn contains(self, instant: i64) -> bool {
        self.start <= instant && instant < self.end
    }
    fn covers(self, window: Self) -> bool {
        self.start <= window.start && window.start < window.end && window.end <= self.end
    }
}

/// Overlapping descriptive categories; actual capabilities are always explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GroupTag {
    Noncyclist,
    WheelchairUser,
    Child,
    OlderAdult,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TravelerGroup {
    pub id: String,
    pub tags: BTreeSet<GroupTag>,
    pub step_free: bool,
    pub mobility_aid: bool,
    pub protected_cycling_only: bool,
    pub maximum_cycling_stress: Option<u8>,
    /// Modes present here are available to the traveler, including any required vehicle/operator.
    pub modes: BTreeMap<TravelMode, Load>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Load {
    pub cargo_grams: u32,
    pub companions: u16,
}
impl Load {
    fn fits(self, capacity: Self) -> bool {
        self.cargo_grams <= capacity.cargo_grams && self.companions <= capacity.companions
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Leg {
    pub from: NodeId,
    pub to: NodeId,
    pub window: Window,
    pub departure_utc: i64,
    /// Inclusive latest arrival at the supplied destination.
    pub deadline_utc: i64,
    pub purpose: Purpose,
    pub essential: bool,
    /// Expansion in trips represented by this leg; distinct from population.
    pub expansion_weight: f64,
    /// Load carried during this leg, including pickup/dropoff effects at prior stops.
    pub load: Load,
    /// Minimum activity before the next leg (zero for the final leg).
    pub dwell_seconds: u32,
    /// Missing evidence leaves a feasible alternative unpriced, not infeasible.
    pub costs: BTreeMap<TravelMode, CostEvidence>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Tour {
    pub id: String,
    pub home_zone: String,
    pub group: String,
    pub return_to_origin: bool,
    pub legs: Vec<Leg>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Demand {
    pub version: u32,
    pub revision: String,
    pub network_revision: String,
    pub provenance: Provenance,
    pub population: Population,
    pub groups: Vec<TravelerGroup>,
    pub tours: Vec<Tour>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CostEvidence {
    /// Caller-estimated reliability buffer in elapsed seconds, not inferred from the timetable.
    pub reliability_seconds: f64,
    pub expense_usd: f64,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CostModel {
    pub version: u32,
    pub revision: String,
    pub non_wait_multiplier: f64,
    pub wait_multiplier: f64,
    pub reliability_multiplier: f64,
    pub seconds_per_usd: f64,
    pub provenance: Provenance,
}
impl CostModel {
    fn valid(&self) -> bool {
        self.version == VERSION
            && text(&self.revision)
            && provenance(&self.provenance)
            && [
                self.non_wait_multiplier,
                self.wait_multiplier,
                self.reliability_multiplier,
                self.seconds_per_usd,
            ]
            .into_iter()
            .all(nonnegative)
    }
    fn score(&self, elapsed: i64, wait: i64, evidence: &CostEvidence) -> f64 {
        (elapsed - wait) as f64 * self.non_wait_multiplier
            + wait as f64 * self.wait_multiplier
            + evidence.reliability_seconds * self.reliability_multiplier
            + evidence.expense_usd * self.seconds_per_usd
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    pub label: String,
    pub weather: String,
    pub season: String,
    pub coverage: Window,
    /// Sorted, non-overlapping daylight intervals within coverage. Empty means all dark.
    pub daylight: Vec<Window>,
    /// Availability at departure, not a claim about conditions throughout the journey.
    pub light_modes: Vec<TravelMode>,
    pub dark_modes: Vec<TravelMode>,
    pub provenance: Provenance,
}
impl Environment {
    fn allows(&self, mode: TravelMode, departure: i64) -> bool {
        let light = self.daylight.partition_point(|w| w.end <= departure);
        let modes = if self
            .daylight
            .get(light)
            .is_some_and(|w| w.contains(departure))
        {
            &self.light_modes
        } else {
            &self.dark_modes
        };
        modes.contains(&mode)
    }
    fn valid(&self) -> bool {
        [&self.label, &self.weather, &self.season]
            .iter()
            .all(|s| text(s))
            && provenance(&self.provenance)
            && self.coverage.start < self.coverage.end
            && self.daylight.iter().all(|w| self.coverage.covers(*w))
            && self.daylight.windows(2).all(|ws| ws[0].end <= ws[1].start)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    pub environment: Environment,
    pub cost: CostModel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Excluded {
    Traveler,
    Environment { leg: usize },
    Load { leg: usize },
    ActivityWindow { leg: usize },
    NoJourney { leg: usize },
}
#[derive(Debug, Clone, PartialEq)]
pub struct LegChoice {
    pub journey: Journey,
    pub waiting_seconds: i64,
    pub generalized_seconds: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub legs: Vec<LegChoice>,
    pub generalized_seconds: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TourChoices {
    pub tour_id: String,
    pub choices: BTreeMap<TravelMode, Result<Choice, Excluded>>,
}
impl TourChoices {
    /// None when no alternative exists or any feasible alternative is unpriced.
    /// Ties use TravelMode::ALL order; no behavioral calibration is implied.
    pub fn lowest_cost_mode(&self) -> Option<TravelMode> {
        let mut best = None;
        for (&mode, outcome) in &self.choices {
            if let Ok(choice) = outcome {
                let cost = choice.generalized_seconds?;
                if best.is_none_or(|(_, prior)| cost < prior) {
                    best = Some((mode, cost));
                }
            }
        }
        best.map(|(mode, _)| mode)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EssentialWeights {
    pub total_weight: f64,
    pub no_car_free_weight: f64,
    pub no_mode_weight: f64,
}
impl EssentialWeights {
    /// Share of essential leg weight in tours lacking any complete car-free option.
    pub fn no_car_free_share(&self) -> Option<f64> {
        (self.total_weight > 0.0).then(|| self.no_car_free_weight / self.total_weight)
    }
    fn add(&mut self, weight: f64, car_free: bool, any: bool) -> Result<(), Error> {
        self.total_weight += weight;
        if !car_free {
            self.no_car_free_weight += weight;
        }
        if !any {
            self.no_mode_weight += weight;
        }
        require(self.total_weight.is_finite(), || {
            "essential leg weights overflow".into()
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub demand_revision: String,
    pub network_revision: String,
    pub environment_label: String,
    pub cost_revision: String,
    pub essential: EssentialWeights,
    pub essential_by_group: BTreeMap<String, EssentialWeights>,
    pub tours: Vec<TourChoices>,
}

/// Evaluates supplied departure instants, not every instant of each window. Data
/// and resource errors fail the run; they are never counted as unmet journeys.
pub fn evaluate(
    network: &Network,
    demand: &Demand,
    scenario: &Scenario,
    max_states_per_query: usize,
) -> Result<Report, Error> {
    let graph = network.data();
    require(
        demand.version == VERSION
            && text(&demand.revision)
            && demand.network_revision == graph.revision
            && provenance(&demand.provenance),
        || "invalid demand version, revision, graph binding or provenance".into(),
    )?;
    require(
        max_states_per_query > 0 && scenario.cost.valid() && scenario.environment.valid(),
        || "invalid routing state limit, cost model or environment profile".into(),
    )?;
    demand.population.validate()?;
    let mut groups = BTreeMap::new();
    for group in &demand.groups {
        require(
            text(&group.id)
                && groups.insert(group.id.as_str(), group).is_none()
                && group
                    .maximum_cycling_stress
                    .is_none_or(|s| (1..=4).contains(&s))
                && (!(group.tags.contains(&GroupTag::Noncyclist) || group.mobility_aid)
                    || !group.modes.keys().any(|mode| mode.cycling()))
                && (!group.tags.contains(&GroupTag::WheelchairUser)
                    || (group.step_free && group.mobility_aid)),
            || format!("invalid or duplicate traveler group {}", group.id),
        )?;
    }
    let mut strata = BTreeSet::new();
    for zone in &demand.population.zones {
        for stratum in &zone.strata {
            require(groups.contains_key(stratum.group.as_str()), || {
                format!(
                    "unknown traveler group {} in zone {}",
                    stratum.group, zone.id
                )
            })?;
            strata.insert((zone.id.as_str(), stratum.group.as_str()));
        }
    }
    let mut ids = BTreeSet::new();
    // Validate all rows before routing, even when every allowed mode is empty.
    for tour in &demand.tours {
        require(
            text(&tour.id)
                && ids.insert(&tour.id)
                && strata.contains(&(tour.home_zone.as_str(), tour.group.as_str())),
            || {
                format!(
                    "invalid tour ID, duplicate tour or unknown zone/group for {}",
                    tour.id
                )
            },
        )?;
        let Some(first) = tour.legs.first() else {
            return Err(Error::Invalid(format!("tour {} has no legs", tour.id)));
        };
        let last = tour.legs.last().unwrap_or(first);
        require(
            (!tour.return_to_origin || last.to == first.from) && last.dwell_seconds == 0,
            || {
                format!(
                    "tour {} missing declared return or has dwell after final leg",
                    tour.id
                )
            },
        )?;
        for (i, leg) in tour.legs.iter().enumerate() {
            require(
                leg.window.contains(leg.departure_utc)
                    && leg.window.start >= graph.coverage_start
                    && leg
                        .window
                        .end
                        .checked_sub(1)
                        .is_some_and(|end| end <= graph.coverage_end && end <= leg.deadline_utc)
                    && scenario.environment.coverage.covers(leg.window)
                    && leg.deadline_utc <= graph.coverage_end
                    && leg.from != leg.to
                    && network.node(leg.from).is_some()
                    && network.node(leg.to).is_some()
                    && nonnegative(leg.expansion_weight)
                    && leg.expansion_weight > 0.0
                    && leg.costs.values().all(|c| {
                        nonnegative(c.reliability_seconds)
                            && nonnegative(c.expense_usd)
                            && provenance(&c.provenance)
                    })
                    && (i == 0
                        || (tour.legs[i - 1].to == leg.from
                            && tour.legs[i - 1].departure_utc < leg.departure_utc)),
                || {
                    format!("tour {}, leg {i}: invalid window/deadline, endpoints, weight, cost evidence or chain continuity", tour.id)
                },
            )?;
        }
    }
    let mut report = Report {
        demand_revision: demand.revision.clone(),
        network_revision: graph.revision.clone(),
        environment_label: scenario.environment.label.clone(),
        cost_revision: scenario.cost.revision.clone(),
        essential: EssentialWeights::default(),
        essential_by_group: BTreeMap::new(),
        tours: Vec::with_capacity(demand.tours.len()),
    };
    for tour in &demand.tours {
        let mut choices = BTreeMap::new();
        for mode in TravelMode::ALL {
            choices.insert(
                mode,
                evaluate_mode(
                    network,
                    tour,
                    groups[tour.group.as_str()],
                    scenario,
                    max_states_per_query,
                    mode,
                )?,
            );
        }
        let any = choices.values().any(Result::is_ok);
        let car_free = choices
            .iter()
            .any(|(mode, result)| *mode != TravelMode::Car && result.is_ok());
        for leg in tour.legs.iter().filter(|leg| leg.essential) {
            report.essential.add(leg.expansion_weight, car_free, any)?;
            report
                .essential_by_group
                .entry(tour.group.clone())
                .or_default()
                .add(leg.expansion_weight, car_free, any)?;
        }
        report.tours.push(TourChoices {
            tour_id: tour.id.clone(),
            choices,
        });
    }
    Ok(report)
}

fn evaluate_mode(
    network: &Network,
    tour: &Tour,
    group: &TravelerGroup,
    scenario: &Scenario,
    max_states: usize,
    mode: TravelMode,
) -> Result<Result<Choice, Excluded>, Error> {
    let Some(&capacity) = group.modes.get(&mode) else {
        return Ok(Err(Excluded::Traveler));
    };
    let mut legs = Vec::with_capacity(tour.legs.len());
    let mut total = Some(0.0);
    let mut car_approach = None;
    for (i, leg) in tour.legs.iter().enumerate() {
        if !leg.load.fits(capacity) {
            return Ok(Err(Excluded::Load { leg: i }));
        }
        if !scenario.environment.allows(mode, leg.departure_utc) {
            return Ok(Err(Excluded::Environment { leg: i }));
        }
        let (initial_mode, allowed_modes) = mode.routing_modes();
        let request = Request {
            from: leg.from,
            to: leg.to,
            departure_utc: leg.departure_utc,
            deadline_utc: leg.deadline_utc,
            initial_mode,
            vehicle: (initial_mode != Mode::Walk).then_some(initial_mode),
            max_states,
            profile: Profile {
                allowed_modes,
                step_free: group.step_free,
                mobility_aid: group.mobility_aid,
                allow_unknown_access: false,
                protected_cycling_only: group.protected_cycling_only,
                maximum_cycling_stress: group.maximum_cycling_stress,
            },
        };
        let route = match (mode, car_approach) {
            (TravelMode::Transit, _) => {
                mobility_routing::earliest_transit_arrival(network, &request)
            }
            (TravelMode::Car, Some(approach)) => {
                mobility_routing::earliest_car_arrival_with_approach(network, &request, approach)
            }
            _ => mobility_routing::earliest_arrival(network, &request),
        };
        let outcome = route.map_err(|source| Error::Routing {
            tour: tour.id.clone(),
            leg: i,
            mode,
            source,
        })?;
        let Outcome::Journey(journey) = outcome else {
            return Ok(Err(Excluded::NoJourney { leg: i }));
        };
        if mode == TravelMode::Car {
            car_approach = journey.steps.last().map(|step| step.edge);
        }
        if let Some(next) = tour.legs.get(i + 1) {
            let dwell = i64::from(leg.dwell_seconds);
            if next.departure_utc - journey.arrival_utc < dwell {
                return Ok(Err(Excluded::ActivityWindow { leg: i + 1 }));
            }
        }
        let waiting_seconds = journey.steps.iter().map(|step| step.waiting_seconds).sum();
        let generalized_seconds = leg.costs.get(&mode).map(|e| {
            scenario
                .cost
                .score(journey.arrival_utc - leg.departure_utc, waiting_seconds, e)
        });
        require(generalized_seconds.is_none_or(nonnegative), || {
            format!(
                "tour {}, leg {i}, {mode:?}: generalized cost overflow",
                tour.id
            )
        })?;
        total = total.zip(generalized_seconds).map(|(a, b)| a + b);
        require(total.is_none_or(nonnegative), || {
            format!(
                "tour {}, {mode:?}: total generalized cost overflow",
                tour.id
            )
        })?;
        legs.push(LegChoice {
            journey,
            waiting_seconds,
            generalized_seconds,
        });
    }
    Ok(Ok(Choice {
        legs,
        generalized_seconds: total,
    }))
}
