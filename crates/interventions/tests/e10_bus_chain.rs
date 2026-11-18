//! E10-S03: whole-trip timetable consistency is atomic across a bus corridor.
#[allow(dead_code)] // This focused chain test intentionally uses only part of the shared fixture.
#[path = "../examples/support/e10.rs"]
mod fixture;
use fixture::*;
use interventions::scenario::*;
use mobility_graph::{network::*, EdgeId, NodeId};

fn retime(mut l: Link, depart: u32, arrive: u32) -> Link {
    l.seconds = arrive - depart;
    if let LinkKind::Transit { timetable, .. } = &mut l.kind {
        *timetable = Timetable::Scheduled(vec![Departure {
            depart,
            arrive,
            run: 0,
        }]);
    }
    l
}
#[test]
fn e10_s03_given_bus_chain_when_retimed_then_run_and_dwell_consistency_is_preserved_atomically() {
    let mut d = network().data().clone();
    let first = d.links.iter_mut().find(|l| l.id == EdgeId(11)).unwrap();
    first.to = NodeId(3);
    *first = retime(first.clone(), 20, 30);
    let mut second = first.clone();
    second.id = EdgeId(12);
    second.from = NodeId(3);
    second.to = NodeId(2);
    if let LinkKind::Transit { stop_index, .. } = &mut second.kind {
        *stop_index = 1;
    }
    d.links.push(retime(second, 32, 42));
    let mut h = history_for(Network::new(d).unwrap(), requirements());
    let a = retime(
        h.current().network.link(EdgeId(11)).unwrap().clone(),
        20,
        35,
    );
    let b = retime(
        h.current().network.link(EdgeId(12)).unwrap().clone(),
        37,
        47,
    );
    let first = Change::Allocate {
        road: EdgeId(100),
        lanes: 1,
        target: a,
        assessment: None,
    };
    let original = h.clone();
    assert!(h
        .apply(&batch(&h, "partial", vec![first.clone()]))
        .unwrap_err()
        .0
        .contains("dwell"));
    assert_eq!(h, original);
    h.apply(&batch(
        &h,
        "whole-chain",
        vec![
            first,
            Change::Allocate {
                road: EdgeId(101),
                lanes: 1,
                target: b,
                assessment: None,
            },
        ],
    ))
    .unwrap();
    let transit = h
        .current()
        .continuity
        .iter()
        .find(|r| r.kind == AccessKind::Transit)
        .unwrap();
    assert_eq!((transit.before_seconds, transit.after_seconds), (44, 49));
    assert_eq!(h.current().roads[&EdgeId(100)].bus_lanes, 1);
    assert_eq!(h.current().roads[&EdgeId(101)].bus_lanes, 1);
}
