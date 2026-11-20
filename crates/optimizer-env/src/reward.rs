//! E14-S04/S07: reference feasibility, fixed door parity and assignment safeguards.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Term {
    pub weight: f64,
    pub scale: f64,
}

/// Weights may be zero for ablations. Scales are positive, in the units of the
/// raw terms. Guard tolerances are in weighted trips (absolute deltas).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    pub car_required: Term,
    pub car_free_feasible: Term,
    pub fixed_door_parity: Term,
    pub max_unserved_increase: f64,
    pub max_denied_increase: f64,
    pub max_essential_unserved_increase: f64,
    pub max_noncycling_unserved_increase: f64,
    pub max_step_free_unserved_increase: f64,
    pub max_no_mode_increase: f64,
    pub max_protected_car_free_loss: f64,
    pub require_convergence: bool,
}

impl Config {
    pub(crate) fn valid(self) -> bool {
        [
            self.car_required,
            self.car_free_feasible,
            self.fixed_door_parity,
        ]
        .iter()
        .all(|t| nonnegative(t.weight) && t.scale.is_finite() && t.scale > 0.0)
            && [
                self.max_unserved_increase,
                self.max_denied_increase,
                self.max_essential_unserved_increase,
                self.max_noncycling_unserved_increase,
                self.max_step_free_unserved_increase,
                self.max_no_mode_increase,
                self.max_protected_car_free_loss,
            ]
            .into_iter()
            .all(nonnegative)
    }
}

fn nonnegative(x: f64) -> bool {
    x.is_finite() && x >= 0.0
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cohort {
    pub total: f64,
    pub car_required: f64,
    pub car_free: f64,
    pub no_mode: f64,
    pub unserved: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Metrics {
    pub all: Cohort,
    pub essential: Cohort,
    pub noncycling: Cohort,
    pub step_free: Cohort,
    pub denied: f64,
    pub fixed_door_parity: f64,
    pub door_weight: f64,
    pub status: mobility_sim::simulation::Status,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ProtectedLoss {
    pub essential: f64,
    pub noncycling: f64,
    pub step_free: f64,
}

impl ProtectedLoss {
    pub(crate) fn add(&mut self, essential: bool, noncycling: bool, step_free: bool, weight: f64) {
        if essential {
            self.essential += weight;
        }
        if noncycling {
            self.noncycling += weight;
        }
        if step_free {
            self.step_free += weight;
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ServiceLoss {
    pub unserved: ProtectedLoss,
    pub car_free: ProtectedLoss,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub metric: &'static str,
    pub increase_or_loss: f64,
    pub maximum: f64,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    pub metric: &'static str,
    pub raw_gain: f64,
    pub denominator: f64,
    pub term: Term,
    pub weighted: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Audit {
    pub config: Config,
    pub before: Metrics,
    pub after: Metrics,
    pub components: Vec<Component>,
    pub constraints: Vec<Comparison>,
    pub convergence_passed: bool,
    pub reward: f64,
}

impl Audit {
    pub fn passed(&self) -> bool {
        self.convergence_passed && self.constraints.iter().all(|c| c.passed)
    }

    pub(crate) fn evaluate(
        config: Config,
        before: Metrics,
        after: Metrics,
        loss: Option<ServiceLoss>,
    ) -> Self {
        let project = loss.is_some();
        let loss = loss.unwrap_or_default();
        let constraints = [
            (
                "unserved",
                after.all.unserved - before.all.unserved,
                config.max_unserved_increase,
            ),
            (
                "denied",
                after.denied - before.denied,
                config.max_denied_increase,
            ),
            (
                "essential_unserved",
                after.essential.unserved - before.essential.unserved,
                config.max_essential_unserved_increase,
            ),
            (
                "noncycling_unserved",
                after.noncycling.unserved - before.noncycling.unserved,
                config.max_noncycling_unserved_increase,
            ),
            (
                "step_free_unserved",
                after.step_free.unserved - before.step_free.unserved,
                config.max_step_free_unserved_increase,
            ),
            (
                "newly_unserved_essential",
                loss.unserved.essential,
                config.max_essential_unserved_increase,
            ),
            (
                "newly_unserved_noncycling",
                loss.unserved.noncycling,
                config.max_noncycling_unserved_increase,
            ),
            (
                "newly_unserved_step_free",
                loss.unserved.step_free,
                config.max_step_free_unserved_increase,
            ),
            (
                "gross_essential_car_free_loss",
                loss.car_free.essential,
                config.max_protected_car_free_loss,
            ),
            (
                "gross_noncycling_car_free_loss",
                loss.car_free.noncycling,
                config.max_protected_car_free_loss,
            ),
            (
                "gross_step_free_car_free_loss",
                loss.car_free.step_free,
                config.max_protected_car_free_loss,
            ),
            (
                "no_mode",
                after.all.no_mode - before.all.no_mode,
                config.max_no_mode_increase,
            ),
            (
                "essential_car_free",
                before.essential.car_free - after.essential.car_free,
                config.max_protected_car_free_loss,
            ),
            (
                "noncycling_car_free",
                before.noncycling.car_free - after.noncycling.car_free,
                config.max_protected_car_free_loss,
            ),
            (
                "step_free_car_free",
                before.step_free.car_free - after.step_free.car_free,
                config.max_protected_car_free_loss,
            ),
        ]
        .into_iter()
        .map(|(metric, increase_or_loss, maximum)| Comparison {
            metric,
            increase_or_loss,
            maximum,
            passed: increase_or_loss <= maximum,
        })
        .collect();
        let components: Vec<_> = [
            (
                "reference_car_required_share",
                (before.all.car_required - after.all.car_required) / before.all.total,
                before.all.total,
                config.car_required,
            ),
            (
                "reference_car_free_share",
                (after.all.car_free - before.all.car_free) / before.all.total,
                before.all.total,
                config.car_free_feasible,
            ),
            (
                "fixed_door_parity",
                after.fixed_door_parity - before.fixed_door_parity,
                before.door_weight,
                config.fixed_door_parity,
            ),
        ]
        .into_iter()
        .map(|(metric, raw_gain, denominator, term)| Component {
            metric,
            raw_gain,
            denominator,
            term,
            weighted: if project && term.weight != 0.0 && raw_gain != 0.0 {
                weighted_gain(raw_gain, term)
            } else {
                0.0
            },
        })
        .collect();
        let reward = components.iter().map(|c| c.weighted).sum();
        let convergence_passed = !config.require_convergence
            || after.status == mobility_sim::simulation::Status::Converged;
        Self {
            config,
            before,
            after,
            components,
            constraints,
            convergence_passed,
            reward,
        }
    }
}

fn weighted_gain(raw: f64, term: Term) -> f64 {
    // Raw gains are differences of shares. Prefer forming the weight/scale
    // ratio, particularly for equal subnormal terms. If both divisions would
    // overflow, the nonzero operands are large enough for the product first.
    let ratio = term.weight / term.scale;
    if ratio.is_finite() {
        return raw * ratio;
    }
    let scaled = raw / term.scale;
    if scaled.is_finite() {
        scaled * term.weight
    } else {
        (raw * term.weight) / term.scale
    }
}
