//! E12 reviewable text demo and bounded synthetic screening benchmark.
#[path = "support/e12.rs"]
mod fixture;
use fixture::*;
use interventions::{feasibility::*, scenario::Change};
use mobility_graph::{EdgeId, Mode};
use std::{collections::BTreeMap, time::Instant};

fn plan_inputs(count: usize) -> (interventions::scenario::History, Vec<PhaseInput>, Inputs) {
    let h = base::history();
    let mut permanent = h.clone();
    let mut phases = vec![];
    for index in 0..count {
        let change = if index == 0 {
            base::bike(&permanent, 7, 3)
        } else {
            Change::Pedestrian {
                link: base::street(300 + index as u32, 1, 2, Mode::Walk, 10),
                assessment: base::approval(false, 8),
            }
        };
        let b = base::batch(&permanent, &format!("milestone-{index}"), vec![change]);
        permanent.apply(&b).unwrap();
        let temporary = if index == 0 {
            Some(base::batch(
                &permanent,
                "temporary-closure",
                vec![Change::CloseRoad { edge: EdgeId(7) }],
            ))
        } else {
            None
        };
        let effective = temporary
            .as_ref()
            .map(|b| permanent.preview(b).unwrap())
            .unwrap_or_else(|| permanent.current().clone());
        let mut i = for_snapshot(h.current(), &effective);
        let start = index as i64 * 60;
        for a in &mut i.access {
            for r in [&mut a.outbound, &mut a.returning] {
                r.departure_utc += start;
                r.deadline_utc += start;
            }
        }
        phases.push(PhaseInput {
            start,
            end: start + 60,
            permanent: b,
            temporary,
            services: BTreeMap::new(),
            inputs: i,
        });
    }
    let mut final_i = for_snapshot(h.current(), permanent.current());
    for c in &mut final_i.costs {
        if c.category != Category::Maintenance {
            c.quantity = 0.0;
        }
    }
    (h, phases, final_i)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let h = base::history();
    let b = proposal(&h, "e12-demo");
    let i = inputs(&h, &b);
    println!("E12 synthetic MIT abstract corridor; 2026-01-05, seed 42. NOT observed, calibrated or engineering approval.");
    if std::env::args().any(|a| a == "--bench") {
        println!(
            "network_nodes={},network_links={},repeat=20",
            h.current().network.data().nodes.len(),
            h.current().network.data().links.len()
        );
        for multiplier in [1, 4, 16, 64] {
            let mut input = i.clone();
            input.access.clear();
            for index in 0..multiplier {
                for mut sample in i.access.clone() {
                    sample.id = format!("{}-{index}", sample.id);
                    input.access.push(sample);
                }
            }
            let start = Instant::now();
            for _ in 0..20 {
                std::hint::black_box(screen(&h, &b, &input)?);
            }
            println!(
                "samples={},routes={},mean_us={:.3}",
                input.access.len(),
                2 * input.access.len(),
                start.elapsed().as_secs_f64() * 1e6 / 20.0
            );
        }
        for extra in [0u32, 32, 128] {
            let baseline = base::history();
            let mut data = baseline.current().network.data().clone();
            let mut roads = baseline
                .current()
                .roads
                .values()
                .cloned()
                .collect::<Vec<_>>();
            for index in 0..extra {
                let from = 10000 + 2 * index;
                let to = from + 1;
                for id in [from, to] {
                    data.nodes.push(mobility_graph::network::Node {
                        id: mobility_graph::NodeId(id),
                        access: mobility_graph::network::Access::Accessible,
                    });
                }
                let edge = 20000 + index;
                data.links.push(base::street(edge, from, to, Mode::Car, 10));
                let mut road = roads[0].clone();
                road.road = EdgeId(edge);
                road.segment = format!("extra-{index}");
                roads.push(road);
            }
            let network = mobility_graph::network::Network::new(data)?;
            let h = interventions::scenario::History::new(
                "baseline".into(),
                network,
                roads,
                base::requirements(),
            )?;
            let b = proposal(&h, "scaled");
            let input = inputs(&h, &b);
            let start = Instant::now();
            for _ in 0..20 {
                std::hint::black_box(screen(&h, &b, &input)?);
            }
            println!(
                "segments={},network_links={},mean_us={:.3}",
                input.segments.len(),
                h.current().network.data().links.len(),
                start.elapsed().as_secs_f64() * 1e6 / 20.0
            );
        }
        for count in [1, 2, 4] {
            let (h, p, i) = plan_inputs(count);
            let start = Instant::now();
            for _ in 0..20 {
                std::hint::black_box(screen_plan(&h, &p, &i)?);
            }
            println!(
                "phases={count},mean_us={:.3}",
                start.elapsed().as_secs_f64() * 1e6 / 20.0
            );
        }
        return Ok(());
    }
    let screened = screen(&h, &b, &i)?;
    println!(
        "USD 2026: capital {:?}, maintenance {:?}, transit/year {}, lifecycle {:?}",
        screened.report().capital,
        screened.report().maintenance,
        screened.report().transit_annual,
        screened.report().lifecycle
    );
    for access in &screened.report().access {
        println!(
            "{}: outbound {}s, return {}s",
            access.id, access.outbound_seconds, access.return_seconds
        );
    }
    let mut bad = i.clone();
    bad.segments[0].restrictions.insert(
        Restriction::Bridge,
        Evidence::Missing("bridge load review absent".into()),
    );
    println!(
        "Missing engineering evidence blocks: {}",
        screen(&h, &b, &bad).unwrap_err()
    );
    bad = i.clone();
    bad.budget.capital = 1.0;
    println!(
        "Budget blocks before assignment: {}",
        screen(&h, &b, &bad).unwrap_err()
    );
    let (h, p, i) = plan_inputs(2);
    let plan = screen_plan(&h, &p, &i)?;
    println!("{} valid phases, incremental capital {:?}; temporary closure expires, final general lanes={}",plan.phases().len(),plan.capital,plan.final_state().snapshot().roads[&EdgeId(7)].general_lanes);
    let r = RiskInput {
        exposure_scenario_id: screened.snapshot().id.clone(),
        exposure_graph_revision: screened.snapshot().network.data().revision.clone(),
        model: "synthetic rate-times-exposure".into(),
        version: "1".into(),
        mode: Mode::Walk,
        severity: "all collisions".into(),
        unit: ExposureUnit::PersonKm,
        exposure: 1000.0,
        exposure_source: base::synthetic(),
        calibration: Some(base::synthetic()),
        applicability: "abstract corridor only; arbitrary rates, no empirical validity".into(),
        period_start: 0,
        period_end: 60,
        rate: Bounds {
            lower: 0.001,
            central: 0.002,
            upper: 0.003,
        },
        uncertainty:
            "supplied parameter sensitivity bounds, NOT a confidence interval or causal benefit"
                .into(),
    };
    let risk = screened.risk(&r)?;
    println!(
        "Independent modeled expected collisions {:?}; unit={:?}, source={}, uncertainty={}",
        risk.expected_collisions,
        risk.input.unit,
        risk.input.calibration.unwrap().source,
        risk.input.uncertainty
    );
    println!("Declared OD samples are not citywide access guarantees. Phase screening does not validate E08 passenger supply or empirical baseline fitness.");
    Ok(())
}
