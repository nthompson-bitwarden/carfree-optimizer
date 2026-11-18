//! E10 regressions from independent design/conformance review.
#[allow(dead_code)] // Focused regressions intentionally use a subset of shared helpers.
#[path = "../examples/support/e10.rs"]
mod fixture;
use fixture::*;
use interventions::{corridor, evaluation::*, scenario::*};
use mobility_graph::{network::*, EdgeId, Mode, NodeId};
use mobility_routing::planner::{self, Comfort, Options, PlannedOutcome};
use mobility_sim::{
    demand::TravelMode,
    simulation::{Adapter, Iterative},
};

#[test]
fn e10_s03_two_services_share_one_lane_without_reducing_access_or_passenger_capacity() {
    let mut data = network().data().clone();
    let mut second = data
        .links
        .iter()
        .find(|l| l.id == EdgeId(11))
        .unwrap()
        .clone();
    second.id = EdgeId(12);
    if let LinkKind::Transit {
        trip, timetable, ..
    } = &mut second.kind
    {
        *trip = "second".into();
        *timetable = Timetable::Scheduled(vec![Departure {
            depart: 21,
            arrive: 45,
            run: 0,
        }]);
    }
    data.links.push(second);
    let mut h = history_for(Network::new(data).unwrap(), requirements());
    let evaluated = |s: &Snapshot| {
        let (mut demand, mut input) = inputs(s);
        demand.tours[0].legs.truncate(1);
        demand.tours[0].legs[0].dwell_seconds = 0;
        demand.tours[0].return_to_origin = false;
        demand.groups[0].modes = vec![(
            TravelMode::Transit,
            demand.groups[0].modes[&TravelMode::Car],
        )]
        .into_iter()
        .collect();
        let mut second = input.transit[0].clone();
        second.edge = EdgeId(12);
        second.run.trip = "second".into();
        input.transit.push(second);
        evaluate(
            s,
            &Iterative,
            &demand,
            &input,
            &DemandBinding {
                scenario_id: s.id.clone(),
                demand_revision: demand.revision.clone(),
                land_use_revision: None,
            },
        )
        .unwrap()
    };
    let baseline = evaluated(h.current());
    let first = bus(&h, 10);
    let mut second = h.current().network.link(EdgeId(12)).unwrap().clone();
    second.seconds = 10;
    if let LinkKind::Transit {
        timetable: Timetable::Scheduled(events),
        ..
    } = &mut second.kind
    {
        events[0].arrive = 31;
    }
    h.apply(&batch(
        &h,
        "shared-bus-space",
        vec![
            Change::Allocate {
                road: EdgeId(7),
                lanes: 1,
                target: first,
                assessment: None,
            },
            Change::AssociateBus {
                road: EdgeId(7),
                link: second,
            },
        ],
    ))
    .unwrap();
    assert_eq!(h.current().roads[&EdgeId(7)].general_lanes, 2);
    assert_eq!(h.current().roads[&EdgeId(7)].bus_lanes, 1);
    assert_eq!(h.current().allocations[&EdgeId(11)], EdgeId(7));
    assert_eq!(h.current().allocations[&EdgeId(12)], EdgeId(7));
    let changed = evaluated(h.current());
    assert_eq!(baseline.report.person_seconds, 42.0 * 9.0);
    assert_eq!(changed.report.person_seconds, 32.0 * 9.0);
    assert_eq!(
        changed
            .report
            .transit
            .iter()
            .map(|r| r.capacity_passengers)
            .collect::<Vec<_>>(),
        vec![10.0, 10.0]
    );
    assert_eq!(
        changed
            .report
            .transit
            .iter()
            .map(|r| r.served_passengers)
            .sum::<f64>(),
        9.0
    );
    assert_eq!(changed.report.unserved_essential_weight, 0.0);
    for downgrade in ["access", "carriage"] {
        let mut link = bus(&h, 8);
        if downgrade == "access" {
            link.access = Access::Steps;
        } else if let LinkKind::Transit {
            bicycle_allowed, ..
        } = &mut link.kind
        {
            *bicycle_allowed = true;
        }
        assert!(h
            .preview(&batch(
                &h,
                &format!("downgrade-{downgrade}"),
                vec![Change::BusTiming { link }]
            ))
            .unwrap_err()
            .0
            .contains("non-timing"));
    }
    let mut unknown = bus(&h, 8);
    unknown.id = EdgeId(99);
    assert!(h
        .preview(&batch(
            &h,
            "new-service",
            vec![Change::Allocate {
                road: EdgeId(7),
                lanes: 1,
                target: unknown,
                assessment: None
            }]
        ))
        .unwrap_err()
        .0
        .contains("existing service"));
    // E08 itself also rejects a demand revision bound to the old graph.
    let (mut demand, input) = inputs(h.current());
    demand.network_revision = network().data().revision.clone();
    assert!(Iterative
        .evaluate(&h.current().network, &demand, &input)
        .is_err());
}

