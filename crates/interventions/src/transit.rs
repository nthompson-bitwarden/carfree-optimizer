//! E11 in-memory transit proposal contract v1. Inputs are explicit modeled
//! assumptions, not approved operations or calibrated impact predictions.
use crate::scenario::{validate_provenance, CrossingAssessment, Error, SnapshotData};
use mobility_graph::network::*;
use mobility_graph::{EdgeId, Mode};
use mobility_routing::{
    planner::{plan, Options, PlannedOutcome},
    Outcome, Request,
};
use std::collections::{BTreeMap, BTreeSet};

pub const VERSION: u32 = 1;
fn require(ok: bool, message: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error(message.into()))
    }
}

/// Complete directed trip-template replacement. Short turns and reverse service
/// have distinct trip IDs. Every ride must use this calendar and trip ID.
#[derive(Debug, Clone, PartialEq)]
pub struct Service {
    pub trip: String,
    pub calendar: Calendar,
    pub rides: Vec<Link>,
    pub operations: Operations,
}
/// All rates are modeled, nonnegative currency minor units (e.g. USD cents).
/// Staffing and overhead are explicit, including explicit zeros when appropriate.
/// No interlining/return movement is inferred; provide reverse patterns separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operations {
    pub currency: String,
    pub turnaround_seconds: u32,
    pub deadhead_seconds_per_run: u32,
    pub deadhead_metres_per_run: u32,
    pub operators_per_vehicle: u16,
    pub vehicle_minor_per_hour: u32,
    pub operator_minor_per_hour: u32,
    pub minor_per_km: u32,
    /// Bound template departures and examined date/run instances (including those
    /// outside coverage on overlapping days); exhaustion is an error.
    pub max_run_instances: usize,
}
#[derive(Debug, Clone, PartialEq)]
pub struct OperatingEstimate {
    pub graph_revision: String,
    pub runs: usize,
    /// Per-pattern peak simultaneous duties; not a shared network fleet optimum.
    pub peak_vehicles: usize,
    pub peak_operators: usize,
    pub vehicle_seconds: u64,
    pub operator_seconds: u64,
    pub vehicle_metres: f64,
    /// Unrounded modeled minor currency units; includes all three explicit rates.
    pub operating_minor: f64,
    pub currency: String,
    pub coverage_start_utc: i64,
    pub coverage_end_utc: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub operations: Operations,
    pub estimate: OperatingEstimate,
    pub provenance: Provenance,
}
/// Add-only station topology; existing transfer links can be replaced to change
/// hours/minimum connection times without changing their action or endpoints.
/// Street connectors are explicitly off-road additions, never lane conversions.
#[derive(Debug, Clone, PartialEq)]
pub struct Station {
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub assessments: BTreeMap<EdgeId, CrossingAssessment>,
}

/// Reject competing writes before staging; a service plus an identical lane
/// allocation/association is the sole intentional shared edge write.
pub(crate) fn validate_batch(
    network: &Network,
    changes: &[crate::scenario::Change],
) -> Result<(), Error> {
    use crate::scenario::Change;
    let mut owners = BTreeMap::new();
    let mut rides = BTreeMap::new();
    let mut stations = BTreeSet::new();
    for change in changes {
        match change {
            Change::Transit(service) => {
                for link in network.data().links.iter().filter(|l| matches!(&l.kind, LinkKind::Transit { trip, .. } if trip == &service.trip)).chain(service.rides.iter()) {
                    if let Some(owner) = owners.insert(link.id, &service.trip) {
                        require(owner == &service.trip, "conflicting service edge ownership in batch")?;
                    }
                }
                for link in &service.rides {
                    rides.insert(link.id, link);
                }
            }
            Change::Station(station) => {
                for link in &station.links {
                    require(
                        stations.insert(link.id),
                        "duplicate station target in batch",
                    )?;
                }
            }
            _ => {}
        }
    }
    require(
        stations.iter().all(|id| !owners.contains_key(id)),
        "station/service target conflict",
    )?;
    for change in changes {
        let (id, allowed) = match change {
            Change::Allocate { target, .. } => (
                target.id,
                rides.get(&target.id).is_some_and(|ride| **ride == *target),
            ),
            Change::AssociateBus { link, .. } => (
                link.id,
                rides.get(&link.id).is_some_and(|ride| **ride == *link),
            ),
            Change::BusTiming { link } | Change::Pedestrian { link, .. } => (link.id, false),
            Change::Crossing { edge, .. } | Change::CloseRoad { edge } => (*edge, false),
            _ => continue,
        };
        require(
            !stations.contains(&id) && (!owners.contains_key(&id) || allowed),
            "conflicting effective edge writes in batch",
        )?;
    }
    Ok(())
}

