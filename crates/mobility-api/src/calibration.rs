//! E09 presentation and versioned audit export; calculations remain in mobility-sim.
use mobility_sim::calibration::{Report, Run, VERSION};
use std::fmt::Write;

fn html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn cell(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}
fn value(x: Option<f64>) -> String {
    x.map(|x| x.to_string()).unwrap_or_else(|| "missing".into())
}

/// Every result page is a project-specific envelope containing the same immutable
/// banner/limitations as the export, even when no fitting candidate was usable.
pub fn render(report: &Report) -> String {
    let mut out = String::from("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>Baseline calibration</title><main><h1>Baseline calibration</h1>");
    let _ = write!(out,"<p>Project: {}</p><section aria-label=\"Calibration fitness\"><h2>Fitness and limitations</h2><p role=\"status\">{}</p><ul>",html(&report.request().project_id),html(report.banner()));
    for limitation in report.limitations() {
        let _ = write!(out, "<li>{}</li>", html(limitation));
    }
    out.push_str("</ul></section><h2>Audit identity</h2>");
    let _ = write!(
        out,
        "<p>Scenario: {}; network: {}; demand: {}; engine: {}. Selected candidate: {}</p>",
        html(report.scenario_id()),
        html(report.network_revision()),
        html(report.demand_revision()),
        html(report.engine()),
        report
            .selected()
            .map(|i| html(&report.candidates()[i].parameters.label))
            .unwrap_or_else(|| "none".into())
    );
    out.push_str("<h2>Targets and source coverage</h2><table><caption>Exact UTC intervals; missing is not zero</caption><thead><tr><th>Target</th><th>Split</th><th>Location</th><th>Crosswalk / cohort definition</th><th>Source geography / date</th><th>Interval</th><th>Quantity / unit</th><th>Observed or synthetic target</th><th>Error scale</th><th>Source / version / basis</th><th>Uncertainty or missing data</th></tr></thead><tbody>");
    for o in &report.request().observations {
        let _ = write!(out,"<tr><th scope=\"row\">{}</th><td>{:?}</td><td>{}</td><td>{}; graph {}; demand {}</td><td>{} / {}</td><td>[{}, {})</td><td>{:?} / {}</td><td>{}</td><td>{}</td><td>{} / {} / {:?}</td><td>{}; {}</td></tr>",html(&o.id),o.split,html(o.target.location()),html(&format!("{:?}",o.target)),html(&o.network_revision),html(&o.demand_revision),html(&o.source.geography),html(&o.source.date),o.start,o.end,o.target.metric(),o.target.metric().unit(),value(o.value),o.scale,html(&o.source.source),html(&o.source.source_version),o.source.basis,html(&o.source.uncertainty),html(o.missing_reason.as_deref().unwrap_or("present")));
    }
    out.push_str("</tbody></table><h2>Fitting candidates (holdout excluded)</h2>");
    for c in report.candidates() {
        let _ = write!(out,"<section><h3>{}</h3><p>alpha: {}; demand multiplier: {}; preference seconds: {}; mean seed RMSE: {}; population variance: {}</p>",html(&c.parameters.label),c.parameters.alpha,c.parameters.demand_multiplier,c.parameters.preference_seconds,value(c.score),value(c.score_variance));
        for r in &c.runs {
            render_run(&mut out, r);
        }
        out.push_str("</section>");
    }
    out.push_str("<h2>Separate holdout scores (selected candidate only)</h2>");
    for r in report.holdout() {
        render_run(&mut out, r);
    }
    out.push_str("<p>Residual = modeled minus target. Declared error scale is not a confidence interval. Synthetic examples establish software behavior, not real-world accuracy.</p></main></html>");
    out
}
fn render_run(out: &mut String, r: &Run) {
    let _ = write!(out,"<h4>Seed {}</h4><p>Status: {:?}; usable coverage: {}/{}; normalized RMSE: {}; error: {}</p><table><caption>Per-target predictions and residuals</caption><thead><tr><th>Target</th><th>Modeled</th><th>Residual</th><th>Absolute scaled error</th><th>Eligible person trips</th><th>Unserved person trips</th><th>Missing-data / coverage flags</th></tr></thead><tbody>",r.seed,r.status,r.score.compared,r.score.expected,value(r.score.normalized_rmse),html(r.error.as_deref().unwrap_or("none")));
    for c in &r.comparisons {
        let _ = write!(out,"<tr><th scope=\"row\">{}</th><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",html(&c.observation),value(c.modeled),value(c.residual),value(c.normalized_error),value(c.eligible_person_trips),value(c.unserved_person_trips),html(&c.flags.join("; ")));
    }
    out.push_str("</tbody></table>");
}