#[test]
fn e10_s02_s07_all_crossing_edits_require_explicit_permission_and_preserve_step_free_access() {
    let mut h = history();
    let mut missing = bike(&h, 7, 3);
    if let Change::Allocate { assessment, .. } = &mut missing {
        *assessment = None;
    }
    assert!(h
        .preview(&batch(&h, "missing-assessment", vec![missing]))
        .unwrap_err()
        .0
        .contains("assessment"));
    let bad = Change::Crossing {
        edge: EdgeId(3),
        seconds: 1,
        safety: Some(Safety {
            stress_level: 1,
            provenance: synthetic(),
        }),
        assessment: approval(true, 8),
    };
    assert!(h
        .preview(&batch(&h, "baseline-crossing-bypass", vec![bad]))
        .unwrap_err()
        .0
        .contains("minimum"));
    let mut link = h.current().network.link(EdgeId(1)).unwrap().clone();
    link.access = Access::Steps;
    let bad = Change::Pedestrian {
        link,
        assessment: approval(false, 1),
    };
    assert!(h
        .apply(&batch(&h, "step-free-loss", vec![bad]))
        .unwrap_err()
        .0
        .contains("downgrade"));
    let mut d = network().data().clone();
    for l in &mut d.links {
        if matches!(
            l.kind,
            LinkKind::Street {
                mode: Mode::Walk,
                ..
            }
        ) && (l.to == NodeId(1) || l.from == NodeId(2))
        {
            l.access = Access::Impassable;
        }
    }
    let h = history_for(Network::new(d).unwrap(), vec![]);
    let bad = Change::Pedestrian {
        link: street(300, 1, 2, Mode::Walk, 10),
        assessment: approval(false, 8),
    };
    assert!(h
        .preview(&batch(&h, "wrong-directed-attachment", vec![bad]))
        .unwrap_err()
        .0
        .contains("walking graph"));
}

#[test]
fn e10_s04_profile_aware_preview_rejects_stair_junction_and_transit_without_continuity() {
    let mut d = network().data().clone();
    d.nodes
        .iter_mut()
        .find(|n| n.id == NodeId(3))
        .unwrap()
        .access = Access::Steps;
    let n = Network::new(d).unwrap();
    let mut r = request(1, 2, Mode::Walk);
    r.deadline_utc = 200;
    assert!(
        preview_corridor(&n, &[EdgeId(200), EdgeId(201)], Mode::Walk)
            .issues
            .is_empty()
    );
    assert!(
        corridor::preview(&n, &[EdgeId(200), EdgeId(201)], Mode::Walk, &r)
            .issues
            .iter()
            .any(|i| i.contains("infeasible"))
    );
    let mut d = network().data().clone();
    let first = d.links.iter_mut().find(|l| l.id == EdgeId(11)).unwrap();
    first.to = NodeId(3);
    let mut next = first.clone();
    next.id = EdgeId(12);
    next.from = NodeId(3);
    next.to = NodeId(2);
    if let LinkKind::Transit {
        trip, timetable, ..
    } = &mut next.kind
    {
        *trip = "disconnected-service".into();
        *timetable = Timetable::Scheduled(vec![Departure {
            depart: 45,
            arrive: 55,
            run: 0,
        }]);
    }
    d.links.push(next);
    let n = Network::new(d).unwrap();
    r.profile.allowed_modes = vec![Mode::Walk, Mode::Bus];
    r.profile.mobility_aid = false;
    assert!(preview_corridor(&n, &[EdgeId(11), EdgeId(12)], Mode::Bus)
        .issues
        .is_empty());
    assert!(
        corridor::preview(&n, &[EdgeId(11), EdgeId(12)], Mode::Bus, &r)
            .issues
            .iter()
            .any(|i| i.contains("infeasible"))
    );
}

