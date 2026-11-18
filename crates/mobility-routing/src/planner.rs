//! E07 opt-in door-node planning. All durations and comfort costs are seconds.
//! Inputs are caller-supplied, not inferred from geometry or observed behavior.
use crate::*;

/// Nonnegative modeled preference costs per traversed bicycle/e-bike edge.
/// Assessed stress contributes `(level - 1) * stress_seconds_per_level`.
/// Unknown assessment contributes `unknown_seconds`, never an inferred level.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Comfort {
    pub stress_seconds_per_level: u32,
    /// Applied to Shared and Unspecified facilities, not Protected.
    pub shared_seconds: u32,
    pub unknown_seconds: u32,
}
impl Comfort {
    pub(crate) fn penalty(&self, link: &Link) -> u64 {
        match &link.kind {
            LinkKind::Street {
                mode,
                cycling,
                safety,
                ..
            } if is_bicycle(*mode) => {
                let stress = safety
                    .as_ref()
                    .map_or(u64::from(self.unknown_seconds), |s| {
                        u64::from(s.stress_level - 1) * u64::from(self.stress_seconds_per_level)
                    });
                stress
                    + if *cycling == CycleFacility::Protected {
                        0
                    } else {
                        u64::from(self.shared_seconds)
                    }
            }
            _ => 0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stop {
    pub node: NodeId,
    pub dwell_seconds: u32,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub comfort: Comfort,
    pub stops: Vec<Stop>,
    /// Requires at least one ride and final walk mode, not merely nearby service.
    pub require_transit: bool,
    /// Total boardings over the whole journey (not per stop). None is unbounded.
    pub max_boardings: Option<u32>,
    /// Each Board edge takes max(structural seconds, this value).
    pub minimum_board_seconds: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub node: NodeId,
    pub arrival_utc: i64,
    pub departure_utc: i64,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Times {
    pub access_seconds: i64,
    pub wait_seconds: i64,
    pub ride_seconds: i64,
    pub transfer_seconds: i64,
    pub egress_seconds: i64,
    pub dwell_seconds: i64,
    /// Walking between the first and last non-walk street/transit movement.
    pub intermediate_walk_seconds: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Itinerary {
    pub journey: Journey,
    pub visits: Vec<Visit>,
    pub times: Times,
    pub comfort_penalty_seconds: u64,
    /// Inspectable edge preference costs; stress provenance remains in Network.
    pub edge_penalties: Vec<(EdgeId, u64)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedOutcome {
    Journey(Itinerary),
    Unmet(UnmetTrip),
}
/// Minimize elapsed + comfort cost, subject to physical deadlines, hard profile
/// restrictions, ordered dwell stops, and door arrival (never aboard transit).
pub fn plan(
    network: &Network,
    request: &Request,
    options: &Options,
) -> Result<PlannedOutcome, RoutingError> {
    if options.stops.len() > 1024 || options.stops.iter().any(|s| network.node(s.node).is_none()) {
        return Err(invalid("at most 1024 stops, all in network, are supported"));
    }
    if options.require_transit && (request.initial_mode != Mode::Walk || request.vehicle.is_some())
    {
        return Err(invalid(
            "transit choice requires walking access without a personal vehicle",
        ));
    }
    search_options(
        network,
        request,
        options.require_transit,
        None,
        options,
        true,
    )
}

pub(crate) fn itinerary(
    network: &Network,
    journey: Journey,
    visits: Vec<Visit>,
    options: &Options,
) -> Result<Itinerary, RoutingError> {
    let mut times = Times::default();
    let mut edge_penalties = Vec::new();
    let movement = |s: &Step| {
        network.link(s.edge).is_some_and(|l| {
            matches!(
                l.kind,
                LinkKind::Transit { .. }
                    | LinkKind::Street {
                        mode: Mode::Car | Mode::Bicycle | Mode::ElectricBicycle,
                        ..
                    }
            )
        })
    };
    let first = journey.steps.iter().position(movement);
    let last = journey.steps.iter().rposition(movement);
    let mut cost = 0u64;
    for (i, s) in journey.steps.iter().enumerate() {
        let l = network
            .link(s.edge)
            .ok_or_else(|| invalid("missing itinerary edge"))?;
        let duration = s
            .arrival_utc
            .checked_sub(s.departure_utc)
            .ok_or(RoutingError::Arithmetic)?;
        times.wait_seconds = times
            .wait_seconds
            .checked_add(s.waiting_seconds)
            .ok_or(RoutingError::Arithmetic)?;
        match l.kind {
            LinkKind::Transfer { .. } => times.transfer_seconds += duration,
            LinkKind::Street {
                mode: Mode::Walk, ..
            } => {
                if first.is_none_or(|f| i < f) {
                    times.access_seconds += duration;
                } else if last.is_some_and(|end| i > end) {
                    times.egress_seconds += duration;
                } else {
                    times.intermediate_walk_seconds += duration;
                }
            }
            _ => times.ride_seconds += duration,
        }
        let p = options.comfort.penalty(l);
        cost = cost.checked_add(p).ok_or(RoutingError::Arithmetic)?;
        edge_penalties.push((s.edge, p));
    }
    for v in &visits {
        times.dwell_seconds += v.departure_utc - v.arrival_utc;
    }
    Ok(Itinerary {
        journey,
        visits,
        times,
        comfort_penalty_seconds: cost,
        edge_penalties,
    })
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matrix {
    pub graph_revision: String,
    /// Caller-order OD cells; shape/row/column labels remain caller-owned.
    pub results: Vec<PlannedOutcome>,
}
/// At most 100,000 cells. Any invalid/resource-failed cell fails the job; it is
/// never converted to Unmet. No partial successful matrix is returned.
pub fn matrix(
    network: &Network,
    expected_revision: &str,
    requests: &[Request],
    options: &Options,
) -> Result<Matrix, RoutingError> {
    if expected_revision != network.data().revision || requests.len() > 100_000 {
        return Err(invalid(
            "matrix revision mismatch or more than 100000 cells",
        ));
    }
    let results = requests
        .iter()
        .map(|r| plan(network, r, options))
        .collect::<Result<_, _>>()?;
    Ok(Matrix {
        graph_revision: network.data().revision.clone(),
        results,
    })
}
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub graph_revision: String,
    pub departures_utc: Vec<i64>,
    pub feasible: usize,
    pub unmet: usize,
    /// No route within budget under this OD/profile, not absence of all service.
    pub no_route_fraction: f64,
    /// Nearest-rank percentiles, conditional on feasible sampled journeys.
    pub p50_seconds: Option<i64>,
    pub p95_seconds: Option<i64>,
}
/// Explicit, strictly increasing sample grid; same elapsed budget per departure.
/// Percentiles describe deterministic schedule variation, not confidence bounds.
pub fn departure_window(
    network: &Network,
    template: &Request,
    options: &Options,
    departures: &[i64],
    budget_seconds: u32,
) -> Result<Window, RoutingError> {
    if departures.is_empty()
        || departures.len() > 100_000
        || departures.windows(2).any(|w| w[0] >= w[1])
    {
        return Err(invalid(
            "window needs 1..=100000 strictly increasing departures",
        ));
    }
    let mut durations = Vec::new();
    let mut unmet = 0;
    for &departure in departures {
        let mut r = template.clone();
        r.departure_utc = departure;
        r.deadline_utc = add(departure, budget_seconds)?;
        match plan(network, &r, options)? {
            PlannedOutcome::Journey(j) => durations.push(j.journey.arrival_utc - departure),
            PlannedOutcome::Unmet(_) => unmet += 1,
        }
    }
    durations.sort_unstable();
    let percentile = |p: usize| {
        if durations.is_empty() {
            None
        } else {
            Some(durations[(durations.len() * p).div_ceil(100) - 1])
        }
    };
    Ok(Window {
        graph_revision: network.data().revision.clone(),
        departures_utc: departures.to_vec(),
        feasible: durations.len(),
        unmet,
        no_route_fraction: unmet as f64 / departures.len() as f64,
        p50_seconds: percentile(50),
        p95_seconds: percentile(95),
    })
}
