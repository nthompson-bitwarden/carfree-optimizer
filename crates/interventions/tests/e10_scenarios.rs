//! E10 Given/When/Then: deterministic abstract corridor, MIT, modeled 2026-01-05.
#[path = "../examples/support/e10.rs"]
mod fixture;
use fixture::*;
use interventions::{evaluation::*, scenario::*};
use mobility_graph::{network::*, EdgeId, Mode, NodeId};
use mobility_routing::{earliest_arrival, earliest_transit_arrival, Outcome};
use mobility_sim::simulation::{self as sim, Iterative};

fn arrival(n: &Network, r: &mobility_routing::Request) -> i64 {
    match earliest_arrival(n, r).unwrap() {
        Outcome::Journey(j) => j.arrival_utc,
        _ => panic!("unmet fixture journey"),
    }
}
fn evaluated(s: &Snapshot) -> Evaluation {
    let (d, input) = inputs(s);
    evaluate(
        s,
        &Iterative,
        &d,
        &input,
        &DemandBinding {
            scenario_id: s.id.clone(),
            demand_revision: d.revision.clone(),
            land_use_revision: s.land_use_revision.clone(),
        },
    )
    .unwrap()
}
fn pedestrian(id: u32, from: u32, to: u32) -> Change {
    let mut l = street(id, from, to, Mode::Walk, 10);
    if let LinkKind::Street { crossing, .. } = &mut l.kind {
        *crossing = true;
    }
    Change::Pedestrian {
        link: l,
        assessment: CrossingAssessment {
            crossing: true,
            permitted: true,
            minimum_seconds: 8,
            provenance: synthetic(),
        },
    }
}

#[test]
fn e10_s01_given_road_when_bike_lane_allocated_then_capacity_connectivity_and_safety_agree() {
    let mut h = history();
    let baseline = h.current().network.encode();
    let mut r = request(1, 2, Mode::Bicycle);
    r.profile.protected_cycling_only = true;
    r.profile.maximum_cycling_stress = Some(1);
    assert!(matches!(
        earliest_arrival(&h.current().network, &r).unwrap(),
        Outcome::Unmet(_)
    ));
    let change = bike(&h, 7, 3);
    let b = batch(&h, "bike-v1", vec![change]);
    let preview = h.preview(&b).unwrap();
    assert_eq!(h.current().network.encode(), baseline);
    assert_eq!(preview, *h.apply(&b).unwrap());
    assert_eq!(arrival(&h.current().network, &r), 15);
    assert_eq!(h.current().roads[&EdgeId(7)].general_lanes, 2);
    assert_eq!(h.current().roads[&EdgeId(7)].bike_lanes, 1);
    assert_eq!(
        h.current()
            .road_capacities()
            .unwrap()
            .iter()
            .find(|(e, _)| *e == EdgeId(7))
            .unwrap()
            .1,
        180
    );
    assert!(matches!(&h.current().network.link(EdgeId(3)).unwrap().kind,
        LinkKind::Street { cycling: CycleFacility::Protected, safety: Some(s), .. } if s.stress_level==1 && s.provenance.basis==Basis::Modeled));
    assert_eq!(
        h.current().network.link(EdgeId(1)),
        network().link(EdgeId(1))
    ); // noncyclists retained
}

#[test]
fn e10_s01_s03_given_shared_lane_inventory_when_overallocated_then_entire_batch_rolls_back() {
    let mut h = history();
    let original = h.clone();
    let bike_change = bike(&h, 7, 3);
    let bus = bus(&h, 10);
    let mut third = bike_change.clone();
    if let Change::Allocate { lanes, target, .. } = &mut third {
        *lanes = 2;
        target.id = EdgeId(300);
    }
    let b = batch(
        &h,
        "overallocated",
        vec![
            bike_change,
            Change::Allocate {
                road: EdgeId(7),
                lanes: 1,
                target: bus,
                assessment: None,
            },
            third,
        ],
    );
    assert!(h.apply(&b).unwrap_err().0.contains("over-allocation"));
    assert_eq!(h, original);
    let mut roads = h.current().roads.values().cloned().collect::<Vec<_>>();
    roads[1].segment = roads[0].segment.clone();
    assert!(History::new("bad".into(), network(), roads, requirements())
        .unwrap_err()
        .0
        .contains("owner"));
    let mut bad = bike(&h, 7, 3);
    if let Change::Allocate { target, .. } = &mut bad {
        target.access = Access::Unknown;
    }
    assert!(h.apply(&batch(&h, "unknown-access", vec![bad])).is_err());
    assert_eq!(h, original);
}

