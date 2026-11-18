//! E09 review regressions. Every fixture is synthetic, including basis-flag tests.
#[path = "../examples/support/e09.rs"]
mod fixture;
use mobility_graph::network::{Basis, Network};
use mobility_sim::calibration::*;
use mobility_sim::simulation::Iterative;

#[test]
fn e09_s01_s03_reused_ids_in_new_input_revisions_are_stale_crosswalks() {
    let (n, mut d, s, r) = fixture::inputs();
    let mut data = n.data().clone();
    data.revision = "new-graph-same-edge-ids".into();
    let n = Network::new(data).unwrap();
    d.network_revision = n.data().revision.clone();
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert_eq!(report.selected(), None);
    assert!(report.candidates()[0].runs[0].comparisons[0].flags[0].contains("stale crosswalk"));
    let (n, mut d, s, r) = fixture::inputs();
    d.revision = "new-demand-same-group-ids".into();
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert_eq!(report.selected(), None);
    assert_eq!(report.candidates()[0].runs[0].comparisons[0].modeled, None);
}
#[test]
fn e09_s02_scheduled_service_without_supply_is_not_a_covered_zero() {
    let (n, d, mut s, mut r) = fixture::inputs();
    // Car-only demand: E08 has no transit candidate and therefore does not need
    // supply for this unused scheduled trip. E09 still requires it for coverage.
    r.observations[0] = fixture::observation(
        "boardings",
        Split::Fit,
        Target::Transit {
            route_or_area: "route-A".into(),
            trips: vec!["outbound".into()],
        },
        0,
        61,
        0.0,
    );
    let covered = run(&Iterative, &n, &d, &s, &r).unwrap();
    let c = &covered.candidates()[0].runs[0].comparisons[0];
    assert_eq!(c.modeled, Some(0.0));
    assert!(c.flags.is_empty());
    s.transit.clear();
    let uncovered = run(&Iterative, &n, &d, &s, &r).unwrap();
    let c = &uncovered.candidates()[0].runs[0].comparisons[0];
    assert_eq!(c.modeled, None);
    assert!(c.flags[0].contains("transit model supply coverage"));
    assert_eq!(uncovered.selected(), None);
}
#[test]
fn e09_s04_source_republication_and_changed_geography_do_not_bypass_holdout_guard() {
    let (n, d, s, mut r) = fixture::inputs();
    r.observations[1].record = r.observations[0].record.clone();
    r.observations[1].source.source_version = "republished-v2".into();
    r.observations[1].source.geography = "renamed-geography".into();
    assert!(run(&Iterative, &n, &d, &s, &r)
        .unwrap_err()
        .0
        .contains("source record leaks"));
    r.observations[1].record = "different-record".into();
    r.observations[1].target = r.observations[0].target.clone();
    r.observations[1].start = 0;
    r.observations[1].end = 61;
    assert!(run(&Iterative, &n, &d, &s, &r)
        .unwrap_err()
        .0
        .contains("duplicate mapped"));
}
#[test]
fn e09_s06_holdout_normalization_overflow_keeps_fit_and_limitation_report() {
    let (n, d, s, mut r) = fixture::inputs();
    r.observations[1].value = Some(0.0);
    r.observations[1].scale = f64::MIN_POSITIVE / 100.0;
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert_eq!(report.selected(), Some(0));
    assert_eq!(report.candidates()[0].score, Some(0.0));
    assert_eq!(report.holdout()[0].score.normalized_rmse, None);
    assert!(report.holdout()[0].comparisons[0].flags[0].contains("calculation failed"));
    assert_eq!(report.fitness(), &Fitness::Limited);
    assert!(report
        .limitations()
        .iter()
        .any(|l| l.contains("Holdout") && l.contains("calculation failed")));
    // Finite normalization but overflowing squared-error aggregation is retained too.
    r.observations[1].scale = 1e-200;
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert_eq!(report.selected(), Some(0));
    assert!(report.holdout()[0].error.is_some());
}
#[test]
fn e09_s06_demand_basis_cannot_hide_behind_other_observed_basis_flags() {
    let (n, mut d, mut s, mut r) = fixture::inputs();
    // Deliberately manipulate metadata flags to isolate this branch. The source
    // name/uncertainty still explicitly say synthetic; no observation is claimed.
    let mut data = n.data().clone();
    data.provenance.basis = Basis::Observed;
    let n = Network::new(data).unwrap();
    s.provenance.basis = Basis::Observed;
    for o in &mut r.observations {
        o.source.basis = Basis::Observed;
    }
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert_eq!(report.fitness(), &Fitness::Limited);
    assert_eq!(report.limitations().len(), 1);
    assert!(report.limitations()[0].contains("modeled or imputed inputs"));
    d.provenance.basis = Basis::Observed;
    assert_eq!(
        run(&Iterative, &n, &d, &s, &r).unwrap().fitness(),
        &Fitness::ChecksPassed
    );
}
#[test]
fn e09_s03_s06_survey_interval_must_fit_demand_environment_not_only_graph() {
    let (n, d, mut s, mut r) = fixture::inputs();
    s.demand_scenario.environment.coverage.end = 200;
    r.observations.push(fixture::observation(
        "partial-survey",
        Split::Fit,
        Target::Survey {
            area: "abstract-cohort".into(),
            groups: vec!["adult".into()],
            purpose: None,
            measure: SurveyMeasure::ModeShare(mobility_sim::demand::TravelMode::Car),
        },
        0,
        241,
        1.0,
    ));
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    let c = &report.candidates()[0].runs[0].comparisons[1];
    assert_eq!(c.modeled, None);
    assert!(c.flags[0].contains("survey demand-environment time coverage"));
    assert_eq!(report.selected(), None);
    assert_eq!(report.fitness(), &Fitness::Limited);
}
#[test]
fn e09_s05_invalid_base_settings_and_signed_dates_cannot_pollute_audit_fields() {
    let (n, d, mut s, mut r) = fixture::inputs();
    s.settings.alpha = f64::NAN;
    assert!(run(&Iterative, &n, &d, &s, &r).is_err());
    s.settings.alpha = 0.0;
    r.observations[0].source.date = "+026-01-05".into();
    assert!(run(&Iterative, &n, &d, &s, &r).is_err());
}
