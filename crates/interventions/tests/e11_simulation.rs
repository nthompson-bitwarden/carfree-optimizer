//! E11-S02/S03: edited run identities, road ledger and passenger supply on one E08 snapshot.
#[allow(dead_code)]
#[path = "../examples/support/e11.rs"]
mod e11;
#[allow(dead_code)]
#[path = "../examples/support/e10.rs"]
mod fixture;
use fixture::*;
use interventions::{evaluation::*, scenario::*, transit::Service};
use mobility_graph::{network::*, EdgeId};
use mobility_sim::{
    demand::TravelMode,
    simulation::{Adapter, Iterative, TransitSupply},
};
#[test]
fn e11_s02_s03_edited_frequency_requires_rebound_passenger_supply_and_single_road_capacity() {
    let mut h = history();
    let mut link = h.current().network.link(EdgeId(11)).unwrap().clone();
    link.seconds = 10;
    if let LinkKind::Transit { timetable, .. } = &mut link.kind {
        *timetable = Timetable::Frequency {
            first: 20,
            end: 25,
            headway: 2,
            first_run: 10,
        };
    }
    let service = Service {
        trip: "outbound".into(),
        calendar: h.current().network.data().calendars[0].clone(),
        rides: vec![link.clone()],
        operations: e11::operations(),
    };
    h.apply(&batch(
        &h,
        "frequent-priority",
        vec![
            Change::Transit(service),
            Change::Allocate {
                road: EdgeId(7),
                lanes: 1,
                target: link,
                assessment: None,
            },
        ],
    ))
    .unwrap();
    assert_eq!(h.current().transit["outbound"].estimate.runs, 3);
    let (mut demand, mut input) = inputs(h.current());
    demand.tours[0].legs.truncate(1);
    demand.tours[0].legs[0].dwell_seconds = 0;
    demand.tours[0].return_to_origin = false;
    demand.groups[0].modes = vec![(
        TravelMode::Transit,
        demand.groups[0].modes[&TravelMode::Car],
    )]
    .into_iter()
    .collect();
    assert!(Iterative
        .evaluate(&h.current().network, &demand, &input)
        .is_err()); // Stale run ordinal zero is not the edited supply.
    input.transit = (10..13)
        .map(|ordinal| TransitSupply {
            edge: EdgeId(11),
            run: mobility_routing::Run {
                trip: "outbound".into(),
                service_date: 20260105,
                ordinal,
                stop_index: 0,
            },
            passengers: 10.0,
        })
        .collect();
    let result = evaluate(
        h.current(),
        &Iterative,
        &demand,
        &input,
        &DemandBinding {
            scenario_id: h.current().id.clone(),
            demand_revision: demand.revision.clone(),
            land_use_revision: None,
        },
    )
    .unwrap();
    assert_eq!(result.report.person_seconds, 32.0 * 9.0);
    assert_eq!(
        result
            .report
            .transit
            .iter()
            .map(|r| r.capacity_passengers)
            .collect::<Vec<_>>(),
        vec![10.0, 10.0, 10.0]
    );
    assert_eq!(
        result
            .report
            .transit
            .iter()
            .map(|r| r.served_passengers)
            .sum::<f64>(),
        9.0
    );
    assert_eq!(result.report.unserved_essential_weight, 0.0);
    let roads = road_supply(h.current(), 2).unwrap();
    assert_eq!(
        roads
            .iter()
            .find(|r| r.edge == EdgeId(7))
            .unwrap()
            .vehicles_per_hour,
        vec![180.0, 180.0]
    );
    assert_eq!(h.current().roads[&EdgeId(7)].bus_lanes, 1);
}