#[test]
fn e10_s02_given_walking_graph_when_connected_crossing_added_then_step_free_noncyclist_can_use_it()
{
    let mut h = history();
    let r = request(1, 2, Mode::Walk);
    assert_eq!(arrival(&h.current().network, &r), 35);
    h.apply(&batch(&h, "pedestrian-v1", vec![pedestrian(300, 1, 2)]))
        .unwrap();
    assert_eq!(arrival(&h.current().network, &r), 10);
    let original = h.clone();
    assert!(h
        .preview(&batch(
            &h,
            "below-crossing-minimum",
            vec![Change::Crossing {
                edge: EdgeId(300),
                seconds: 7,
                safety: None,
                assessment: approval(true, 1),
            }]
        ))
        .unwrap_err()
        .0
        .contains("recorded minimum"));
    for (id, crossing, permitted, min) in [
        (301, false, true, 8),
        (302, true, false, 8),
        (303, true, true, 11),
    ] {
        let mut change = pedestrian(id, 1, 2);
        if let Change::Pedestrian { assessment, .. } = &mut change {
            assessment.crossing = crossing;
            assessment.permitted = permitted;
            assessment.minimum_seconds = min;
        }
        assert!(h
            .apply(&batch(&h, &format!("bad-{id}"), vec![change]))
            .is_err());
        assert_eq!(h, original);
    }
    let mut d = network().data().clone();
    d.nodes.push(Node {
        id: NodeId(4),
        access: Access::Accessible,
    });
    let mut isolated = history_for(Network::new(d).unwrap(), requirements());
    assert!(isolated
        .apply(&batch(&isolated, "isolated", vec![pedestrian(300, 1, 4)]))
        .unwrap_err()
        .0
        .contains("walking graph"));
}

#[test]
fn e10_s03_given_bus_conversion_when_evaluated_then_road_time_and_bus_time_change_once() {
    let mut h = history();
    let base = evaluated(h.current());
    let bus_link = bus(&h, 10);
    let b = batch(
        &h,
        "bus-v1",
        vec![Change::Allocate {
            road: EdgeId(7),
            lanes: 1,
            target: bus_link,
            assessment: None,
        }],
    );
    h.apply(&b).unwrap();
    let changed = evaluated(h.current());
    let road = |e: &Evaluation| {
        e.report
            .roads
            .iter()
            .find(|r| r.edge == EdgeId(7) && r.period == 0)
            .unwrap()
            .used_seconds
    };
    assert_eq!(road(&base), 20);
    assert_eq!(road(&changed), 25);
    assert_eq!(
        changed
            .report
            .roads
            .iter()
            .find(|r| r.edge == EdgeId(7) && r.period == 0)
            .unwrap()
            .capacity_vehicles,
        180.0 * 61.0 / 3600.0
    );
    let mut req = request(1, 2, Mode::Walk);
    req.profile.allowed_modes = vec![Mode::Walk, Mode::Bus];
    let transit_arrival = |n: &Network| match earliest_transit_arrival(n, &req).unwrap() {
        Outcome::Journey(j) => j.arrival_utc,
        _ => panic!("missing transit"),
    };
    assert_eq!(transit_arrival(&network()), 42);
    assert_eq!(transit_arrival(&h.current().network), 32);
    let timing = bus(&h, 8);
    h.apply(&batch(
        &h,
        "bus-v2",
        vec![Change::BusTiming { link: timing }],
    ))
    .unwrap();
    assert_eq!(h.current().roads[&EdgeId(7)].bus_lanes, 1);
    assert_eq!(transit_arrival(&h.current().network), 30);
    let (d, mut s) = inputs(h.current());
    assert_eq!(s.transit[0].passengers, 10.0);
    let binding = DemandBinding {
        scenario_id: s.id.clone(),
        demand_revision: d.revision.clone(),
        land_use_revision: None,
    };
    s.roads[0].vehicles_per_hour[0] += 270.0;
    assert!(evaluate(h.current(), &Iterative, &d, &s, &binding)
        .unwrap_err()
        .0
        .contains("single E10 projection"));
    s.roads = road_supply(h.current(), 2).unwrap();
    s.edits.push(sim::Edit::RoadCapacity {
        edge: EdgeId(7),
        period: 0,
        vehicles_per_hour: 90.0,
    });
    assert!(evaluate(h.current(), &Iterative, &d, &s, &binding)
        .unwrap_err()
        .0
        .contains("overlapping"));
    let mut changed_run = bus(&h, 7);
    if let LinkKind::Transit {
        timetable: Timetable::Scheduled(events),
        ..
    } = &mut changed_run.kind
    {
        events[0].run = 1;
    }
    assert!(h
        .preview(&batch(
            &h,
            "bad-run",
            vec![Change::BusTiming { link: changed_run }]
        ))
        .unwrap_err()
        .0
        .contains("identity"));
}