#[test]
fn e10_s07_crossing_changes_inspectable_comfort_penalty_not_only_travel_time() {
    let mut h = history();
    let r = request(1, 2, Mode::Bicycle);
    let options = Options {
        comfort: Comfort {
            stress_seconds_per_level: 3,
            shared_seconds: 5,
            unknown_seconds: 20,
        },
        ..Options::default()
    };
    let metrics = |n: &Network| match planner::plan(n, &r, &options).unwrap() {
        PlannedOutcome::Journey(i) => (i.journey.arrival_utc, i.comfort_penalty_seconds),
        _ => panic!("unmet fixture"),
    };
    assert_eq!(metrics(&h.current().network), (20, 14));
    h.apply(&batch(
        &h,
        "protected-crossing",
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
    assert_eq!(metrics(&h.current().network), (8, 0));
}

#[test]
fn e10_s02_s07_successive_edits_cannot_weaken_recorded_crossing_minimum_or_classification() {
    let mut h = history();
    h.apply(&batch(
        &h,
        "assessed-crossing",
        vec![Change::Crossing {
            edge: EdgeId(3),
            seconds: 10,
            safety: Some(Safety {
                stress_level: 1,
                provenance: synthetic(),
            }),
            assessment: approval(true, 8),
        }],
    ))
    .unwrap();
    let original = h.clone();
    for seconds in [10, 1] {
        let edit = Change::Crossing {
            edge: EdgeId(3),
            seconds,
            safety: Some(Safety {
                stress_level: 1,
                provenance: synthetic(),
            }),
            assessment: approval(true, 1),
        };
        assert!(h
            .apply(&batch(&h, &format!("weaken-{seconds}"), vec![edit]))
            .unwrap_err()
            .0
            .contains("recorded minimum"));
        assert_eq!(h, original);
    }
    for classification in [true, false] {
        let mut edit = bike(&h, 7, 3);
        if let Change::Allocate {
            target, assessment, ..
        } = &mut edit
        {
            *assessment = Some(approval(classification, if classification { 1 } else { 8 }));
            if let LinkKind::Street { crossing, .. } = &mut target.kind {
                *crossing = classification;
            }
        }
        assert!(h
            .apply(&batch(&h, "allocation-bypass", vec![edit]))
            .is_err());
        assert_eq!(h, original);
    }
    let link = street(300, 1, 2, Mode::Walk, 10);
    h.apply(&batch(
        &h,
        "walking-assessment",
        vec![Change::Pedestrian {
            link,
            assessment: approval(false, 8),
        }],
    ))
    .unwrap();
    let original = h.clone();
    for classification in [false, true] {
        let mut link = h.current().network.link(EdgeId(300)).unwrap().clone();
        if let LinkKind::Street { crossing, .. } = &mut link.kind {
            *crossing = classification;
        }
        let edit = Change::Pedestrian {
            link,
            assessment: approval(classification, if classification { 8 } else { 1 }),
        };
        assert!(h
            .apply(&batch(&h, "pedestrian-bypass", vec![edit]))
            .unwrap_err()
            .0
            .contains("recorded minimum"));
        assert_eq!(h, original);
    }
}

#[test]
fn e10_s06_closed_only_turn_destination_never_legalizes_a_parallel_detour() {
    let mut d = network().data().clone();
    let mut parallel = d
        .links
        .iter()
        .find(|l| l.id == EdgeId(101))
        .unwrap()
        .clone();
    parallel.id = EdgeId(104);
    d.links.push(parallel);
    d.turns.push(TurnRule {
        incoming: EdgeId(100),
        outgoing: EdgeId(101),
        only: true,
    });
    let mut reqs = requirements();
    for r in &mut reqs {
        if let Some(p) = &mut r.permitted_car_edges {
            p.insert(EdgeId(104));
        }
    }
    let mut h = history_for(Network::new(d.clone()).unwrap(), reqs.clone());
    let original = h.clone();
    let changes = vec![
        Change::CloseRoad { edge: EdgeId(7) },
        Change::CloseRoad { edge: EdgeId(101) },
    ];
    assert!(h
        .apply(&batch(&h, "only-destination-closed", changes.clone()))
        .unwrap_err()
        .0
        .contains("unmet"));
    assert_eq!(h, original);
    d.turns.clear();
    let h = history_for(Network::new(d).unwrap(), reqs);
    assert!(h
        .preview(&batch(&h, "legal-parallel-detour", changes))
        .is_ok());
}

#[test]
fn e10_s04_corridors_explicitly_reject_repeated_nodes_instead_of_misreporting_route_feasibility() {
    let n = network();
    let edges = [EdgeId(100), EdgeId(103), EdgeId(7)]; // 1 -> 3 -> 1 -> 2
    let result = corridor::preview(&n, &edges, Mode::Car, &request(1, 2, Mode::Car));
    assert!(result
        .issues
        .iter()
        .any(|s| s.contains("only simple paths supported")));
    assert!(!result.issues.iter().any(|s| s.contains("infeasible")));
}
