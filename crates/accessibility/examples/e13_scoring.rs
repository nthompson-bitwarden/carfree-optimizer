//! E13 v1 synthetic, MIT, abstract map coordinates, arbitrary modeled inputs.
//! HTML is a developer demo, not a policy report or a geographic basemap.
#[path = "../tests/support/mod.rs"]
mod fixture;
use accessibility::{scoring::*, spatial::*};
use std::{hint::black_box, time::Instant};
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn benchmark() {
    for n in [100, 1000, 10000] {
        let mut input = fixture::access_input();
        input.stratum.departures_utc = vec![0];
        input.destinations = (0..n)
            .map(|i| format!("synthetic-destination-{i}"))
            .collect();
        input.pairs = input
            .destinations
            .iter()
            .map(|d| Pair {
                destination: d.clone(),
                departure_utc: 0,
                car_free: fixture::journey(60),
                car: fixture::journey(40),
            })
            .collect();
        let repetitions = 50;
        let start = Instant::now();
        for _ in 0..repetitions {
            black_box(access(black_box(&input)).unwrap());
        }
        println!(
            "E13-v1 cells={n} repetitions={repetitions} mean_us={:.2}",
            start.elapsed().as_secs_f64() * 1e6 / f64::from(repetitions)
        );
    }
}
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--bench") {
        benchmark();
        return;
    }
    let mut input = fixture::access_input();
    input.stratum.origin = "north-origin".into();
    let access = access(&input).unwrap();
    let mut disabled = fixture::trip("noncycling-essential", Some(false), Some(false), None);
    disabled.group = "step-free-noncyclist".into();
    disabled.neighborhood = "south".into();
    let trips = trips(
        &input.context,
        &[
            fixture::trip("car-only", Some(false), Some(true), Some(Mode::Car)),
            disabled,
        ],
        fixture::unavailable(),
    )
    .unwrap();
    let areas = vec![
        Neighborhood {
            id: "north".into(),
            population: 100.0,
            ring: vec![(10.0, 10.0), (140.0, 10.0), (140.0, 110.0), (10.0, 110.0)],
        },
        Neighborhood {
            id: "south".into(),
            population: 200.0,
            ring: vec![(10.0, 120.0), (140.0, 120.0), (140.0, 220.0), (10.0, 220.0)],
        },
    ];
    let mut south_input = input.clone();
    south_input.stratum.origin = "south-origin".into();
    south_input.stratum.neighborhood = "south".into();
    south_input.stratum.group = "step-free-noncyclist".into();
    for p in &mut south_input.pairs {
        p.car_free = Evidence::Infeasible;
    }
    let south_score = accessibility::scoring::access(&south_input).unwrap();
    let origins = [
        Origin {
            id: "north-origin".into(),
            neighborhood: "north".into(),
            group: "noncyclist".into(),
            population: 100.0,
            score: Some(accessibility::projection::Projection::from_access(&access)),
        },
        Origin {
            id: "south-origin".into(),
            neighborhood: "south".into(),
            group: "step-free-noncyclist".into(),
            population: 40.0,
            score: Some(accessibility::projection::Projection::from_access(
                &south_score,
            )),
        },
    ];
    let map = neighborhoods(&input.context, &areas, &origins, 0.8).unwrap();
    println!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>E13 synthetic scoring demo</title><style>body{{font:18px system-ui;max-width:70rem;margin:2rem auto;padding:1rem}}td,th{{padding:.5rem;border:1px solid #777}}table{{border-collapse:collapse}}svg{{max-width:25rem}}.warning{{border:3px solid #925800;padding:1rem}}</style><h1>E13 synthetic scoring demo</h1><p class=\"warning\">LIMITED FITNESS: arbitrary synthetic inputs, no empirical calibration or policy recommendations. Abstract map coordinates, not geographic boundaries. Shared color scale 0–100%; hatched areas have insufficient coverage.</p>");
    let c = &map.context;
    println!("<dl><dt>Scenario / source</dt><dd>{} / {}</dd><dt>Geography / date</dt><dd>{} / {}</dd><dt>Horizon / behavior / period</dt><dd>{:?} / {} / {}</dd><dt>Graph / demand / destinations / geography revisions</dt><dd>{} / {} / {} / {}</dd></dl>",escape(&c.scenario),escape(&c.source),escape(&c.geography),escape(&c.date),c.horizon,escape(&c.behavior),escape(&c.period),escape(&c.graph_revision),escape(&c.demand_revision),escape(&c.destination_revision),escape(&c.geography_revision));
    println!("<svg viewBox=\"0 0 300 240\" role=\"img\" aria-labelledby=\"map-title map-description\"><title id=\"map-title\">Synthetic neighborhood accessibility</title><desc id=\"map-description\">North: 50 percent, full population coverage. South: zero percent among 20 percent represented population; insufficient coverage. Exact values follow in a table.</desc><defs><pattern id=\"missing\" width=\"8\" height=\"8\" patternUnits=\"userSpaceOnUse\"><rect width=\"8\" height=\"8\" fill=\"#eee\"/><path d=\"M0 8L8 0\" stroke=\"#555\"/></pattern></defs>");
    for a in &map.areas {
        let fill = if a.insufficient_coverage {
            "url(#missing)"
        } else {
            "#5ab4ac"
        };
        let points = a
            .neighborhood
            .ring
            .iter()
            .map(|(x, y)| format!("{x},{y}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!("<polygon points=\"{}\" fill=\"{fill}\" stroke=\"#111\"><title>{}: represented {} of {}, coverage {:?}, accessibility {:?}</title></polygon>",points,escape(&a.neighborhood.id),a.represented_population,a.total_population,a.coverage,a.represented_accessibility);
    }
    println!("<text x=\"150\" y=\"60\">North: 50%</text><text x=\"150\" y=\"170\">South: insufficient</text></svg><table><caption>Population-weighted accessibility on supplied noncycling profile, healthcare destinations, departures 0 and 100, 60-second budget</caption><tr><th scope=\"col\">Neighborhood</th><th scope=\"col\">Total population</th><th scope=\"col\">Represented</th><th scope=\"col\">Coverage</th><th scope=\"col\">Access among represented</th><th scope=\"col\">Coverage status</th></tr>");
    for a in &map.areas {
        println!("<tr><th scope=\"row\">{}</th><td>{}</td><td>{}</td><td>{:.0}%</td><td>{:.0}%</td><td>{}</td></tr>",escape(&a.neighborhood.id),a.total_population,a.represented_population,a.coverage.unwrap_or(0.0)*100.0,a.represented_accessibility.unwrap_or(0.0)*100.0,if a.insufficient_coverage {"INSUFFICIENT"}else{"sufficient under declared threshold"});
    }
    println!("</table><h2>Door-to-door and trip outcomes</h2><p>Destination/departure pairs: eligible {}, reached {}, missing {}. Car-free p50/p95 {:?}/{:?} seconds (conditional on {} feasible). Paired both/car-free-only/car-only/neither: {:?}. Joint-feasible mean difference: {:?} seconds. These are schedule samples, not confidence intervals.</p><p>All-trip car-required share {:?}; neither-mode weight {}; unserved weight {}; denominator {}. Confidence interval: unavailable, synthetic deterministic inputs lack a sampling model. Served-mode shares use all-trip weight, not only served trips.</p>",access.eligible,access.reached,access.missing,access.car_free.p50_seconds,access.car_free.p95_seconds,access.car_free.feasible,access.paired_feasibility,access.mean_time_difference_seconds,trips.all.car_required_share(),trips.all.no_mode,trips.all.unserved,trips.all.total);
    println!("<h2>Mobility-needs breakdown</h2><table><tr><th scope=\"col\">Group / neighborhood</th><th scope=\"col\">Trip denominator</th><th scope=\"col\">Car-required</th><th scope=\"col\">No mode</th></tr>");
    for ((g, n), d) in &trips.by_group_neighborhood {
        println!(
            "<tr><th scope=\"row\">{} / {}</th><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape(g),
            escape(n),
            d.total,
            d.car_required,
            d.no_mode
        );
    }
    println!("</table><p>Disjoint primary cohorts; population denominators are separate from trip expansion weights. Zero-mode trips are not car-required. Missing evaluations never improve a headline score.</p></html>");
}
