//! Cumulative E12 inventory and aggregates extracted from E13 evidence.
use crate::{
    reference::{DoorAudit, Error, Project},
    reward::{Cohort, Metrics, ServiceLoss},
};
use accessibility::upstream::{FeasibilityDistribution, TripBridge};
use interventions::scenario::{AccessKind as RouteKind, History, Requirement};
use interventions::{
    feasibility::{AccessKind, Bounds, Budget, Category, Cost, Inputs},
    scenario::Snapshot,
    transit,
};
use mobility_graph::{
    network::{LinkKind, Network},
    Mode,
};
use mobility_sim::demand::{Demand, GroupTag, TravelMode, TravelerGroup};
use std::collections::{BTreeMap, BTreeSet};

fn config(message: &str) -> Error {
    Error::Configuration(message.into())
}
fn evidence(message: &str) -> Error {
    Error::Evidence(message.into())
}
fn nonnegative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

fn add(a: Bounds, b: Bounds) -> Result<Bounds, Error> {
    let result = Bounds {
        lower: a.lower + b.lower,
        central: a.central + b.central,
        upper: a.upper + b.upper,
    };
    if !nonnegative(result.upper) {
        return Err(config("cost sum overflow"));
    }
    Ok(result)
}

pub(crate) fn project_costs(rows: &[Cost], budget: &Budget) -> Result<(Bounds, Bounds), Error> {
    let mut ids = BTreeSet::new();
    let mut construction = Bounds::default();
    let mut disruption = Bounds::default();
    let mut annual = Bounds::default();
    for row in rows {
        if row.id.trim().is_empty()
            || !ids.insert(&row.id)
            || row.unit.trim().is_empty()
            || row.currency != budget.currency
            || row.price_year != budget.price_year
            || !nonnegative(row.rate.lower)
            || row.rate.lower > row.rate.central
            || row.rate.central > row.rate.upper
            || !row.rate.central.is_finite()
            || !nonnegative(row.rate.upper)
            || !nonnegative(row.quantity)
            || !row.local_factor.is_finite()
            || row.local_factor <= 0.0
        {
            return Err(config("invalid, duplicate or incompatible cost assumption"));
        }
        let price = Bounds {
            lower: (row.rate.lower * row.quantity) * row.local_factor,
            central: (row.rate.central * row.quantity) * row.local_factor,
            upper: (row.rate.upper * row.quantity) * row.local_factor,
        };
        match row.category {
            Category::Construction => construction = add(construction, price)?,
            Category::Disruption => disruption = add(disruption, price)?,
            Category::Maintenance => annual = add(annual, price)?,
        }
    }
    Ok((add(construction, disruption)?, annual))
}

