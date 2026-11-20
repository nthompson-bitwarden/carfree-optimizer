use super::{model, store, Error, Inputs, PlanInput, Projection, Response, Result};
use parquet::{
    data_type::{ByteArray, ByteArrayType, DoubleType, Int32Type},
    file::{metadata::KeyValue, properties::WriterProperties, writer::SerializedFileWriter},
    schema::parser::parse_message_type,
};
use serde_json::{json, Value};
use std::{fmt::Write, sync::Arc};

const RESULT_SCHEMA: &str = "carfree-workspace-evaluation-v1";
const BASIS: &str =
    "Synthetic schematic model. No Census network correspondence or empirical calibration.";

pub fn evaluation(inputs: &Inputs, plan: &PlanInput, completed: model::Completed) -> Result<Value> {
    let report: Value = store::exact_json(&completed.report).map_err(export_error)?;
    let phases = completed
        .phases
        .iter()
        .map(|phase| store::exact_json(&phase.view()))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(export_error)?;
    let scenario = json!({"plan":plan,"phases":phases});
    let manifest = json!({
        "schema":"carfree-workspace-manifest-v1",
        "scenario_schema":RESULT_SCHEMA,
        "report_version":report["version"],
        "inputs":inputs,
        "seed":report["phases"][0]["trace"]["seed"],
        "scenario_sha256":store::digest(scenario.to_string().as_bytes()),
        "report_sha256":store::digest(report.to_string().as_bytes()),
        "basis":BASIS,
        "plan_units":{"width":"mm","capital":"2026 USD","annual":"2026 USD/year"},
        "parquet":{"metric_columns":["phase","metric","unit","value","incremental","cumulative"],
                   "phase_zero":"baseline","missing":"null"}
    });
    Ok(json!({"schema":RESULT_SCHEMA,"manifest":manifest,"scenario":scenario,"report":report}))
}

pub fn verify(inputs: &Inputs, plan: &PlanInput, result: &Value) -> Result<()> {
    let manifest = &result["manifest"];
    let scenario = &result["scenario"];
    let report = &result["report"];
    if result["schema"] != RESULT_SCHEMA
        || manifest["schema"] != "carfree-workspace-manifest-v1"
        || manifest["scenario_schema"] != RESULT_SCHEMA
        || manifest["report_version"] != report["version"]
        || manifest["seed"] != report["phases"][0]["trace"]["seed"]
        || manifest["inputs"] != json!(inputs)
        || scenario["plan"] != json!(plan)
        || manifest["scenario_sha256"] != store::digest(scenario.to_string().as_bytes())
        || manifest["report_sha256"] != store::digest(report.to_string().as_bytes())
    {
        return Err(malformed());
    }
    Ok(())
}

pub fn download(name: &str, result: &Value) -> Result<Response> {
    match name {
        "scenario.json" => Ok(Response::json("200 OK", result.to_string())),
        "manifest.json" => Ok(Response::json("200 OK", result["manifest"].to_string())),
        "metrics.parquet" => Ok(Response::bytes(
            "200 OK",
            "application/vnd.apache.parquet",
            parquet(result)?,
        )),
        "report.html" => projection(
            Projection::Report,
            &public_payload(Projection::Report, result)?,
        ),
        "map.svg" => projection(Projection::Map, &public_payload(Projection::Map, result)?),
        _ => Err(Error::missing()),
    }
}

pub fn public_payload(kind: Projection, result: &Value) -> Result<Value> {
    match kind {
        Projection::Report => Ok(result["report"].clone()),
        Projection::Map => {
            let network = result["scenario"]["phases"]
                .as_array()
                .and_then(|items| items.last())
                .ok_or_else(malformed)?;
            let phase = result["report"]["phases"]
                .as_array()
                .and_then(|items| items.last())
                .ok_or_else(malformed)?;
            Ok(json!({"links":network["links"],"trace":phase["trace"],
                "methodology":result["report"]["methodology"]}))
        }
        Projection::Scenario => Err(malformed()),
    }
}

pub fn projection(kind: Projection, payload: &Value) -> Result<Response> {
    match kind {
        Projection::Scenario => Ok(Response::json("200 OK", payload.to_string())),
        Projection::Report => Ok(Response::new(
            "200 OK",
            "text/html; charset=utf-8",
            html(payload)?,
        )),
        Projection::Map => Ok(Response::new("200 OK", "image/svg+xml", svg(payload)?)),
    }
}

