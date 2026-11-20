//! E14-S01–S05/S07 transactional acceptance against actual E10/E12/E08/E13.
use accessibility::scoring::Evidence;
use interventions::{
    feasibility::LaneUse,
    scenario::{Change, History},
};
use mobility_graph::{network::Network, EdgeId, Mode, NodeId};
use mobility_routing::Request;
use mobility_sim::{demand::TravelMode, simulation::Status};
use optimizer_env::{
    catalog::{Catalog, Reason},
    reference::{Config, Environment, Error, Project, TransitPortfolio},
    synthetic,
    wire::BatchEnvironment,
};

fn project(config: &mut Config, index: usize, edit: impl FnOnce(&mut Project)) {
    let mut projects = config.catalog.projects().to_vec();
    edit(&mut projects[index].payload);
    config.catalog = Catalog::new(config.catalog.revision().into(), projects).unwrap();
}

fn configured(topology: usize) -> (Config, Environment) {
    let config = synthetic::configuration(topology).unwrap();
    let env = Environment::new(config.clone()).unwrap();
    (config, env)
}

#[test]
fn e14_s01_s02_given_seeded_episode_when_stale_unknown_or_repeated_then_state_is_unchanged() {
    let (_, mut env) = configured(0);
    assert!(env
        .step(0, 0)
        .unwrap_err()
        .to_string()
        .contains("reset required"));
    let reset = env.reset(42).unwrap().clone();
    assert_eq!(reset.version, 1);
    assert_eq!(reset.reward.reward, 0.0);
    assert!(!reset.terminated && !reset.truncated);
    assert!(reset.evidence.screening.is_none());
    assert_eq!(reset.evidence.trips.evidence.run().seed, 42);
    assert_eq!(reset.wire().action_mask, [true; 4]);
    assert_eq!(reset.wire().observation.len(), synthetic::WIRE_FEATURES);
    assert!(env.step(100, reset.state).is_err());
    assert_eq!(env.current(), Some(&reset));
    let first = env.step(0, reset.state).unwrap().clone();
    assert!(first.evidence.screening.is_some());
    assert!(env
        .step(1, reset.state)
        .unwrap_err()
        .to_string()
        .contains("stale"));
    assert!(env
        .step(0, first.state)
        .unwrap_err()
        .to_string()
        .contains("AlreadyApplied"));
    assert_eq!(env.current(), Some(&first));
    let second = env.step(1, first.state).unwrap().clone();
    assert_eq!(second.wire().action_mask, [false; 4]);
    assert!(second.truncated && !second.terminated);
    assert!(env.step(1, second.state).is_err());
    assert_eq!(env.current(), Some(&second));
    assert_eq!(
        second.evidence.trips.evidence.context.graph_snapshot,
        env.snapshot().unwrap().network().encode()
    );

    let again = env.reset(42).unwrap().clone();
    assert!(again.state > second.state);
    assert_eq!(again.observation, reset.observation);
    assert_eq!(again.evidence, reset.evidence);
    assert!(env.step(0, first.state).is_err());
    let replay_first = env.step(0, again.state).unwrap().clone();
    let replay_second = env.step(1, replay_first.state).unwrap();
    assert_eq!(replay_first.reward, first.reward);
    assert_eq!(replay_first.evidence, first.evidence);
    assert_eq!(replay_second.evidence, second.evidence);
    assert_eq!(replay_second.reward, second.reward);

    let mut wire = synthetic::environment(0).unwrap();
    let state = wire.reset(42).unwrap().state;
    let stop = wire.step(3, state).unwrap();
    assert!(stop.terminated && !stop.truncated);
    assert_eq!(stop.reward, 0.0);
    assert_eq!(stop.action_mask, [false; 4]);
    assert!(wire.step(0, stop.state).is_err());
}