pub(crate) fn opening_annual(snapshot: &Snapshot, inputs: &Inputs) -> Result<Bounds, Error> {
    let budget = &inputs.budget;
    if budget.currency.len() != 3
        || !budget.currency.bytes().all(|c| c.is_ascii_uppercase())
        || !(1..=9999).contains(&budget.price_year)
        || !(1..=1_000_000).contains(&budget.minor_units_per_major)
        || !nonnegative(budget.capital)
        || !nonnegative(budget.annual)
        || !(1..=200).contains(&budget.years)
        || !nonnegative(budget.real_discount_rate)
    {
        return Err(config("invalid budget"));
    }
    let (capital, maintenance) = project_costs(&inputs.costs, budget)?;
    let categories: BTreeSet<_> = inputs.costs.iter().map(|c| c.category).collect();
    if capital.upper != 0.0 || categories.len() != 3 {
        return Err(config(
            "opening costs require explicit zero construction/disruption and maintenance evidence",
        ));
    }
    let network = snapshot.network();
    let mut validated = vec![&network.data().provenance];
    for provenance in inputs
        .costs
        .iter()
        .map(|r| &r.provenance)
        .chain(inputs.transit.iter().map(|r| &r.provenance))
        .chain(inputs.access.iter().map(|r| &r.provenance))
        .chain(inputs.segments.iter().map(|r| &r.provenance))
        .chain(inputs.rows.iter().map(|r| &r.provenance))
    {
        if !validated.contains(&provenance) {
            let mut data = network.data().clone();
            data.provenance = provenance.clone();
            Network::new(data).map_err(|e| Error::Configuration(e.to_string()))?;
            validated.push(provenance);
        }
    }
    // Opening does not create an E10 edit. E11 supplies the same per-pattern
    // computation that E12 uses for each subsequent gross annual commitment.
    let trips: BTreeSet<_> = network
        .data()
        .links
        .iter()
        .filter_map(|l| match &l.kind {
            LinkKind::Transit { trip, .. } => Some(trip),
            _ => None,
        })
        .collect();
    let mut seen = BTreeSet::new();
    let mut operating = 0.0;
    for row in &inputs.transit {
        if !trips.contains(&row.trip)
            || !seen.insert(&row.trip)
            || row.operations.currency != budget.currency
            || row.price_year != budget.price_year
            || !row.coverage_repetitions_per_year.is_finite()
            || row.coverage_repetitions_per_year <= 0.0
            || !row.local_factor.is_finite()
            || row.local_factor <= 0.0
            || row.annualization.trim().is_empty()
        {
            return Err(config(
                "invalid opening transit costs, currency or annualization",
            ));
        }
        let estimate = transit::estimate(network, &row.trip, &row.operations)
            .map_err(|e| Error::Configuration(e.to_string()))?;
        operating += estimate.operating_minor / f64::from(budget.minor_units_per_major)
            * row.coverage_repetitions_per_year
            * row.local_factor;
    }
    if seen != trips {
        return Err(config("missing opening transit cost evidence"));
    }
    let annual = add(
        maintenance,
        Bounds {
            lower: operating,
            central: operating,
            upper: operating,
        },
    )?;
    if annual.upper > budget.annual {
        return Err(config("opening annual commitments exceed budget"));
    }
    let kinds: BTreeSet<_> = inputs.access.iter().map(|a| a.kind).collect();
    if kinds.len() != AccessKind::ALL.len() {
        return Err(config(
            "opening protected OD registry lacks a required class",
        ));
    }
    validate_opening_inventory(inputs)?;
    validate_opening_access(snapshot, inputs)?;
    Ok(annual)
}

fn validate_opening_inventory(inputs: &Inputs) -> Result<(), Error> {
    let segments: BTreeSet<_> = inputs.segments.iter().map(|s| &s.id).collect();
    let rows: BTreeSet<_> = inputs.rows.iter().map(|r| &r.id).collect();
    let access: BTreeSet<_> = inputs.access.iter().map(|a| &a.id).collect();
    if segments.len() != inputs.segments.len()
        || rows.len() != inputs.rows.len()
        || access.len() != inputs.access.len()
        || inputs.segments.iter().any(|s| !rows.contains(&s.row))
    {
        return Err(config(
            "duplicate opening inventory or missing shared right-of-way group",
        ));
    }
    Ok(())
}

fn validate_opening_access(snapshot: &Snapshot, inputs: &Inputs) -> Result<(), Error> {
    let mut registry = Vec::with_capacity(2 * inputs.access.len());
    for sample in &inputs.access {
        if sample.outbound.from != sample.returning.to
            || sample.outbound.to != sample.returning.from
        {
            return Err(config(
                "opening protected endpoints require explicit reversed journeys",
            ));
        }
        for (direction, request) in [
            ("outbound", &sample.outbound),
            ("return", &sample.returning),
        ] {
            let car = match sample.kind {
                AccessKind::Disability => false,
                AccessKind::Property => request.initial_mode == Mode::Car,
                _ => true,
            };
            let mode = if car { Mode::Car } else { Mode::Walk };
            if request.initial_mode != mode
                || request.vehicle != car.then_some(Mode::Car)
                || request.profile.allowed_modes != [mode]
                || request.profile.allow_unknown_access
                || (sample.kind == AccessKind::Disability
                    && !(request.profile.step_free && request.profile.mobility_aid))
            {
                return Err(config(
                    "invalid opening protected mode, vehicle or accessibility profile",
                ));
            }
            // Reuse E10's explicit permission-aware travel checks on the unchanged
            // opening graph. The actual E12 gate is applied to every project.
            registry.push(Requirement {
                id: format!("{}:{direction}", sample.id),
                kind: if car {
                    RouteKind::Emergency
                } else {
                    RouteKind::Walking
                },
                request: request.clone(),
                permitted_car_edges: if car {
                    sample.permitted_car_edges.clone()
                } else {
                    None
                },
                provenance: sample.provenance.clone(),
            });
        }
    }
    History::new(
        snapshot.id().clone(),
        snapshot.network().clone(),
        snapshot.roads().values().cloned().collect(),
        registry,
    )
    .map_err(|e| Error::Configuration(format!("opening protected access: {e}")))?;
    Ok(())
}