pub(crate) fn service(
    s: &mut SnapshotData,
    d: &mut NetworkData,
    proposal: &Service,
    provenance: &Provenance,
) -> Result<(), Error> {
    require(
        !proposal.trip.trim().is_empty() && !proposal.rides.is_empty(),
        "empty service trip/rides",
    )?;
    let mut ids = BTreeSet::new();
    for l in &proposal.rides {
        require(ids.insert(l.id), "duplicate service edge")?;
        require(
            matches!(&l.kind, LinkKind::Transit { trip, calendar, .. }
            if trip == &proposal.trip && *calendar == proposal.calendar.id),
            "service ride trip/calendar mismatch",
        )?;
        require(
            d.nodes.iter().any(|n| n.id == l.from) && d.nodes.iter().any(|n| n.id == l.to),
            "unknown service stop",
        )?;
        if let Some(old) = d.links.iter().find(|old| old.id == l.id) {
            require(
                matches!(&old.kind, LinkKind::Transit { trip, .. } if trip == &proposal.trip),
                "service edge owned by another link/trip",
            )?;
            require(
                old.access != Access::Accessible || l.access == Access::Accessible,
                "service cannot downgrade accessible ride",
            )?;
        }
    }
    // Allocated physical space cannot be silently orphaned, moved or changed mode.
    for old in d
        .links
        .iter()
        .filter(|l| matches!(&l.kind, LinkKind::Transit { trip, .. } if trip == &proposal.trip))
    {
        if s.allocations.contains_key(&old.id) {
            require(
                proposal.rides.iter().any(|l| {
                    l.id == old.id
                        && l.from == old.from
                        && l.to == old.to
                        && matches!(
                            l.kind,
                            LinkKind::Transit {
                                mode: Mode::Bus,
                                ..
                            }
                        )
                }),
                "reroute would orphan a bus lane association",
            )?;
        }
    }
    if let Some(calendar) = d.calendars.iter().find(|c| c.id == proposal.calendar.id) {
        require(
            calendar == &proposal.calendar,
            "calendar ID already exists; use a new ID for changed calendar",
        )?;
    } else {
        d.calendars.push(proposal.calendar.clone());
    }
    d.links
        .retain(|l| !matches!(&l.kind, LinkKind::Transit { trip, .. } if trip == &proposal.trip));
    d.links.extend(proposal.rides.clone());
    // Final graph validation and estimation happen after all changes, atomically.
    s.transit.insert(
        proposal.trip.clone(),
        Record {
            operations: proposal.operations.clone(),
            estimate: OperatingEstimate {
                graph_revision: d.revision.clone(),
                runs: 0,
                peak_vehicles: 0,
                peak_operators: 0,
                vehicle_seconds: 0,
                operator_seconds: 0,
                vehicle_metres: 0.0,
                operating_minor: 0.0,
                currency: proposal.operations.currency.clone(),
                coverage_start_utc: d.coverage_start,
                coverage_end_utc: d.coverage_end,
            },
            provenance: provenance.clone(),
        },
    );
    Ok(())
}

