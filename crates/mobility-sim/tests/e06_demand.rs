#[path = "../examples/support/e06.rs"]
mod fixture;

use fixture::*;
use mobility_graph::network::{Access, Network, TurnRule};
use mobility_graph::{EdgeId, NodeId};
use mobility_routing::RoutingError;
use mobility_sim::demand::*;

#[test]
fn e06_s01_s03_s04_given_time_specific_demand_then_choices_include_real_wait_and_distinct_costs() {
    let network = corridor();
    let mut demand = sample();
    demand.tours[0].legs.truncate(1);
    demand.tours[0].legs[0].dwell_seconds = 0;
    demand.tours[0].return_to_origin = false;
    let report = evaluate(&network, &demand, &scenario(), 500).unwrap();
    let result = &report.tours[0];
    assert_eq!(demand.tours[0].legs[0].purpose, Purpose::School);
    assert_eq!(report.essential.total_weight, 4.5);
    assert_eq!(report.essential.no_car_free_share(), Some(0.0));
    assert_eq!(result.choices.len(), 5);
    let transit = result.choices[&TravelMode::Transit].as_ref().unwrap();
    assert_eq!(transit.legs[0].journey.arrival_utc, 42);
    assert_eq!(transit.legs[0].waiting_seconds, 18);
    // 24 non-wait seconds + 2*18 wait + 3*4 reliability + 10*2 USD.
    assert_eq!(transit.generalized_seconds, Some(92.0));
    assert_eq!(result.lowest_cost_mode(), Some(TravelMode::Car));

    // No transit on this return leg, and walk alone must not count as transit.
    let report = evaluate(&network, &sample(), &scenario(), 500).unwrap();
    assert_eq!(
        report.tours[0].choices[&TravelMode::Transit],
        Err(Excluded::NoJourney { leg: 1 })
    );
}

#[test]
fn e06_s02_given_source_totals_then_weighted_representatives_reconcile() {
    let mut demand = sample();
    let records = demand.population.generate().unwrap();
    assert_eq!(records.map(|x| x.persons).collect::<Vec<_>>(), [4, 3, 3, 6]);
    demand.population.zones[0].source_persons = 12;
    assert!(demand.population.generate().is_err());
    demand.population.tolerance_persons = 2;
    assert!(demand.population.generate().is_ok());
}

#[test]
fn e06_s05_given_pickup_then_require_load_capacity_and_time_for_activity_and_return() {
    let network = corridor();
    let mut demand = sample();
    let report = evaluate(&network, &demand, &scenario(), 500).unwrap();
    assert_eq!(
        report.tours[0].choices[&TravelMode::Bicycle],
        Err(Excluded::Load { leg: 1 })
    );
    assert!(report.tours[0].choices[&TravelMode::Car].is_ok());
    demand.tours[0].legs[0].dwell_seconds = 40;
    let report = evaluate(&network, &demand, &scenario(), 500).unwrap();
    assert_eq!(
        report.tours[0].choices[&TravelMode::Walk],
        Err(Excluded::ActivityWindow { leg: 1 })
    );
    assert!(report.tours[0].choices[&TravelMode::Car].is_ok());
    demand.tours[0].legs[1].to = NodeId(2);
    assert!(evaluate(&network, &demand, &scenario(), 500).is_err());
}

#[test]
fn e06_s06_s07_given_mobility_and_night_scenarios_then_essential_shortfall_is_conditional() {
    let network = corridor();
    let mut demand = sample();
    let mut scene = scenario();
    demand.tours[0].group = "chair".into();
    let report = evaluate(&network, &demand, &scene, 500).unwrap();
    assert_eq!(
        report.tours[0].choices[&TravelMode::Bicycle],
        Err(Excluded::Traveler)
    );
    assert_eq!(report.essential.no_car_free_share(), Some(0.0));
    scene.environment.label = "winter rain".into();
    scene.environment.weather = "rain".into();
    scene.environment.season = "winter".into();
    scene.environment.daylight.clear();
    scene.environment.dark_modes = vec![TravelMode::Car];
    let report = evaluate(&network, &demand, &scene, 500).unwrap();
    assert_eq!(
        report.tours[0].choices[&TravelMode::Walk],
        Err(Excluded::Environment { leg: 0 })
    );
    assert_eq!(report.essential.no_car_free_share(), Some(1.0));
    assert_eq!(report.essential.no_mode_weight, 4.5);
}