pub(crate) fn extend(previous: &Inputs, project: &Project) -> Result<Inputs, String> {
    let mut inputs = previous.clone();
    inputs.costs.extend(project.costs.iter().cloned());
    let mut seen = BTreeSet::new();
    for row in &project.rows {
        if !seen.insert(&row.id) {
            return Err("duplicate project right-of-way group".into());
        }
        match inputs.rows.iter().find(|r| r.id == row.id) {
            Some(old) if old != row => {
                return Err(format!(
                    "project changes shared right-of-way evidence {}",
                    row.id
                ))
            }
            Some(_) => {}
            None => inputs.rows.push(row.clone()),
        }
    }
    seen.clear();
    for segment in &project.segments {
        if !seen.insert(&segment.id) {
            return Err("duplicate project physical segment".into());
        }
        if let Some(old) = inputs.segments.iter_mut().find(|p| p.id == segment.id) {
            let new_edges: BTreeSet<_> = segment.edges.iter().collect();
            let mut previous_lanes: Vec<_> = old.lanes.iter().map(|l| l.width_mm).collect();
            let mut candidate_lanes: Vec<_> = segment.lanes.iter().map(|l| l.width_mm).collect();
            previous_lanes.sort_unstable_by(|a, b| b.cmp(a));
            candidate_lanes.sort_unstable_by(|a, b| b.cmp(a));
            if old.row != segment.row
                || old.road != segment.road
                || old.edges.iter().any(|edge| !new_edges.contains(edge))
                || segment.other_width_mm < old.other_width_mm
                || previous_lanes.len() > candidate_lanes.len()
                || previous_lanes
                    .iter()
                    .zip(candidate_lanes)
                    .any(|(old, new)| *old > new)
            {
                return Err(format!(
                    "project removes prior physical footprint evidence {}",
                    segment.id
                ));
            }
            *old = segment.clone();
        } else {
            inputs.segments.push(segment.clone());
        }
    }
    if let Some(transit) = &project.transit {
        inputs.transit.clone_from(&transit.costs);
    }
    Ok(inputs)
}

/// Existing services retain their annual cost basis. A timetable/calendar
/// change may warrant new operating inputs, but coverage and local cost
/// assumptions remain fixed for that service throughout the episode.
pub(crate) fn validate_transit_costs(
    before: &Snapshot,
    after: &Snapshot,
    previous: &Inputs,
    candidate: &Inputs,
) -> Result<(), String> {
    let old_costs: BTreeMap<_, _> = previous.transit.iter().map(|c| (&c.trip, c)).collect();
    for cost in &candidate.transit {
        let Some(old) = old_costs.get(&cost.trip) else {
            continue;
        };
        if old.coverage_repetitions_per_year != cost.coverage_repetitions_per_year
            || old.annualization != cost.annualization
            || old.local_factor != cost.local_factor
            || (*old != cost && same_transit_pattern(before.network(), after.network(), &cost.trip))
        {
            return Err(format!(
                "transit cost basis changed for service {}",
                cost.trip
            ));
        }
    }
    Ok(())
}

