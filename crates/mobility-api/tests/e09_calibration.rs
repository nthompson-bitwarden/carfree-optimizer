//! E09-S05/S06 presentation/export acceptance, synthetic evidence only.
#[path = "../../mobility-sim/examples/support/e09.rs"]
mod fixture;
use mobility_api::calibration::{export_tsv, render};
use mobility_sim::calibration::{run, Split};
use mobility_sim::simulation::Iterative;

#[test]
fn e09_s06_results_and_exports_share_every_limitation_including_no_candidate() {
    let (n, d, mut s, mut r) = fixture::inputs();
    r.observations[1].value = Some(90.0);
    for capped in [false, true] {
        if capped {
            s.settings.max_iterations = 1;
        }
        let report = run(&Iterative, &n, &d, &s, &r).unwrap();
        let html = render(&report);
        let tsv = export_tsv(&report);
        assert!(html.contains(report.banner()));
        assert!(tsv.contains(report.banner()));
        for limitation in report.limitations() {
            assert!(html.contains(limitation), "{limitation}");
            assert!(tsv.contains(limitation), "{limitation}");
        }
        assert!(html.contains("Holdout") || capped);
        assert!(tsv.contains("schema_version\t1"));
        assert!(html.contains("Fitting candidates (holdout excluded)"));
        assert!(html.contains("Separate holdout scores"));
        if capped {
            assert!(html.contains("Selected candidate: none"));
            assert!(html.contains("Nonconverged"));
        }
    }
}
#[test]
fn e09_s05_export_is_deterministic_preserves_source_split_seed_parameters_and_errors() {
    let (n, d, s, r) = fixture::inputs();
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    let tsv = export_tsv(&report);
    assert_eq!(tsv, export_tsv(&run(&Iterative, &n, &d, &s, &r).unwrap()));
    for expected in [
        "source_version\te06-corridor-v1",
        "date\t2026-01-05",
        "basis\tModeled",
        "split\tFit",
        "split\tHoldout",
        "seed\t7",
        "seed\t42",
        "demand_multiplier.before\t1",
        "demand_multiplier.delta\t1",
        "unit\tvehicles",
        "residual\t0",
        "effective_network_revision\t",
        "scenario_snapshot\t",
        "score_population_variance\t0",
    ] {
        assert!(tsv.contains(expected), "{expected}");
    }
    for line in tsv.lines() {
        assert_eq!(line.split('\t').count(), 3);
    }
    assert_eq!(report.request().observations[1].split, Split::Holdout);
}
#[test]
fn e09_s06_escaped_source_project_and_flags_cannot_inject_html_or_tsv_rows() {
    let (n, d, s, mut r) = fixture::inputs();
    r.project_id = "<script>alert('x')</script>\t\n\\".into();
    r.observations[1].value = None;
    r.observations[1].missing_reason = Some("<img src=x onerror=alert(1)>\r\nFORGED\tpass".into());
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    let html = render(&report);
    let tsv = export_tsv(&report);
    assert!(!html.contains("<script>"));
    assert!(!html.contains("<img src=x"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(tsv.contains("\\r\\nFORGED\\tpass"));
    assert!(tsv.contains("\\t\\n\\\\"));
    for line in tsv.lines() {
        assert_eq!(line.split('\t').count(), 3);
    }
    assert!(html.contains("missing observation"));
    assert!(tsv.contains("missing observation"));
}