#[test]
fn e14_s05_given_fixed_caps_then_capital_is_charged_once_and_annual_is_gross() {
    let (config, mut env) = configured(0);
    let state = env.reset(42).unwrap().clone();
    let first = env.step(0, state.state).unwrap().clone();
    let second = env.step(1, first.state).unwrap();
    assert_eq!(state.budget.annual.upper, 5.0);
    assert_eq!(first.budget.capital.upper, 60.0);
    assert_eq!(first.budget.annual.upper, 13.0);
    assert_eq!(second.budget.incremental_capital.upper, 40.0);
    assert_eq!(second.budget.capital.upper, second.budget.limit.capital);
    assert_eq!(second.budget.annual.upper, second.budget.limit.annual);
    let report = second.evidence.screening.as_ref().unwrap();
    assert_eq!(report.maintenance.upper, 18.0);
    assert_eq!(report.transit_annual, 2.0);
    assert_eq!(report.disruption.upper, 4.0);
    let inventory = env.portfolio().unwrap();
    assert_eq!(inventory.access, config.opening.access);
    assert!(inventory
        .segments
        .iter()
        .any(|s| s.edges.contains(&EdgeId(100))));
    assert!(inventory
        .segments
        .iter()
        .any(|s| s.edges.contains(&EdgeId(101))));
}

#[test]
fn e14_s02_s05_given_individually_feasible_projects_then_joint_caps_and_space_mask_them() {
    for (capital, annual, action, reason) in [
        (100.0, 100.0, 2, "capital upper"),
        (1000.0, 19.0, 1, "annual upper"),
        (1000.0, 100.0, 2, "right-of-way shared-corridor"),
    ] {
        let mut config = synthetic::configuration(0).unwrap();
        config.opening.budget.capital = capital;
        config.opening.budget.annual = annual;
        let mut env = Environment::new(config).unwrap();
        let reset = env.reset(42).unwrap();
        assert!(reset.mask.entries[action].allowed());
        let state = reset.state;
        let first = env.step(0, state).unwrap().clone();
        assert!(matches!(first.mask.entries[action].reasons.as_slice(),
            [Reason::Screening(message)] if message.contains(reason)));
        assert!(env
            .step(action, first.state)
            .unwrap_err()
            .to_string()
            .contains(reason));
        assert_eq!(env.current(), Some(&first));
        assert!(env
            .snapshot()
            .unwrap()
            .network()
            .link(EdgeId(100))
            .is_some());
    }

    let mut config = synthetic::configuration(0).unwrap();
    let mut projects = config.catalog.projects().to_vec();
    let prerequisite = projects[0].id.clone();
    projects[1].requires.insert(prerequisite);
    config.catalog = Catalog::new("e14-order".into(), projects).unwrap();
    let mut env = Environment::new(config).unwrap();
    let opening = env.reset(42).unwrap().clone();
    assert!(matches!(
        opening.mask.entries[1].reasons.as_slice(),
        [Reason::MissingPrerequisite(_)]
    ));
    assert!(env.step(1, opening.state).is_err());
    assert_eq!(env.current(), Some(&opening));
    let first = env.step(0, opening.state).unwrap();
    assert!(first.mask.entries[1].allowed());
}

#[test]
fn e14_s04_s07_given_car_still_assigned_then_reference_reward_tracks_alternatives() {
    let (_, mut env) = configured(0);
    let reset = env.reset(42).unwrap().clone();
    let first = env.step(0, reset.state).unwrap();
    let report = first.evidence.trips.evidence.run();
    assert_eq!(report.status, Status::Converged);
    assert_eq!(report.mode_trip_weights[&TravelMode::Car], 4.0);
    assert_eq!(first.reward.before.all.car_required, 4.0);
    assert_eq!(first.reward.after.all.car_required, 0.0);
    assert_eq!(first.reward.components[0].denominator, 6.0);
    assert_eq!(first.reward.components[0].raw_gain, 4.0 / 6.0);
    assert!(first.reward.constraints.iter().all(|c| c.passed));
    assert!(first.reward.reward > 0.0);

    let mut config = synthetic::configuration(0).unwrap();
    config.reward.car_required.weight = 0.0;
    config.reward.car_free_feasible.weight = 0.0;
    config.reward.fixed_door_parity.weight = 0.0;
    let mut ablated = Environment::new(config).unwrap();
    let state = ablated.reset(42).unwrap().state;
    let ablation = ablated.step(0, state).unwrap();
    assert_eq!(ablation.reward.reward, 0.0);
    assert_eq!(ablation.reward.after, first.reward.after);
    assert_eq!(
        ablation.reward.components[0].raw_gain,
        first.reward.components[0].raw_gain
    );
}