fn same_transit_pattern(before: &Network, after: &Network, trip: &str) -> bool {
    let old = before.data();
    let new = after.data();
    // Compare actual operating patterns, independent of edge and calendar
    // labels, level boarding and bicycle permissions.
    let old_links = operating_segments(before, trip);
    let new_links = operating_segments(after, trip);
    old_links.len() == new_links.len()
        && old_links.into_iter().zip(new_links).all(|(a, b)| {
            let (
                LinkKind::Transit {
                    calendar: ca,
                    timetable: ta,
                    mode: ma,
                    stop_index: sa,
                    ..
                },
                LinkKind::Transit {
                    calendar: cb,
                    timetable: tb,
                    mode: mb,
                    stop_index: sb,
                    ..
                },
            ) = (&a.kind, &b.kind)
            else {
                return false;
            };
            if (a.from, a.to, a.distance_m, ma, sa) != (b.from, b.to, b.distance_m, mb, sb) {
                return false;
            }
            let calendars = (
                old.calendars.iter().find(|c| c.id == *ca),
                new.calendars.iter().find(|c| c.id == *cb),
            );
            let departures = (ta.departures(a.seconds), tb.departures(b.seconds));
            match (calendars, departures) {
                ((Some(ca), Some(cb)), (Ok(da), Ok(db))) => {
                    da.map(|d| (d.depart, d.arrive))
                        .eq(db.map(|d| (d.depart, d.arrive)))
                        && old
                            .days
                            .iter()
                            .filter(|d| ca.active(d))
                            .eq(new.days.iter().filter(|d| cb.active(d)))
                }
                _ => false,
            }
        })
}

fn operating_segments<'a>(
    network: &'a Network,
    trip: &str,
) -> Vec<&'a mobility_graph::network::Link> {
    let mut links: Vec<_> = network
        .data()
        .links
        .iter()
        .filter_map(|link| match &link.kind {
            LinkKind::Transit {
                trip: id,
                stop_index,
                ..
            } if id == trip => Some((*stop_index, link)),
            _ => None,
        })
        .collect();
    links.sort_unstable_by_key(|(stop_index, _)| *stop_index);
    links.into_iter().map(|(_, link)| link).collect()
}

fn cohort(reference: &FeasibilityDistribution, unserved: f64) -> Result<Cohort, Error> {
    if reference.unknown != 0.0 {
        return Err(evidence("missing reference feasibility classification"));
    }
    let car_free = reference.total - reference.car_required - reference.no_mode;
    let metric = Cohort {
        total: reference.total,
        car_required: reference.car_required,
        car_free: car_free.max(0.0),
        no_mode: reference.no_mode,
        unserved,
    };
    if ![
        metric.total,
        metric.car_required,
        metric.car_free,
        metric.no_mode,
        metric.unserved,
    ]
    .into_iter()
    .all(nonnegative)
        || car_free < -1e-9 * reference.total
    {
        return Err(evidence("non-finite or inconsistent reference feasibility"));
    }
    Ok(metric)
}

fn group_cohort(trips: &TripBridge, group_ids: &BTreeSet<&str>) -> Result<Cohort, Error> {
    let mut reference = FeasibilityDistribution {
        total: 0.0,
        car_required: 0.0,
        no_mode: 0.0,
        unknown: 0.0,
    };
    let mut unserved = 0.0;
    for (key, row) in &trips.reference_feasibility.by_group_neighborhood {
        if group_ids.contains(key.0.as_str()) {
            reference.total += row.total;
            reference.car_required += row.car_required;
            reference.no_mode += row.no_mode;
            reference.unknown += row.unknown;
            unserved += trips
                .assignment
                .by_group_neighborhood
                .get(key)
                .ok_or_else(|| evidence("missing E08 assignment cohort"))?
                .unserved;
        }
    }
    cohort(&reference, unserved)
}

fn noncycling(g: &TravelerGroup) -> bool {
    g.tags.contains(&GroupTag::Noncyclist)
        || g.mobility_aid
        || (!g.modes.contains_key(&TravelMode::Bicycle)
            && !g.modes.contains_key(&TravelMode::ElectricBicycle))
}

