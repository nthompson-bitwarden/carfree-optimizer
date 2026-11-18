//! E09-S06 review regression: overflow must not suppress results/exports.
#[path = "../../mobility-sim/examples/support/e09.rs"]
mod fixture;
use mobility_api::calibration::{export_tsv, render};
use mobility_sim::{calibration::run, simulation::Iterative};
#[test]
fn e09_s06_failed_holdout_arithmetic_preserves_both_outputs_and_fit() {
    let (n, d, s, mut r) = fixture::inputs();
    r.observations[1].value = Some(0.0);
    r.observations[1].scale = f64::MIN_POSITIVE / 100.0;
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert_eq!(report.selected(), Some(0));
    let html = render(&report);
    let tsv = export_tsv(&report);
    for limitation in report.limitations() {
        assert!(html.contains(limitation));
        assert!(tsv.contains(limitation));
    }
    assert!(html.contains("calculation failed"));
    assert!(tsv.contains("calculation failed"));
    assert!(html.contains("directed-link"));
    assert!(html.contains("Abstract west/east zones / 2026-01-05"));
    assert!(tsv.contains("observation.0\tnetwork_revision\tcorridor"));
    assert!(tsv.contains("observation.0\tdemand_revision\tsynthetic-demand-v1"));
}

#[test]
fn e09_s03_s06_partial_survey_coverage_is_visible_in_results_and_export() {
    use mobility_sim::calibration::{Split, SurveyMeasure, Target};
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
    for output in [render(&report), export_tsv(&report)] {
        assert!(output.contains("survey demand-environment time coverage"));
        assert!(output.contains("LIMITED FITNESS"));
    }
}
#[test]
fn e09_s06_static_results_have_textual_fitness_and_accessible_table_structure() {
    let (n, d, s, r) = fixture::inputs();
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    let html = render(&report);
    for marker in [
        "lang=\"en\"",
        "<main>",
        "<h1>Baseline calibration</h1>",
        "aria-label=\"Calibration fitness\"",
        "role=\"status\"",
        "<caption>",
        "<thead>",
        "scope=\"row\"",
    ] {
        assert!(
            html.contains(marker),
            "missing accessibility structure: {marker}"
        );
    }
    assert!(html.contains("LIMITED FITNESS")); // not conveyed by color alone
    assert!(!html.contains("<script")); // no browser calculation or interaction dependency
}
