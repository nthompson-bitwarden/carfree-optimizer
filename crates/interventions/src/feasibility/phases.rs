//! Ordered permanent milestones with temporary overlays on isolated histories.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    Operates,
    Suspended,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporaryService {
    pub status: ServiceStatus,
    pub reason: String,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseInput {
    pub start: i64,
    pub end: i64,
    pub permanent: Batch,
    pub temporary: Option<Batch>,
    pub services: BTreeMap<String, TemporaryService>,
    pub inputs: Inputs,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Phase {
    pub start: i64,
    pub end: i64,
    pub services: BTreeMap<String, TemporaryService>,
    screened: Screened,
}
impl Phase {
    pub fn screened(&self) -> &Screened {
        &self.screened
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ConstructionPlan {
    phases: Vec<Phase>,
    final_state: Screened,
    pub capital: Bounds,
    /// All phase incremental capital at time zero plus discounted final-state
    /// annual costs. This is a real-price comparison, not a cash-flow forecast.
    pub lifecycle: Bounds,
}
impl ConstructionPlan {
    pub fn phases(&self) -> &[Phase] {
        &self.phases
    }
    pub fn final_state(&self) -> &Screened {
        &self.final_state
    }
}
fn same_registry(a: &[AccessSample], b: &[AccessSample]) -> bool {
    let canonical = |rows: &[AccessSample]| {
        let mut rows = rows.to_vec();
        for a in &mut rows {
            a.permitted_car_edges = None;
            for r in [&mut a.outbound, &mut a.returning] {
                r.departure_utc = 0;
                r.deadline_utc = 0;
                r.max_states = 0;
            }
        }
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        rows
    };
    canonical(a) == canonical(b)
}
fn trip_links(s: &Snapshot, trip: &str) -> Vec<Link> {
    s.network
        .data()
        .links
        .iter()
        .filter(|l| matches!(&l.kind,LinkKind::Transit {trip:id,..} if id==trip))
        .cloned()
        .collect()
}
fn affected_services(permanent: &Snapshot, effective: &Snapshot) -> BTreeSet<String> {
    let trips: BTreeSet<_> = permanent
        .network
        .data()
        .links
        .iter()
        .chain(effective.network.data().links.iter())
        .filter_map(|l| match &l.kind {
            LinkKind::Transit { trip, .. } => Some(trip.clone()),
            _ => None,
        })
        .collect();
    trips
        .into_iter()
        .filter(|trip| {
            let a = trip_links(permanent, trip);
            let b = trip_links(effective, trip);
            a != b
                || a.iter().chain(&b).any(|l| {
                    if let LinkKind::Transit { calendar, .. } = l.kind {
                        permanent
                            .network
                            .data()
                            .calendars
                            .iter()
                            .find(|c| c.id == calendar)
                            != effective
                                .network
                                .data()
                                .calendars
                                .iter()
                                .find(|c| c.id == calendar)
                    } else {
                        false
                    }
                })
                || permanent.network.data().days != effective.network.data().days
                || permanent
                    .network
                    .data()
                    .links
                    .iter()
                    .chain(effective.network.data().links.iter())
                    .any(|transfer| {
                        matches!(transfer.kind, LinkKind::Transfer { .. })
                            && permanent.network.link(transfer.id)
                                != effective.network.link(transfer.id)
                            && a.iter().chain(&b).any(|ride| {
                                ride.from == transfer.from
                                    || ride.from == transfer.to
                                    || ride.to == transfer.from
                                    || ride.to == transfer.to
                            })
                    })
        })
        .collect()
}
fn service_assumptions(
    permanent: &Snapshot,
    effective: &Snapshot,
    p: &PhaseInput,
) -> Result<(), Rejection> {
    let changed = affected_services(permanent, effective);
    check(
        changed == p.services.keys().cloned().collect(),
        "temporary service declaration does not match affected patterns",
    )?;
    for (trip, assumption) in &p.services {
        check(text(&assumption.reason), "missing temporary service reason")?;
        provenance(&effective.network, &assumption.provenance)?;
        match assumption.status {
            ServiceStatus::Operates => {
                let cost = p
                    .inputs
                    .transit
                    .iter()
                    .find(|t| &t.trip == trip)
                    .ok_or_else(|| err("missing temporary service operating inputs"))?;
                let mut d = effective.network.data().clone();
                d.coverage_start = p.start;
                d.coverage_end = p.end;
                let n = Network::new(d).map_err(err)?;
                // Partial service or resource exhaustion is an error, never 'no runs'.
                transit::estimate(&n, trip, &cost.operations).map_err(err)?;
            }
            ServiceStatus::Suspended => {
                let mut legs = trip_links(effective, trip);
                legs.sort_by_key(|l| match l.kind {
                    LinkKind::Transit { stop_index, .. } => stop_index,
                    _ => 0,
                });
                if let (Some(first), Some(last)) = (legs.first(), legs.last()) {
                    let LinkKind::Transit {
                        calendar,
                        timetable: starts,
                        ..
                    } = &first.kind
                    else {
                        return Err(err("invalid service first leg"));
                    };
                    let LinkKind::Transit {
                        timetable: ends, ..
                    } = &last.kind
                    else {
                        return Err(err("invalid service final leg"));
                    };
                    let calendar = effective
                        .network
                        .data()
                        .calendars
                        .iter()
                        .find(|c| c.id == *calendar)
                        .ok_or_else(|| err("missing temporary calendar"))?;
                    let mut end_by_run = BTreeMap::new();
                    for event in ends.departures(last.seconds).map_err(err)? {
                        check(
                            end_by_run.len() < 1_000_000,
                            "temporary template examination bound exhausted",
                        )?;
                        end_by_run.insert(event.run, event.arrive);
                    }
                    let mut examined = 0usize;
                    for day in &effective.network.data().days {
                        if !calendar.active(day) {
                            continue;
                        }
                        for event in starts.departures(first.seconds).map_err(err)? {
                            examined += 1;
                            check(
                                examined <= 1_000_000,
                                "temporary service examination bound exhausted",
                            )?;
                            let arrival = end_by_run
                                .get(&event.run)
                                .ok_or_else(|| err("missing temporary final run"))?;
                            let start = day
                                .origin_utc
                                .checked_add(i64::from(event.depart))
                                .ok_or_else(|| err("temporary time overflow"))?;
                            let end = day
                                .origin_utc
                                .checked_add(i64::from(*arrival))
                                .ok_or_else(|| err("temporary time overflow"))?;
                            // A through run remains active while dwelling between legs.
                            check(
                                end <= p.start || start >= p.end,
                                format!(
                                    "suspended temporary service {trip} still operates in phase"
                                ),
                            )?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
/// Transactional planning: permanent milestones accumulate on a clone, temporary
/// overlays expire at each boundary. The caller's History (and redo) is untouched.
/// The final state's construction/disruption must be zero: phase rows already
/// charge these incremental costs. One budget and protected registry govern all.
pub fn screen_plan(
    history: &History,
    phases: &[PhaseInput],
    final_inputs: &Inputs,
) -> Result<ConstructionPlan, Rejection> {
    check(!phases.is_empty(), "construction plan needs phases")?;
    let mut permanent = history.clone();
    let base = history.current();
    let mut prior_end = None;
    let mut accepted = vec![];
    let mut capital = Bounds::default();
    let mut used_ids = BTreeSet::new();
    for p in phases {
        check(
            p.inputs.budget == final_inputs.budget,
            "phase cannot change project budget",
        )?;
        check(
            same_registry(&p.inputs.access, &final_inputs.access),
            "phase cannot change protected OD registry/endpoints/profiles",
        )?;
        check(
            p.start < p.end && prior_end.is_none_or(|end| end == p.start),
            "phase intervals must be positive, ordered and contiguous",
        )?;
        check(
            used_ids.insert(p.permanent.id.clone()),
            "reused phase milestone ID",
        )?;
        permanent.apply(&p.permanent).map_err(err)?;
        let effective = if let Some(temp) = &p.temporary {
            check(
                used_ids.insert(temp.id.clone()),
                "reused temporary scenario ID",
            )?;
            permanent.preview(temp).map_err(err)?
        } else {
            permanent.current().clone()
        };
        check(
            p.start >= effective.network.data().coverage_start
                && p.end <= effective.network.data().coverage_end,
            "phase outside network service coverage",
        )?;
        service_assumptions(permanent.current(), &effective, p)?;
        let screened = assess(base, effective, &p.inputs, Some((p.start, p.end)))?;
        capital = capital.plus(screened.report.capital)?;
        check(
            capital.upper <= final_inputs.budget.capital,
            "cumulative phase capital exceeds project budget",
        )?;
        accepted.push(Phase {
            start: p.start,
            end: p.end,
            services: p.services.clone(),
            screened,
        });
        prior_end = Some(p.end);
    }
    let final_state = assess(base, permanent.current().clone(), final_inputs, None)?;
    check(
        final_state.report.capital.upper == 0.0,
        "final construction/disruption must be zero; already charged in phases",
    )?;
    let lifecycle = capital.plus(final_state.report.lifecycle)?;
    Ok(ConstructionPlan {
        phases: accepted,
        final_state,
        capital,
        lifecycle,
    })
}