fn step_free(g: &TravelerGroup) -> bool {
    g.step_free || g.mobility_aid
}

pub(crate) fn protected_losses(
    previous: &crate::reference::Evaluation,
    candidate: &crate::reference::Evaluation,
    demand: &Demand,
    multiplier: f64,
) -> Result<ServiceLoss, Error> {
    let served = |a: &mobility_sim::simulation::Assignment| a.mode.is_some() && !a.denied_boarding;
    let before: BTreeSet<_> = previous
        .trips
        .evidence
        .run()
        .assignments
        .iter()
        .filter(|a| served(a))
        .map(|a| a.tour_id.as_str())
        .collect();
    let after: BTreeSet<_> = candidate
        .trips
        .evidence
        .run()
        .assignments
        .iter()
        .filter(|a| served(a))
        .map(|a| a.tour_id.as_str())
        .collect();
    let groups: BTreeMap<_, _> = demand.groups.iter().map(|g| (&g.id, g)).collect();
    let mut loss = ServiceLoss::default();
    for tour in &demand.tours {
        let unserved = before.contains(tour.id.as_str()) && !after.contains(tour.id.as_str());
        let car_free_lost = previous.reference_car_free.contains(&tour.id)
            && !candidate.reference_car_free.contains(&tour.id);
        if !unserved && !car_free_lost {
            continue;
        }
        let group = groups
            .get(&tour.group)
            .ok_or_else(|| evidence("missing protected tour group"))?;
        for leg in &tour.legs {
            let weight = leg.expansion_weight * multiplier;
            if unserved {
                loss.unserved
                    .add(leg.essential, noncycling(group), step_free(group), weight);
            }
            if car_free_lost {
                loss.car_free
                    .add(leg.essential, noncycling(group), step_free(group), weight);
            }
        }
    }
    Ok(loss)
}

pub(crate) fn metrics(
    trips: &TripBridge,
    demand: &Demand,
    multiplier: f64,
    doors: &[DoorAudit],
) -> Result<Metrics, Error> {
    let all = cohort(
        &trips.reference_feasibility.all,
        trips.assignment.all.unserved,
    )?;
    let essential = cohort(
        &trips.reference_feasibility.essential,
        trips.assignment.essential.unserved,
    )?;
    let noncycling_ids = demand
        .groups
        .iter()
        .filter(|g| noncycling(g))
        .map(|g| g.id.as_str())
        .collect();
    let step_free_ids = demand
        .groups
        .iter()
        .filter(|g| step_free(g))
        .map(|g| g.id.as_str())
        .collect();
    let noncycling = group_cohort(trips, &noncycling_ids)?;
    let step_free = group_cohort(trips, &step_free_ids)?;
    if [all, essential, noncycling, step_free]
        .iter()
        .any(|c| c.total <= 0.0)
    {
        return Err(evidence(
            "positive all, essential, noncycling and step-free denominators required",
        ));
    }
    let tours: BTreeMap<_, _> = demand.tours.iter().map(|t| (&t.id, &t.legs)).collect();
    let run = trips.evidence.run();
    let mut denied = 0.0;
    for assignment in run.assignments.iter().filter(|a| a.denied_boarding) {
        let legs = tours
            .get(&assignment.tour_id)
            .ok_or_else(|| evidence("missing denied demand tour"))?;
        denied += legs
            .iter()
            .map(|l| l.expansion_weight * multiplier)
            .sum::<f64>();
    }
    let door_weight = doors.iter().map(|d| d.sample.weight).sum::<f64>();
    let fixed_door_parity = doors
        .iter()
        .map(|d| d.fixed_parity * d.sample.weight)
        .sum::<f64>()
        / door_weight;
    if !nonnegative(denied)
        || !door_weight.is_finite()
        || door_weight <= 0.0
        || !nonnegative(fixed_door_parity)
    {
        return Err(evidence("non-finite denial or door parity aggregate"));
    }
    Ok(Metrics {
        all,
        essential,
        noncycling,
        step_free,
        denied,
        fixed_door_parity,
        door_weight,
        status: run.status.clone(),
    })
}