fn malformed() -> Error {
    Error::internal("Saved evaluation does not match the export schema.")
}

fn export_error(_: impl std::fmt::Display) -> Error {
    Error::internal("Evaluation export could not be encoded.")
}

fn array(value: &Value) -> Result<&Vec<Value>> {
    value.as_array().ok_or_else(malformed)
}

fn string(value: &Value) -> Result<&str> {
    value.as_str().ok_or_else(malformed)
}

fn parquet(result: &Value) -> Result<Vec<u8>> {
    let schema = parse_message_type(
        "message metrics_v1 {
          REQUIRED INT32 phase;
          REQUIRED BINARY metric (UTF8);
          REQUIRED BINARY unit (UTF8);
          OPTIONAL DOUBLE value;
          OPTIONAL DOUBLE incremental;
          OPTIONAL DOUBLE cumulative;
        }",
    )
    .map_err(export_error)?;
    let report = &result["report"];
    let metrics = array(&report["metrics"])?;
    let phases = array(&report["phases"])?;
    let mut indices = Vec::new();
    let mut names = Vec::new();
    let mut units = Vec::new();
    let keys = ["values", "incremental", "cumulative"];
    let mut columns: [(Vec<f64>, Vec<i16>); 3] = Default::default();
    for (index, phase) in phases.iter().enumerate() {
        for (i, metric) in metrics.iter().enumerate() {
            indices.push(i32::try_from(index).map_err(export_error)?);
            names.push(ByteArray::from(string(&metric[0])?));
            units.push(ByteArray::from(string(&metric[1])?));
            for (key, (values, definitions)) in keys.iter().zip(&mut columns) {
                let value = phase[*key].get(i).ok_or_else(malformed)?;
                if let Some(number) = value.as_f64() {
                    values.push(number);
                    definitions.push(1);
                } else if value.is_null() {
                    definitions.push(0);
                } else {
                    return Err(malformed());
                }
            }
        }
    }
    let properties = WriterProperties::builder()
        .set_key_value_metadata(Some(vec![
            KeyValue::new("carfree.manifest".into(), result["manifest"].to_string()),
            KeyValue::new(
                "carfree.plan".into(),
                result["scenario"]["plan"].to_string(),
            ),
        ]))
        .build();
    let mut writer = SerializedFileWriter::new(Vec::new(), Arc::new(schema), Arc::new(properties))
        .map_err(export_error)?;
    let mut group = writer.next_row_group().map_err(export_error)?;
    macro_rules! column {
        ($type:ty, $values:expr, $definitions:expr) => {
            let mut writer = group
                .next_column()
                .map_err(export_error)?
                .ok_or_else(malformed)?;
            writer
                .typed::<$type>()
                .write_batch($values, $definitions, None)
                .map_err(export_error)?;
            writer.close().map_err(export_error)?;
        };
    }
    column!(Int32Type, &indices, None);
    column!(ByteArrayType, &names, None);
    column!(ByteArrayType, &units, None);
    for (values, definitions) in &columns {
        column!(DoubleType, values, Some(definitions));
    }
    group.close().map_err(export_error)?;
    writer.into_inner().map_err(export_error)
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn attribution(trace: &Value) -> Result<String> {
    let fields = ["source", "sourceVersion", "geography", "date", "license"];
    let parts = fields
        .iter()
        .map(|key| string(&trace[*key]).map(escape))
        .collect::<Result<Vec<_>>>()?;
    Ok(parts.join(" · "))
}

fn caveats(methodology: &Value) -> Result<String> {
    ["calibration", "uncertainty", "limitations"]
        .iter()
        .map(|key| string(&methodology[*key]).map(escape))
        .collect::<Result<Vec<_>>>()
        .map(|items| items.join(" "))
}