fn close_direct_road(config: &mut Config) {
    let mut segments = config.opening.segments[0..1].to_vec();
    segments[0].lanes[0].kind = LaneUse::Retired;
    project(config, 0, |p| {
        p.changes = vec![Change::CloseRoad { edge: EdgeId(1) }];
        p.segments = segments;
        p.rows.clear();
    });
}

#[test]
fn e14_s04_s07_given_car_route_only_slows_then_door_parity_reward_does_not_increase() {
    let mut config = synthetic::configuration(0).unwrap();
    config.demand.tours[0].legs[0].deadline_utc = 60;
    close_direct_road(&mut config);
    let mut env = Environment::new(config).unwrap();
    let opening = env.reset(42).unwrap().clone();
    let changed = env.step(0, opening.state).unwrap();
    let door = &changed.evidence.doors[0];
    assert_eq!(door.baseline_car_seconds, 22);
    assert!(matches!(&door.current.pair().car, Evidence::Feasible(j) if j.seconds() == 50));
    assert_eq!(
        door.current.pair().car_free,
        opening.evidence.doors[0].current.pair().car_free
    );
    assert_eq!(changed.reward.components[2].raw_gain, 0.0);
    assert_eq!(changed.reward.reward, 0.0);
    assert!(changed
        .evidence
        .trips
        .evidence
        .run()
        .roads
        .iter()
        .all(|r| r.edge != EdgeId(1)));
}

#[test]
fn e14_s04_s07_given_car_share_falls_by_suppression_then_new_essential_loss_blocks_commit() {
    let mut config = synthetic::configuration(0).unwrap();
    close_direct_road(&mut config);
    let mut env = Environment::new(config).unwrap();
    let opening = env.reset(42).unwrap().clone();
    assert!(opening.mask.entries[0].allowed());
    let Error::Constraint(audit) = env.step(0, opening.state).unwrap_err() else {
        panic!("candidate must reach and fail hard constraints");
    };
    assert_eq!(audit.before.all.car_required, 4.0);
    assert_eq!(audit.after.all.car_required, 0.0);
    assert_eq!(audit.after.all.no_mode, 4.0);
    assert!(audit
        .constraints
        .iter()
        .any(|c| c.metric == "newly_unserved_essential" && !c.passed));
    assert!(audit
        .constraints
        .iter()
        .any(|c| c.metric == "no_mode" && !c.passed));
    assert_eq!(env.current(), Some(&opening));
    assert!(env
        .snapshot()
        .unwrap()
        .road_capacities()
        .unwrap()
        .iter()
        .any(|(e, _)| *e == EdgeId(1)));
}

#[test]
fn e14_s04_s07_given_missing_or_denied_passenger_supply_then_step_is_atomic() {
    for passengers in [None, Some(0.0)] {
        let mut config = synthetic::configuration(7).unwrap();
        let costs = config.opening.transit.clone();
        let mut supply = config.simulation.transit.clone();
        if let Some(p) = passengers {
            supply[0].passengers = p;
        } else {
            supply.clear();
        }
        project(&mut config, 2, |p| {
            p.transit = Some(TransitPortfolio { costs, supply })
        });
        let mut env = Environment::new(config).unwrap();
        let reset = env.reset(42).unwrap().clone();
        assert_eq!(
            reset.evidence.trips.evidence.run().mode_trip_weights[&TravelMode::Transit],
            2.0
        );
        assert!(reset.mask.entries[2].allowed());
        match env.step(2, reset.state).unwrap_err() {
            Error::Evidence(message) if passengers.is_none() => {
                assert!(message.contains("missing transit segment/run capacity"))
            }
            Error::Constraint(audit) if passengers.is_some() => {
                assert_eq!(audit.after.denied, 2.0);
                for protected in ["newly_unserved_noncycling", "newly_unserved_step_free"] {
                    assert!(audit
                        .constraints
                        .iter()
                        .any(|c| c.metric == protected && !c.passed));
                }
            }
            error => panic!("unexpected error {error}"),
        }
        assert_eq!(env.current(), Some(&reset));
        assert!(env.step(1, reset.state).is_ok());
    }
}

