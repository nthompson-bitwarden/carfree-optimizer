//! E12 independent review regressions: currency scale, phase time binding and
//! through-run dwell must not bypass budget/construction constraints.
#[path = "../examples/support/e12.rs"]
mod fixture;
use fixture::*;
use interventions::{feasibility::*, scenario::*};
use mobility_graph::{network::*, EdgeId, NodeId};
use std::collections::BTreeMap;

#[test]
fn e12_s01_s05_given_zero_decimal_currency_when_capped_then_minor_units_are_not_divided_by_100() {
    let h = base::history();
    let b = proposal(&h, "currency");
    let mut i = inputs(&h, &b);
    i.budget.currency = "JPY".into();
    i.budget.minor_units_per_major = 1;
    for c in &mut i.costs {
        c.currency = "JPY".into();
    }
    i.transit[0].operations.currency = "JPY".into();
    assert_eq!(screen(&h, &b, &i).unwrap().report().transit_annual, 200.0);
    i.budget.annual = 100.0;
    assert!(screen(&h, &b, &i).unwrap_err().contains("annual"));
    i.budget.minor_units_per_major = 0;
    assert!(screen(&h, &b, &i).is_err());
}
fn retime(mut l: Link, start: u32, end: u32) -> Link {
    l.seconds = end - start;
    if let LinkKind::Transit { timetable, .. } = &mut l.kind {
        *timetable = Timetable::Scheduled(vec![Departure {
            depart: start,
            arrive: end,
            run: 0,
        }]);
    }
    l
}
#[test]
fn e12_s07_given_through_run_dwelling_across_phase_when_declared_suspended_then_blocked() {
    let mut data = base::network().data().clone();
    let first = data.links.iter_mut().find(|l| l.id == EdgeId(11)).unwrap();
    first.to = NodeId(3);
    *first = retime(first.clone(), 20, 30);
    let mut second = first.clone();
    second.id = EdgeId(12);
    second.from = NodeId(3);
    second.to = NodeId(2);
    if let LinkKind::Transit { stop_index, .. } = &mut second.kind {
        *stop_index = 1;
    }
    data.links.push(retime(second, 100, 110));
    let h = base::history_for(Network::new(data).unwrap(), vec![]);
    let mut perm = h.clone();
    let milestone = base::batch(
        &h,
        "milestone",
        vec![Change::Allocate {
            road: EdgeId(101),
            lanes: 1,
            target: h.current().network.link(EdgeId(12)).unwrap().clone(),
            assessment: None,
        }],
    );
    perm.apply(&milestone).unwrap();
    let temp = base::batch(
        &perm,
        "temp-dwell",
        vec![Change::BusTiming {
            link: retime(
                perm.current().network.link(EdgeId(12)).unwrap().clone(),
                120,
                130,
            ),
        }],
    );
    let mut phase_i = inputs(&perm, &temp);
    for a in &mut phase_i.access {
        for r in [&mut a.outbound, &mut a.returning] {
            r.departure_utc = 40;
            r.deadline_utc = 100;
        }
    }
    let mut final_i = for_snapshot(h.current(), perm.current());
    for c in &mut final_i.costs {
        if c.category != Category::Maintenance {
            c.quantity = 0.0;
        }
    }
    let p = vec![PhaseInput {
        start: 40,
        end: 100,
        permanent: milestone,
        temporary: Some(temp),
        services: BTreeMap::from([(
            "outbound".into(),
            TemporaryService {
                status: ServiceStatus::Suspended,
                reason: "claims no service during dwell".into(),
                provenance: base::synthetic(),
            },
        )]),
        inputs: phase_i,
    }];
    assert!(screen_plan(&h, &p, &final_i)
        .unwrap_err()
        .contains("suspended"));
}