#[test]
fn e10_s04_given_corridor_selection_when_previewed_then_gaps_directions_and_turns_are_visible() {
    let n = network();
    let good = preview_corridor(&n, &[EdgeId(100), EdgeId(101)], Mode::Car);
    assert!(good.issues.is_empty());
    let selection = select_corridor(&n, &request(1, 2, Mode::Car)).unwrap();
    assert_eq!(selection, vec![EdgeId(7)]);
    for edges in [
        vec![],
        vec![EdgeId(101), EdgeId(100)],
        vec![EdgeId(100), EdgeId(8)],
        vec![EdgeId(999)],
        vec![EdgeId(100), EdgeId(100)],
    ] {
        assert!(!preview_corridor(&n, &edges, Mode::Car).issues.is_empty());
    }
    let mut data = n.data().clone();
    data.turns.push(TurnRule {
        incoming: EdgeId(100),
        outgoing: EdgeId(101),
        only: false,
    });
    let n = Network::new(data).unwrap();
    assert!(preview_corridor(&n, &[EdgeId(100), EdgeId(101)], Mode::Car)
        .issues
        .iter()
        .any(|s| s.contains("forbidden turn")));
    assert!(
        !preview_corridor(&n, &[EdgeId(100), EdgeId(101)], Mode::Walk)
            .issues
            .is_empty()
    );
    let mut h = history();
    let changes = vec![bike(&h, 100, 300), bike(&h, 101, 301)];
    h.apply(&batch(&h, "whole-corridor", changes)).unwrap();
    assert!(preview_corridor(
        &h.current().network,
        &[EdgeId(300), EdgeId(301)],
        Mode::Bicycle
    )
    .issues
    .is_empty());
}

#[test]
fn e10_s05_given_version_history_when_undo_redo_branch_and_failures_then_exact_state_restored() {
    let mut h = history();
    let root = h.current().clone();
    h.apply(&batch(&h, "one", vec![bike(&h, 7, 3)])).unwrap();
    let first = h.current().clone();
    h.apply(&batch(&h, "two", vec![pedestrian(300, 1, 2)]))
        .unwrap();
    let second = h.current().clone();
    assert!(h.undo());
    assert_eq!(h.current(), &first);
    assert_eq!(h.current().network.encode(), first.network.encode());
    assert!(h.undo());
    assert_eq!(h.current(), &root);
    assert!(!h.undo());
    assert!(h.redo());
    assert!(h.redo());
    assert_eq!(h.current(), &second);
    assert!(!h.redo());
    let mut stale = batch(&h, "three", vec![pedestrian(301, 1, 2)]);
    stale.expected_parent = "baseline".into();
    let before = h.clone();
    assert!(h.apply(&stale).unwrap_err().0.contains("stale"));
    assert_eq!(h, before);
    assert!(h.undo());
    assert!(h
        .preview(&batch(&h, "two", vec![pedestrian(301, 1, 2)]))
        .unwrap_err()
        .0
        .contains("reused"));
    h.apply(&batch(&h, "new-branch", vec![pedestrian(301, 1, 2)]))
        .unwrap();
    assert!(!h.redo());
    assert_eq!(h.current().parent.as_deref(), Some("one"));
    assert!(h
        .preview(&batch(&h, "e10-base-network", vec![pedestrian(302, 1, 2)]))
        .is_err());
}

