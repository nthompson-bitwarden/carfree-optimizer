//! E18 read-only projections of the bounded E17 scenario evaluations.
use super::{array, evaluate, fixture, json, Phase, Plan, Result};
use interventions::scenario::Snapshot;
use mobility_sim::demand::TravelMode;
use optimizer_env::{reference::IndexedTransition, synthetic};

const METRICS: [(&str, &str); 9] = [
    ("Essential trips unserved in assignment", "%"),
    ("Essential trips with no car-free option in reference", "%"),
    ("Essential trips with no feasible mode in reference", "%"),
    ("Noncycling cohort unserved in assignment", "weighted trips"),
    ("Step-free cohort unserved in assignment", "weighted trips"),
    ("Fixed-door access parity", "ratio"),
    ("Central capital commitment", "2026 USD"),
    ("Central annual commitment", "2026 USD/year"),
    ("Essential trips requiring a car in reference", "%"),
];
type Values = [Option<f64>; METRICS.len()];

const CALIBRATION: &str =
    "Uncalibrated synthetic fixture. No observed calibration or holdout evidence.";
const UNCERTAINTY: &str = "Empirical uncertainty unavailable. Sensitivity ranges are extrema from declared tests. Cost bounds are supplied fixture assumptions.";
const LIMITATIONS: &str = "Modeled contrasts cannot establish real-world causal effects or support policy recommendations. Timetable, waiting time, whole-tour feasibility and mobility constraints reflect only the supplied service window and cohorts. The abstract origins have no Census neighborhood correspondence. Land-use response and wider network spillovers are unassessed.";

pub struct Completed {
    pub phases: Vec<Phase>,
    pub report: String,
}

#[derive(Debug)]
pub(super) struct PhaseEvidence {
    values: Values,
    trace: String,
    groups: String,
    reward: f64,
    capital_upper: f64,
    assignment_essential_total: f64,
    reference_essential_total: f64,
    status: String,
}

fn share(numerator: f64, total: f64) -> Option<f64> {
    (total > 0.0).then(|| 100.0 * numerator / total)
}

fn optional(value: Option<f64>) -> String {
    value.map_or_else(|| "null".into(), |value| value.to_string())
}

fn numbers(values: Values) -> String {
    array(values.into_iter().map(optional))
}

fn changes(before: Values, after: Values) -> Values {
    std::array::from_fn(|i| after[i].zip(before[i]).map(|(a, b)| a - b))
}

