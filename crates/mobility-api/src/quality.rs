//! Standalone E04 quality dashboard. All scores are calculated by mobility-ingest.
use mobility_ingest::connectors::normalization::Snapshot;
use std::io::{self, Write};

fn escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn regions(items: &[String]) -> String {
    if items.is_empty() {
        return "None".into();
    }
    let mut out = String::from("<ul>");
    for item in items {
        out.push_str("<li>");
        out.push_str(&escape(item));
        out.push_str("</li>");
    }
    out.push_str("</ul>");
    out
}

/// Render a dashboard from a loaded snapshot; `id` identifies its lineage manifest.
pub fn render(out: &mut impl Write, id: &str, snapshot: &Snapshot) -> io::Result<()> {
    write!(out, "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Data quality dashboard</title><style>body{{max-width:72rem;margin:1rem auto;padding:0 1rem;font:1rem system-ui;color:#172126;background:#fff}}table{{border-collapse:collapse}}td,th{{border:1px solid #758289;padding:.5rem;text-align:left;vertical-align:top}}.table{{overflow:auto}}code{{overflow-wrap:anywhere}}</style></head><body><main><h1>Data quality dashboard</h1><p>Report schema 1. Snapshot SHA-256: <code>{}</code>. Processing: <code>{}</code>. Assessment date: {}.</p>", escape(id), escape(&snapshot.processing_version), escape(&snapshot.recipe.as_of))?;
    write!(out, "<p>Completeness measures successful import partitions, not spatial or schedule coverage within a region. An absent or rejected partition is not zero demand. Uncertainty is the fraction missing, rejected or marked for review; it is not a statistical interval. Screened data does not establish realistic journeys, accessibility, calibration, or car independence.</p><div class=\"table\" role=\"region\" aria-label=\"Quality summary table\" tabindex=\"0\"><table><caption>Quality by input layer</caption><thead><tr>")?;
    for header in [
        "Layer",
        "Completeness",
        "Uncertainty",
        "Confidence tier",
        "Missing regions",
        "Rejected regions",
        "Review regions",
        "Invalid findings",
    ] {
        write!(out, "<th scope=\"col\">{header}</th>")?;
    }
    write!(out, "</tr></thead><tbody>")?;
    for (kind, q) in &snapshot.quality {
        write!(out, "<tr><th scope=\"row\">{}</th><td>{:.1}%</td><td>{:.1}%</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>", escape(kind), q.completeness*100.0, q.uncertainty*100.0, escape(&q.confidence), regions(&q.missing_regions), regions(&q.rejected_regions), regions(&q.review_regions), q.invalid_findings)?;
    }
    write!(
        out,
        "</tbody></table></div><h2>Source and review details</h2>"
    )?;
    for layer in &snapshot.layers {
        let input = snapshot
            .recipe
            .inputs
            .get(layer.input)
            .ok_or_else(|| io::Error::other("missing input for layer"))?;
        write!(
            out,
            "<section><h3>{} — {}</h3><ul>",
            escape(&layer.source.kind),
            escape(&input.region)
        )?;
        for item in
            std::iter::once(&layer.source.inventory).chain(layer.source.realtime_inventory.as_ref())
        {
            let s = &item.source;
            write!(
                out,
                "<li>Source: {} ({}). Attribution: {}. Terms: {}. Retrieved: {}.</li>",
                escape(&s.id),
                escape(&s.version),
                escape(&s.attribution),
                escape(&s.license_terms),
                escape(&s.retrieved_on)
            )?;
        }
        write!(out, "</ul>")?;
        let r = &layer.source.report;
        let count = r.errors.len() + r.warnings.len() + layer.findings.len();
        if count > 0 {
            write!(out, "<details><summary>{count} recorded finding(s); {} omitted connector errors</summary><ul>", r.omitted_errors)?;
            for (label, entries) in [("Invalid input", &r.errors), ("Warning", &r.warnings)] {
                for diagnostic in entries {
                    let row = diagnostic
                        .row
                        .map(|r| format!(", row {r}"))
                        .unwrap_or_default();
                    write!(
                        out,
                        "<li>{label} ({}{row}): {}</li>",
                        escape(&diagnostic.input),
                        escape(&diagnostic.message)
                    )?;
                }
            }
            for f in layer.findings.iter().take(200) {
                write!(
                    out,
                    "<li>{}: {} ({}): {}</li>",
                    if f.invalid {
                        "Invalid or out of range"
                    } else {
                        "Review"
                    },
                    escape(&f.code),
                    escape(&f.record),
                    escape(&f.detail)
                )?;
            }
            if layer.findings.len() > 200 {
                write!(out, "<li>Showing the first 200 normalization findings; see the snapshot for the full list.</li>")?;
            }
            write!(out, "</ul></details>")?;
        }
        write!(out, "</section>")?;
    }
    writeln!(out, "</main></body></html>")
}
