//! E14-S07/S08: bounded synthetic ablations and frozen-policy topology transfer.
//! Fixed message-passing features + a fitted linear head, NOT a trained GNN/RL
//! algorithm or an empirical cross-city evaluation. Equal 12-parameter heads,
//! eight coefficient candidates and fixed episode budgets for each representation.
use optimizer_env::{
    observation::Observation,
    reference::{Config, Environment, IndexedTransition},
};

const TRAIN: [usize; 2] = [0, 1];
const FIT_SEEDS: [u64; 2] = [7, 19];
const TEST_SEEDS: [u64; 3] = [31, 47, 73];
const CANDIDATES: usize = 8;
const FEATURES: usize = 12;
#[derive(Clone, Copy, Debug)]
enum Representation {
    Vector,
    Graph,
}
impl Representation {
    fn name(self) -> &'static str {
        match self {
            Self::Vector => "vector-linear",
            Self::Graph => "graph-message-linear",
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Ablation {
    Full,
    NoCarRequired,
    NoParity,
    FeasibilityOnly,
}
impl Ablation {
    fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::NoCarRequired => "no-car-required",
            Self::NoParity => "no-parity",
            Self::FeasibilityOnly => "feasibility-only",
        }
    }
}
fn configuration(
    topology: usize,
    ablation: Ablation,
) -> Result<Config, Box<dyn std::error::Error>> {
    let mut c = optimizer_env::synthetic::configuration(topology)?;
    match ablation {
        Ablation::Full => {}
        Ablation::NoCarRequired => c.reward.car_required.weight = 0.0,
        Ablation::NoParity => c.reward.fixed_door_parity.weight = 0.0,
        Ablation::FeasibilityOnly => {
            c.reward.car_required.weight = 0.0;
            c.reward.fixed_door_parity.weight = 0.0;
        }
    }
    Ok(c)
}
fn features(o: &Observation, index: usize, repr: Representation) -> [f64; FEATURES] {
    let a = &o.actions[index];
    let mut f = [
        a.features[0],
        a.features[1],
        a.features[2],
        a.features[4],
        a.features[3],
        o.global[17],
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ];
    match repr {
        Representation::Vector => {
            // Action-conditioned fixed-vector baseline: known link/time
            // presence, anchor degrees/presence and current affordability.
            // Unlike global-only inputs these can change action ranking.
            f[6..].copy_from_slice(&[
                a.features[5],
                a.features[6],
                a.features[7],
                a.features[8],
                a.features[9],
                if o.global[17] > 0.0 {
                    a.features[1] / o.global[17]
                } else {
                    0.0
                },
            ]);
        }
        Representation::Graph => {
            // One shared message pass: each directed edge sends its source's
            // degree to target and target's degree to source. Pool at the
            // candidate's public anchors, never city IDs or demand records.
            for (side, anchor) in a.anchors.iter().enumerate() {
                if let Some(anchor) = anchor {
                    let node = &o.nodes[*anchor];
                    f[6 + side * 3] = node.in_degree as f64;
                    f[7 + side * 3] = node.out_degree as f64;
                    let mut sum = 0.0;
                    let mut count = 0;
                    for edge in &o.edges {
                        if edge.from == *anchor {
                            let n = &o.nodes[edge.to];
                            sum += (n.in_degree + n.out_degree) as f64;
                            count += 1;
                        }
                        if edge.to == *anchor {
                            let n = &o.nodes[edge.from];
                            sum += (n.in_degree + n.out_degree) as f64;
                            count += 1;
                        }
                    }
                    f[8 + side * 3] = sum / count.max(1) as f64;
                }
            }
        }
    }
    f
}
fn scaler(
    repr: Representation,
    ablation: Ablation,
) -> Result<[f64; FEATURES], Box<dyn std::error::Error>> {
    let mut scale = [1.0_f64; FEATURES];
    for topology in TRAIN {
        let mut env = Environment::new(configuration(topology, ablation)?)?;
        for seed in FIT_SEEDS {
            let t = env.reset(seed)?;
            for a in 0..t.observation.actions.len() {
                for (i, v) in features(&t.observation, a, repr).into_iter().enumerate() {
                    scale[i] = scale[i].max(v.abs());
                }
            }
        }
    }
    Ok(scale)
}
fn choose(
    t: &IndexedTransition,
    repr: Representation,
    weights: &[f64; FEATURES],
    scale: &[f64; FEATURES],
) -> usize {
    let mut best = t.observation.actions.len() - 1;
    let mut score = f64::NEG_INFINITY;
    for a in 0..t.observation.actions.len() {
        let allowed = if a == t.mask.entries.len() {
            t.stop_allowed
        } else {
            t.mask.entries[a].allowed()
        };
        if !allowed {
            continue;
        }
        let value: f64 = features(&t.observation, a, repr)
            .iter()
            .zip(weights)
            .zip(scale)
            .map(|((v, w), s)| v / s * w)
            .sum();
        if value > score {
            score = value;
            best = a;
        }
    }
    best
}
struct Outcome {
    reward: f64,
    failures: usize,
    trivial: bool,
}
struct Policy<'a> {
    repr: Representation,
    ablation: Ablation,
    weights: &'a [f64; FEATURES],
    scale: &'a [f64; FEATURES],
}
fn episode(
    topology: usize,
    seed: u64,
    policy: Policy<'_>,
    log: Option<&str>,
    stop_only: bool,
) -> Result<Outcome, Box<dyn std::error::Error>> {
    let Policy {
        repr,
        ablation,
        weights,
        scale,
    } = policy;
    let c = configuration(topology, ablation)?;
    if let Some(split) = log {
        println!(
            "config\t{}\t{}\t{split}\t{topology}\t{seed}\t{:?}\t{:?}",
            ablation.name(),
            repr.name(),
            c.reward,
            c.opening.budget
        );
    }
    let horizon = c.horizon;
    let mut env = Environment::new(c)?;
    env.reset(seed)?;
    let mut outcome = Outcome {
        reward: 0.0,
        failures: 0,
        trivial: true,
    };
    for _ in 0..=horizon {
        let t = env.current().ok_or("missing transition")?;
        if t.terminated || t.truncated {
            break;
        }
        let action = if stop_only {
            env.stop_index()
        } else {
            choose(t, repr, weights, scale)
        };
        let state = t.state;
        match env.step(action, state) {
            Ok(t) => {
                outcome.reward += t.reward.reward;
                if t.reward.components.iter().any(|c| c.raw_gain.abs() > 1e-12) {
                    outcome.trivial = false;
                }
                if let Some(split) = log {
                    for c in &t.reward.components {
                        println!("component\t{}\t{}\t{split}\t{topology}\t{seed}\t{action}\t{}\t{}\t{}\t{}\t{}\t{}", ablation.name(), repr.name(), c.metric, c.raw_gain, c.denominator, c.term.weight, c.term.scale, c.weighted);
                    }
                    for c in &t.reward.constraints {
                        println!("constraint\t{}\t{}\t{split}\t{topology}\t{seed}\t{action}\t{}\t{}\t{}\t{}", ablation.name(), repr.name(), c.metric, c.increase_or_loss, c.maximum, c.passed);
                    }
                    println!(
                        "convergence\t{}\t{}\t{split}\t{topology}\t{seed}\t{action}\t{}",
                        ablation.name(),
                        repr.name(),
                        t.reward.convergence_passed
                    );
                }
            }
            Err(error) => {
                outcome.failures += 1;
                if let Some(split) = log {
                    println!(
                        "failure\t{}\t{}\t{split}\t{topology}\t{seed}\t{action}\t{error}",
                        ablation.name(),
                        repr.name()
                    );
                    if let optimizer_env::reference::Error::Constraint(audit) = &error {
                        for c in &audit.constraints {
                            println!("failed-constraint\t{}\t{}\t{split}\t{topology}\t{seed}\t{action}\t{}\t{}\t{}\t{}", ablation.name(), repr.name(), c.metric, c.increase_or_loss, c.maximum, c.passed);
                        }
                    }
                }
                break;
            }
        }
    }
    if let Some(split) = log {
        let t = env.current().ok_or("missing final metrics")?;
        let m = &t.reward.after;
        println!(
            "outcome\t{}\t{}\t{split}\t{topology}\t{seed}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            ablation.name(),
            repr.name(),
            m.all.total,
            m.all.car_required,
            m.all.car_free,
            m.all.no_mode,
            m.all.unserved,
            m.essential.unserved,
            m.fixed_door_parity,
            t.budget.capital.upper,
            t.budget.annual.upper
        );
        println!(
            "episode\t{}\t{}\t{split}\t{topology}\t{seed}\t{}\t{}\t{}",
            ablation.name(),
            repr.name(),
            outcome.reward,
            outcome.failures,
            outcome.trivial
        );
    }
    Ok(outcome)
}
fn csv(values: &[f64]) -> String {
    values
        .iter()
        .map(f64::to_string)
        .collect::<Vec<_>>()
        .join(",")
}
fn known_failure_diagnostic() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = configuration(7, Ablation::Full)?;
    let mut supply = config.simulation.transit.clone();
    supply[0].passengers = 0.0;
    let mut projects = config.catalog.projects().to_vec();
    projects[2].payload.transit = Some(optimizer_env::reference::TransitPortfolio {
        costs: config.opening.transit.clone(),
        supply,
    });
    config.catalog = optimizer_env::catalog::Catalog::new("e14-diagnostic-only".into(), projects)?;
    println!(
        "diagnostic-config\tknown-capacity-denial\t{:?}",
        config.reward
    );
    let mut env = Environment::new(config)?;
    let opening = env.reset(42)?.clone();
    let error = match env.step(2, opening.state) {
        Ok(_) => return Err("known denial unexpectedly passed hard constraints".into()),
        Err(error) => error,
    };
    let optimizer_env::reference::Error::Constraint(audit) = error else {
        return Err("capacity diagnostic did not reach the reward guard".into());
    };
    for c in &audit.components {
        println!(
            "diagnostic-component\tknown-capacity-denial\t{}\t{}\t{}\t{}\t{}\t{}",
            c.metric, c.raw_gain, c.denominator, c.term.weight, c.term.scale, c.weighted
        );
    }
    for c in &audit.constraints {
        println!(
            "diagnostic-constraint\tknown-capacity-denial\t{}\t{}\t{}\t{}",
            c.metric, c.increase_or_loss, c.maximum, c.passed
        );
    }
    if audit.passed() || env.current() != Some(&opening) {
        return Err("failure diagnostic unexpectedly committed".into());
    }
    println!("diagnostic\tknown-capacity-denial\trejected-without-mutation");
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let heldout = match args.as_slice() {
        [] => [4, 7],
        [flag] if flag == "--heldout-alt" => [5, 8],
        _ => return Err("usage: e14_experiments [--heldout-alt]".into()),
    };
    println!("e14-experiments\t1\tsynthetic-only; no calibration or cross-city policy claim");
    println!("split\ttrain\t0,1\tseeds=7,19");
    println!(
        "split\theldout\t{},{}\tseeds=31,47,73",
        heldout[0], heldout[1]
    );
    println!("compute\tparameters={FEATURES}\tcandidates={CANDIDATES}\ttraining_topologies=2\ttraining_seeds=2\tselection=training-only");
    println!("outcome-schema\ttotal-trip-weight\treference-car-required-weight\treference-car-free-weight\tno-mode-weight\tunserved-weight\tunserved-essential-weight\tfixed-door-parity\tcapital-upper-major\tannual-upper-major-per-year");
    println!("seed-interpretation\tpreference-noise-disabled; deterministic replay spread only, not stochastic uncertainty");
    known_failure_diagnostic()?;
    for ablation in [
        Ablation::Full,
        Ablation::NoCarRequired,
        Ablation::NoParity,
        Ablation::FeasibilityOnly,
    ] {
        for repr in [Representation::Vector, Representation::Graph] {
            let scale = scaler(repr, ablation)?;
            println!(
                "normalizer\t{}\t{}\t{}",
                ablation.name(),
                repr.name(),
                csv(&scale)
            );
            let mut random = 42u64;
            let mut best_weights = [0.0; FEATURES];
            let mut best_score = f64::NEG_INFINITY;
            let mut best_failures = usize::MAX;
            for candidate in 0..CANDIDATES {
                let mut weights = [0.0; FEATURES];
                for w in &mut weights {
                    random ^= random << 13;
                    random ^= random >> 7;
                    random ^= random << 17;
                    *w = (random >> 11) as f64 / ((1u64 << 53) as f64) * 2.0 - 1.0;
                }
                if candidate == 0 {
                    weights = [0.0; FEATURES];
                    weights[3] = -1.0;
                    weights[0] = -2.0;
                }
                let mut score = 0.0;
                let mut failures = 0;
                for topology in TRAIN {
                    for seed in FIT_SEEDS {
                        let o = episode(
                            topology,
                            seed,
                            Policy {
                                repr,
                                ablation,
                                weights: &weights,
                                scale: &scale,
                            },
                            None,
                            false,
                        )?;
                        score += o.reward;
                        failures += o.failures;
                    }
                }
                if failures < best_failures || (failures == best_failures && score > best_score) {
                    best_failures = failures;
                    best_score = score;
                    best_weights = weights;
                }
            }
            println!("policy\t{}\t{}\t{}\ttraining_reward={best_score}\ttraining_failures={best_failures}", ablation.name(), repr.name(), csv(&best_weights));
            for topology in heldout {
                let mut rewards = vec![];
                for seed in TEST_SEEDS {
                    let o = episode(
                        topology,
                        seed,
                        Policy {
                            repr,
                            ablation,
                            weights: &best_weights,
                            scale: &scale,
                        },
                        Some("heldout"),
                        false,
                    )?;
                    rewards.push(o.reward);
                    episode(
                        topology,
                        seed,
                        Policy {
                            repr,
                            ablation,
                            weights: &best_weights,
                            scale: &scale,
                        },
                        Some("stop-baseline"),
                        true,
                    )?;
                }
                // Center first so identical deterministic replays have exact
                // zero spread rather than a rounding artifact from summation.
                let origin = rewards[0];
                let mean =
                    origin + rewards.iter().map(|v| v - origin).sum::<f64>() / rewards.len() as f64;
                let variance = rewards.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                    / (rewards.len() - 1) as f64;
                println!("summary\t{}\t{}\t{topology}\tn={}\tmean={mean}\tbetween_seed_sample_variance={variance}\tno-confidence-interval", ablation.name(), repr.name(), rewards.len());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn e14_s08_vector_baseline_is_action_conditioned() {
        let mut env = Environment::new(configuration(0, Ablation::Full).unwrap()).unwrap();
        let mut observation = env.reset(42).unwrap().observation.clone();
        observation.actions[0].features[6] = 10.0;
        observation.actions[1].features[6] = 2.0;
        assert_eq!(features(&observation, 0, Representation::Vector)[7], 10.0);
        assert_eq!(features(&observation, 1, Representation::Vector)[7], 2.0);
    }
    #[test]
    fn e14_s08_graph_policy_features_are_permutation_invariant() {
        let mut env = Environment::new(configuration(4, Ablation::Full).unwrap()).unwrap();
        let original = env.reset(42).unwrap().observation.clone();
        let mut permuted = original.clone();
        let n = permuted.nodes.len();
        permuted.nodes.reverse();
        permuted.edges.reverse();
        for edge in &mut permuted.edges {
            edge.from = n - 1 - edge.from;
            edge.to = n - 1 - edge.to;
        }
        for action in &mut permuted.actions {
            action.anchors = action.anchors.map(|i| i.map(|j| n - 1 - j));
        }
        for a in 0..original.actions.len() {
            assert_eq!(
                features(&original, a, Representation::Graph),
                features(&permuted, a, Representation::Graph)
            );
        }
    }
}