fn html(report: &Value) -> Result<String> {
    let phases = array(&report["phases"])?;
    let baseline = phases.first().ok_or_else(malformed)?;
    let mut html = format!(concat!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\">",
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
        "<title>Scenario evaluation</title><main><h1>Scenario evaluation</h1>",
        "<p>{}</p><p>{}</p><p>{}</p><p>{}</p>",
        "<table><caption>Nominal phase metrics. Phase 0 is the baseline. Unavailable means no estimate.</caption>",
        "<thead><tr><th scope=\"col\">Phase</th><th scope=\"col\">Metric</th>",
        "<th scope=\"col\">Unit</th><th scope=\"col\">Value</th><th scope=\"col\">Change from baseline</th></tr></thead><tbody>"
    ), BASIS, attribution(&baseline["trace"])?, caveats(&report["methodology"])?,
        escape(string(&report["methodology"]["assumptions"])?));
    for (index, phase) in phases.iter().enumerate() {
        for (i, metric) in array(&report["metrics"])?.iter().enumerate() {
            let number = |key: &str| {
                phase[key][i]
                    .as_f64()
                    .map_or_else(|| "Unavailable".into(), |value| value.to_string())
            };
            write!(html, "<tr><td>{index}</td><th scope=\"row\">{}</th><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape(string(&metric[0])?), escape(string(&metric[1])?), number("values"), number("cumulative")).unwrap();
        }
    }
    html.push_str("</tbody></table><h2>Sensitivity</h2><ul>");
    for case in array(&report["sensitivity"]["cases"])? {
        write!(
            html,
            "<li><strong>{}</strong> {}",
            escape(string(&case["assumption"])?),
            escape(string(&case["detail"])?)
        )
        .unwrap();
        if let Some(reason) = case["error"].as_str() {
            write!(html, " Unavailable. {}", escape(reason)).unwrap();
        }
        html.push_str("</li>");
    }
    html.push_str(concat!(
        "</ul><table><caption>Final minus baseline extrema across available declared cases. ",
        "These ranges are not confidence intervals.</caption><thead><tr>",
        "<th scope=\"col\">Metric</th><th scope=\"col\">Unit</th><th scope=\"col\">Minimum</th>",
        "<th scope=\"col\">Maximum</th><th scope=\"col\">Samples</th></tr></thead><tbody>"
    ));
    for (metric, range) in array(&report["metrics"])?
        .iter()
        .zip(array(&report["sensitivity"]["ranges"])?)
    {
        write!(
            html,
            "<tr><th scope=\"row\">{}</th><td>{}</td>",
            escape(string(&metric[0])?),
            escape(string(&metric[1])?)
        )
        .unwrap();
        if range.is_null() {
            html.push_str("<td>Unavailable</td><td>Unavailable</td><td>0</td>");
        } else {
            for value in array(range)? {
                if !value.is_number() {
                    return Err(malformed());
                }
                write!(html, "<td>{value}</td>").unwrap();
            }
        }
        html.push_str("</tr>");
    }
    html.push_str(concat!(
        "</tbody></table><h2>Trade-offs</h2><table><caption>Feasible alternatives and their sampled frontier status.</caption>",
        "<thead><tr><th scope=\"col\">Candidate indices</th><th scope=\"col\">Upper capital (2026 USD)</th>",
        "<th scope=\"col\">Reward (dimensionless)</th><th scope=\"col\">On frontier</th></tr></thead><tbody>"
    ));
    for item in array(&report["tradeoffs"]["alternatives"])? {
        let indices = if item["indices"] == json!([]) {
            "Baseline".to_owned()
        } else {
            escape(&item["indices"].to_string())
        };
        write!(
            html,
            "<tr><th scope=\"row\">{indices}</th><td>{}</td><td>{}</td><td>{}</td></tr>",
            item["capital"].as_f64().ok_or_else(malformed)?,
            item["reward"].as_f64().ok_or_else(malformed)?,
            if item["frontier"].as_bool().ok_or_else(malformed)? {
                "Yes"
            } else {
                "No"
            }
        )
        .unwrap();
    }
    html.push_str("</tbody></table><h3>Rejected alternatives</h3><ul>");
    for item in array(&report["tradeoffs"]["rejected"])? {
        write!(
            html,
            "<li>{} · {}</li>",
            escape(&item["projects"].to_string()),
            escape(string(&item["error"])?)
        )
        .unwrap();
    }
    html.push_str("</ul></main></html>");
    Ok(html)
}