#[test]
fn e14_s04_s07_given_iteration_limit_or_missing_opening_evidence_then_no_reset_commits() {
    let mut config = synthetic::configuration(0).unwrap();
    config.simulation.settings.alpha = 1.0;
    config.simulation.settings.max_iterations = 1;
    let mut env = Environment::new(config).unwrap();
    let Error::Constraint(audit) = env.reset(42).unwrap_err() else {
        panic!("nonconverged opening must fail the hard constraint");
    };
    assert_eq!(audit.after.status, Status::Nonconverged);
    assert!(!audit.convergence_passed);
    assert!(env.current().is_none());

    let mut config = synthetic::configuration(0).unwrap();
    config.demand.tours.pop();
    let mut env = Environment::new(config).unwrap();
    assert!(env
        .reset(42)
        .unwrap_err()
        .to_string()
        .contains("denominators required"));
    assert!(env.current().is_none());
}

fn reverse_node(id: NodeId) -> NodeId {
    NodeId(1000 - id.0)
}

fn reverse_edge(id: EdgeId) -> EdgeId {
    EdgeId(10_000 - id.0)
}

fn reverse_request(req: &mut Request) {
    req.from = reverse_node(req.from);
    req.to = reverse_node(req.to);
}

fn relabel(config: &mut Config) {
    let snapshot = config.history.current();
    let mut data = snapshot.network().data().clone();
    data.revision = "arbitrary-opaque-graph-id".into();
    for node in &mut data.nodes {
        node.id = reverse_node(node.id);
    }
    for link in &mut data.links {
        link.id = reverse_edge(link.id);
        link.from = reverse_node(link.from);
        link.to = reverse_node(link.to);
    }
    let mut roads: Vec<_> = snapshot.roads().values().cloned().collect();
    for road in &mut roads {
        road.road = reverse_edge(road.road);
    }
    let graph = Network::new(data).unwrap();
    config.context.graph_revision = graph.data().revision.clone();
    config.context.graph_snapshot = graph.encode();
    config.demand.network_revision = graph.data().revision.clone();
    config.history = History::new(snapshot.id().clone(), graph, roads, vec![]).unwrap();
    for tour in &mut config.demand.tours {
        for leg in &mut tour.legs {
            leg.from = reverse_node(leg.from);
            leg.to = reverse_node(leg.to);
        }
    }
    for door in &mut config.doors {
        reverse_request(&mut door.car_free);
        reverse_request(&mut door.car);
    }
    for sample in &mut config.opening.access {
        reverse_request(&mut sample.outbound);
        reverse_request(&mut sample.returning);
        sample.permitted_car_edges = sample
            .permitted_car_edges
            .take()
            .map(|edges| edges.into_iter().map(reverse_edge).collect());
    }
    for supply in &mut config.simulation.transit {
        supply.edge = reverse_edge(supply.edge);
    }
    let fix_segment = |s: &mut interventions::feasibility::Segment| {
        s.road = s.road.map(reverse_edge);
        s.edges = s.edges.iter().copied().map(reverse_edge).collect();
    };
    for segment in &mut config.opening.segments {
        fix_segment(segment);
    }
    let mut projects = config.catalog.projects().to_vec();
    for p in &mut projects {
        p.payload.anchors = p.payload.anchors.map(|id| id.map(reverse_node));
        for segment in &mut p.payload.segments {
            fix_segment(segment);
        }
        for change in &mut p.payload.changes {
            if let Change::Pedestrian { link, .. } = change {
                link.id = reverse_edge(link.id);
                link.from = reverse_node(link.from);
                link.to = reverse_node(link.to);
            }
        }
    }
    config.catalog = Catalog::new(config.catalog.revision().into(), projects).unwrap();
}

