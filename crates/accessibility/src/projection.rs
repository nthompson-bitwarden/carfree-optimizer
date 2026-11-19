//! Immutable scalar projections retain the score's complete common estimand.
use crate::scoring::{AccessScore, Context, Mode};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Estimand {
    pub destinations: Vec<String>,
    pub destination_type: String,
    pub destination_revision: String,
    pub modes: Vec<Mode>,
    pub profile: String,
    pub departures_utc: Vec<i64>,
    pub budget_seconds: u32,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    pub(crate) origin: String,
    pub(crate) neighborhood: String,
    pub(crate) group: String,
    pub(crate) context: Context,
    pub(crate) estimand: Estimand,
    pub(crate) share: Option<f64>,
}
impl Projection {
    pub fn from_access(s: &AccessScore) -> Self {
        Self {
            origin: s.stratum.origin.clone(),
            neighborhood: s.stratum.neighborhood.clone(),
            group: s.stratum.group.clone(),
            context: s.context.clone(),
            estimand: Estimand {
                destinations: s.destinations.clone(),
                destination_type: s.stratum.destination_type.clone(),
                destination_revision: s.context.destination_revision.clone(),
                modes: s.stratum.modes.clone(),
                profile: s.stratum.profile.clone(),
                departures_utc: s.stratum.departures_utc.clone(),
                budget_seconds: s.stratum.budget_seconds,
            },
            share: s.reachable_share,
        }
    }
    pub fn context(&self) -> &Context {
        &self.context
    }
    pub fn estimand(&self) -> &Estimand {
        &self.estimand
    }
    pub fn share(&self) -> Option<f64> {
        self.share
    }
}
