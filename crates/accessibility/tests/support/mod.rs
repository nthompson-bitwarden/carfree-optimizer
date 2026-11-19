//! E13 v1 MIT synthetic fixture. No observed transportation inputs.
#![allow(dead_code)]
use accessibility::scoring::*;
pub fn context() -> Context {
    Context {
        scenario: "synthetic-v1".into(),
        graph_revision: "graph-v1".into(),
        graph_snapshot: vec![0],
        demand_revision: "demand-v1".into(),
        destination_revision: "destinations-v1".into(),
        service_revision: "synthetic-service-v1".into(),
        land_use_revision: None,
        geography_revision: "areas-v1".into(),
        period: "synthetic day 0..1000 UTC".into(),
        horizon: Horizon::ShortRun,
        behavior: "fixed supplied demand; no relocation".into(),
        source: "MIT synthetic E13 v1".into(),
        geography: "abstract two-neighborhood fixture".into(),
        date: "2026-11-19".into(),
        limitations: vec![
            "arbitrary modeled assumptions; uncalibrated; no policy recommendations".into(),
        ],
    }
}
pub fn journey(seconds: u32) -> Evidence {
    Evidence::Feasible(Journey {
        access: 2,
        wait: 3,
        ride: seconds - 8,
        transfer: 1,
        intermediate_walk: 1,
        egress: 1,
        dwell: 0,
    })
}
pub fn access_input() -> AccessInput {
    AccessInput {
        context: context(),
        stratum: Stratum {
            origin: "origin-a".into(),
            neighborhood: "north".into(),
            group: "noncyclist".into(),
            destination_type: "healthcare".into(),
            modes: vec![Mode::Walk, Mode::Transit],
            profile: "step-free noncyclist".into(),
            departures_utc: vec![0, 100],
            budget_seconds: 60,
        },
        destinations: vec!["clinic-a".into(), "clinic-b".into()],
        pairs: vec![
            Pair {
                destination: "clinic-a".into(),
                departure_utc: 0,
                car_free: journey(60),
                car: journey(40),
            },
            Pair {
                destination: "clinic-a".into(),
                departure_utc: 100,
                car_free: Evidence::Infeasible,
                car: journey(40),
            },
            Pair {
                destination: "clinic-b".into(),
                departure_utc: 0,
                car_free: journey(50),
                car: Evidence::Infeasible,
            },
            Pair {
                destination: "clinic-b".into(),
                departure_utc: 100,
                car_free: Evidence::Infeasible,
                car: Evidence::Infeasible,
            },
        ],
    }
}
pub fn trip(id: &str, cf: Option<bool>, car: Option<bool>, mode: Option<Mode>) -> Trip {
    Trip {
        id: id.into(),
        group: "noncyclist".into(),
        neighborhood: "north".into(),
        weight: 2.0,
        essential: true,
        car_free: cf,
        car,
        served_mode: mode,
    }
}
pub fn projection(context: Context, share: Option<f64>) -> accessibility::projection::Projection {
    let mut i = access_input();
    i.context = context;
    i.stratum.departures_utc = vec![0];
    i.destinations = (0..100).map(|n| format!("destination-{n}")).collect();
    i.pairs = i
        .destinations
        .iter()
        .enumerate()
        .map(|(n, d)| Pair {
            destination: d.clone(),
            departure_utc: 0,
            car_free: match share {
                Some(s) if (n as f64) < s * 100.0 => journey(60),
                Some(_) => Evidence::Infeasible,
                None => Evidence::Missing("unavailable fixture cell".into()),
            },
            car: journey(40),
        })
        .collect();
    accessibility::projection::Projection::from_access(&access(&i).unwrap())
}
pub fn demand_rows(d: &mobility_sim::demand::Demand) -> Vec<Trip> {
    d.tours
        .iter()
        .flat_map(|t| {
            t.legs.iter().enumerate().map(move |(i, l)| Trip {
                id: format!("{}:{i}", t.id),
                group: t.group.clone(),
                neighborhood: t.home_zone.clone(),
                weight: l.expansion_weight,
                essential: l.essential,
                car_free: Some(false),
                car: Some(true),
                served_mode: Some(Mode::Car),
            })
        })
        .collect()
}
pub fn unavailable() -> Uncertainty {
    Uncertainty::Unavailable {
        reason: "deterministic synthetic fixture; no statistical sampling model".into(),
    }
}
