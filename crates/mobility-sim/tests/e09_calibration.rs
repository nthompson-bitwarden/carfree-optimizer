//! E09 Given/When/Then acceptance: all numbers are synthetic test values.
#[path = "../examples/support/e09.rs"]
mod fixture;
use mobility_graph::network::Network;
use mobility_graph::EdgeId;
use mobility_sim::calibration::*;
use mobility_sim::demand::{Demand, Purpose, TravelMode};
use mobility_sim::simulation::{self, Adapter, Iterative, Scenario, Status};

#[test]
fn e09_s01_location_time_residuals_and_covered_zero_are_not_missing() {
    let (n, d, s, mut request) = fixture::inputs();
    request.observations[0].value = Some(3.0);
    request
        .observations
        .push(fixture::road("zero", Split::Fit, 8, 0, 61, 0.0));
    let report = run(&Iterative, &n, &d, &s, &request).unwrap();
    let c = &report.candidates()[0].runs[0].comparisons;
    assert_eq!(c[0].modeled, Some(4.5));
    assert_eq!(c[0].residual, Some(1.5));
    assert_eq!(c[1].modeled, Some(0.0));
    assert_eq!(c[1].residual, Some(0.0));
    assert!(c[1].flags.is_empty());
    assert_eq!(
        report.request().observations[0].target.location(),
        "cutline-7"
    );
    assert_eq!(
        (
            report.request().observations[0].start,
            report.request().observations[0].end
        ),
        (0, 61)
    );
    request.observations[0].value = Some(6.0);
    let report = run(&Iterative, &n, &d, &s, &request).unwrap();
    assert_eq!(
        report.candidates()[0].runs[0].comparisons[0].residual,
        Some(-1.5)
    );
}

#[test]
fn e09_s01_missing_source_crosswalk_and_partial_time_coverage_are_distinct() {
    let (n, d, s, mut r) = fixture::inputs();
    r.observations[0].value = None;
    r.observations[0].missing_reason = Some("station not reporting".into());
    r.observations
        .push(fixture::road("unmapped", Split::Fit, 999, 0, 61, 4.5));
    r.observations
        .push(fixture::road("partial", Split::Fit, 7, 1, 61, 4.5));
    r.observations
        .push(fixture::road("outside", Split::Fit, 7, -1, 61, 4.5));
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    let c = &report.candidates()[0].runs[0].comparisons;
    assert!(c[0].flags[0].contains("missing observation"));
    assert!(c[1].flags[0].contains("crosswalk"));
    assert!(c[2].flags[0].contains("whole model entry periods"));
    assert!(c[3].flags[0].contains("time coverage"));
    assert_eq!(c[0].modeled, Some(4.5));
    assert_eq!(c[0].residual, None);
    assert_eq!(c[1].modeled, None);
    assert_eq!(report.selected(), None);
}

fn transit_target() -> Target {
    Target::Transit {
        route_or_area: "route-A".into(),
        trips: vec!["outbound".into()],
    }
}
#[test]
fn e09_s02_real_adapter_counts_served_boardings_with_route_missing_flags() {
    let (n, mut d, s, mut r) = fixture::inputs();
    d.groups[0].modes = [(TravelMode::Transit, d.groups[0].modes[&TravelMode::Car])].into();
    d.tours[0].legs.truncate(1);
    d.tours[0].legs[0].dwell_seconds = 0;
    d.tours[0].return_to_origin = false;
    r.observations = vec![
        fixture::observation("transit-fit", Split::Fit, transit_target(), 0, 61, 9.0),
        fixture::observation(
            "transit-holdout",
            Split::Holdout,
            transit_target(),
            61,
            240,
            0.0,
        ),
    ];
    r.required_metrics = [Metric::TransitBoardings].into();
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert_eq!(report.selected(), Some(0), "{:?}", report.candidates());
    assert_eq!(
        report.candidates()[0].runs[0].comparisons[0].modeled,
        Some(9.0)
    );
    assert_eq!(report.holdout()[0].comparisons[0].modeled, Some(0.0));
    r.observations[0].target = Target::Transit {
        route_or_area: "missing-route".into(),
        trips: vec!["not-in-network".into()],
    };
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert!(report.candidates()[0].runs[0].comparisons[0].flags[0].contains("trip-to-route"));
    r.observations[0].target = transit_target();
    r.observations[0].value = None;
    r.observations[0].missing_reason = Some("ridership unavailable".into());
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert!(
        report.candidates()[0].runs[0].comparisons[0].flags[0].contains("ridership unavailable")
    );
}