/// Audit TSV v1, three columns record / field / value. Backslash, tab, CR and LF
/// are escaped in that order. Stable input ordering, no wall-clock timestamps.
/// This is an audit artifact, not a simulation-input loader or model snapshot.
pub fn export_tsv(report: &Report) -> String {
    let mut out = String::from("record\tfield\tvalue\n");
    let mut row = |record: &str, field: &str, v: &str| {
        let _ = writeln!(out, "{}\t{}\t{}", cell(record), cell(field), cell(v));
    };
    row("report", "schema_version", &VERSION.to_string());
    row("report", "project_id", &report.request().project_id);
    row("report", "banner", report.banner());
    row("report", "fitness", &format!("{:?}", report.fitness()));
    for (i, l) in report.limitations().iter().enumerate() {
        row("report", &format!("limitation.{i}"), l);
    }
    row("report", "scenario_id", report.scenario_id());
    row("report", "network_revision", report.network_revision());
    row("report", "demand_revision", report.demand_revision());
    row("report", "engine", report.engine());
    row(
        "report",
        "selected_index",
        &report
            .selected()
            .map(|i| i.to_string())
            .unwrap_or_else(|| "none".into()),
    );
    row(
        "report",
        "required_metrics",
        &format!("{:?}", report.request().required_metrics),
    );
    row("report", "seeds", &format!("{:?}", report.request().seeds));
    // Complete typed scenario diagnostic includes fixed demand/cost settings,
    // periods, edits and supplies. Original network/demand inputs remain external.
    row(
        "report",
        "scenario_snapshot",
        &format!("{:?}", report.scenario_snapshot()),
    );
    for (name, p) in [
        ("scenario_source", report.scenario_source()),
        ("network_source", report.network_source()),
        ("demand_source", report.demand_source()),
    ] {
        source(&mut row, name, p);
    }
    let base = report.base_settings();
    for (key, x) in [
        ("alpha", base.alpha),
        ("beta", base.beta),
        ("demand_multiplier", base.demand_multiplier),
        ("preference_seconds", base.preference_seconds),
        ("vehicles_per_trip", base.vehicles_per_trip),
        ("tolerance", base.tolerance),
    ] {
        row("base_settings", key, &x.to_string());
    }
    row("base_settings", "seed", &base.seed.to_string());
    row(
        "base_settings",
        "max_iterations",
        &base.max_iterations.to_string(),
    );
    row(
        "base_settings",
        "max_states_per_query",
        &base.max_states_per_query.to_string(),
    );
    for (i, o) in report.request().observations.iter().enumerate() {
        let key = format!("observation.{i}");
        row(&key, "id", &o.id);
        row(&key, "record", &o.record);
        row(&key, "split", &format!("{:?}", o.split));
        row(&key, "mapping", &format!("{:?}", o.target));
        row(&key, "location", o.target.location());
        row(&key, "network_revision", &o.network_revision);
        row(&key, "demand_revision", &o.demand_revision);
        row(&key, "start_utc", &o.start.to_string());
        row(&key, "end_utc", &o.end.to_string());
        row(&key, "metric", &format!("{:?}", o.target.metric()));
        row(&key, "unit", o.target.metric().unit());
        row(&key, "target", &value(o.value));
        row(&key, "scale", &o.scale.to_string());
        row(
            &key,
            "missing_reason",
            o.missing_reason.as_deref().unwrap_or("none"),
        );
        source(&mut row, &format!("{key}.source"), &o.source);
        source(&mut row, &format!("{key}.crosswalk"), &o.crosswalk);
    }
    for (i, c) in report.candidates().iter().enumerate() {
        let key = format!("candidate.{i}");
        row(&key, "label", &c.parameters.label);
        row(&key, "mean_seed_normalized_rmse", &value(c.score));
        row(&key, "score_population_variance", &value(c.score_variance));
        for (name, b, v) in [
            ("alpha", base.alpha, c.parameters.alpha),
            (
                "demand_multiplier",
                base.demand_multiplier,
                c.parameters.demand_multiplier,
            ),
            (
                "preference_seconds",
                base.preference_seconds,
                c.parameters.preference_seconds,
            ),
        ] {
            row(&key, name, &v.to_string());
            row(&key, &format!("{name}.before"), &b.to_string());
            row(&key, &format!("{name}.delta"), &(v - b).to_string());
        }
        for (j, r) in c.runs.iter().enumerate() {
            export_run(&mut row, &format!("{key}.fit_run.{j}"), r);
        }
    }
    for (i, r) in report.holdout().iter().enumerate() {
        export_run(&mut row, &format!("holdout_run.{i}"), r);
    }
    out
}
fn source(
    row: &mut impl FnMut(&str, &str, &str),
    key: &str,
    p: &mobility_sim::calibration::SourceProvenance,
) {
    for (field, v) in [
        ("source", &p.source),
        ("source_version", &p.source_version),
        ("license", &p.license),
        ("geography", &p.geography),
        ("date", &p.date),
        ("uncertainty", &p.uncertainty),
    ] {
        row(key, field, v);
    }
    row(key, "basis", &format!("{:?}", p.basis));
}
fn export_run(row: &mut impl FnMut(&str, &str, &str), key: &str, r: &Run) {
    row(key, "seed", &r.seed.to_string());
    row(key, "status", &format!("{:?}", r.status));
    row(
        key,
        "effective_network_revision",
        r.effective_network_revision.as_deref().unwrap_or("missing"),
    );
    row(key, "error", r.error.as_deref().unwrap_or("none"));
    row(key, "expected", &r.score.expected.to_string());
    row(key, "compared", &r.score.compared.to_string());
    row(key, "normalized_rmse", &value(r.score.normalized_rmse));
    for (i, c) in r.comparisons.iter().enumerate() {
        let key = format!("{key}.comparison.{i}");
        row(&key, "observation", &c.observation);
        for (name, v) in [
            ("modeled", c.modeled),
            ("residual", c.residual),
            ("normalized_error", c.normalized_error),
            ("eligible_person_trips", c.eligible_person_trips),
            ("unserved_person_trips", c.unserved_person_trips),
        ] {
            row(&key, name, &value(v));
        }
        row(&key, "flags", &c.flags.join("; "));
    }
}