#[test]
fn e10_s06_given_closed_road_when_permissions_and_gateways_hold_then_diversion_is_evaluated() {
    let mut h = history();
    let base = evaluated(h.current());
    h.apply(&batch(
        &h,
        "closure",
        vec![Change::CloseRoad { edge: EdgeId(7) }],
    ))
    .unwrap();
    let s = h.current();
    assert!(matches!(
        s.network.link(EdgeId(7)).unwrap().kind,
        LinkKind::Street {
            blocked: true,
            capacity: None,
            ..
        }
    ));
    let r = s
        .continuity
        .iter()
        .find(|r| r.kind == AccessKind::Regional && r.requirement.ends_with("1-2"))
        .unwrap();
    assert_eq!((r.before_seconds, r.after_seconds), (10, 40));
    let changed = evaluated(s);
    assert!(base
        .report
        .roads
        .iter()
        .any(|r| r.edge == EdgeId(7) && r.vehicles > 0.0));
    assert!(!changed.report.roads.iter().any(|r| r.edge == EdgeId(7)));
    assert!(changed
        .report
        .roads
        .iter()
        .any(|r| r.edge == EdgeId(100) && r.vehicles > 0.0));
    assert_eq!(s.roads[&EdgeId(7)].retired_lanes, 3);
}

#[test]
fn e10_s06_given_missing_permissions_or_forbidden_detour_or_return_loss_then_closure_is_rejected() {
    let mut reqs = requirements();
    reqs[0].permitted_car_edges = None;
    assert!(History::new("x".into(), network(), vec![], reqs)
        .unwrap_err()
        .0
        .contains("permissions"));
    for missing in [
        AccessKind::Emergency,
        AccessKind::Freight,
        AccessKind::Regional,
        AccessKind::Walking,
        AccessKind::Transit,
    ] {
        let mut h = history_for(
            network(),
            requirements()
                .into_iter()
                .filter(|r| r.kind != missing)
                .collect(),
        );
        assert!(h
            .apply(&batch(
                &h,
                "closure",
                vec![Change::CloseRoad { edge: EdgeId(7) }]
            ))
            .unwrap_err()
            .0
            .contains("coverage"));
    }
    let mut d = network().data().clone();
    d.turns.push(TurnRule {
        incoming: EdgeId(100),
        outgoing: EdgeId(101),
        only: false,
    });
    let mut h = history_for(Network::new(d).unwrap(), requirements());
    let original = h.clone();
    assert!(h
        .apply(&batch(
            &h,
            "closure",
            vec![Change::CloseRoad { edge: EdgeId(7) }]
        ))
        .unwrap_err()
        .0
        .contains("unmet"));
    assert_eq!(h, original);
    let mut reqs = requirements();
    for r in &mut reqs {
        if r.kind == AccessKind::Emergency {
            r.permitted_car_edges = Some([7, 8, 100, 101].map(EdgeId).into());
        }
    }
    let mut h = history_for(network(), reqs);
    assert!(h
        .apply(&batch(
            &h,
            "return-loss",
            vec![Change::CloseRoad { edge: EdgeId(8) }]
        ))
        .unwrap_err()
        .0
        .contains("Emergency-2-1"));
    let mut reqs = requirements();
    reqs.iter_mut()
        .find(|r| r.kind == AccessKind::Regional && r.request.from == NodeId(1))
        .unwrap()
        .request
        .deadline_utc = 30;
    let mut h = history_for(network(), reqs);
    assert!(h
        .apply(&batch(
            &h,
            "excess-diversion",
            vec![Change::CloseRoad { edge: EdgeId(7) }]
        ))
        .unwrap_err()
        .0
        .contains("Regional"));
}