fn svg(map: &Value) -> Result<String> {
    // E17's schematic node positions. These coordinates have no geographic units.
    const POSITIONS: [(i32, i32); 5] = [(88, 136), (288, 69), (282, 224), (436, 151), (118, 254)];
    const LAYERS: [(&str, &str); 5] = [
        ("road", "#52525b"),
        ("walk", "#227247"),
        ("bike", "#7253a2"),
        ("bus", "#1561a7"),
        ("rail", "#a64d20"),
    ];
    let source = attribution(&map["trace"])?;
    let caveat = caveats(&map["methodology"])?;
    let mut svg = format!(concat!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 720 458\" role=\"img\" aria-labelledby=\"title desc\">",
        "<title id=\"title\">Final phase schematic network</title><desc id=\"desc\">{} {} {}</desc>",
        "<rect width=\"720\" height=\"458\" fill=\"#fafaf8\"/><g font-family=\"sans-serif\" fill=\"#202124\">",
        "<text x=\"20\" y=\"30\" font-size=\"21\">Final phase schematic network</text>",
        "<text x=\"20\" y=\"52\" font-size=\"13\">Node IDs and directed links · schematic coordinates, no distance scale</text>"
    ), BASIS, source, caveat);
    svg.push_str(concat!(
        "<defs><marker id=\"arrow\" viewBox=\"0 0 10 10\" refX=\"8\" refY=\"5\" markerWidth=\"5\" markerHeight=\"5\" orient=\"auto-start-reverse\">",
        "<path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"context-stroke\"/></marker></defs>"
    ));
    let links = array(&map["links"])?;
    for (index, link) in links.iter().enumerate() {
        let point = |key: usize| {
            let id = link[key].as_u64().ok_or_else(malformed)?;
            id.checked_sub(1)
                .and_then(|i| POSITIONS.get(i as usize).copied())
                .ok_or_else(malformed)
        };
        let ((x1, y1), (x2, y2)) = (point(1)?, point(2)?);
        let mode = string(&link[3])?;
        let (layer, (_, color)) = LAYERS
            .iter()
            .enumerate()
            .find(|(_, (name, _))| *name == mode)
            .ok_or_else(malformed)?;
        let dash = if link[4] == "Unknown" {
            " stroke-dasharray=\"4 3\""
        } else {
            ""
        };
        let offset = 9.0 * (layer as f64 - 2.0) + (index % 3) as f64;
        let (dx, dy) = ((x2 - x1) as f64, (y2 - y1) as f64);
        let length = dx.hypot(dy);
        if length == 0.0 {
            continue;
        }
        let (ux, uy) = (dx / length, dy / length);
        let start = (
            x1 as f64 + 11.0 * ux - offset * uy,
            y1 as f64 + 11.0 * uy + offset * ux + 33.0,
        );
        let end = (
            x2 as f64 - 13.0 * ux - offset * uy,
            y2 as f64 - 13.0 * uy + offset * ux + 33.0,
        );
        write!(svg, "<path d=\"M {:.2} {:.2} L {:.2} {:.2}\" stroke=\"{color}\" stroke-width=\"2.5\" marker-end=\"url(#arrow)\"{dash}><title>{mode} {} to {}, access {}</title></path>",
            start.0,start.1,end.0,end.1,link[1],link[2],escape(string(&link[4])?)).unwrap();
    }
    for (i, (x, y)) in POSITIONS.into_iter().enumerate() {
        let y = y + 33;
        write!(svg,"<circle cx=\"{x}\" cy=\"{y}\" r=\"9\" fill=\"#fafaf8\" stroke=\"#202124\"/><text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" font-size=\"10\">{}</text>",y+4,i+1).unwrap();
    }
    for (i, (mode, color)) in LAYERS.into_iter().enumerate() {
        let label = if links.iter().any(|link| link[3] == mode) {
            mode.to_owned()
        } else {
            format!("{mode} (absent)")
        };
        write!(
            svg,
            "<text x=\"500\" y=\"{}\" font-size=\"14\" fill=\"{color}\">{label}</text>",
            95 + i * 25
        )
        .unwrap();
    }
    svg.push_str("<text x=\"500\" y=\"230\" font-size=\"12\">Dashed · access unknown</text>");
    let text = format!("{source}. {BASIS} {caveat}");
    let mut line = String::new();
    let mut y = 318;
    for word in text.split_whitespace() {
        if line.len() + word.len() > 105 {
            write!(
                svg,
                "<text x=\"20\" y=\"{y}\" font-size=\"11\">{line}</text>"
            )
            .unwrap();
            line.clear();
            y += 15;
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    write!(
        svg,
        "<text x=\"20\" y=\"{y}\" font-size=\"11\">{line}</text></g></svg>"
    )
    .unwrap();
    Ok(svg)
}