impl PhaseEvidence {
    pub(super) fn new(
        snapshot: &Snapshot,
        projects: &[String],
        transition: &IndexedTransition,
        catalog: &str,
    ) -> Self {
        let bridge = &transition.evidence.trips;
        let run = bridge.evidence.run();
        let assigned = &bridge.assignment.essential;
        let reference = &bridge.reference_feasibility.essential;
        let metrics = &transition.reward.after;
        let context = &bridge.assignment.context;
        let provenance = &snapshot.network().data().provenance;

        let groups = array(bridge.assignment.by_group_neighborhood.iter().map(|(key, assigned)| {
            let (group, zone) = key;
            let reference = &bridge.reference_feasibility.by_group_neighborhood[key];
            let served = array(assigned.served.iter()
                .map(|(mode, value)| format!("[{},{value}]", json(&format!("{mode:?}")))));
            // E14/E15 topology 0 binds both primary cohorts in abstract-origin to node 1.
            format!(
                r#"{{"origin":1,"zone":{},"group":{},"total":{},"unserved":{},"served":{served},"referenceTotal":{},"carRequired":{},"noMode":{},"referenceUnknown":{}}}"#,
                json(zone), json(group), assigned.total, assigned.unserved,
                reference.total, reference.car_required, reference.no_mode, reference.unknown,
            )
        }));

        let trace = format!(
            r#"{{"scenario":{},"scenarioVersion":{},"networkRevision":{},"effectiveNetworkRevision":{},"demandRevision":{},"serviceRevision":{},"destinationRevision":{},"catalogRevision":{},"projects":{},"seed":{},"models":{{"assignment":{},"simulationVersion":{},"demandVersion":{},"scoringVersion":{},"optimizerProtocol":{}}},"source":{},"sourceVersion":{},"geography":{},"date":{},"license":{},"period":{},"basis":{},"sourceUncertainty":{},"horizon":{},"behavior":{}}}"#,
            json(snapshot.id()),
            snapshot.version(),
            json(&context.graph_revision),
            json(&run.effective_network_revision),
            json(&run.demand_revision),
            json(&context.service_revision),
            json(&context.destination_revision),
            json(catalog),
            array(projects.iter().map(|id| json(id))),
            run.seed,
            json(&run.engine),
            run.version,
            mobility_sim::demand::VERSION,
            bridge.assignment.version,
            transition.version,
            json(&provenance.source),
            json(&provenance.source_version),
            json(&provenance.geography),
            json(&provenance.date),
            json(&provenance.license),
            json(&context.period),
            json(&format!("{:?}", provenance.basis)),
            json(&provenance.uncertainty),
            json(&format!("{:?}", context.horizon)),
            json(&context.behavior),
        );
        Self {
            values: [
                share(assigned.unserved, assigned.total),
                (reference.unknown == 0.0)
                    .then(|| share(reference.car_required + reference.no_mode, reference.total))
                    .flatten(),
                (reference.unknown == 0.0)
                    .then(|| share(reference.no_mode, reference.total))
                    .flatten(),
                Some(metrics.noncycling.unserved),
                Some(metrics.step_free.unserved),
                (metrics.door_weight > 0.0).then_some(metrics.fixed_door_parity),
                Some(transition.budget.capital.central),
                Some(transition.budget.annual.central),
                (reference.unknown == 0.0)
                    .then(|| share(reference.car_required, reference.total))
                    .flatten(),
            ],
            trace,
            groups,
            reward: transition.reward.reward,
            capital_upper: transition.budget.capital.upper,
            assignment_essential_total: assigned.total,
            reference_essential_total: reference.total,
            status: json(&format!("{:?}", run.status)),
        }
    }

    pub(super) fn trace(&self) -> &str {
        &self.trace
    }
}

fn last(phases: &[Phase]) -> &PhaseEvidence {
    &phases
        .last()
        .expect("evaluation includes the baseline")
        .report
}

fn step_views(phases: &[Phase]) -> String {
    array(phases.iter().enumerate().map(|(i, phase)| {
        let row = &phase.report;
        let prior = &phases[i.saturating_sub(1)].report;
        format!(
            r#"{{"trace":{},"values":{},"incremental":{},"cumulative":{},"groups":{},"status":{},"reward":{},"assignmentEssentialTotal":{},"referenceEssentialTotal":{}}}"#,
            row.trace, numbers(row.values), numbers(changes(prior.values, row.values)),
            numbers(changes(phases[0].report.values, row.values)), row.groups, row.status, row.reward, row.assignment_essential_total, row.reference_essential_total,
        )
    }))
}

struct Case {
    assumption: &'static str,
    detail: &'static str,
    demand_mix: u32,
    walk_restriction: bool,
    wait_multiplier: f64,
}

const CONDITIONS: [Case; 3] = [
    Case {
        assumption: "Demand",
        detail: "E15 demand mix 2. Adult weight 6, mobility-aid weight 4. Nominal weights 4 and 2.",
        demand_mix: 2,
        walk_restriction: false,
        wait_multiplier: 1.0,
    },
    Case {
        assumption: "Weather",
        detail: "Hypothetical storm removes walk-only tours from the modeled daylight choice set. Access legs to transit remain allowed.",
        demand_mix: 0,
        walk_restriction: true,
        wait_multiplier: 1.0,
    },
    Case {
        assumption: "Behavior",
        detail: "Waiting carries twice the nominal generalized time penalty. Timetable times stay fixed.",
        demand_mix: 0,
        walk_restriction: false,
        wait_multiplier: 2.0,
    },
];

