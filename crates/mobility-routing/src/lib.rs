//! E05 bounded, deterministic earliest-arrival reference routing.
//! Explicit transfer edges govern mode changes; this is not a calibrated planner.
use mobility_graph::network::*;
use mobility_graph::{EdgeId, Mode, NodeId};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

pub mod planner;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub allowed_modes: Vec<Mode>,
    pub step_free: bool,
    pub mobility_aid: bool,
    /// Explicit opt-in; unknown infrastructure is excluded by default policy.
    pub allow_unknown_access: bool,
    pub protected_cycling_only: bool,
    /// Inclusive 1..=4 ordinal traffic-stress limit; missing assessments excluded.
    pub maximum_cycling_stress: Option<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub from: NodeId,
    pub to: NodeId,
    pub departure_utc: i64,
    pub deadline_utc: i64,
    pub initial_mode: Mode,
    /// At most one personal vehicle; initially with the traveler, not parked.
    pub vehicle: Option<Mode>,
    pub profile: Profile,
    /// Resource bound; exhaustion is an error, never an unmet-trip result.
    pub max_states: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub edge: EdgeId,
    pub mode: Mode,
    pub departure_utc: i64,
    pub arrival_utc: i64,
    pub waiting_seconds: i64,
    pub run: Option<Run>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Journey {
    pub departure_utc: i64,
    pub arrival_utc: i64,
    pub steps: Vec<Step>,
    pub graph_revision: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmetTrip {
    pub from: NodeId,
    pub to: NodeId,
    pub deadline_utc: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Journey(Journey),
    Unmet(UnmetTrip),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingError {
    InvalidRequest(String),
    StateLimit,
    Arithmetic,
}
impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RoutingError {}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Run {
    pub trip: String,
    pub service_date: u32,
    pub ordinal: u32,
    pub stop_index: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Vehicle {
    mode: Mode,
    parked: Option<NodeId>,
    road_incoming: Option<EdgeId>,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct State {
    time: i64,
    node: NodeId,
    mode: Mode,
    incoming: Option<EdgeId>,
    run: Option<Run>,
    /// Boarding facility must still be open at the actual vehicle departure.
    boarding_window: Option<(i64, i64)>,
    ridden_transit: bool,
    stop_index: usize,
    boardings: u32,
    vehicle: Option<Vehicle>,
}
struct Label {
    state: State,
    previous: Option<(usize, Option<Step>)>,
    penalty: u64,
    visits: Vec<planner::Visit>,
}
fn add(a: i64, b: u32) -> Result<i64, RoutingError> {
    a.checked_add(i64::from(b)).ok_or(RoutingError::Arithmetic)
}
fn accessible(a: Access, p: &Profile) -> bool {
    match a {
        Access::Accessible => true,
        Access::Steps => !p.step_free && !p.mobility_aid,
        Access::Impassable => false,
        Access::Unknown => p.allow_unknown_access,
    }
}
fn allowed(mode: Mode, p: &Profile) -> bool {
    p.allowed_modes.contains(&mode) && !(p.mobility_aid && is_bicycle(mode))
}
fn carrying_bike(state: &State) -> bool {
    state
        .vehicle
        .as_ref()
        .is_some_and(|v| is_bicycle(v.mode) && v.parked.is_none())
}
fn personal_transfer(
    vehicle: &mut Option<Vehicle>,
    action: TransferAction,
    from: Mode,
    to: Mode,
    node: NodeId,
) -> bool {
    let Some(v) = vehicle else {
        return false;
    };
    match action {
        TransferAction::Park if v.mode == from && v.parked.is_none() => {
            v.parked = Some(node);
            true
        }
        TransferAction::Retrieve if v.mode == to && v.parked == Some(node) => {
            v.parked = None;
            true
        }
        TransferAction::Mount => v.mode == to && v.parked.is_none(),
        TransferAction::Dismount => v.mode == from && v.parked.is_none(),
        _ => false,
    }
}
fn invalid(message: &str) -> RoutingError {
    RoutingError::InvalidRequest(message.into())
}

/// Reference time-expanded Dijkstra. Exact-time labels preserve all legal states;
/// `max_states` bounds memory. Later departures are retained (including overtaking).
/// Capacity is an input attribute, not a congestion/travel-time prediction.
pub fn earliest_arrival(network: &Network, request: &Request) -> Result<Outcome, RoutingError> {
    search(network, request, false, None)
}

/// Earliest arrival after at least one scheduled/frequency ride and explicit
/// alighting. Only walking-access transit is supported by this entry point.
/// Callers must still supply actual door endpoints, not substitute nearby stops.
pub fn earliest_transit_arrival(
    network: &Network,
    request: &Request,
) -> Result<Outcome, RoutingError> {
    if request.initial_mode != Mode::Walk || request.vehicle.is_some() {
        return Err(invalid(
            "transit-only choice requires walking access without a personal vehicle",
        ));
    }
    search(network, request, true, None)
}

/// Continue driving from a stop while retaining the car's last approach edge.
/// The edge must be an unblocked car link into the request origin.
pub fn earliest_car_arrival_with_approach(
    network: &Network,
    request: &Request,
    approach: EdgeId,
) -> Result<Outcome, RoutingError> {
    if request.initial_mode != Mode::Car
        || !network.link(approach).is_some_and(|link| {
            link.to == request.from
                && matches!(
                    link.kind,
                    LinkKind::Street {
                        mode: Mode::Car,
                        blocked: false,
                        ..
                    }
                )
        })
    {
        return Err(invalid(
            "car approach must be an open car link into the origin",
        ));
    }
    search(network, request, false, Some(approach))
}

fn search(
    network: &Network,
    request: &Request,
    require_transit: bool,
    car_approach: Option<EdgeId>,
) -> Result<Outcome, RoutingError> {
    search_options(
        network,
        request,
        require_transit,
        car_approach,
        &planner::Options::default(),
        false,
    )
    .map(|out| match out {
        planner::PlannedOutcome::Journey(j) => Outcome::Journey(j.journey),
        planner::PlannedOutcome::Unmet(u) => Outcome::Unmet(u),
    })
}

fn search_options(
    network: &Network,
    request: &Request,
    require_transit: bool,
    car_approach: Option<EdgeId>,
    options: &planner::Options,
    door: bool,
) -> Result<planner::PlannedOutcome, RoutingError> {
    let d = network.data();
    if network.node(request.from).is_none() || network.node(request.to).is_none() {
        return Err(invalid("unknown endpoint"));
    }
    if request.departure_utc < d.coverage_start
        || request.deadline_utc > d.coverage_end
        || request.departure_utc > request.deadline_utc
    {
        return Err(invalid(
            "query must be ordered and inside declared UTC coverage",
        ));
    }
    if request
        .profile
        .maximum_cycling_stress
        .is_some_and(|s| !(1..=4).contains(&s))
    {
        return Err(invalid("cycling stress limit must be 1..=4"));
    }
    if request.max_states == 0
        || !allowed(request.initial_mode, &request.profile)
        || is_transit(request.initial_mode)
    {
        return Err(invalid(
            "invalid state bound or initial mode; board transit explicitly",
        ));
    }
    if request.vehicle.is_some_and(|m| !is_vehicle(m))
        || (is_vehicle(request.initial_mode) && request.vehicle != Some(request.initial_mode))
        || (request.initial_mode == Mode::Walk && request.vehicle == Some(Mode::Car))
    {
        return Err(invalid("initial mode/vehicle possession mismatch"));
    }
    let initial = State {
        time: request.departure_utc,
        node: request.from,
        mode: request.initial_mode,
        incoming: None,
        run: None,
        boarding_window: None,
        ridden_transit: false,
        stop_index: 0,
        boardings: 0,
        vehicle: request.vehicle.map(|mode| Vehicle {
            mode,
            parked: None,
            road_incoming: car_approach,
        }),
    };
    let mut labels = vec![Label {
        state: initial.clone(),
        previous: None,
        penalty: 0,
        visits: Vec::new(),
    }];
    let mut seen = BTreeMap::from([(initial.clone(), (0u64, 0usize))]);
    let mut queue = BinaryHeap::from([Reverse((0u64, 0usize))]);
    let mut outgoing: BTreeMap<NodeId, Vec<&Link>> = BTreeMap::new();
    for link in &d.links {
        outgoing.entry(link.from).or_default().push(link);
    }
    while let Some(Reverse((_, index))) = queue.pop() {
        let state = labels[index].state.clone();
        if seen
            .get(&state)
            .is_some_and(|(_, current)| *current != index)
        {
            continue;
        }
        if state.time > request.deadline_utc {
            continue;
        }
        let node = network
            .node(state.node)
            .ok_or_else(|| invalid("missing graph node"))?;
        if !accessible(node.access, &request.profile) {
            continue;
        }
        if state.node == request.to
            && state.stop_index == options.stops.len()
            && (!door || !is_transit(state.mode))
            && (!require_transit || (state.ridden_transit && state.mode == Mode::Walk))
        {
            let mut steps = Vec::new();
            let mut visits = Vec::new();
            let mut cursor = index;
            loop {
                visits.extend(labels[cursor].visits.iter().rev().cloned());
                let Some((prev, step)) = &labels[cursor].previous else {
                    break;
                };
                if let Some(step) = step {
                    steps.push(step.clone());
                }
                cursor = *prev;
            }
            steps.reverse();
            visits.reverse();
            let journey = Journey {
                departure_utc: request.departure_utc,
                arrival_utc: state.time,
                steps,
                graph_revision: d.revision.clone(),
            };
            return Ok(planner::PlannedOutcome::Journey(planner::itinerary(
                network, journey, visits, options,
            )?));
        }
        // Activity is a separate transition: same-node parking/alighting may
        // precede it without losing possession, approach or transfer history.
        if let Some(stop) = options.stops.get(state.stop_index) {
            if stop.node == state.node
                && !is_transit(state.mode)
                && i64::from(stop.dwell_seconds) <= request.deadline_utc - state.time
            {
                let mut next = state.clone();
                next.time = add(state.time, stop.dwell_seconds)?;
                next.stop_index += 1;
                let penalty = labels[index].penalty;
                if !seen.get(&next).is_some_and(|(p, _)| *p <= penalty) {
                    if labels.len() >= request.max_states {
                        return Err(RoutingError::StateLimit);
                    }
                    let objective = u64::try_from(next.time - request.departure_utc)
                        .map_err(|_| RoutingError::Arithmetic)?
                        .checked_add(penalty)
                        .ok_or(RoutingError::Arithmetic)?;
                    let next_index = labels.len();
                    let visit = planner::Visit {
                        node: state.node,
                        arrival_utc: state.time,
                        departure_utc: next.time,
                    };
                    seen.insert(next.clone(), (penalty, next_index));
                    queue.push(Reverse((objective, next_index)));
                    labels.push(Label {
                        state: next,
                        previous: Some((index, None)),
                        penalty,
                        visits: vec![visit],
                    });
                }
            }
        }
        let Some(links) = outgoing.get(&state.node) else {
            continue;
        };
        for link in links {
            if !accessible(link.access, &request.profile)
                || !accessible(
                    network
                        .node(link.to)
                        .ok_or_else(|| invalid("missing graph node"))?
                        .access,
                    &request.profile,
                )
            {
                continue;
            }
            let mut emit = |next: State, departure: i64| -> Result<(), RoutingError> {
                let physical_arrival = next.time;
                let penalty = labels[index]
                    .penalty
                    .checked_add(options.comfort.penalty(link))
                    .ok_or(RoutingError::Arithmetic)?;
                if next.time > request.deadline_utc
                    || seen.get(&next).is_some_and(|(p, _)| *p <= penalty)
                {
                    return Ok(());
                }
                if labels.len() >= request.max_states {
                    return Err(RoutingError::StateLimit);
                }
                let objective = u64::try_from(
                    next.time
                        .checked_sub(request.departure_utc)
                        .ok_or(RoutingError::Arithmetic)?,
                )
                .map_err(|_| RoutingError::Arithmetic)?
                .checked_add(penalty)
                .ok_or(RoutingError::Arithmetic)?;
                let next_index = labels.len();
                let step = Step {
                    edge: link.id,
                    mode: next.mode,
                    departure_utc: departure,
                    arrival_utc: physical_arrival,
                    waiting_seconds: departure - state.time,
                    run: next.run.clone(),
                };
                seen.insert(next.clone(), (penalty, next_index));
                queue.push(Reverse((objective, next_index)));
                labels.push(Label {
                    state: next,
                    previous: Some((index, Some(step))),
                    penalty,
                    visits: Vec::new(),
                });
                Ok(())
            };
            match &link.kind {
                LinkKind::Street {
                    mode,
                    blocked,
                    cycling,
                    safety,
                    ..
                } => {
                    if *mode != state.mode || *blocked || !allowed(*mode, &request.profile) {
                        continue;
                    }
                    if is_bicycle(*mode)
                        && request.profile.protected_cycling_only
                        && *cycling != CycleFacility::Protected
                    {
                        continue;
                    }
                    if is_bicycle(*mode)
                        && request.profile.maximum_cycling_stress.is_some_and(|limit| {
                            safety.as_ref().is_none_or(|s| s.stress_level > limit)
                        })
                    {
                        continue;
                    }
                    if *mode == Mode::Car
                        && state
                            .vehicle
                            .as_ref()
                            .and_then(|v| v.road_incoming)
                            .is_some_and(|incoming| {
                                d.turns.iter().any(|t| {
                                    t.incoming == incoming
                                        && ((t.only && t.outgoing != link.id)
                                            || (!t.only && t.outgoing == link.id))
                                })
                            })
                    {
                        continue;
                    }
                    if i64::from(link.seconds) > request.deadline_utc - state.time {
                        continue;
                    }
                    let mut next = state.clone();
                    next.time = add(state.time, link.seconds)?;
                    if *mode == Mode::Car {
                        if let Some(vehicle) = &mut next.vehicle {
                            vehicle.road_incoming = Some(link.id);
                        }
                    }
                    next.node = link.to;
                    next.incoming = Some(link.id);
                    next.run = None;
                    emit(next, state.time)?;
                }
                LinkKind::Transfer {
                    from,
                    to,
                    action,
                    windows,
                    bicycle_allowed,
                } => {
                    if *from != state.mode
                        || !allowed(*to, &request.profile)
                        || (carrying_bike(&state) && !bicycle_allowed)
                    {
                        continue;
                    }
                    let mut next = state.clone();
                    if *action == TransferAction::Board {
                        if options
                            .max_boardings
                            .is_some_and(|max| state.boardings >= max)
                        {
                            continue;
                        }
                        // Only counts relevant to an explicit bound affect state space.
                        if options.max_boardings.is_some() {
                            next.boardings = state
                                .boardings
                                .checked_add(1)
                                .ok_or(RoutingError::Arithmetic)?;
                        }
                    }
                    let seconds = if *action == TransferAction::Board {
                        link.seconds.max(options.minimum_board_seconds)
                    } else {
                        link.seconds
                    };
                    let legal = match action {
                        TransferAction::Board => state.run.is_none(),
                        TransferAction::Alight => state.run.is_some(),
                        _ => personal_transfer(&mut next.vehicle, *action, *from, *to, state.node),
                    };
                    if !legal {
                        continue;
                    }
                    next.mode = *to;
                    next.incoming = Some(link.id);
                    next.run = None;
                    next.boarding_window = None;
                    // Alight immediately on arrival; do not wait aboard for a
                    // facility to open after this run may have left the stop.
                    for (start, end) in windows {
                        if *action == TransferAction::Alight && state.time < *start {
                            continue;
                        }
                        let depart = state.time.max(*start);
                        if depart >= *end || depart > request.deadline_utc {
                            continue;
                        }
                        // Compare remaining time before adding: irrelevant extreme
                        // windows must not overflow and abort another legal route.
                        if *end - depart < i64::from(seconds)
                            || request.deadline_utc - depart < i64::from(seconds)
                        {
                            continue;
                        }
                        let arrival = add(depart, seconds)?;
                        let mut n = next.clone();
                        n.time = arrival;
                        if *action == TransferAction::Board {
                            n.boarding_window = Some((*start, *end));
                        }
                        emit(n, depart)?;
                    }
                }
                LinkKind::Transit {
                    mode,
                    calendar,
                    trip,
                    stop_index,
                    timetable,
                    bicycle_allowed,
                } => {
                    if state.mode != *mode
                        || !allowed(*mode, &request.profile)
                        || (carrying_bike(&state) && !bicycle_allowed)
                    {
                        continue;
                    }
                    let cal = d
                        .calendars
                        .iter()
                        .find(|c| c.id == *calendar)
                        .ok_or_else(|| invalid("missing calendar"))?;
                    for day in &d.days {
                        if !cal.active(day) {
                            continue;
                        }
                        for event in timetable
                            .departures(link.seconds)
                            .map_err(|e| invalid(&e.0))?
                        {
                            let depart = add(day.origin_utc, event.depart)?;
                            let arrive = add(day.origin_utc, event.arrive)?;
                            if depart < state.time || arrive > request.deadline_utc {
                                continue;
                            }
                            if state.run.is_none()
                                && !state
                                    .boarding_window
                                    .is_some_and(|(start, end)| depart >= start && depart < end)
                            {
                                continue;
                            }
                            if let Some(run) = &state.run {
                                if run.trip != *trip
                                    || run.service_date != day.date
                                    || run.ordinal != event.run
                                    || run.stop_index.checked_add(1) != Some(*stop_index)
                                {
                                    continue;
                                }
                            }
                            let mut next = state.clone();
                            next.time = arrive;
                            next.node = link.to;
                            next.incoming = Some(link.id);
                            next.boarding_window = None;
                            next.ridden_transit = require_transit;
                            next.run = Some(Run {
                                trip: trip.clone(),
                                service_date: day.date,
                                ordinal: event.run,
                                stop_index: *stop_index,
                            });
                            emit(next, depart)?;
                        }
                    }
                }
            }
        }
    }
    Ok(planner::PlannedOutcome::Unmet(UnmetTrip {
        from: request.from,
        to: request.to,
        deadline_utc: request.deadline_utc,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn e05_s04_s06_legal_vehicle_retrieval_preserves_road_approach() {
        for mode in [Mode::Car, Mode::Bicycle, Mode::ElectricBicycle] {
            let mut vehicle = Some(Vehicle {
                mode,
                parked: None,
                road_incoming: Some(EdgeId(7)),
            });
            assert!(personal_transfer(
                &mut vehicle,
                TransferAction::Park,
                mode,
                Mode::Walk,
                NodeId(1)
            ));
            assert!(!personal_transfer(
                &mut vehicle,
                TransferAction::Retrieve,
                Mode::Walk,
                mode,
                NodeId(2)
            ));
            assert!(personal_transfer(
                &mut vehicle,
                TransferAction::Retrieve,
                Mode::Walk,
                mode,
                NodeId(1)
            ));
            assert_eq!(
                vehicle,
                Some(Vehicle {
                    mode,
                    parked: None,
                    road_incoming: Some(EdgeId(7))
                })
            );
        }
    }
}