#[test]
fn e14_s03_given_relabeling_then_vector_is_invariant_and_graph_is_equivariant() {
    let (mut config, mut original) = configured(7);
    relabel(&mut config);
    let mut reordered = Environment::new(config).unwrap();
    let a = original.reset(42).unwrap().clone();
    let b = reordered.reset(42).unwrap().clone();
    assert_eq!(a.wire(), b.wire());
    let n = a.observation.nodes.len();
    for (old, new) in a
        .observation
        .nodes
        .iter()
        .zip(b.observation.nodes.iter().rev())
    {
        assert_eq!(old, new);
    }
    for (old, new) in a
        .observation
        .edges
        .iter()
        .zip(b.observation.edges.iter().rev())
    {
        assert_eq!((n - 1 - old.from, n - 1 - old.to), (new.from, new.to));
        assert_eq!(old.kind, new.kind);
        assert_eq!(old.distance_metres, new.distance_metres);
    }
    for (left, right) in a.observation.actions.iter().zip(&b.observation.actions) {
        assert_eq!(left.anchors.map(|i| i.map(|j| n - 1 - j)), right.anchors);
    }
    assert_eq!(
        original.step(1, a.state).unwrap().wire(),
        reordered.step(1, b.state).unwrap().wire()
    );

    let (_, mut different_size) = configured(63);
    let changed = different_size.reset(42).unwrap();
    assert_eq!(changed.wire().observation.len(), a.wire().observation.len());
    assert_ne!(changed.observation.nodes.len(), n);
    assert!(changed.observation.edges.iter().any(|edge| matches!(
        edge.kind,
        optimizer_env::observation::EdgeKind::Street {
            mode: Mode::Walk,
            ..
        }
    )));
}

#[test]
fn e14_s05_unchanged_service_cannot_rewrite_cost_basis_even_with_new_supply() {
    for field in [
        "operations",
        "coverage",
        "annualization",
        "factor",
        "provenance",
    ] {
        let mut config = synthetic::configuration(0).unwrap();
        let mut costs = config.opening.transit.clone();
        let mut supply = config.simulation.transit.clone();
        supply[0].passengers += 1.0;
        match field {
            "operations" => costs[0].operations.vehicle_minor_per_hour /= 2,
            "coverage" => costs[0].coverage_repetitions_per_year /= 2.0,
            "annualization" => costs[0].annualization.push_str(" rewritten"),
            "factor" => costs[0].local_factor /= 2.0,
            "provenance" => costs[0].provenance.source_version.push_str("-rewrite"),
            _ => unreachable!(),
        }
        project(&mut config, 0, |p| {
            p.transit = Some(TransitPortfolio { costs, supply })
        });
        let mut env = Environment::new(config).unwrap();
        let before = env.reset(42).unwrap().clone();
        let snapshot = env.snapshot().unwrap().clone();
        let portfolio = env.portfolio().unwrap().clone();
        assert!(matches!(before.mask.entries[0].reasons.as_slice(),
            [Reason::Screening(message)] if message.contains("transit cost basis changed")));
        let error = env.step(0, before.state).unwrap_err();
        assert!(
            error.to_string().contains("transit cost basis changed"),
            "{field}: {error}"
        );
        assert_eq!(env.current(), Some(&before));
        assert_eq!(env.snapshot(), Some(&snapshot));
        assert_eq!(env.portfolio(), Some(&portfolio));
        let valid = env.step(1, before.state).unwrap();
        assert_eq!(
            valid.evidence.screening.as_ref().unwrap().transit_annual,
            2.0
        );
    }
}