/// An alternate adapter isolates the boarding counter's multi-segment/transfer semantics.
struct BoardingOracle {
    denied: bool,
}
impl Adapter for BoardingOracle {
    fn name(&self) -> &'static str {
        "boarding-oracle"
    }
    fn evaluate(
        &self,
        n: &Network,
        d: &Demand,
        s: &Scenario,
    ) -> Result<simulation::Report, simulation::Error> {
        let mut r = Iterative.evaluate(n, d, s)?;
        r.engine = self.name().into();
        let a = &mut r.assignments[0];
        a.mode = Some(TravelMode::Transit);
        a.denied_boarding = self.denied;
        let leg = &mut a.choice.as_mut().unwrap().legs[0];
        let mut first = leg.journey.steps[0].clone();
        first.mode = mobility_graph::Mode::Bus;
        first.edge = EdgeId(11);
        first.departure_utc = 20;
        first.arrival_utc = 25;
        first.run = Some(mobility_routing::Run {
            trip: "outbound".into(),
            service_date: 20260105,
            ordinal: 0,
            stop_index: 0,
        });
        let mut second = first.clone();
        second.run.as_mut().unwrap().stop_index = 1;
        second.departure_utc = 25;
        second.arrival_utc = 30;
        let mut transfer = second.clone();
        transfer.run = None;
        let mut reboard = first.clone();
        reboard.departure_utc = 40;
        reboard.arrival_utc = 45;
        leg.journey.steps = vec![first, second, transfer, reboard];
        Ok(r)
    }
}
#[test]
fn e09_s02_multisegment_dedup_transfer_reboarding_and_denial_oracle() {
    let (n, d, s, mut r) = fixture::inputs();
    r.observations[0] = fixture::observation("boarding", Split::Fit, transit_target(), 0, 61, 18.0);
    let report = run(&BoardingOracle { denied: false }, &n, &d, &s, &r).unwrap();
    // 4.5 expanded travelers * 2 persons * 2 boardings, not 3 segment loads.
    assert_eq!(
        report.candidates()[0].runs[0].comparisons[0].modeled,
        Some(18.0)
    );
    let denied = run(&BoardingOracle { denied: true }, &n, &d, &s, &r).unwrap();
    assert_eq!(
        denied.candidates()[0].runs[0].comparisons[0].modeled,
        Some(0.0)
    );
}

