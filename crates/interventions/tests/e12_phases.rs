//! E12-S07 phase ordering, temporary expiry, fixed registry and complete evidence.
#[path = "../examples/support/e12.rs"]
mod fixture;
use fixture::*;
use interventions::{feasibility::*, scenario::*};
use mobility_graph::{EdgeId, Mode};
use std::collections::BTreeMap;

fn phases() -> (History, Vec<PhaseInput>, Inputs) {
    let h = base::history();
    let mut permanent = h.clone();
    let b1 = proposal(&h, "milestone-1");
    permanent.apply(&b1).unwrap();
    let temp = base::batch(
        &permanent,
        "temporary-closure",
        vec![Change::CloseRoad { edge: EdgeId(7) }],
    );
    let i1 = inputs(&permanent, &temp);
    let b2 = base::batch(
        &permanent,
        "milestone-2",
        vec![Change::Pedestrian {
            link: base::street(300, 1, 2, Mode::Walk, 10),
            assessment: base::approval(false, 8),
        }],
    );
    let mut i2 = inputs(&permanent, &b2);
    let mut final_i = i2.clone();
    for a in &mut i2.access {
        for r in [&mut a.outbound, &mut a.returning] {
            r.departure_utc += 60;
            r.deadline_utc += 60;
        }
    }
    for c in &mut final_i.costs {
        if c.category != Category::Maintenance {
            c.quantity = 0.0;
        }
    }
    let phases = vec![
        PhaseInput {
            start: 0,
            end: 60,
            permanent: b1,
            temporary: Some(temp),
            services: BTreeMap::new(),
            inputs: i1,
        },
        PhaseInput {
            start: 60,
            end: 120,
            permanent: b2,
            temporary: None,
            services: BTreeMap::new(),
            inputs: i2,
        },
    ];
    (h, phases, final_i)
}
#[test]
fn e12_s07_given_temporary_closure_when_phases_screened_then_it_expires_and_history_is_unchanged() {
    let (h, p, i) = phases();
    let before = h.clone();
    let plan = screen_plan(&h, &p, &i).unwrap();
    assert_eq!(h, before);
    assert_eq!(
        plan.phases()[0].screened().snapshot().roads[&EdgeId(7)].general_lanes,
        0
    );
    assert_eq!(
        plan.phases()[0].screened().report().access[0].outbound_seconds,
        40
    );
    assert_eq!(
        plan.phases()[1].screened().snapshot().roads[&EdgeId(7)].general_lanes,
        2
    );
    assert_eq!(plan.final_state().snapshot().id, "milestone-2");
    assert_eq!(plan.capital.upper, 144.0);
    assert_eq!(plan.lifecycle.central, 440.0); // 120 construction/disruption + 10*32 recurring
    let mut x = p.clone();
    x[1].start = 50;
    assert!(screen_plan(&h, &x, &i).is_err());
    let mut x = p.clone();
    x[1].permanent.expected_parent = "temporary-closure".into();
    assert!(screen_plan(&h, &x, &i).is_err());
    let mut x = p.clone();
    x[0].inputs.access[0].outbound.deadline_utc = 61;
    assert!(screen_plan(&h, &x, &i)
        .unwrap_err()
        .contains("outside phase"));
    let mut x = p.clone();
    x[0].inputs.access[0].outbound.deadline_utc = 30;
    assert!(screen_plan(&h, &x, &i).unwrap_err().contains("unmet")); // final still valid
}
#[test]
fn e12_s01_s03_s04_s07_given_per_phase_policy_changes_when_screened_then_fixed_project_policy_wins()
{
    let (h, p, i) = phases();
    let mut x = p.clone();
    x[0].inputs.access.remove(0);
    assert!(screen_plan(&h, &x, &i).unwrap_err().contains("registry"));
    let mut x = p.clone();
    x[0].inputs.budget.capital = 2000.0;
    assert!(screen_plan(&h, &x, &i).unwrap_err().contains("budget"));
    let mut x = p.clone();
    let mut final_i = i.clone();
    final_i.budget.capital = 100.0;
    for p in &mut x {
        p.inputs.budget.capital = 100.0;
    }
    assert!(screen_plan(&h, &x, &final_i)
        .unwrap_err()
        .contains("cumulative"));
    let mut x = i.clone();
    x.costs[0].quantity = 1.0;
    assert!(screen_plan(&h, &p, &x)
        .unwrap_err()
        .contains("already charged"));
}
#[test]
fn e12_s02_s07_s08_given_early_offroad_work_when_later_or_final_evidence_omitted_then_blocked() {
    let h = base::history();
    let mut permanent = h.clone();
    let first = base::batch(
        &h,
        "early-walk",
        vec![Change::Pedestrian {
            link: base::street(300, 1, 2, Mode::Walk, 10),
            assessment: base::approval(false, 8),
        }],
    );
    let first_i = inputs(&h, &first);
    permanent.apply(&first).unwrap();
    let second = proposal(&permanent, "later-bike");
    let mut second_i = inputs(&permanent, &second);
    // Fixture helper covers only this immediate diff; project must retain early work.
    let early = first_i
        .segments
        .iter()
        .find(|s| s.edges.contains(&EdgeId(300)))
        .unwrap()
        .clone();
    let early_row = first_i
        .rows
        .iter()
        .find(|r| r.id == early.row)
        .unwrap()
        .clone();
    let mut final_i = second_i.clone();
    final_i.segments.push(early.clone());
    final_i.rows.push(early_row.clone());
    for c in &mut final_i.costs {
        if c.category != Category::Maintenance {
            c.quantity = 0.0;
        }
    }
    for a in &mut second_i.access {
        for r in [&mut a.outbound, &mut a.returning] {
            r.departure_utc += 60;
            r.deadline_utc += 60;
        }
    }
    let mut p = vec![
        PhaseInput {
            start: 0,
            end: 60,
            permanent: first,
            temporary: None,
            services: BTreeMap::new(),
            inputs: first_i,
        },
        PhaseInput {
            start: 60,
            end: 120,
            permanent: second,
            temporary: None,
            services: BTreeMap::new(),
            inputs: second_i,
        },
    ];
    assert!(screen_plan(&h, &p, &final_i).unwrap_err().contains("300"));
    p[1].inputs.segments.push(early);
    p[1].inputs.rows.push(early_row);
    assert!(screen_plan(&h, &p, &final_i).is_ok());
    final_i.segments.retain(|s| !s.edges.contains(&EdgeId(300)));
    assert!(screen_plan(&h, &p, &final_i).unwrap_err().contains("300"));
}
#[test]
fn e12_s07_given_temporary_transit_when_assumptions_disagree_or_exhausted_then_not_a_valid_phase() {
    let h = base::history();
    let mut permanent = h.clone();
    let ride = base::bus(&h, 20);
    let b = base::batch(
        &h,
        "bus-lane",
        vec![Change::Allocate {
            road: EdgeId(7),
            lanes: 1,
            target: ride,
            assessment: None,
        }],
    );
    permanent.apply(&b).unwrap();
    let temp = base::batch(
        &permanent,
        "temporary-bus",
        vec![Change::BusTiming {
            link: base::bus(&permanent, 10),
        }],
    );
    let phase_i = inputs(&permanent, &temp);
    let mut final_i = inputs(&h, &b);
    for c in &mut final_i.costs {
        if c.category != Category::Maintenance {
            c.quantity = 0.0;
        }
    }
    let mut p = vec![PhaseInput {
        start: 0,
        end: 60,
        permanent: b,
        temporary: Some(temp),
        services: BTreeMap::new(),
        inputs: phase_i,
    }];
    assert!(screen_plan(&h, &p, &final_i)
        .unwrap_err()
        .contains("declaration"));
    p[0].services.insert(
        "outbound".into(),
        TemporaryService {
            status: ServiceStatus::Operates,
            reason: "synthetic temporary timetable".into(),
            provenance: base::synthetic(),
        },
    );
    let plan = screen_plan(&h, &p, &final_i).unwrap();
    assert_eq!(plan.phases()[0].screened().report().transit_annual, 1.0);
    assert_eq!(plan.final_state().report().transit_annual, 2.0);
    p[0].services.get_mut("outbound").unwrap().status = ServiceStatus::Suspended;
    assert!(screen_plan(&h, &p, &final_i)
        .unwrap_err()
        .contains("still operates"));
    p[0].services.get_mut("outbound").unwrap().status = ServiceStatus::Operates;
    p[0].inputs.transit[0].operations.max_run_instances = 0;
    assert!(screen_plan(&h, &p, &final_i).is_err());
}
