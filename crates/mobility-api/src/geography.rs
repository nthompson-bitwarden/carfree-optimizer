//! E02-S01 static, accessible geography discovery presentation.
use mobility_ingest::{Catalogue, Geography, GeographyKind, SearchError};
use std::fmt::Write;

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        escaped.push_str(match character {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            '\'' => "&#39;",
            _ => {
                escaped.push(character);
                continue;
            }
        });
    }
    escaped
}

fn boundary_path(area: &Geography) -> String {
    let points = || area.rings.iter().flatten();
    let min_lon = points().map(|p| p.longitude).fold(f64::INFINITY, f64::min);
    let max_lon = points()
        .map(|p| p.longitude)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_lat = points().map(|p| p.latitude).fold(f64::INFINITY, f64::min);
    let max_lat = points()
        .map(|p| p.latitude)
        .fold(f64::NEG_INFINITY, f64::max);
    // Normalize before scaling so very small, valid coordinate extents cannot
    // overflow a pixels-per-degree factor and emit NaN SVG coordinates.
    let span = (max_lon - min_lon).max(max_lat - min_lat);
    let width = (max_lon - min_lon) / span;
    let height = (max_lat - min_lat) / span;
    let scale = (600.0 / width).min(320.0 / height);
    let left = (640.0 - width * scale) / 2.0;
    let top = (360.0 - height * scale) / 2.0;
    let mut path = String::new();
    for ring in &area.rings {
        for (index, point) in ring.iter().enumerate() {
            // Formatting into String is infallible; no external IO occurs here.
            let _ = write!(
                path,
                "{}{:.6},{:.6} ",
                if index == 0 { 'M' } else { 'L' },
                left + ((point.longitude - min_lon) / span) * scale,
                top + ((max_lat - point.latitude) / span) * scale
            );
        }
        path.push_str("Z ");
    }
    path
}

/// Render every candidate; ambiguity is preserved. Geometry comes exclusively
/// from validated catalogue records. Source URLs are escaped text, not links.
/// Complexity: search plus O(matched boundary vertices + output metadata).
pub fn render_search_results(catalogue: &Catalogue, query: &str) -> Result<String, SearchError> {
    let matches = catalogue.search(query)?;
    let mut html = String::from("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>City and metropolitan area search</title><style>body{font:16px/1.6 system-ui,sans-serif;margin:0;background:#f3f7f5;color:#16352d}main{max-width:960px;margin:auto;padding:2rem 1rem}h1,h2{line-height:1.2}article,aside{background:white;border:1px solid #b7ccc3;border-radius:12px;padding:1.25rem;margin:1rem 0}svg{display:block;width:100%;height:auto;background:#edf4f0;border-radius:8px}dt{font-weight:650}dd{margin:0 0 .6rem;overflow-wrap:anywhere}.notice{border-left:4px solid #236955;padding-left:1rem}code{overflow-wrap:anywhere}li{margin:.3rem 0}</style></head><body><main><h1>City and metropolitan area search</h1>");
    let _ = write!(html, "<p>Query: <strong>{}</strong></p><p>{} canonical candidate(s). Compare the names, states and identifiers before choosing a study area.</p>", escape(query), matches.len());
    html.push_str("<aside aria-labelledby=\"coverage\"><h2 id=\"coverage\">Catalogue coverage</h2><p>Coverage is limited to the entries below. The bundled pilot contains three areas; it is not a nationwide search service. An unsupported query does not imply a city or metropolitan area does not exist.</p><ul>");
    for area in catalogue.entries() {
        let _ = write!(
            html,
            "<li>{} — {} — {} — vintage {}</li>",
            escape(&area.name),
            escape(&area.states.join(", ")),
            escape(&area.id),
            escape(&area.vintage)
        );
    }
    html.push_str("</ul></aside><p class=\"notice\">Boundary preview uses longitude and latitude degrees (EPSG:4326), north up, without a basemap. This schematic display preserves all supplied rings, including islands and holes. It is not a distance or area measurement, topology certification, or planning prediction. The bundled 2024 Census pilot boundaries are generalized at 1:500,000 scale.</p>");
    if matches.is_empty() {
        html.push_str("<p role=\"status\">No matching geography in this catalogue. Check the supported coverage above or load a catalogue containing the requested geography.</p>");
    }
    for (index, area) in matches.iter().enumerate() {
        let kind = match area.kind {
            GeographyKind::City => "City",
            GeographyKind::MetropolitanArea => "Metropolitan area",
        };
        let title = format!("{} — {}", area.name, area.states.join(", "));
        let _ = write!(html, "<article aria-labelledby=\"area-{index}\"><h2 id=\"area-{index}\">{}</h2><p>{kind} · <code>{}</code></p><svg viewBox=\"0 0 640 360\" role=\"img\" aria-labelledby=\"boundary-title-{index} boundary-desc-{index}\"><title id=\"boundary-title-{index}\">Boundary of {}</title><desc id=\"boundary-desc-{index}\">North-up longitude/latitude preview of {}. All {} boundary rings are drawn, including holes. Canonical identity and source metadata follow the image.</desc><path d=\"{}\" fill=\"#99cdb8\" stroke=\"#174e3c\" stroke-width=\"1.2\" fill-rule=\"evenodd\"/></svg><dl><dt>Canonical identifier</dt><dd>{}</dd><dt>States</dt><dd>{}</dd><dt>Source URL</dt><dd>{}</dd><dt>Source vintage</dt><dd>{}</dd><dt>Retrieved date</dt><dd>{}</dd><dt>License / attribution</dt><dd>{}</dd><dt>Coordinate reference</dt><dd>EPSG:4326; longitude, latitude in degrees</dd></dl></article>", escape(&title), escape(&area.id), escape(&title), escape(&area.id), area.rings.len(), boundary_path(area), escape(&area.id), escape(&area.states.join(", ")), escape(&area.source_url), escape(&area.vintage), escape(&area.retrieved_date), escape(&area.license));
    }
    html.push_str("</main></body></html>\n");
    Ok(html)
}