fn sensitivity(plan: &Plan, nominal: &[Phase]) -> String {
    let base = &nominal[0].report;
    let after = last(nominal);
    let mut samples = vec![changes(base.values, after.values)];
    let cases = CONDITIONS
        .iter()
        .map(|case| {
            let evaluate_case = || {
                let mut config = synthetic::benchmark_configuration(0, case.demand_mix)?;
                let assumptions = &mut config.simulation.demand_scenario;
                assumptions.cost.wait_multiplier = case.wait_multiplier;
                if case.wait_multiplier != 1.0 {
                    assumptions.cost.revision = "e18-double-wait-v1".into();
                }
                if case.walk_restriction {
                    assumptions.environment.label = "synthetic storm, walk-only option unavailable".into();
                    assumptions.environment.weather = "synthetic storm".into();
                    assumptions
                        .environment
                        .light_modes
                        .retain(|mode| *mode != TravelMode::Walk);
                }
                evaluate(plan.configure(config)?, plan)
            };
            let result = match evaluate_case() {
                Ok(phases) => {
                    let values = changes(phases[0].report.values, last(&phases).values);
                    samples.push(values);
                    format!(
                        r#""phases":{},"changes":{}"#,
                        step_views(&phases),
                        numbers(values)
                    )
                }
                Err(error) => format!(r#""error":{}"#, json(&error)),
            };
            format!(
                r#"{{"assumption":{},"detail":{},"demandMix":{},"walkRestriction":{},"waitMultiplier":{},{result}}}"#,
                json(case.assumption),
                json(case.detail),
                case.demand_mix, case.walk_restriction, case.wait_multiplier,
            )
        })
        .collect::<Vec<_>>();
    let ranges = array((0..METRICS.len()).map(|i| {
        let values: Vec<_> = samples.iter().filter_map(|v| v[i]).collect();
        let bounds = values
            .iter()
            .copied()
            .reduce(f64::min)
            .zip(values.iter().copied().reduce(f64::max));
        bounds.map_or_else(
            || "null".into(),
            |(min, max)| format!("[{min},{max},{}]", values.len()),
        )
    }));
    format!(r#"{{"cases":{},"ranges":{ranges}}}"#, array(cases))
}

struct Alternative {
    indices: Vec<usize>,
    capital: f64,
    reward: f64,
    phases: String,
}

impl Alternative {
    fn from_phases(indices: Vec<usize>, phases: &[Phase]) -> Self {
        Self {
            indices,
            capital: last(phases).capital_upper,
            reward: phases.iter().skip(1).map(|p| p.report.reward).sum(),
            phases: step_views(phases),
        }
    }

    fn dominates(&self, other: &Self) -> bool {
        self.capital <= other.capital
            && self.reward >= other.reward
            && (self.capital < other.capital || self.reward > other.reward)
    }
}

fn tradeoffs(plan: &Plan, nominal: &[Phase]) -> Result<String> {
    let fixture = fixture()?;
    let project_ids: Vec<_> = fixture.catalog.projects().iter().map(|p| &p.id).collect();
    let mut alternatives = vec![Alternative::from_phases(vec![], &nominal[..1])];
    let mut rejected = Vec::new();
    let candidates = (0..project_ids.len()).flat_map(|i| {
        std::iter::once(vec![i]).chain(
            (0..project_ids.len())
                .filter(move |&j| j != i)
                .map(move |j| vec![i, j]),
        )
    });
    for indices in candidates {
        let alternative = Plan {
            projects: indices.clone(),
            ..plan.clone()
        };
        if indices == plan.projects {
            alternatives.push(Alternative::from_phases(indices, nominal));
            continue;
        }
        match alternative
            .configure(fixture.clone())
            .and_then(|config| evaluate(config, &alternative))
        {
            Ok(phases) => alternatives.push(Alternative::from_phases(indices, &phases)),
            Err(error) => rejected.push(format!(
                r#"{{"projects":{},"error":{}}}"#,
                array(indices.iter().map(|&i| json(project_ids[i]))),
                json(&error),
            )),
        }
    }
    let rows = alternatives.iter().map(|row| {
        let frontier = !alternatives.iter().any(|other| other.dominates(row));
        format!(
            r#"{{"indices":{},"capital":{},"reward":{},"frontier":{frontier},"phases":{}}}"#,
            array(row.indices.iter()),
            row.capital,
            row.reward,
            row.phases,
        )
    });
    Ok(format!(
        r#"{{"alternatives":{},"rejected":{}}}"#,
        array(rows),
        array(rejected),
    ))
}

pub(super) fn build(plan: &Plan) -> Result<Completed> {
    let phases = evaluate(plan.configure(fixture()?)?, plan)?;
    let metrics = array(METRICS.map(|(name, unit)| format!("[{},{}]", json(name), json(unit))));
    let report = format!(
        r#"{{"version":1,"methodology":{{"calibration":{},"uncertainty":{},"limitations":{},"assumptions":{}}},"metrics":{metrics},"phases":{},"sensitivity":{},"tradeoffs":{}}}"#,
        json(CALIBRATION), json(UNCERTAINTY), json(LIMITATIONS),
        json("Matched nominal runs use E14 topology 0, a fixed 180 s fixture clock and fixed synthetic demand with declared mode costs. Regional rows use whole-tour reference feasibility and weighted assignment legs grouped by home zone. Each condition uses its own matched baseline. The frontier maximizes summed E14 reward (car requirement, car-free feasibility and fixed-door parity) and minimizes upper-bound capital under the submitted budget and width."),
        step_views(&phases), sensitivity(plan, &phases), tradeoffs(plan, &phases)?,
    );
    Ok(Completed { phases, report })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e18_s01_s04_unavailable_values_do_not_produce_numeric_changes() {
        let mut before = [Some(1.0); METRICS.len()];
        let mut after = [Some(0.0); METRICS.len()];
        before[0] = None;
        after[1] = None;
        let deltas = changes(before, after);
        assert_eq!(&deltas[..3], &[None, None, Some(-1.0)]);
    }

    #[test]
    fn e18_s03_s04_s05_s07_report_is_deterministic_and_bound_to_phases() {
        let plan = Plan::parse("0,1", "1900", "200", "30").unwrap();
        let result = build(&plan).unwrap();
        assert_eq!(result.report, build(&plan).unwrap().report);
        let [base, first, final_phase]: &[Phase; 3] = result.phases.as_slice().try_into().unwrap();
        let steps = [&base.report, &first.report, &final_phase.report];
        assert_eq!(
            steps.map(|p| p.values[6]),
            [Some(0.0), Some(60.0), Some(100.0)]
        );
        assert!(steps[2]
            .trace
            .contains(r#""projects":["parking-walk","direct-door"],"seed":42"#));
        assert!(result.report.contains("Uncalibrated synthetic fixture"));
    }

    #[test]
    fn e18_s06_exact_dominance_retains_ties_and_tradeoffs() {
        let row = |capital, reward| Alternative {
            indices: vec![],
            capital,
            reward,
            phases: String::new(),
        };
        let baseline = row(0.0, 0.0);
        let better = row(50.0, 1.0);
        assert!(!baseline.dominates(&better));
        assert!(!better.dominates(&baseline));
        assert!(!better.dominates(&row(50.0, 1.0)));
        assert!(better.dominates(&row(50.0, 0.9)));
        assert!(better.dominates(&row(60.0, 1.0)));
    }
}
