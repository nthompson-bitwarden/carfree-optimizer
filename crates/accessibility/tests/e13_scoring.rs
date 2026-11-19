//! E13-S01/S02/S03/S04/S05/S06/S07/S09/S10 Given/When/Then fixtures.
mod support;
use accessibility::{comparison::*, scoring::*, spatial::*};
use support::*;
#[test]
fn e13_s01_s02_given_four_paired_outcomes_then_denominators_and_reliability_reconcile() {
    let i = access_input();
    let s = access(&i).unwrap();
    assert_eq!(
        (s.eligible, s.evaluated, s.reached, s.missing),
        (4, 4, 2, 0)
    );
    assert_eq!(s.reachable_share, Some(0.5));
    assert_eq!(s.paired_feasibility, [1, 1, 1, 1]);
    assert_eq!(
        (s.car_free.p50_seconds, s.car_free.p95_seconds),
        (Some(50), Some(60))
    );
    assert_eq!(s.mean_time_difference_seconds, Some(20.0));
    assert_eq!(s.context, i.context);
}
#[test]
fn e13_s01_given_missing_or_duplicate_cell_then_no_false_complete_share() {
    let mut i = access_input();
    i.pairs[0].car_free = Evidence::Missing("routing evidence not supplied".into());
    let s = access(&i).unwrap();
    assert_eq!(
        (s.reachable_share, s.missing, s.unknown_pairs),
        (None, 1, 1)
    );
    i.pairs[0] = i.pairs[1].clone();
    assert!(access(&i).is_err());
    i = access_input();
    i.pairs.pop();
    assert!(access(&i).is_err());
    i = access_input();
    i.stratum.budget_seconds = 59;
    assert!(access(&i).is_err());
    i = access_input();
    i.stratum.modes.push(Mode::Car);
    assert!(access(&i).is_err());
}
#[test]
fn e13_s03_s06_given_neither_denied_and_unknown_then_not_all_failures_are_car_required() {
    let mut rows = vec![
        trip("car-only", Some(false), Some(true), Some(Mode::Car)),
        trip("neither", Some(false), Some(false), None),
        trip("capacity-denied", Some(true), Some(true), None),
    ];
    rows[1].group = "wheelchair-user".into();
    let s = trips(&context(), &rows, unavailable()).unwrap();
    assert_eq!(
        (
            s.all.total,
            s.all.car_required,
            s.all.no_mode,
            s.all.unserved
        ),
        (6.0, 2.0, 2.0, 4.0)
    );
    assert_eq!(s.all.car_required_share(), Some(1.0 / 3.0));
    assert_eq!(
        s.by_group_neighborhood[&("wheelchair-user".into(), "north".into())].no_mode,
        2.0
    );
    rows.push(trip("unknown", None, Some(true), None));
    assert_eq!(
        trips(&context(), &rows, unavailable())
            .unwrap()
            .all
            .car_required_share(),
        None
    );
    rows[0].weight = f64::NAN;
    assert!(trips(&context(), &rows, unavailable()).is_err());
}
#[test]
fn e13_s03_given_confidence_bounds_then_method_level_and_sampling_unit_required() {
    let rows = [
        trip("a", Some(false), Some(true), Some(Mode::Car)),
        trip("b", Some(true), Some(true), Some(Mode::Walk)),
    ];
    let ci = Uncertainty::Confidence {
        lower: 0.1,
        upper: 0.9,
        level: 0.95,
        binding: IntervalBinding {
            inventory: trip_inventory(&rows),
            context: context(),
            metric: ShareMetric::CarRequiredAllTrips,
            denominator_trip_weight: 4.0,
            design_revision: "synthetic-design-v1".into(),
        },
        method: "caller-supplied survey design interval".into(),
        sampling_unit: "independent household".into(),
        source: "synthetic interval metadata test only".into(),
    };
    assert!(trips(&context(), &rows, ci.clone()).is_ok());
    if let Uncertainty::Confidence {
        mut lower,
        upper,
        level,
        binding,
        method,
        sampling_unit,
        source,
    } = ci
    {
        lower += 0.5;
        assert!(trips(
            &context(),
            &rows,
            Uncertainty::Confidence {
                lower,
                upper,
                level,
                binding,
                method,
                sampling_unit,
                source
            }
        )
        .is_err());
    }
}
#[test]
fn e13_s04_s06_given_uncovered_neighborhood_then_population_and_coverage_remain_visible() {
    let area = |id: &str| Neighborhood {
        id: id.into(),
        population: 100.0,
        ring: vec![(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)],
    };
    let areas = [area("north"), area("south")];
    let origins = [
        Origin {
            id: "origin-a".into(),
            neighborhood: "north".into(),
            group: "noncyclist".into(),
            population: 30.0,
            score: Some(accessibility::projection::Projection::from_access(
                &access(&access_input()).unwrap(),
            )),
        },
        Origin {
            id: "b".into(),
            neighborhood: "north".into(),
            group: "other".into(),
            population: 70.0,
            score: None,
        },
    ];
    let m = neighborhoods(&context(), &areas, &origins, 0.8).unwrap();
    assert_eq!(
        (
            m.areas[0].total_population,
            m.areas[0].represented_population,
            m.areas[0].coverage
        ),
        (100.0, 30.0, Some(0.3))
    );
    assert!(m.areas.iter().all(|a| a.insufficient_coverage));
    assert_eq!(m.areas[1].represented_accessibility, None);
    assert_eq!(
        m.groups[&("north".into(), "noncyclist".into())],
        (30.0, 30.0, Some(0.5))
    );
    let mut duplicate = origins.to_vec();
    duplicate.push(origins[0].clone());
    assert!(neighborhoods(&context(), &areas, &duplicate, 0.8).is_err());
}
#[test]
fn e13_s05_given_opposing_edges_and_boundary_split_then_shared_footprint_counts_once() {
    let f = Footprint {
        id: "physical-segment-a".into(),
        area_m2: 1000.0,
        private_car_fraction: 0.6,
        neighborhood_fractions: [("north".into(), 0.25), ("south".into(), 0.75)].into(),
    };
    let inventory = Inventory {
        ids: [f.id.clone()].into(),
        completeness_source: "synthetic disjoint footprint ledger v1".into(),
    };
    let r = road_space(&context(), &inventory, &[f.clone(), f.clone()]).unwrap();
    assert!(road_space(&context(), &inventory, &[]).is_err());
    assert_eq!((r.total_m2, r.private_car_m2), (1000.0, 600.0));
    assert_eq!(r.by_neighborhood_m2.values().sum::<f64>(), 600.0);
    assert_eq!(r.by_neighborhood_m2["north"], 150.0);
    let mut conflict = f.clone();
    conflict.area_m2 = 900.0;
    assert!(road_space(&context(), &inventory, &[f, conflict]).is_err());
}
#[test]
fn e13_s07_given_known_vehicle_km_and_factors_then_bounds_are_not_confidence_intervals() {
    let a = VehicleActivity {
        id: "road-fleet".into(),
        vehicle_km: Bounds {
            low: 8.0,
            central: 10.0,
            high: 12.0,
        },
        operational_kg_per_vehicle_km: Bounds {
            low: 0.1,
            central: 0.2,
            high: 0.3,
        },
        methodology_source:
            "synthetic tailpipe+operational factor; bounded inputs, no lifecycle emissions".into(),
        fleet: "synthetic road fleet v1".into(),
        period: context().period,
        factor_revision: "synthetic kg CO2e / vehicle-km v1".into(),
    };
    let inventory = Inventory {
        ids: [a.id.clone()].into(),
        completeness_source: "complete synthetic fleet activity ledger v1".into(),
    };
    let s = emissions(&context(), &inventory, &[a]).unwrap();
    assert!(emissions(&context(), &inventory, &[]).is_err());
    assert_eq!(s.operational_kg_co2e.low, 0.8);
    assert_eq!(s.operational_kg_co2e.central, 2.0);
    assert!((s.operational_kg_co2e.high - 3.6).abs() < 1e-12);
}
#[test]
fn e13_s09_s10_given_weather_service_rank_reversal_and_missing_condition_then_no_ideal_day_bias() {
    let day = Condition {
        weather: "dry".into(),
        day_or_night: "day".into(),
        service_revision: "frequent-v1".into(),
    };
    let night = Condition {
        weather: "ice-no-cycling".into(),
        day_or_night: "night".into(),
        service_revision: "night-hourly-v1".into(),
    };
    let mut rows = Vec::new();
    for (id, d, n) in [
        ("a", 0.9, Some(0.1)),
        ("b", 0.7, Some(0.6)),
        ("missing", 1.0, None),
    ] {
        let mut c = context();
        c.scenario = id.into();
        c.service_revision = day.service_revision.clone();
        c.graph_revision = format!("{id}-day-graph");
        rows.push(ConditionScore {
            condition: day.clone(),
            score: projection(c.clone(), Some(d)),
        });
        c.service_revision = night.service_revision.clone();
        c.graph_revision = format!("{id}-night-graph");
        rows.push(ConditionScore {
            condition: night.clone(),
            score: projection(c, n),
        });
    }
    let grid = [(day.clone(), 1.0), (night.clone(), 1.0)];
    let r = rank_conditions(&grid, &rows).unwrap();
    assert_eq!(r.ranked[0].0, "b");
    assert_eq!(r.unranked, ["missing"]);
    assert_eq!(r.per_condition[&day][1].0, "a");
    assert_eq!(r.per_condition[&night][0].0, "b");
    let mut long = rows[0].score.context().clone();
    long.horizon = Horizon::LongRun;
    rows[0].score = projection(long, Some(0.9));
    assert!(rank_conditions(&grid, &rows).is_err());
    let mut c = context();
    c.behavior.clear();
    assert!(trips(&c, &[], unavailable()).is_err());
}