#[test]
fn e10_s07_given_protected_crossing_when_routed_then_comfort_feasibility_and_physical_time_change()
{
    let mut h = history();
    let mut r = request(1, 2, Mode::Bicycle);
    assert_eq!(arrival(&h.current().network, &r), 20);
    r.profile.maximum_cycling_stress = Some(1);
    r.profile.protected_cycling_only = true;
    assert!(matches!(
        earliest_arrival(&h.current().network, &r).unwrap(),
        Outcome::Unmet(_)
    ));
    h.apply(&batch(
        &h,
        "crossing",
        vec![Change::Crossing {
            edge: EdgeId(3),
            seconds: 8,
            safety: Some(Safety {
                stress_level: 1,
                provenance: synthetic(),
            }),
            assessment: approval(true, 8),
        }],
    ))
    .unwrap();
    assert_eq!(arrival(&h.current().network, &r), 8);
    assert!(h
        .preview(&batch(
            &h,
            "not-crossing",
            vec![Change::Crossing {
                edge: EdgeId(1),
                seconds: 8,
                safety: None,
                assessment: approval(true, 8),
            }]
        ))
        .is_err());
    let mut d = network().data().clone();
    if let LinkKind::Street { blocked, .. } =
        &mut d.links.iter_mut().find(|l| l.id == EdgeId(3)).unwrap().kind
    {
        *blocked = true;
    }
    let h = history_for(Network::new(d).unwrap(), requirements());
    assert!(h
        .preview(&batch(
            &h,
            "blocked-crossing",
            vec![Change::Crossing {
                edge: EdgeId(3),
                seconds: 8,
                safety: Some(Safety {
                    stress_level: 1,
                    provenance: synthetic()
                }),
                assessment: approval(true, 8),
            }]
        ))
        .unwrap_err()
        .0
        .contains("blocked"));
}

#[test]
fn e10_s08_given_land_use_extension_then_transport_bytes_unchanged_and_revised_demand_binding_required(
) {
    let mut h = history();
    let original = h.current().clone();
    let change = Change::LandUse {
        destinations: vec![Destination {
            id: "clinic".into(),
            node: NodeId(2),
            category: "healthcare".into(),
            opportunities: 1,
            provenance: synthetic(),
        }],
    };
    h.apply(&batch(&h, "land-use-v1", vec![change])).unwrap();
    let s = h.current();
    assert_eq!(s.kind(), ScenarioKind::TransportPlusLandUse);
    assert_eq!(s.network.encode(), original.network.encode());
    assert_eq!(
        s.road_capacities().unwrap(),
        original.road_capacities().unwrap()
    );
    let (mut d, input) = inputs(s);
    d.revision = "explicitly-revised-destination-demand-v2".into();
    // Explicit proposed destination changes a real trip purpose, not only an ID.
    d.tours[0].legs[0].purpose = mobility_sim::demand::Purpose::Healthcare;
    let mut binding = DemandBinding {
        scenario_id: s.id.clone(),
        demand_revision: d.revision.clone(),
        land_use_revision: None,
    };
    assert!(evaluate(s, &Iterative, &d, &input, &binding)
        .unwrap_err()
        .0
        .contains("land-use"));
    binding.land_use_revision = s.land_use_revision.clone();
    let result = evaluate(s, &Iterative, &d, &input, &binding).unwrap();
    assert_eq!(result.kind, ScenarioKind::TransportPlusLandUse);
    assert_eq!(result.land_use_revision.as_deref(), Some("land-use-v1"));
    assert!(h.undo());
    assert_eq!(h.current(), &original);
    assert!(h.redo());
    h.apply(&batch(&h, "transport-after-land-use", vec![bike(&h, 7, 3)]))
        .unwrap();
    assert_eq!(
        h.current().land_use_revision.as_deref(),
        Some("land-use-v1")
    );
    let (_, input) = inputs(h.current());
    binding.scenario_id = h.current().id.clone();
    assert!(evaluate(h.current(), &Iterative, &d, &input, &binding)
        .unwrap_err()
        .0
        .contains("stale"));
}