pub(crate) fn station(
    s: &mut SnapshotData,
    d: &mut NetworkData,
    proposal: &Station,
) -> Result<(), Error> {
    require(
        !proposal.nodes.is_empty() || !proposal.links.is_empty(),
        "empty station edit",
    )?;
    for node in &proposal.nodes {
        require(
            !d.nodes.iter().any(|n| n.id == node.id),
            "station node already exists",
        )?;
        d.nodes.push(node.clone());
    }
    let mut targets = BTreeSet::new();
    let mut assessed = BTreeSet::new();
    for link in &proposal.links {
        require(targets.insert(link.id), "duplicate station link")?;
        let old = d.links.iter().position(|l| l.id == link.id);
        match &link.kind {
            LinkKind::Transfer {
                from, to, action, ..
            } => {
                if let Some(i) = old {
                    require(
                        matches!(&d.links[i].kind, LinkKind::Transfer { from: a, to: b, action: c, .. }
                        if a == from && b == to && c == action)
                            && d.links[i].from == link.from
                            && d.links[i].to == link.to,
                        "station replacement must retain transfer identity",
                    )?;
                    require(
                        d.links[i].access != Access::Accessible
                            || link.access == Access::Accessible,
                        "station cannot downgrade accessible transfer",
                    )?;
                }
            }
            LinkKind::Street {
                mode,
                blocked,
                cycling,
                safety,
                capacity,
                ..
            } => {
                require(
                    old.is_none()
                        && (*mode == Mode::Walk || is_bicycle(*mode))
                        && !blocked
                        && capacity.is_none(),
                    "station streets must be new usable off-road walking/cycling connectors",
                )?;
                let assessment = proposal.assessments.get(&link.id).ok_or_else(|| {
                    Error("station connector requires crossing assessment".into())
                })?;
                crate::scenario::validate_assessment(&s.network, link, assessment, None)?;
                if is_bicycle(*mode) {
                    require(
                        *cycling == CycleFacility::Protected
                            && safety
                                .as_ref()
                                .is_some_and(|v| v.provenance.basis == Basis::Modeled),
                        "station cycling connector needs protected modeled safety",
                    )?;
                }
                s.crossings.insert(link.id, assessment.clone());
                assessed.insert(link.id);
            }
            _ => {
                return Err(Error(
                    "station links cannot change transit/road supply".into(),
                ))
            }
        }
        if let Some(i) = old {
            d.links[i] = link.clone();
        } else {
            d.links.push(link.clone());
        }
    }
    require(
        proposal.assessments.keys().all(|id| assessed.contains(id)),
        "unused station crossing assessment",
    )
}

