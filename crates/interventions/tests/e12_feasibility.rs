//! E12 acceptance: Given explicit synthetic evidence, When screened, Then block
//! hard failures before assignment and retain an auditable deterministic report.
#[path = "../examples/support/e12.rs"]
mod fixture;
use fixture::*;
use interventions::{
    feasibility::*,
    scenario::{Batch, Change},
};
use mobility_graph::{network::*, EdgeId, Mode};

fn setup() -> (interventions::scenario::History, Batch, Inputs) {
    let h = base::history();
    let b = proposal(&h, "screened");
    let i = inputs(&h, &b);
    (h, b, i)
}
#[test]
fn e12_s01_s05_given_cost_assumptions_when_screened_then_categories_year_factors_and_lifecycle_are_exposed(
) {
    let (h, b, i) = setup();
    let s = screen(&h, &b, &i).unwrap();
    assert_eq!(
        s.report().construction,
        Bounds {
            lower: 24.0,
            central: 30.0,
            upper: 36.0
        }
    );
    assert_eq!(s.report().maintenance.central, 30.0);
    assert_eq!(s.report().disruption.central, 30.0);
    // One 20-second covered run: 0.2 major units x 10 repeats = 2/year.
    assert!((s.report().transit_annual - 2.0).abs() < 1e-12);
    assert_eq!(s.report().capital.upper, 72.0);
    assert_eq!(s.report().annual.upper, 38.0);
    assert_eq!(s.report().lifecycle.central, 380.0);
    assert_eq!(s.inputs(), &i);
    assert_eq!(h.current().id, "baseline");
    assert_eq!(s.snapshot().id, "screened");
    let mut exact = i.clone();
    exact.budget.capital = 72.0;
    exact.budget.annual = 38.0;
    assert!(screen(&h, &b, &exact).is_ok());
    exact.budget.capital = 71.999;
    assert!(screen(&h, &b, &exact).unwrap_err().contains("capital"));
    exact.budget.capital = 72.0;
    exact.budget.annual = 37.999;
    assert!(screen(&h, &b, &exact).unwrap_err().contains("annual"));
}
#[test]
fn e12_s01_s05_given_bad_or_omitted_costs_when_screened_then_fail_closed() {
    let (h, b, i) = setup();
    for bad in [f64::NAN, f64::INFINITY, -1.0] {
        let mut x = i.clone();
        x.costs[0].quantity = bad;
        assert!(screen(&h, &b, &x).is_err());
    }
    let mut x = i.clone();
    x.costs[0].price_year = 2020;
    assert!(screen(&h, &b, &x).unwrap_err().contains("price year"));
    let mut x = i.clone();
    x.costs[0].currency = "EUR".into();
    assert!(screen(&h, &b, &x).is_err());
    let mut x = i.clone();
    x.costs.retain(|c| c.category != Category::Maintenance);
    assert!(screen(&h, &b, &x).is_err());
    let mut x = i.clone();
    x.transit.clear();
    assert!(screen(&h, &b, &x)
        .unwrap_err()
        .contains("transit cost coverage"));
    let mut x = i.clone();
    x.transit.push(x.transit[0].clone());
    assert!(screen(&h, &b, &x).is_err());
    let mut x = i.clone();
    x.costs[0].rate.upper = f64::MAX;
    assert!(screen(&h, &b, &x).is_err());
    let mut x = i.clone();
    x.transit[0].annualization.clear();
    assert!(screen(&h, &b, &x).is_err());
}
#[test]
fn e12_s02_s08_given_width_and_engineering_failures_when_screened_then_segment_reasons_are_reported(
) {
    let (h, b, i) = setup();
    let mut x = i.clone();
    x.segments[0].lanes[0].width_mm = 2000;
    let e = screen(&h, &b, &x).unwrap_err();
    assert!(e.contains(&x.segments[0].id));
    assert!(e.contains("2000 < 2500"));
    let mut x = i.clone();
    x.rows[0].available_mm = 500;
    assert!(screen(&h, &b, &x).unwrap_err().contains("right-of-way"));
    let mut x = i.clone();
    x.segments[0].lanes.pop();
    assert!(screen(&h, &b, &x).unwrap_err().contains("lane inventory"));
    for restriction in Restriction::ALL {
        let mut x = i.clone();
        x.segments[0].restrictions.insert(
            restriction,
            Evidence::Restricted("assessed bridge load/grade/utility/terrain violation".into()),
        );
        let e = screen(&h, &b, &x).unwrap_err();
        assert!(e.contains(&format!("{restriction:?}")));
        assert!(e.contains("restricted"));
        x.segments[0].restrictions.remove(&restriction);
        assert!(screen(&h, &b, &x)
            .unwrap_err()
            .contains("missing engineering evidence"));
    }
    let mut x = i.clone();
    x.segments.remove(0);
    assert!(screen(&h, &b, &x).is_err());
    // Both direction inventories must consume the same explicitly shared ROW.
    let mut x = i.clone();
    let row = x.segments[0].row.clone();
    x.segments[1].row = row.clone();
    x.rows.retain(|r| r.id != i.segments[1].row);
    assert!(screen(&h, &b, &x).is_ok()); // exact 20,000 mm shared boundary
    x.rows
        .iter_mut()
        .find(|r| r.id == row)
        .unwrap()
        .available_mm = 19000;
    assert!(screen(&h, &b, &x).unwrap_err().contains("right-of-way"));
}
#[test]
fn e12_s03_s04_given_each_missing_access_class_or_unreachable_return_when_screened_then_blocked() {
    let (h, b, i) = setup();
    for kind in AccessKind::ALL {
        let mut x = i.clone();
        x.access.retain(|a| a.kind != kind);
        assert!(screen(&h, &b, &x)
            .unwrap_err()
            .contains(&format!("{kind:?}")));
        let mut x = i.clone();
        let a = x.access.iter_mut().find(|a| a.kind == kind).unwrap();
        a.returning.deadline_utc = 1;
        let e = screen(&h, &b, &x).unwrap_err();
        assert!(e.contains("return"));
        assert!(e.contains("unmet"));
    }
    let mut x = i.clone();
    x.access[0].permitted_car_edges = None;
    assert!(screen(&h, &b, &x).is_err());
    let mut x = i.clone();
    x.access[1].outbound.profile.step_free = false;
    assert!(screen(&h, &b, &x).is_err());
    let mut x = i.clone();
    x.access[1]
        .outbound
        .profile
        .allowed_modes
        .push(Mode::Bicycle);
    assert!(screen(&h, &b, &x).is_err());
    let mut x = i.clone();
    x.access[0].outbound.max_states = 1;
    assert!(screen(&h, &b, &x).unwrap_err().contains("routing error"));
}
#[test]
fn e12_s04_given_turn_restricted_critical_return_when_permission_filtered_then_no_illegal_detour() {
    let mut n = base::network().data().clone();
    // Return direct edge excluded by permission; detour's sole permitted exit is blocked.
    n.turns.push(TurnRule {
        incoming: EdgeId(102),
        outgoing: EdgeId(103),
        only: false,
    });
    let h = base::history_for(Network::new(n).unwrap(), vec![]);
    let b = proposal(&h, "turn-check");
    let mut i = inputs(&h, &b);
    for a in &mut i.access {
        if let Some(edges) = &mut a.permitted_car_edges {
            edges.remove(&EdgeId(8));
        }
    }
    let e = screen(&h, &b, &i).unwrap_err();
    assert!(e.contains("return"));
    assert!(e.contains("unmet"));
}
#[test]
fn e12_s02_given_new_offroad_facility_when_evidence_omitted_then_blocked() {
    let h = base::history();
    let b = base::batch(
        &h,
        "walk",
        vec![Change::Pedestrian {
            link: base::street(300, 1, 2, Mode::Walk, 10),
            assessment: base::approval(false, 8),
        }],
    );
    let mut i = inputs(&h, &b);
    i.segments.retain(|s| !s.edges.contains(&EdgeId(300)));
    assert!(screen(&h, &b, &i).unwrap_err().contains("300"));
}
#[test]
fn e12_s06_given_independent_exposure_model_when_reported_then_units_calibration_and_bounds_survive(
) {
    let (h, b, i) = setup();
    let s = screen(&h, &b, &i).unwrap();
    let mut r = RiskInput {
        exposure_scenario_id: s.snapshot().id.clone(),
        exposure_graph_revision: s.snapshot().network.data().revision.clone(),
        model: "synthetic exposure-rate".into(),
        version: "1".into(),
        mode: Mode::Walk,
        severity: "all recorded collisions (synthetic)".into(),
        unit: ExposureUnit::PersonKm,
        exposure: 1000.0,
        exposure_source: base::synthetic(),
        calibration: Some(base::synthetic()),
        applicability: "abstract corridor only; no empirical calibration".into(),
        period_start: 0,
        period_end: 60,
        rate: Bounds {
            lower: 0.001,
            central: 0.002,
            upper: 0.003,
        },
        uncertainty: "supplied rate sensitivity bounds, not a confidence interval".into(),
    };
    let risk = s.risk(&r).unwrap();
    assert_eq!(
        risk.expected_collisions,
        Some(Bounds {
            lower: 1.0,
            central: 2.0,
            upper: 3.0
        })
    );
    assert_eq!(risk.input, r);
    assert_eq!(risk.basis, Basis::Modeled);
    r.calibration = None;
    assert!(s.risk(&r).unwrap().expected_collisions.is_none());
    r.exposure = -1.0;
    assert!(s.risk(&r).is_err());
    r.exposure = 1000.0;
    r.unit = ExposureUnit::VehicleKm;
    assert!(s.risk(&r).unwrap_err().contains("incompatible"));
    r.unit = ExposureUnit::PersonKm;
    r.calibration = Some(base::synthetic());
    r.calibration.as_mut().unwrap().geography = "unrelated region".into();
    assert!(s.risk(&r).unwrap_err().contains("geography mismatch"));
    r.calibration = Some(base::synthetic());
    r.exposure_graph_revision = "unrelated-network".into();
    assert!(s.risk(&r).unwrap_err().contains("scenario/revision"));
    r.exposure_graph_revision = s.snapshot().network.data().revision.clone();
    r.exposure_scenario_id = "unrelated-proposal".into();
    assert!(s.risk(&r).unwrap_err().contains("scenario/revision"));
    r.exposure_scenario_id = s.snapshot().id.clone();
    r.period_end = 241;
    assert!(s.risk(&r).is_err());
}

#[test]
fn e12_s05_given_real_discount_when_lifecycle_computed_then_independent_present_value_matches() {
    let (h, b, mut i) = setup();
    i.budget.years = 2;
    i.budget.real_discount_rate = 0.1;
    let s = screen(&h, &b, &i).unwrap();
    assert!((s.report().lifecycle.central - (60.0 + 32.0 / 1.1 + 32.0 / 1.21)).abs() < 1e-12);
}

#[test]
fn e12_s03_given_inaccessible_property_door_when_step_free_access_screened_then_blocked() {
    let mut n = base::network().data().clone();
    n.nodes.iter_mut().find(|n| n.id.0 == 1).unwrap().access = Access::Steps;
    let h = base::history_for(Network::new(n).unwrap(), vec![]);
    let b = proposal(&h, "steps");
    let mut i = inputs(&h, &b);
    // Vehicle eligibility is assessed independently; allow steps on car samples
    // so the failed disability door (not a prior vehicle class) is observed.
    for a in &mut i.access {
        if a.kind != AccessKind::Disability && a.kind != AccessKind::Property {
            a.outbound.profile.step_free = false;
            a.returning.profile.step_free = false;
        }
    }
    assert!(screen(&h, &b, &i).unwrap_err().contains("Disability"));
}
