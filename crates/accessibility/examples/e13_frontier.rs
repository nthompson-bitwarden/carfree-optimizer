//! E13 synthetic worst-case tie frontier benchmark. MIT, no observed inputs.
#[path = "../../interventions/examples/support/e12.rs"]
mod fixture;
#[path = "../tests/support/mod.rs"]
mod metric;
use accessibility::{
    comparison::{pareto, Candidate, ProjectCostInventory, SingleProject},
    scoring::*,
};
use interventions::feasibility::*;
use mobility_graph::Mode as GMode;
use std::{hint::black_box, time::Instant};
fn main() {
    for size in [16, 64, 256] {
        let mut candidates = Vec::new();
        for n in 0..size {
            let h = fixture::base::history();
            let batch = fixture::proposal(&h, &format!("candidate-{n}"));
            let gate = screen(&h, &batch, &fixture::inputs(&h, &batch)).unwrap();
            let mut i = metric::access_input();
            i.context.scenario = gate.snapshot().id().clone();
            i.context.graph_revision = gate.snapshot().network().data().revision.clone();
            i.context.graph_snapshot = gate.snapshot().network().encode();
            let (d, _) = fixture::base::inputs(gate.snapshot());
            i.context.demand_revision = d.revision.clone();
            i.context.land_use_revision = gate.snapshot().land_use_revision().clone();
            let a = access(&i).unwrap();
            let t = trips(&i.context, &metric::demand_rows(&d), metric::unavailable()).unwrap();
            let binding = interventions::evaluation::DemandBinding {
                scenario_id: i.context.scenario.clone(),
                demand_revision: d.revision.clone(),
                land_use_revision: i.context.land_use_revision.clone(),
            };
            let t =
                accessibility::demand_binding::bind_demand(t, &gate, &d, &binding, 1.0).unwrap();
            let risk = RiskInput {
                exposure_scenario_id: i.context.scenario.clone(),
                exposure_graph_revision: i.context.graph_revision.clone(),
                model: "synthetic exposure-rate".into(),
                version: "1".into(),
                mode: GMode::Walk,
                severity: "synthetic all collisions".into(),
                unit: ExposureUnit::PersonKm,
                exposure: 1000.0,
                exposure_source: fixture::base::synthetic(),
                calibration: Some(fixture::base::synthetic()),
                applicability: "abstract synthetic corridor, no empirical calibration".into(),
                period_start: 0,
                period_end: 60,
                rate: Bounds {
                    lower: 0.001,
                    central: 0.001,
                    upper: 0.001,
                },
                uncertainty: "synthetic point factor, not measured precision".into(),
            };
            let inventory = ProjectCostInventory {
                baseline: h.current().clone(),
                scenario: gate.snapshot().clone(),
                costs: gate.inputs().costs.clone(),
                transit: gate.inputs().transit.clone(),
                completeness_source: fixture::base::synthetic(),
            };
            candidates.push(
                Candidate::single(
                    &SingleProject {
                        cost_inventory: &inventory,
                        history: &h,
                        batch: &batch,
                        inputs: gate.inputs(),
                    },
                    &a,
                    &t,
                    Some(&risk),
                    "protected fixture registry v1",
                )
                .unwrap(),
            );
        }
        assert_eq!(pareto(&candidates).unwrap().nondominated.len(), size);
        let repetitions = 20;
        let start = Instant::now();
        for _ in 0..repetitions {
            black_box(pareto(black_box(&candidates)).unwrap());
        }
        println!(
            "E13-frontier-v1 candidates={size} repetitions={repetitions} mean_us={:.2}",
            start.elapsed().as_secs_f64() * 1e6 / f64::from(repetitions)
        );
    }
}