/// Preserve at least one actual endpoint boarding/alighting opportunity, not
/// merely a nonempty transfer window detached from all runs. Complete station
/// closure is intentionally unsupported here; it needs a separate access plan.
pub(crate) fn validate_facilities(
    before: &Network,
    after: &Network,
    managed: &BTreeMap<String, Record>,
) -> Result<(), Error> {
    let mut trips: BTreeSet<String> = managed.keys().cloned().collect();
    for link in &before.data().links {
        if let LinkKind::Transit { trip, .. } = &link.kind {
            trips.insert(trip.clone());
        }
    }
    for trip in trips {
        if managed.contains_key(&trip) || has_endpoint_opportunity(before, &trip)? {
            require(has_endpoint_opportunity(after, &trip)?, "service would lose its last usable endpoint boarding/alighting opportunity; complete closure requires a separate access plan")?;
        }
    }
    Ok(())
}
pub(crate) fn validate_station_opportunities(
    before: &Network,
    after: &Network,
    changes: &[crate::scenario::Change],
) -> Result<(), Error> {
    let mut affected = BTreeSet::new();
    for change in changes {
        if let crate::scenario::Change::Station(station) = change {
            for link in &station.links {
                if before.link(link.id).is_none() {
                    continue;
                }
                if let LinkKind::Transfer {
                    from, to, action, ..
                } = link.kind
                {
                    match action {
                        TransferAction::Board => {
                            affected.insert((link.from, to, true));
                        }
                        TransferAction::Alight => {
                            affected.insert((link.from, from, false));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    for (node, mode, board) in affected {
        if stop_opportunity(before, node, mode, board)? {
            require(stop_opportunity(after,node,mode,board)?, "station would lose its last usable boarding/alighting opportunity; complete closure requires a separate access plan")?;
        }
    }
    Ok(())
}
fn stop_opportunity(
    network: &Network,
    node: mobility_graph::NodeId,
    mode: Mode,
    board: bool,
) -> Result<bool, Error> {
    if !network
        .node(node)
        .is_some_and(|n| !matches!(n.access, Access::Unknown | Access::Impassable))
    {
        return Ok(false);
    }
    let transfers: Vec<_> = network.data().links.iter().filter(|l| l.from == node && !matches!(l.access,Access::Unknown|Access::Impassable)
        && matches!(l.kind,LinkKind::Transfer {from,to,action,..} if if board {from==Mode::Walk && to==mode && action==TransferAction::Board} else {from==mode && to==Mode::Walk && action==TransferAction::Alight})).collect();
    let mut examined = 0usize;
    for link in &network.data().links {
        let LinkKind::Transit {
            mode: ride_mode,
            calendar,
            timetable,
            ..
        } = &link.kind
        else {
            continue;
        };
        if *ride_mode != mode
            || (if board { link.from } else { link.to }) != node
            || matches!(link.access, Access::Unknown | Access::Impassable)
            || !network
                .node(link.from)
                .is_some_and(|n| !matches!(n.access, Access::Unknown | Access::Impassable))
            || !network
                .node(link.to)
                .is_some_and(|n| !matches!(n.access, Access::Unknown | Access::Impassable))
        {
            continue;
        }
        let cal = network
            .data()
            .calendars
            .iter()
            .find(|c| c.id == *calendar)
            .ok_or_else(|| Error("missing stop calendar".into()))?;
        for day in &network.data().days {
            if day.origin_utc >= network.data().coverage_end
                || day.origin_utc + 172800 <= network.data().coverage_start
                || !cal.active(day)
            {
                continue;
            }
            for event in timetable.departures(link.seconds).map_err(|e| Error(e.0))? {
                examined += 1;
                require(
                    examined <= 1_000_000,
                    "station opportunity examination bound exhausted",
                )?;
                let departure = day.origin_utc + i64::from(event.depart);
                let arrival = day.origin_utc + i64::from(event.arrive);
                if departure < network.data().coverage_start
                    || arrival > network.data().coverage_end
                {
                    continue;
                }
                let time = if board { departure } else { arrival };
                if transfers.iter().any(|l| match &l.kind {
                    LinkKind::Transfer { windows, .. } => windows.iter().any(|(a, b)| {
                        time >= *a
                            && time < *b
                            && if board {
                                time - (*a).max(network.data().coverage_start)
                                    >= i64::from(l.seconds)
                            } else {
                                b - time >= i64::from(l.seconds)
                                    && network.data().coverage_end - time >= i64::from(l.seconds)
                            }
                    }),
                    _ => false,
                }) {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}
fn has_endpoint_opportunity(network: &Network, trip: &str) -> Result<bool, Error> {
    let mut legs: Vec<_> = network
        .data()
        .links
        .iter()
        .filter_map(|l| match &l.kind {
            LinkKind::Transit {
                trip: id,
                stop_index,
                calendar,
                mode,
                timetable,
                ..
            } if id == trip => Some((l, *stop_index, *calendar, *mode, timetable)),
            _ => None,
        })
        .collect();
    legs.sort_by_key(|leg| leg.1);
    let (Some(first), Some(last)) = (legs.first(), legs.last()) else {
        return Ok(false);
    };
    let usable = |access| !matches!(access, Access::Unknown | Access::Impassable);
    if legs.iter().any(|leg| {
        !usable(leg.0.access)
            || !network.node(leg.0.from).is_some_and(|n| usable(n.access))
            || !network.node(leg.0.to).is_some_and(|n| usable(n.access))
    }) {
        return Ok(false);
    }
    let transfers = |node, mode, action| {
        network.data().links.iter().filter(move |l| l.from == node && usable(l.access) && matches!(l.kind,
        LinkKind::Transfer { from, to, action: a, .. } if a == action && if action == TransferAction::Board { from == Mode::Walk && to == mode } else { from == mode && to == Mode::Walk }))
    };
    let boards: Vec<_> = transfers(first.0.from, first.3, TransferAction::Board).collect();
    let alights: Vec<_> = transfers(last.0.to, last.3, TransferAction::Alight).collect();
    if boards.is_empty() || alights.is_empty() {
        return Ok(false);
    }
    let calendar = network
        .data()
        .calendars
        .iter()
        .find(|c| c.id == first.2)
        .ok_or_else(|| Error("missing endpoint calendar".into()))?;
    let ends: BTreeMap<_, _> = last
        .4
        .departures(last.0.seconds)
        .map_err(|e| Error(e.0))?
        .map(|e| (e.run, e.arrive))
        .collect();
    let mut examined = 0usize;
    for day in &network.data().days {
        if day.origin_utc >= network.data().coverage_end
            || day.origin_utc + 172800 <= network.data().coverage_start
            || !calendar.active(day)
        {
            continue;
        }
        for departure in first
            .4
            .departures(first.0.seconds)
            .map_err(|e| Error(e.0))?
        {
            examined += 1;
            require(
                examined <= 1_000_000,
                "endpoint opportunity examination bound exhausted",
            )?;
            let Some(end) = ends.get(&departure.run) else {
                continue;
            };
            let start = day.origin_utc + i64::from(departure.depart);
            let finish = day.origin_utc + i64::from(*end);
            if start < network.data().coverage_start || finish > network.data().coverage_end {
                continue;
            }
            let board = boards.iter().any(|l| match &l.kind {
                LinkKind::Transfer { windows, .. } => windows.iter().any(|(a, b)| {
                    start >= *a
                        && start < *b
                        && start - (*a).max(network.data().coverage_start) >= i64::from(l.seconds)
                }),
                _ => false,
            });
            let alight = alights.iter().any(|l| match &l.kind {
                LinkKind::Transfer { windows, .. } => windows.iter().any(|(a, b)| {
                    finish >= *a
                        && finish < *b
                        && b - finish >= i64::from(l.seconds)
                        && network.data().coverage_end - finish >= i64::from(l.seconds)
                }),
                _ => false,
            });
            if board && alight {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Exact accounting over active service days and full runs inside graph coverage.
/// A partially covered run is an error, not a silently truncated cost estimate.
/// Duty = deadhead before first departure + first-to-last elapsed + turnaround.
pub fn estimate(
    network: &Network,
    trip: &str,
    input: &Operations,
) -> Result<OperatingEstimate, Error> {
    require(
        input.currency.len() == 3
            && input.currency.bytes().all(|b| b.is_ascii_uppercase())
            && input.operators_per_vehicle > 0
            && input.max_run_instances > 0
            && input.max_run_instances <= 1_000_000,
        "invalid currency/staffing/run bound",
    )?;
    let mut legs: Vec<_> = network
        .data()
        .links
        .iter()
        .filter(|l| matches!(&l.kind, LinkKind::Transit { trip: id, .. } if id == trip))
        .collect();
    legs.sort_by_key(|l| {
        if let LinkKind::Transit { stop_index, .. } = l.kind {
            stop_index
        } else {
            0
        }
    });
    let first = *legs
        .first()
        .ok_or_else(|| Error("unknown service for operations".into()))?;
    let last = *legs
        .last()
        .ok_or_else(|| Error("unknown service for operations".into()))?;
    let (calendar_id, first_times) = match &first.kind {
        LinkKind::Transit {
            calendar,
            timetable,
            ..
        } => (*calendar, timetable),
        _ => return Err(Error("not transit".into())),
    };
    let last_times = match &last.kind {
        LinkKind::Transit { timetable, .. } => timetable,
        _ => return Err(Error("not transit".into())),
    };
    let calendar = network
        .data()
        .calendars
        .iter()
        .find(|c| c.id == calendar_id)
        .ok_or_else(|| Error("missing calendar".into()))?;
    let mut ends = BTreeMap::new();
    for event in last_times
        .departures(last.seconds)
        .map_err(|e| Error(e.0))?
    {
        require(
            ends.len() < input.max_run_instances,
            "operating template departure bound exhausted",
        )?;
        ends.insert(event.run, event.arrive);
    }
    let metres: f64 = legs
        .iter()
        .map(|l| l.distance_m.unwrap_or(0.0))
        .sum::<f64>()
        + f64::from(input.deadhead_metres_per_run);
    let mut events = Vec::new();
    let mut seconds = 0u64;
    let mut runs = 0usize;
    let mut examined = 0usize;
    for day in &network.data().days {
        if day.origin_utc >= network.data().coverage_end
            || day.origin_utc + 172800 <= network.data().coverage_start
            || !calendar.active(day)
        {
            continue;
        }
        for start in first_times
            .departures(first.seconds)
            .map_err(|e| Error(e.0))?
        {
            examined += 1;
            require(
                examined <= input.max_run_instances,
                "operating examined departure bound exhausted",
            )?;
            let end = *ends
                .get(&start.run)
                .ok_or_else(|| Error("missing final run".into()))?;
            let start_utc = day
                .origin_utc
                .checked_add(i64::from(start.depart))
                .ok_or_else(|| Error("run start overflow".into()))?;
            let end_utc = day
                .origin_utc
                .checked_add(i64::from(end))
                .ok_or_else(|| Error("run end overflow".into()))?;
            if end_utc <= network.data().coverage_start || start_utc >= network.data().coverage_end
            {
                continue;
            }
            require(
                start_utc >= network.data().coverage_start
                    && end_utc <= network.data().coverage_end,
                "partial run outside coverage",
            )?;
            runs += 1;
            require(
                runs <= input.max_run_instances,
                "operating run instance bound exhausted",
            )?;
            let duty_start = start_utc
                .checked_sub(i64::from(input.deadhead_seconds_per_run))
                .ok_or_else(|| Error("duty start overflow".into()))?;
            let duty_end = end_utc
                .checked_add(i64::from(input.turnaround_seconds))
                .ok_or_else(|| Error("duty end overflow".into()))?;
            let duty = duty_end
                .checked_sub(duty_start)
                .and_then(|v| u64::try_from(v).ok())
                .ok_or_else(|| Error("duty duration overflow".into()))?;
            seconds = seconds
                .checked_add(duty)
                .ok_or_else(|| Error("vehicle seconds overflow".into()))?;
            events.push((duty_start, 1i64));
            events.push((duty_end, -1));
        }
    }
    require(runs > 0, "service has no active complete runs in coverage")?;
    events.sort_unstable(); // Ends precede starts: half-open duties can reuse a vehicle.
    let mut current = 0i64;
    let mut peak = 0i64;
    for (_, delta) in events {
        current += delta;
        peak = peak.max(current);
    }
    let operators = seconds
        .checked_mul(u64::from(input.operators_per_vehicle))
        .ok_or_else(|| Error("operator seconds overflow".into()))?;
    let vehicle_metres = metres * runs as f64;
    let cost = seconds as f64 / 3600.0 * f64::from(input.vehicle_minor_per_hour)
        + operators as f64 / 3600.0 * f64::from(input.operator_minor_per_hour)
        + vehicle_metres / 1000.0 * f64::from(input.minor_per_km);
    require(
        vehicle_metres.is_finite() && cost.is_finite(),
        "nonfinite operating estimate",
    )?;
    let peak_vehicles = usize::try_from(peak).map_err(|_| Error("fleet count overflow".into()))?;
    let peak_operators = peak_vehicles
        .checked_mul(usize::from(input.operators_per_vehicle))
        .ok_or_else(|| Error("operator count overflow".into()))?;
    Ok(OperatingEstimate {
        graph_revision: network.data().revision.clone(),
        runs,
        peak_vehicles,
        peak_operators,
        vehicle_seconds: seconds,
        operator_seconds: operators,
        vehicle_metres,
        operating_minor: cost,
        currency: input.currency.clone(),
        coverage_start_utc: network.data().coverage_start,
        coverage_end_utc: network.data().coverage_end,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferFare {
    pub group: String,
    /// Half-open window from the last full-price boarding; discounts do not renew it.
    pub window_seconds: u32,
    pub minor: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fare {
    pub currency: String,
    pub boarding_minor: u32,
    pub transfer: Option<TransferFare>,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub outcome: Outcome,
    pub fare_minor: Option<u64>,
    pub generalized_seconds: Option<f64>,
    pub currency: String,
}
/// Route by physical earliest arrival, then price each actual boarding once.
/// This is policy comparison of a journey, NOT fare-aware path/mode choice.
/// Supply a fare for every ridden trip and an explicit positive value of time.
pub fn evaluate(
    network: &Network,
    request: &Request,
    fares: &BTreeMap<String, Fare>,
    currency: &str,
    value_of_time_minor_per_hour: u32,
) -> Result<Evaluation, Error> {
    require(
        currency.len() == 3
            && currency.bytes().all(|b| b.is_ascii_uppercase())
            && value_of_time_minor_per_hour > 0,
        "invalid currency/value of time",
    )?;
    let outcome =
        match plan(network, request, &Options::default()).map_err(|e| Error(e.to_string()))? {
            PlannedOutcome::Journey(itinerary) => Outcome::Journey(itinerary.journey),
            PlannedOutcome::Unmet(unmet) => Outcome::Unmet(unmet),
        };
    let mut total = 0u64;
    let mut anchor: Option<(String, i64)> = None;
    let mut board = false;
    let generalized_seconds = if let Outcome::Journey(journey) = &outcome {
        for step in &journey.steps {
            let link = network
                .link(step.edge)
                .ok_or_else(|| Error("unknown itinerary edge".into()))?;
            match &link.kind {
                LinkKind::Transfer {
                    action: TransferAction::Board,
                    ..
                } => board = true,
                LinkKind::Transit { trip, .. } if board => {
                    board = false;
                    let fare = fares
                        .get(trip)
                        .ok_or_else(|| Error(format!("missing fare for trip {trip}")))?;
                    validate_provenance(network, &fare.provenance, true)?;
                    require(fare.currency == currency, "fare currency mismatch")?;
                    if let Some(t) = &fare.transfer {
                        require(
                            !t.group.trim().is_empty()
                                && t.window_seconds > 0
                                && t.minor <= fare.boarding_minor,
                            "invalid transfer fare",
                        )?;
                    }
                    let discount = fare.transfer.as_ref().is_some_and(|t| {
                        anchor.as_ref().is_some_and(|(group, time)| {
                            group == &t.group
                                && step
                                    .departure_utc
                                    .checked_sub(*time)
                                    .is_some_and(|elapsed| {
                                        elapsed >= 0 && elapsed < i64::from(t.window_seconds)
                                    })
                        })
                    });
                    let price = if discount {
                        fare.transfer
                            .as_ref()
                            .map_or(fare.boarding_minor, |t| t.minor)
                    } else {
                        anchor = fare
                            .transfer
                            .as_ref()
                            .map(|t| (t.group.clone(), step.departure_utc));
                        fare.boarding_minor
                    };
                    total = total
                        .checked_add(u64::from(price))
                        .ok_or_else(|| Error("fare overflow".into()))?;
                }
                _ => {}
            }
        }
        Some(
            (journey.arrival_utc - journey.departure_utc) as f64
                + total as f64 * 3600.0 / f64::from(value_of_time_minor_per_hour),
        )
    } else {
        None
    };
    Ok(Evaluation {
        fare_minor: generalized_seconds.map(|_| total),
        generalized_seconds,
        outcome,
        currency: currency.into(),
    })
}