fn survey(measure: SurveyMeasure) -> Target {
    Target::Survey {
        area: "abstract-cohort".into(),
        groups: vec!["adult".into()],
        purpose: Some(Purpose::Commute),
        measure,
    }
}
#[test]
fn e09_s03_commute_not_all_trip_time_and_explicit_mode_share_denominator() {
    let (n, d, mut s, mut r) = fixture::inputs();
    s.edits.push(simulation::Edit::StreetSeconds {
        edge: EdgeId(8),
        seconds: 40,
    });
    r.observations.push(fixture::observation(
        "commute",
        Split::Fit,
        survey(SurveyMeasure::MeanCommuteSeconds),
        0,
        61,
        10.0,
    ));
    r.observations.push(fixture::observation(
        "car-share",
        Split::Fit,
        survey(SurveyMeasure::ModeShare(TravelMode::Car)),
        0,
        61,
        1.0,
    ));
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    let c = &report.candidates()[0].runs[0].comparisons;
    assert_eq!(c[1].modeled, Some(10.0)); // return shopping duration is 40s, not in commute mean
    assert_eq!(c[1].eligible_person_trips, Some(9.0));
    assert_eq!(c[2].modeled, Some(1.0));
    assert_eq!(c[2].unserved_person_trips, Some(0.0));
    assert_eq!(report.candidates()[0].parameters.demand_multiplier, 1.0);
    assert_eq!(report.request().observations[2].value, Some(10.0));
    assert_eq!(c[1].residual, Some(0.0));
}
#[test]
fn e09_s03_empty_cohort_and_unserved_trips_do_not_create_good_fit() {
    let (n, mut d, s, mut r) = fixture::inputs();
    r.observations.push(fixture::observation(
        "survey",
        Split::Fit,
        survey(SurveyMeasure::MeanCommuteSeconds),
        61,
        240,
        10.0,
    ));
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert!(report.candidates()[0].runs[0].comparisons[1].flags[0].contains("empty survey"));
    r.observations[2].start = 0;
    r.observations[2].end = 61;
    d.groups[0].modes.clear();
    let report = run(&Iterative, &n, &d, &s, &r).unwrap();
    let c = &report.candidates()[0].runs[0].comparisons[1];
    assert_eq!(c.unserved_person_trips, Some(9.0));
    assert!(c.flags.iter().any(|f| f.contains("unserved")));
    assert_eq!(report.selected(), None);
}
#[test]
fn e09_s04_holdout_changes_scores_not_fitted_parameters() {
    let (n, d, s, mut r) = fixture::inputs();
    let first = run(&Iterative, &n, &d, &s, &r).unwrap();
    r.observations[1].value = Some(9.0);
    let changed = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert_eq!(first.candidates(), changed.candidates());
    assert_eq!(first.selected(), Some(0));
    assert_eq!(first.selected(), changed.selected());
    assert_eq!(first.holdout()[0].comparisons[0].residual, Some(0.0));
    assert_eq!(changed.holdout()[0].comparisons[0].residual, Some(-4.5));
    assert!(changed
        .limitations()
        .iter()
        .any(|s| s.contains("Holdout") && s.contains("exceeds")));
}
#[test]
fn e09_s04_rejects_record_and_relabelled_cross_split_leakage() {
    let (n, d, s, mut r) = fixture::inputs();
    r.observations[1].record = r.observations[0].record.clone();
    assert!(run(&Iterative, &n, &d, &s, &r)
        .unwrap_err()
        .0
        .contains("source record leaks"));
    r.observations[1].record = "different".into();
    r.observations[1].target = Target::Road {
        station: "relabeled".into(),
        direction: "relabeled".into(),
        edges: vec![EdgeId(7)],
    };
    r.observations[1].start = 0;
    r.observations[1].end = 61;
    assert!(run(&Iterative, &n, &d, &s, &r)
        .unwrap_err()
        .0
        .contains("duplicate mapped"));
}
#[test]
fn e09_s05_replay_seeds_source_versions_and_parameter_change_audit() {
    let (n, d, s, r) = fixture::inputs();
    let first = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert_eq!(first, run(&Iterative, &n, &d, &s, &r).unwrap());
    assert_eq!(first.request().seeds, vec![7, 42]);
    assert_eq!(
        first.candidates()[0]
            .runs
            .iter()
            .map(|r| r.seed)
            .collect::<Vec<_>>(),
        vec![7, 42]
    );
    assert_eq!(
        first.candidates()[1].parameters.demand_multiplier
            - first.base_settings().demand_multiplier,
        1.0
    );
    assert_eq!(first.candidates()[0].score_variance, Some(0.0));
    assert_eq!(
        first.request().observations[0].source.source_version,
        "e06-corridor-v1"
    );
    assert_eq!(first.network_revision(), n.data().revision);
    assert_eq!(first.demand_revision(), d.revision);
}
#[test]
fn e09_s06_nonconvergence_and_absent_holdout_and_synthetic_evidence_limit_fitness() {
    let (n, d, mut s, mut r) = fixture::inputs();
    let good = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert_eq!(good.fitness(), &Fitness::Limited);
    assert!(good.banner().contains("Do not use for policy"));
    assert!(good.limitations().iter().any(|l| l.contains("synthetic")));
    s.settings.max_iterations = 1;
    let capped = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert_eq!(capped.selected(), None);
    assert_eq!(
        capped.candidates()[0].runs[0].status,
        Some(Status::Nonconverged)
    );
    assert!(capped
        .limitations()
        .iter()
        .any(|l| l.contains("no candidate")));
    s.settings.max_iterations = 10;
    r.observations.retain(|o| o.split == Split::Fit);
    let no_holdout = run(&Iterative, &n, &d, &s, &r).unwrap();
    assert!(no_holdout.holdout()[0].score.normalized_rmse.is_none());
    assert!(no_holdout
        .limitations()
        .iter()
        .any(|l| l.contains("Holdout")));
}
struct Broken;
impl Adapter for Broken {
    fn name(&self) -> &'static str {
        "broken"
    }
    fn evaluate(
        &self,
        _: &Network,
        _: &Demand,
        _: &Scenario,
    ) -> Result<simulation::Report, simulation::Error> {
        Err(simulation::Error::Arithmetic)
    }
}
#[test]
fn e09_s05_s06_failed_candidates_are_retained_not_silently_dropped() {
    let (n, d, s, r) = fixture::inputs();
    let result = run(&Broken, &n, &d, &s, &r).unwrap();
    assert_eq!(result.candidates().len(), 2);
    assert_eq!(result.candidates()[0].runs.len(), 2);
    assert!(result.candidates()[0].runs[0]
        .error
        .as_ref()
        .unwrap()
        .contains("Arithmetic"));
    assert_eq!(result.selected(), None);
    assert_eq!(result.fitness(), &Fitness::Limited);
}
#[test]
fn e09_invalid_numeric_provenance_and_commute_inputs_fail_closed() {
    let (n, d, s, r) = fixture::inputs();
    for scale in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        let mut bad = r.clone();
        bad.observations[0].scale = scale;
        assert!(run(&Iterative, &n, &d, &s, &bad).is_err());
    }
    let mut bad = r.clone();
    bad.observations[0].source.date = "2026-02-30".into();
    assert!(run(&Iterative, &n, &d, &s, &bad).is_err());
    let mut bad = r.clone();
    bad.seeds.push(7);
    assert!(run(&Iterative, &n, &d, &s, &bad).is_err());
    let mut bad = r.clone();
    bad.observations[0].target = survey(SurveyMeasure::MeanCommuteSeconds);
    if let Target::Survey { purpose, .. } = &mut bad.observations[0].target {
        *purpose = None;
    }
    assert!(run(&Iterative, &n, &d, &s, &bad).is_err());
}
