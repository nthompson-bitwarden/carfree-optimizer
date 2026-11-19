//! E12-S01/S03 exact snapshot binding and pre-assignment failures.
#[path = "../examples/support/e12.rs"]
mod fixture;
use fixture::*;
use interventions::{evaluation::DemandBinding, feasibility::*};
use mobility_graph::{network::Network, EdgeId};
use mobility_sim::{
    demand::Demand,
    simulation::{self as sim, Adapter},
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
struct Counting {
    calls: AtomicUsize,
    expected: Vec<u8>,
}
impl Adapter for Counting {
    fn name(&self) -> &'static str {
        "e12-counting"
    }
    fn evaluate(
        &self,
        n: &Network,
        d: &Demand,
        s: &sim::Scenario,
    ) -> Result<sim::Report, sim::Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(n.encode(), self.expected);
        sim::FixedCost.evaluate(n, d, s)
    }
}
#[test]
fn e12_s01_s03_given_hard_gate_failure_when_evaluation_chained_then_adapter_is_never_called() {
    let h = base::history();
    let b = proposal(&h, "same-id");
    let mut i = inputs(&h, &b);
    let snapshot = h.preview(&b).unwrap();
    let (d, s) = base::inputs(&snapshot);
    let binding = DemandBinding {
        scenario_id: snapshot.id.clone(),
        demand_revision: d.revision.clone(),
        land_use_revision: None,
    };
    let engine = Counting {
        calls: AtomicUsize::new(0),
        expected: snapshot.network.encode(),
    };
    i.budget.capital = 1.0;
    let result = screen(&h, &b, &i)
        .map_err(|e| e.to_string())
        .and_then(|approved| {
            approved
                .evaluate(&engine, &d, &s, &binding)
                .map_err(|e| e.to_string())
        });
    assert!(result.is_err());
    assert_eq!(engine.calls.load(Ordering::SeqCst), 0);
    i = inputs(&h, &b);
    i.access[0].returning.deadline_utc = 1;
    let result = screen(&h, &b, &i)
        .map_err(|e| e.to_string())
        .and_then(|approved| {
            approved
                .evaluate(&engine, &d, &s, &binding)
                .map_err(|e| e.to_string())
        });
    assert!(result.is_err());
    assert_eq!(engine.calls.load(Ordering::SeqCst), 0);
    i = inputs(&h, &b);
    let approved = screen(&h, &b, &i).unwrap();
    assert!(approved.evaluate(&engine, &d, &s, &binding).is_ok());
    assert_eq!(engine.calls.load(Ordering::SeqCst), 1);
    // Another history can reuse IDs but cannot inject its snapshot into Screened.
    let mut other = base::history();
    let other_b = base::batch(&other, "same-id", vec![base::bike(&other, 8, 4)]);
    other.apply(&other_b).unwrap();
    assert_ne!(other.current().network, approved.snapshot().network);
    assert_eq!(approved.snapshot().roads[&EdgeId(7)].general_lanes, 2);
    assert!(approved.evaluate(&engine, &d, &s, &binding).is_ok()); // engine still sees ORIGINAL bytes
    let mut double = s.clone();
    double.roads[0].vehicles_per_hour[0] += 270.0;
    assert!(approved.evaluate(&engine, &d, &double, &binding).is_err());
    assert_eq!(engine.calls.load(Ordering::SeqCst), 2);
    let mut stale = d.clone();
    stale.network_revision = "other".into();
    assert!(approved.evaluate(&engine, &stale, &s, &binding).is_err());
    assert_eq!(engine.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn e12_s07_given_phase_snapshot_when_demand_outlives_it_then_adapter_is_not_called() {
    let h = base::history();
    let b = proposal(&h, "phase");
    let i = inputs(&h, &b);
    let mut final_i = i.clone();
    for c in &mut final_i.costs {
        if c.category != Category::Maintenance {
            c.quantity = 0.0;
        }
    }
    let phases = vec![PhaseInput {
        start: 0,
        end: 60,
        permanent: b,
        temporary: None,
        services: BTreeMap::new(),
        inputs: i,
    }];
    let plan = screen_plan(&h, &phases, &final_i).unwrap();
    let screened = plan.phases()[0].screened();
    let (mut d, s) = base::inputs(screened.snapshot());
    let binding = DemandBinding {
        scenario_id: s.id.clone(),
        demand_revision: d.revision.clone(),
        land_use_revision: None,
    };
    let engine = Counting {
        calls: AtomicUsize::new(0),
        expected: screened.snapshot().network.encode(),
    };
    assert!(screened
        .evaluate(&engine, &d, &s, &binding)
        .unwrap_err()
        .0
        .contains("phase interval"));
    assert_eq!(engine.calls.load(Ordering::SeqCst), 0);
    for tour in &mut d.tours {
        tour.legs[0].deadline_utc = 40;
        tour.legs[1].departure_utc = 40;
        tour.legs[1].deadline_utc = 60;
        tour.legs[1].window.start = 40;
        tour.legs[1].window.end = 55;
    }
    assert!(screened.evaluate(&engine, &d, &s, &binding).is_ok());
    assert_eq!(engine.calls.load(Ordering::SeqCst), 1);
    d.tours[0].legs[1].deadline_utc = 61;
    assert!(screened.evaluate(&engine, &d, &s, &binding).is_err());
    assert_eq!(engine.calls.load(Ordering::SeqCst), 1);
}
