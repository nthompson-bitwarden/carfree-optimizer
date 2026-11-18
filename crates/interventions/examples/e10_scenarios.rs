//! Human-reviewable E10 demo and deterministic scaling benchmark.
#[path = "support/e10.rs"]
mod fixture;
use fixture::*;
use interventions::{evaluation::*, scenario::*};
use mobility_graph::{network::*, EdgeId};
use mobility_sim::simulation::Iterative;
use std::{hint::black_box, time::Instant};

fn evaluate_snapshot(s: &Snapshot) -> Result<Evaluation, Box<dyn std::error::Error>> {
    let (d, input) = inputs(s);
    Ok(evaluate(
        s,
        &Iterative,
        &d,
        &input,
        &DemandBinding {
            scenario_id: s.id.clone(),
            demand_revision: d.revision.clone(),
            land_use_revision: s.land_use_revision.clone(),
        },
    )?)
}
fn demo() -> Result<(), Box<dyn std::error::Error>> {
    println!("E10 synthetic scenario authoring v1 | MIT | 2026-01-05 | seed 42");
    println!("MODELED arbitrary inputs, not observed/calibrated or engineering approval.");
    let mut h = history();
    let original = h.current().network.encode();
    let baseline = evaluate_snapshot(h.current())?;
    let road_time = |e: &Evaluation| {
        e.report
            .roads
            .iter()
            .find(|r| r.edge == EdgeId(7) && r.period == 0)
            .map(|r| r.used_seconds)
    };
    let selection = select_corridor(
        &h.current().network,
        &request(1, 2, mobility_graph::Mode::Car),
    )?;
    println!("Selected connected road corridor: {selection:?}");
    println!(
        "Profile/time-aware preview: {:?}",
        interventions::corridor::preview(
            &h.current().network,
            &selection,
            mobility_graph::Mode::Car,
            &request(1, 2, mobility_graph::Mode::Car),
        )
        .issues
    );
    println!(
        "Gap preview: {:?}",
        preview_corridor(
            &h.current().network,
            &[EdgeId(100), EdgeId(8)],
            mobility_graph::Mode::Car
        )
        .issues
    );
    let bus = bus(&h, 10);
    let edits = vec![
        bike(&h, 7, 3),
        Change::Allocate {
            road: EdgeId(7),
            lanes: 1,
            target: bus,
            assessment: None,
        },
    ];
    let b = batch(&h, "protected-bike-and-bus-v1", edits);
    let preview = h.preview(&b)?;
    println!("Preview lane ledger: {:?}", preview.roads[&EdgeId(7)]);
    h.apply(&b)?;
    let changed = evaluate_snapshot(h.current())?;
    println!(
        "Kind {:?}; road travel seconds {:?} -> {:?}; status {:?}; unserved essential weight {}",
        changed.kind,
        road_time(&baseline),
        road_time(&changed),
        changed.report.status,
        changed.report.unserved_essential_weight
    );
    for r in &h.current().continuity {
        println!(
            "Access {}: {} -> {} seconds",
            r.requirement, r.before_seconds, r.after_seconds
        );
    }
    let edited = h.current().clone();
    h.undo();
    println!(
        "Undo restores exact baseline network bytes: {}",
        h.current().network.encode() == original
    );
    h.redo();
    println!(
        "Redo restores complete edited snapshot: {}",
        h.current() == &edited
    );
    let mut closure = history();
    closure.apply(&batch(
        &closure,
        "direct-road-closed",
        vec![Change::CloseRoad { edge: EdgeId(7) }],
    ))?;
    let diverted = evaluate_snapshot(closure.current())?;
    println!(
        "Closure modeled road volumes: {:?}",
        diverted
            .report
            .roads
            .iter()
            .filter(|r| r.vehicles > 0.0)
            .map(|r| (r.edge, r.period, r.vehicles))
            .collect::<Vec<_>>()
    );
    println!(
        "Closure status {:?}; unserved essential weight {}",
        diverted.report.status, diverted.report.unserved_essential_weight
    );
    h.apply(&batch(
        &h,
        "destinations-v1",
        vec![Change::LandUse {
            destinations: vec![Destination {
                id: "proposed-clinic".into(),
                node: mobility_graph::NodeId(2),
                category: "healthcare".into(),
                opportunities: 1,
                provenance: synthetic(),
            }],
        }],
    ))?;
    println!("Separate extension: {:?}, land-use revision {:?}. No automatic travel-demand or accessibility effect is inferred.",h.current().kind(),h.current().land_use_revision);
    println!(
        "Combined evaluation requires explicitly revised demand and matching land-use binding."
    );
    Ok(())
}
fn benchmark() -> Result<(), Box<dyn std::error::Error>> {
    println!("E10 modeled fixture benchmark; seed=42; timings are not planning predictions");
    println!("copies,nodes,links,requirements,previews,preview_us,apply_depth,apply_us,undo_redo_pairs,undo_redo_ns,retained_network_bytes");
    for copies in [1, 16, 64] {
        let base = network();
        let mut d = base.data().clone();
        for copy in 1..copies {
            let offset = copy * 1000;
            for n in &base.data().nodes {
                let mut n = n.clone();
                n.id.0 += offset;
                d.nodes.push(n);
            }
            for l in &base.data().links {
                let mut l = l.clone();
                l.id.0 += offset;
                l.from.0 += offset;
                l.to.0 += offset;
                if let LinkKind::Transit { trip, .. } = &mut l.kind {
                    trip.push_str(&format!("-{copy}"));
                }
                d.links.push(l);
            }
        }
        d.revision = format!("e10-bench-{copies}");
        let n = Network::new(d)?;
        let (nodes, links) = (n.data().nodes.len(), n.data().links.len());
        let mut h = history_for(n, requirements());
        let changes = vec![Change::Crossing {
            edge: EdgeId(3),
            seconds: 8,
            safety: Some(Safety {
                stress_level: 1,
                provenance: synthetic(),
            }),
            assessment: approval(true, 8),
        }];
        let b = batch(&h, "preview", changes.clone());
        black_box(h.preview(&b)?); // warmup
        let start = Instant::now();
        for _ in 0..100 {
            black_box(h.preview(&b)?);
        }
        let preview_us = start.elapsed().as_secs_f64() * 1e6 / 100.0;
        let mut bytes = h.current().network.encode().len();
        let start = Instant::now();
        for depth in 1..=20 {
            h.apply(&batch(&h, &format!("depth-{depth}"), changes.clone()))?;
        }
        let apply_us = start.elapsed().as_secs_f64() * 1e6 / 20.0;
        // Count canonical network bytes outside the measured apply interval.
        bytes += h.current().network.encode().len();
        for _ in 1..20 {
            h.undo();
            bytes += h.current().network.encode().len();
        }
        for _ in 1..20 {
            h.redo();
        }
        let start = Instant::now();
        for _ in 0..10000 {
            black_box(black_box(&mut h).undo());
            black_box(black_box(&mut h).redo());
        }
        let pair_ns = start.elapsed().as_secs_f64() * 1e9 / 10000.0;
        println!("{copies},{nodes},{links},{},100,{preview_us:.3},20,{apply_us:.3},10000,{pair_ns:.3},{bytes}",requirements().len());
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        None => demo(),
        Some("--benchmark") => benchmark(),
        Some(_) => Err("usage: e10_scenarios [--benchmark]".into()),
    }
}