#[test]
fn e06_s03_s06_given_steps_at_alighting_then_only_compatible_groups_can_use_transit() {
    let network = network_with_alighting(Access::Steps);
    let mut demand = sample();
    demand.tours[0].legs.truncate(1);
    demand.tours[0].legs[0].dwell_seconds = 0;
    demand.tours[0].return_to_origin = false;
    let report = evaluate(&network, &demand, &scenario(), 500).unwrap();
    assert!(report.tours[0].choices[&TravelMode::Transit].is_ok());
    demand.tours[0].group = "chair".into();
    let report = evaluate(&network, &demand, &scenario(), 500).unwrap();
    assert_eq!(
        report.tours[0].choices[&TravelMode::Transit],
        Err(Excluded::NoJourney { leg: 0 })
    );
    assert!(report.tours[0].choices[&TravelMode::Walk].is_ok());
}

#[test]
fn e06_s05_given_car_turn_restriction_at_stop_then_dwell_does_not_reset_approach() {
    let mut graph = corridor().data().clone();
    graph.turns.push(TurnRule {
        incoming: EdgeId(7),
        outgoing: EdgeId(8),
        only: false,
    });
    let network = Network::new(graph).unwrap();
    let mut demand = sample();
    let report = evaluate(&network, &demand, &scenario(), 500).unwrap();
    assert_eq!(
        report.tours[0].choices[&TravelMode::Car],
        Err(Excluded::NoJourney { leg: 1 })
    );
    let mut return_leg = demand.tours[0].legs.remove(1);
    return_leg.essential = false;
    demand.tours[0].legs = vec![return_leg];
    demand.tours[0].return_to_origin = false;
    let report = evaluate(&network, &demand, &scenario(), 500).unwrap();
    assert!(report.tours[0].choices[&TravelMode::Car].is_ok());
    assert_eq!(report.essential.no_car_free_share(), None);
}

#[test]
fn e06_s01_s03_s04_given_missing_and_invalid_inputs_then_never_infer_cost_or_infeasibility() {
    let network = corridor();
    let mut demand = sample();
    demand.tours[0].legs[0].purpose = Purpose::Commute;
    demand.tours[0].legs[0].costs.remove(&TravelMode::Car);
    let report = evaluate(&network, &demand, &scenario(), 500).unwrap();
    assert_eq!(
        report.tours[0].choices[&TravelMode::Car]
            .as_ref()
            .unwrap()
            .generalized_seconds,
        None
    );
    assert_eq!(report.tours[0].lowest_cost_mode(), None);
    assert!(matches!(
        evaluate(&network, &demand, &scenario(), 1),
        Err(Error::Routing {
            source: RoutingError::StateLimit,
            ..
        })
    ));
    demand.tours[0].legs[0].departure_utc = demand.tours[0].legs[0].window.end;
    assert!(matches!(
        evaluate(&network, &demand, &scenario(), 500),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn e06_s08_given_explicit_relocation_and_rates_then_long_horizon_is_separate() {
    let population = sample().population;
    let mut run = Sensitivity {
        revision: "ten-year-high".into(),
        years: 10,
        provenance: synthetic(),
        moves: vec![Move {
            from: "west".into(),
            to: "east".into(),
            persons: 2,
        }],
        rates: vec![Rate {
            zone: "east".into(),
            purpose: Purpose::Shopping,
            trips_per_person: 0.5,
            multiplier: 1.25,
        }],
    };
    let result = long_horizon(&population, &run).unwrap();
    assert_eq!(result.population_before["east"], 6);
    assert_eq!(result.population_after["east"], 8);
    assert_eq!(result.population_after["west"], 8);
    assert_eq!(result.trips[0].baseline, 3.0);
    assert_eq!(result.trips[0].relocation_only, 4.0);
    assert_eq!(result.trips[0].with_induced_demand, 5.0);
    assert_eq!(result.years, 10);
    run.moves[0].persons = 11;
    assert!(long_horizon(&population, &run).is_err());
}
