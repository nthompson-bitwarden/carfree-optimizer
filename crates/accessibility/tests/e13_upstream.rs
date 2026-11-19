//! E13 integration with actual E07, E08 and opaque E12 contracts.
#[path = "../../interventions/examples/support/e12.rs"]
mod fixture;
mod support;
use accessibility::{comparison::*, scoring::*, upstream::*};
use interventions::{
    evaluation::DemandBinding,
    feasibility::{self, Screened},
};
use mobility_graph::{network::*, EdgeId, Mode as GMode, NodeId};
use mobility_routing::{planner::Options, Profile, Request};
use mobility_sim::simulation::{self, Adapter};
use support::*;
fn network() -> Network {
    let source = fixture::base::synthetic();
    let street = |id, from, to, mode, seconds| Link {
        id: EdgeId(id),
        from: NodeId(from),
        to: NodeId(to),
        distance_m: Some(100.0),
        seconds,
        access: Access::Accessible,
        kind: LinkKind::Street {
            mode,
            crossing: false,
            blocked: false,
            cycling: CycleFacility::Unspecified,
            safety: None,
            capacity: None,
        },
    };
    Network::new(NetworkData {
        revision: "paired-doors-v1".into(),
        provenance: source,
        timezone: "UTC".into(),
        tzdb: "synthetic".into(),
        coverage_start: 0,
        coverage_end: 1000,
        nodes: (0..3)
            .map(|id| Node {
                id: NodeId(id),
                access: Access::Accessible,
            })
            .collect(),
        links: vec![
            street(0, 0, 1, GMode::Car, 10),
            street(1, 1, 2, GMode::Walk, 5),
            street(2, 0, 2, GMode::Walk, 30),
            Link {
                id: EdgeId(3),
                from: NodeId(1),
                to: NodeId(1),
                distance_m: None,
                seconds: 2,
                access: Access::Accessible,
                kind: LinkKind::Transfer {
                    from: GMode::Car,
                    to: GMode::Walk,
                    action: TransferAction::Park,
                    windows: vec![(0, 1000)],
                    bicycle_allowed: false,
                },
            },
        ],
        turns: vec![],
        calendars: vec![],
        days: vec![],
    })
    .unwrap()
}
#[test]
fn e13_s01_s02_given_actual_door_requests_then_parking_egress_and_errors_survive() {
    let n = network();
    let cf = Request {
        from: NodeId(0),
        to: NodeId(2),
        departure_utc: 0,
        deadline_utc: 60,
        initial_mode: GMode::Walk,
        vehicle: None,
        profile: Profile {
            allowed_modes: vec![GMode::Walk],
            step_free: true,
            mobility_aid: false,
            allow_unknown_access: false,
            protected_cycling_only: false,
            maximum_cycling_stress: None,
        },
        max_states: 1000,
    };
    let mut car = cf.clone();
    car.initial_mode = GMode::Car;
    car.vehicle = Some(GMode::Car);
    car.profile.allowed_modes.push(GMode::Car);
    let p = route_pair(&n, "clinic", &cf, &car, &Options::default()).unwrap();
    if let Evidence::Feasible(j) = &p.pair().car {
        assert_eq!(j.seconds(), 17);
        assert_eq!((j.transfer, j.egress), (2, 5));
    } else {
        panic!("car must be feasible");
    }
    assert_eq!(
        p.pair().car_free,
        Evidence::Feasible(Journey {
            access: 30,
            wait: 0,
            ride: 0,
            transfer: 0,
            intermediate_walk: 0,
            egress: 0,
            dwell: 0
        })
    );
    let mut c = context();
    c.graph_revision = n.data().revision.clone();
    c.graph_snapshot = n.encode();
    let mut stratum = access_input().stratum;
    stratum.departures_utc = vec![0];
    stratum.modes = vec![Mode::Walk];
    let destinations = [("clinic".into(), NodeId(2))].into();
    let origins = [(stratum.origin.clone(), NodeId(0))].into();
    assert_eq!(
        routed_access(
            &c,
            &stratum,
            &n,
            &cf.profile,
            &destinations,
            &origins,
            std::slice::from_ref(&p)
        )
        .unwrap()
        .reachable_share,
        Some(1.0)
    );
    stratum.budget_seconds = 120;
    assert!(routed_access(
        &c,
        &stratum,
        &n,
        &cf.profile,
        &destinations,
        &origins,
        std::slice::from_ref(&p)
    )
    .is_err());
    let mut invalid_door = n.data().clone();
    let mut direct = invalid_door.links[0].clone();
    direct.id = EdgeId(99);
    direct.to = NodeId(2);
    invalid_door.links.push(direct);
    assert!(route_pair(
        &Network::new(invalid_door).unwrap(),
        "clinic",
        &cf,
        &car,
        &Options::default()
    )
    .unwrap_err()
    .0
    .contains("walk-only door"));
    let mut failed = cf.clone();
    failed.max_states = 1;
    assert!(route_pair(&n, "clinic", &failed, &car, &Options::default())
        .unwrap_err()
        .0
        .contains("routing failed"));
    car.departure_utc = 1;
    assert!(route_pair(&n, "clinic", &cf, &car, &Options::default()).is_err());
}
#[test]
fn e13_s09_given_routed_day_and_night_departures_then_grids_match_within_each_condition() {
    let n = network();
    let mut rows = Vec::new();
    let mut grid = Vec::new();
    for (label, time) in [("day", 0), ("night", 100)] {
        let mut cf = fixture::base::request(0, 2, GMode::Walk);
        cf.departure_utc = time;
        cf.deadline_utc = time + 60;
        let mut car = cf.clone();
        car.initial_mode = GMode::Car;
        car.vehicle = Some(GMode::Car);
        car.profile.allowed_modes.push(GMode::Car);
        let p = route_pair(&n, "clinic", &cf, &car, &Options::default()).unwrap();
        let mut c = context();
        c.graph_revision = n.data().revision.clone();
        c.graph_snapshot = n.encode();
        c.service_revision = format!("{label}-service-v1");
        let mut s = access_input().stratum;
        s.departures_utc = vec![time];
        s.modes = vec![Mode::Walk];
        let origins = [(s.origin.clone(), NodeId(0))].into();
        let destinations = [("clinic".into(), NodeId(2))].into();
        let a = routed_access(
            &c,
            &s,
            &n,
            &cf.profile,
            &destinations,
            &origins,
            std::slice::from_ref(&p),
        )
        .unwrap();
        let bad_origins = [(s.origin.clone(), NodeId(1))].into();
        assert!(routed_access(
            &c,
            &s,
            &n,
            &cf.profile,
            &destinations,
            &bad_origins,
            std::slice::from_ref(&p)
        )
        .is_err());
        let condition = Condition {
            weather: "dry".into(),
            day_or_night: label.into(),
            service_revision: c.service_revision.clone(),
        };
        grid.push((condition.clone(), 1.0));
        rows.push(ConditionScore {
            condition,
            score: accessibility::projection::Projection::from_access(&a),
        });
    }
    assert_eq!(rank_conditions(&grid, &rows).unwrap().ranked[0].1, 1.0);
}
fn screened(id: &str) -> Screened {
    let h = fixture::base::history();
    let b = fixture::proposal(&h, id);
    feasibility::screen(&h, &b, &fixture::inputs(&h, &b)).unwrap()
}
fn scores(gate: &Screened) -> (AccessScore, TripScore) {
    let mut i = access_input();
    i.context.scenario = gate.snapshot().id().clone();
    i.context.graph_revision = gate.snapshot().network().data().revision.clone();
    i.context.graph_snapshot = gate.snapshot().network().encode();
    let (d, _) = fixture::base::inputs(gate.snapshot());
    i.context.demand_revision = d.revision.clone();
    i.context.land_use_revision = gate.snapshot().land_use_revision().clone();
    let a = access(&i).unwrap();
    let t = trips(&i.context, &demand_rows(&d), unavailable()).unwrap();
    let t = accessibility::demand_binding::bind_demand(
        t,
        gate,
        &d,
        &DemandBinding {
            scenario_id: i.context.scenario.clone(),
            demand_revision: d.revision.clone(),
            land_use_revision: i.context.land_use_revision.clone(),
        },
        1.0,
    )
    .unwrap();
    (a, t)
}
fn candidate(
    gate: &Screened,
    access: &AccessScore,
    trips: &TripScore,
    risk: Option<&feasibility::RiskInput>,
    constraints: &str,
) -> Result<Candidate, accessibility::scoring::Error> {
    let h = fixture::base::history();
    let b = fixture::proposal(&h, gate.snapshot().id());
    let inventory = ProjectCostInventory {
        baseline: h.current().clone(),
        scenario: gate.snapshot().clone(),
        costs: gate.inputs().costs.clone(),
        transit: gate.inputs().transit.clone(),
        completeness_source: fixture::base::synthetic(),
    };
    Candidate::single(
        &SingleProject {
            cost_inventory: &inventory,
            history: &h,
            batch: &b,
            inputs: gate.inputs(),
        },
        access,
        trips,
        risk,
        constraints,
    )
}
fn risk(gate: &Screened, rate: f64) -> feasibility::RiskInput {
    feasibility::RiskInput {
        exposure_scenario_id: gate.snapshot().id().clone(),
        exposure_graph_revision: gate.snapshot().network().data().revision.clone(),
        model: "synthetic-rate".into(),
        version: "1".into(),
        mode: GMode::Walk,
        severity: "all synthetic collisions".into(),
        unit: feasibility::ExposureUnit::PersonKm,
        exposure: 1000.0,
        exposure_source: fixture::base::synthetic(),
        calibration: Some(fixture::base::synthetic()),
        applicability: "abstract corridor; not empirical".into(),
        period_start: 0,
        period_end: 60,
        rate: feasibility::Bounds {
            lower: rate,
            central: rate,
            upper: rate,
        },
        uncertainty: "synthetic point factor, no measured precision".into(),
    }
}
#[test]
fn e13_s08_given_screened_candidates_then_dominance_missing_risk_and_currency_checks() {
    let g1 = screened("a");
    let (a, t) = scores(&g1);
    let c1 = candidate(
        &g1,
        &a,
        &t,
        Some(&risk(&g1, 0.001)),
        "protected registry v1",
    )
    .unwrap();
    let g2 = screened("b");
    let (b, u) = scores(&g2);
    let c2 = candidate(
        &g2,
        &b,
        &u,
        Some(&risk(&g2, 0.002)),
        "protected registry v1",
    )
    .unwrap();
    let mut other_mode = risk(&g2, 0.002);
    other_mode.mode = GMode::Bicycle;
    let incompatible = candidate(&g2, &b, &u, Some(&other_mode), "protected registry v1").unwrap();
    assert!(pareto(&[c1.clone(), incompatible]).is_err());
    let g3 = screened("missing");
    let (m, v) = scores(&g3);
    let missing = candidate(&g3, &m, &v, None, "protected registry v1").unwrap();
    let r = pareto(&[c1.clone(), c2, missing]).unwrap();
    assert_eq!(r.nondominated, ["a"]);
    assert_eq!(r.dominated, ["b"]);
    assert!(r.unranked.contains_key("missing"));
    assert!(candidate(
        &g2,
        &a,
        &t,
        Some(&risk(&g2, 0.002)),
        "protected registry v1"
    )
    .is_err());
    let other_constraints = candidate(
        &g2,
        &b,
        &u,
        Some(&risk(&g2, 0.002)),
        "different protected registry",
    )
    .unwrap();
    assert!(pareto(&[c1.clone(), other_constraints]).is_err());
    let h = fixture::base::history();
    let batch = fixture::proposal(&h, "price-year");
    let mut inputs = fixture::inputs(&h, &batch);
    inputs.budget.price_year = 2025;
    for c in &mut inputs.costs {
        c.price_year = 2025;
    }
    for c in &mut inputs.transit {
        c.price_year = 2025;
    }
    let g = feasibility::screen(&h, &batch, &inputs).unwrap();
    let (a, t) = scores(&g);
    let c = candidate(&g, &a, &t, Some(&risk(&g, 0.001)), "protected registry v1").unwrap();
    assert!(pareto(&[c1, c]).is_err());
}
#[test]
fn e13_s08_given_staged_final_gate_then_missing_prior_capital_cannot_improve_frontier() {
    let h = fixture::base::history();
    let b = fixture::proposal(&h, "stage");
    let inputs = fixture::inputs(&h, &b);
    let mut final_inputs = inputs.clone();
    for c in &mut final_inputs.costs {
        if c.category != feasibility::Category::Maintenance {
            c.quantity = 0.0;
        }
    }
    let plan = feasibility::screen_plan(
        &h,
        &[feasibility::PhaseInput {
            start: 0,
            end: 60,
            permanent: b.clone(),
            temporary: None,
            services: Default::default(),
            inputs: inputs.clone(),
        }],
        &final_inputs,
    )
    .unwrap();
    assert!(plan.capital.central > 0.0);
    assert_eq!(plan.final_state().report().capital.central, 0.0);
    let gate = plan.final_state();
    let (a, t) = scores(gate);
    let staged = Candidate::screened(
        gate,
        &a,
        &t,
        Some(&risk(gate, 0.001)),
        "protected registry v1",
    )
    .unwrap();
    assert_eq!(staged.objectives(), None);
    let frontier = pareto(&[staged]).unwrap();
    assert!(frontier.nondominated.is_empty());
    assert!(frontier.unranked.contains_key(gate.snapshot().id()));
    // Re-screening final_inputs must not smuggle omitted phase capital through
    // the single-project API: the independently supplied complete inventory has
    // the original nonzero construction quantities.
    let fresh = feasibility::screen(&h, &b, &final_inputs).unwrap();
    let (a, t) = scores(&fresh);
    let inventory = ProjectCostInventory {
        baseline: h.current().clone(),
        scenario: fresh.snapshot().clone(),
        costs: inputs.costs.clone(),
        transit: inputs.transit.clone(),
        completeness_source: fixture::base::synthetic(),
    };
    let error = Candidate::single(
        &SingleProject {
            cost_inventory: &inventory,
            history: &h,
            batch: &b,
            inputs: &final_inputs,
        },
        &a,
        &t,
        Some(&risk(&fresh, 0.001)),
        "protected registry v1",
    )
    .unwrap_err();
    assert!(error.0.contains("complete project cost inventory"));
}
#[test]
fn e13_s03_s07_given_real_screened_assignment_then_status_mode_weights_and_distance_survive() {
    let gate = screened("assignment");
    let (d, s) = fixture::base::inputs(gate.snapshot());
    let evaluation = gate
        .evaluate(
            &simulation::FixedCost,
            &d,
            &s,
            &DemandBinding {
                scenario_id: s.id.clone(),
                demand_revision: d.revision.clone(),
                land_use_revision: None,
            },
        )
        .unwrap();
    let run = evaluation.report;
    let mut c = context();
    c.scenario = run.scenario_id.clone();
    c.graph_revision = run.network_revision.clone();
    c.graph_snapshot = gate.snapshot().network().encode();
    c.demand_revision = run.demand_revision.clone();
    let binding = DemandBinding {
        scenario_id: s.id.clone(),
        demand_revision: d.revision.clone(),
        land_use_revision: None,
    };
    let e = screened_simulation_evidence(&c, &gate, &d, &s, &binding, &simulation::FixedCost, None)
        .unwrap();
    assert_eq!(e.run(), &run);
    let mut stale_binding = binding.clone();
    stale_binding.demand_revision = "stale".into();
    assert!(screened_simulation_evidence(
        &c,
        &gate,
        &d,
        &s,
        &stale_binding,
        &simulation::FixedCost,
        None
    )
    .is_err());
    let mut double = s.clone();
    double.roads[0].vehicles_per_hour[0] += 1.0;
    assert!(screened_simulation_evidence(
        &c,
        &gate,
        &d,
        &double,
        &binding,
        &simulation::FixedCost,
        None
    )
    .is_err());
    assert_eq!(e.status, simulation::Status::FixedCosts);
    assert!(e.calibration_banner.contains("LIMITED FITNESS"));
    let expected: f64 = run
        .roads
        .iter()
        .map(|r| {
            r.vehicles
                * gate
                    .snapshot()
                    .network()
                    .link(r.edge)
                    .unwrap()
                    .distance_m
                    .unwrap()
                / 1000.0
        })
        .sum();
    assert!((e.road_vehicle_km - expected).abs() < 1e-10);
    assert_eq!(
        e.served_mode_trip_weights.values().sum::<f64>(),
        run.mode_trip_weights.values().sum::<f64>()
    );
    let bridge = screened_trip_bridge(&c, &gate, &d, &s, &binding, &simulation::FixedCost).unwrap();
    assert_eq!(bridge.assignment.all.car_required_share(), None);
    assert!(bridge
        .reference_feasibility
        .context
        .behavior
        .contains("REFERENCE"));
    assert_eq!(bridge.assignment.all.unserved, run.unserved_trip_weight);
    let mut scaled = s.clone();
    scaled.settings.demand_multiplier = 2.0;
    let doubled = trip_bridge(
        &c,
        gate.snapshot().network(),
        &d,
        &scaled,
        &simulation::FixedCost,
    )
    .unwrap();
    assert_eq!(
        doubled.assignment.all.total,
        bridge.assignment.all.total * 2.0
    );
    c.graph_revision = "stale".into();
    assert!(simulation_evidence(
        &c,
        gate.snapshot().network(),
        &d,
        &s,
        &simulation::FixedCost,
        None
    )
    .is_err());
    // Keep Adapter trait exercised independently of the E12 bridge as well.
    assert!(simulation::FixedCost
        .evaluate(gate.snapshot().network(), &d, &s)
        .is_ok());
}
