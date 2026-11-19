//! E13 conformance regression cases, synthetic MIT fixtures only.
#[path = "../../interventions/examples/support/e12.rs"]
mod fixture;
mod support;
use accessibility::{comparison::*, projection::Projection, scoring::*, spatial::*, upstream::*};
use mobility_sim::{
    demand::{Load, TravelMode},
    simulation,
};
use support::*;
#[test]
fn e13_s03_given_capacity_denied_proposal_and_multiplier_then_bridge_reconciles_unserved() {
    let h = fixture::base::history();
    let snapshot = h.current();
    let (mut d, mut s) = fixture::base::inputs(snapshot);
    for g in &mut d.groups {
        g.modes = [(
            TravelMode::Transit,
            Load {
                cargo_grams: 7000,
                companions: 1,
            },
        )]
        .into();
    }
    for t in &mut d.tours {
        t.legs.truncate(1);
        t.legs[0].dwell_seconds = 0;
        t.return_to_origin = false;
    }
    s.settings.demand_multiplier = 2.0;
    s.transit[0].passengers = 0.0;
    let mut c = context();
    c.scenario = s.id.clone();
    c.graph_revision = snapshot.network().data().revision.clone();
    c.graph_snapshot = snapshot.network().encode();
    c.demand_revision = d.revision.clone();
    let b = trip_bridge(&c, snapshot.network(), &d, &s, &simulation::FixedCost).unwrap();
    assert_eq!(b.evidence.denied_tours, 1);
    assert_eq!(
        b.evidence.run().assignments[0].mode,
        Some(TravelMode::Transit)
    );
    assert_eq!(b.assignment.all.unserved, 9.0);
    assert!(b.assignment.all.served.is_empty());
    assert_eq!(b.assignment.all.car_required_share(), None);
    assert_eq!(b.reference_feasibility.all.car_required_share(), Some(0.0));
}
#[test]
fn e13_s03_given_interval_same_point_different_population_then_rejected() {
    let rows = [trip("a", Some(false), Some(true), Some(Mode::Car))];
    let mut other = context();
    other.demand_revision = "different-population".into();
    let ci = Uncertainty::Confidence {
        lower: 0.0,
        upper: 1.0,
        level: 0.95,
        binding: IntervalBinding {
            inventory: trip_inventory(&rows),
            context: other,
            metric: ShareMetric::CarRequiredAllTrips,
            denominator_trip_weight: 2.0,
            design_revision: "household-survey-design-v1".into(),
        },
        method: "synthetic supplied survey interval".into(),
        sampling_unit: "household".into(),
        source: "synthetic metadata test".into(),
    };
    assert!(trips(&context(), &rows, ci.clone()).is_err());
    let mut matched = ci;
    if let Uncertainty::Confidence { binding, .. } = &mut matched {
        binding.context = context();
    }
    assert!(trips(&context(), &rows, matched.clone()).is_ok());
    let mut other_rows = rows.to_vec();
    other_rows[0].id = "different-leg-same-weight-and-point".into();
    assert!(trips(&context(), &other_rows, matched).is_err());
}
#[test]
fn e13_s04_s09_given_mixed_budgets_then_rollup_rejected() {
    let c = context();
    let a = projection(c.clone(), Some(0.5));
    let mut i = access_input();
    i.stratum.budget_seconds = 120;
    let b = Projection::from_access(&access(&i).unwrap());
    let area = Neighborhood {
        id: "north".into(),
        population: 2.0,
        ring: vec![(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)],
    };
    let origins = [
        Origin {
            id: "a".into(),
            neighborhood: "north".into(),
            group: "noncyclist".into(),
            population: 1.0,
            score: Some(a),
        },
        Origin {
            id: "b".into(),
            neighborhood: "north".into(),
            group: "noncyclist".into(),
            population: 1.0,
            score: Some(b),
        },
    ];
    assert!(neighborhoods(&c, std::slice::from_ref(&area), &origins, 0.8).is_err());
    let one = Origin {
        id: "origin-a".into(),
        neighborhood: "north".into(),
        group: "noncyclist".into(),
        population: 1.0,
        score: Some(projection(c.clone(), Some(0.5))),
    };
    assert!(neighborhoods(
        &c,
        std::slice::from_ref(&area),
        std::slice::from_ref(&one),
        0.8
    )
    .is_ok());
    let mut copied = one.clone();
    copied.id = "another-origin".into();
    assert!(neighborhoods(&c, &[area], &[one, copied], 0.8).is_err());
}
#[test]
fn e13_s05_s07_given_explicit_empty_inventory_then_zero_is_distinct_from_missing() {
    let zero = Inventory {
        ids: Default::default(),
        completeness_source:
            "synthetic inventory explicitly has no private-road footprints or vehicle activity"
                .into(),
    };
    assert_eq!(road_space(&context(), &zero, &[]).unwrap().total_m2, 0.0);
    assert_eq!(
        emissions(&context(), &zero, &[])
            .unwrap()
            .vehicle_km
            .central,
        0.0
    );
    let missing = Inventory {
        ids: ["missing-factor".into()].into(),
        completeness_source: "expected synthetic fleet ledger".into(),
    };
    assert!(emissions(&context(), &missing, &[]).is_err());
}
#[test]
fn e13_s09_given_different_origins_or_cohorts_then_condition_ranking_rejected() {
    let mut a = access_input();
    a.context.scenario = "a".into();
    let mut b = a.clone();
    b.context.scenario = "b".into();
    let condition = Condition {
        weather: "dry".into(),
        day_or_night: "day".into(),
        service_revision: a.context.service_revision.clone(),
    };
    let first = ConditionScore {
        condition: condition.clone(),
        score: Projection::from_access(&access(&a).unwrap()),
    };
    let second = |input: &AccessInput| ConditionScore {
        condition: condition.clone(),
        score: Projection::from_access(&access(input).unwrap()),
    };
    let grid = [(condition.clone(), 1.0)];
    assert!(rank_conditions(&grid, &[first.clone(), second(&b)]).is_ok());
    b.stratum.origin = "peripheral-origin".into();
    assert!(rank_conditions(&grid, &[first.clone(), second(&b)]).is_err());
    b.stratum.origin = a.stratum.origin.clone();
    b.stratum.group = "different-cohort".into();
    assert!(rank_conditions(&grid, &[first, second(&b)]).is_err());
}
#[test]
fn e13_s08_given_same_id_different_snapshot_or_failed_budget_then_rejected() {
    let h = fixture::base::history();
    let b = fixture::proposal(&h, "same-id");
    let gate = interventions::feasibility::screen(&h, &b, &fixture::inputs(&h, &b)).unwrap();
    let mut i = access_input();
    i.context.scenario = gate.snapshot().id().clone();
    i.context.graph_revision = gate.snapshot().network().data().revision.clone();
    i.context.graph_snapshot = gate.snapshot().network().encode();
    let t = trips(
        &i.context,
        &[trip("a", Some(false), Some(true), Some(Mode::Car))],
        unavailable(),
    )
    .unwrap();
    let mut wrong = i.clone();
    wrong.context.graph_snapshot.push(1);
    let a = access(&wrong).unwrap();
    assert!(Candidate::screened(&gate, &a, &t, None, "registry").is_err());
    let mut too_low = fixture::inputs(&h, &b);
    too_low.budget.capital = 0.0;
    assert!(interventions::feasibility::screen(&h, &b, &too_low).is_err());
    let (d, _) = fixture::base::inputs(gate.snapshot());
    i.context.demand_revision = d.revision.clone();
    let rows = demand_rows(&d);
    let binding = interventions::evaluation::DemandBinding {
        scenario_id: i.context.scenario.clone(),
        demand_revision: d.revision.clone(),
        land_use_revision: None,
    };
    let good = trips(&i.context, &rows, unavailable()).unwrap();
    assert!(accessibility::demand_binding::bind_demand(good, &gate, &d, &binding, 1.0).is_ok());
    let mut changed = rows;
    changed[0].id = "different-member-same-denominator".into();
    let wrong = trips(&i.context, &changed, unavailable()).unwrap();
    assert!(accessibility::demand_binding::bind_demand(wrong, &gate, &d, &binding, 1.0).is_err());
}