#[test]
fn e14_s04_s07_gross_car_free_loss_blocks_a_served_tour_swap_with_zero_net_loss() {
    let mut config = synthetic::configuration(0).unwrap();
    let snapshot = config.history.current();
    let mut graph = snapshot.network().data().clone();
    for link in graph.links.iter_mut().filter(|l| [4, 6].contains(&l.id.0)) {
        link.seconds = 6;
    }
    let network = Network::new(graph).unwrap();
    config.context.graph_snapshot = network.encode();
    let road_ledger = snapshot.roads().values().cloned().collect();
    config.history = History::new(snapshot.id().clone(), network, road_ledger, vec![]).unwrap();
    let modes = config.demand.groups[0].modes.clone();
    let costs = config.demand.tours[0].legs[0].costs.clone();
    for group in &mut config.demand.groups {
        group.modes = modes.clone();
        group.step_free = true;
        group.mobility_aid = true;
    }
    let opening = &mut config.demand.tours;
    opening[0].legs[0].expansion_weight = 3.0;
    let returning = &mut opening[1].legs[0];
    returning.from = NodeId(2);
    returning.to = NodeId(1);
    returning.deadline_utc = 25;
    returning.expansion_weight = 3.0;
    returning.costs = costs;

    let mut replaced = config
        .history
        .current()
        .network()
        .link(EdgeId(6))
        .unwrap()
        .clone();
    replaced.seconds = 40;
    project(&mut config, 0, |p| {
        let Change::Pedestrian { assessment, .. } = &p.changes[0] else {
            unreachable!()
        };
        p.changes.push(Change::Pedestrian {
            link: replaced,
            assessment: assessment.clone(),
        });
        let mut row = p.rows[0].clone();
        row.id = "returning-corridor".into();
        let mut segment = p.segments[0].clone();
        segment.id = row.id.clone();
        segment.row = row.id.clone();
        segment.edges = vec![EdgeId(6)];
        p.rows.push(row);
        p.segments.push(segment);
    });

    let mut env = Environment::new(config).unwrap();
    let before = env.reset(42).unwrap().clone();
    assert!(
        before.mask.entries[0].allowed(),
        "{:?}",
        before.mask.entries[0]
    );
    let Error::Constraint(audit) = env.step(0, before.state).unwrap_err() else {
        panic!("matched car-free loss must fail the hard constraint");
    };
    assert_eq!(audit.before.all.car_free, 3.0);
    assert_eq!(audit.before.all, audit.after.all);
    assert_eq!(audit.after.all.unserved, 0.0);
    for cohort in ["essential", "noncycling", "step_free"] {
        let metric = format!("gross_{cohort}_car_free_loss");
        assert!(audit
            .constraints
            .iter()
            .any(|c| c.metric == metric && c.increase_or_loss == 3.0 && !c.passed));
    }
    assert_eq!(env.current(), Some(&before));
    assert_eq!(
        env.snapshot()
            .unwrap()
            .network()
            .link(EdgeId(6))
            .unwrap()
            .seconds,
        6
    );
    assert!(env
        .snapshot()
        .unwrap()
        .network()
        .link(EdgeId(100))
        .is_none());
}

#[test]
fn e14_s04_overflow_rejects_and_equal_subnormal_weight_and_scale_preserve_reward() {
    let tiny = f64::from_bits(1);
    for weight in [1.0, tiny] {
        let mut config = synthetic::configuration(0).unwrap();
        config.reward.car_required.weight = weight;
        config.reward.car_required.scale = tiny;
        config.reward.car_free_feasible.weight = 0.0;
        config.reward.fixed_door_parity.weight = 0.0;
        let mut env = Environment::new(config).unwrap();
        let before = env.reset(42).unwrap().clone();
        let snapshot = env.snapshot().unwrap().clone();
        let portfolio = env.portfolio().unwrap().clone();
        if weight == 1.0 {
            assert!(
                matches!(env.step(0, before.state), Err(Error::Evidence(message)) if message == "reward overflow")
            );
            assert_eq!(env.current(), Some(&before));
            assert_eq!(env.snapshot(), Some(&snapshot));
            assert_eq!(env.portfolio(), Some(&portfolio));
            assert_eq!(env.step(3, before.state).unwrap().state, before.state + 1);
        } else {
            let step = env.step(0, before.state).unwrap();
            assert_eq!(step.reward.components[0].weighted, 4.0 / 6.0);
            assert_eq!(step.reward.reward, 4.0 / 6.0);
        }
    }
}

#[test]
fn e14_s08_held_out_routes_change_project_outcomes_at_matched_costs() {
    for (topology, opening_seconds, after_seconds, newly_car_free) in [
        (4, 41, 30, 4.0),
        (7, 38, 38, 0.0),
        (5, 39, 36, 0.0),
        (8, 43, 32, 4.0),
    ] {
        let (_, mut env) = configured(topology);
        let opening = env.reset(42).unwrap().clone();
        assert_eq!(opening.wire().action_mask, [true; 4]);
        assert!(matches!(&opening.evidence.doors[0].current.pair().car_free,
            Evidence::Feasible(journey) if journey.seconds() == opening_seconds));
        let project = env.step(2, opening.state).unwrap();
        assert!(matches!(&project.evidence.doors[0].current.pair().car_free,
            Evidence::Feasible(journey) if journey.seconds() == after_seconds));
        assert_eq!(
            project.reward.after.all.car_free - project.reward.before.all.car_free,
            newly_car_free
        );
        assert_eq!(project.budget.incremental_capital.upper, 45.0);
        assert_eq!(project.budget.annual.upper, 12.0);
        assert_eq!(project.budget.limit, opening.budget.limit);
    }
}
