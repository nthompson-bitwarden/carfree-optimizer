//! Optimization environment interfaces, independent of any RL framework.

pub mod catalog;
pub mod observation;
pub mod reference;
pub mod reward;
pub mod synthetic;
pub mod wire;

mod portfolio;

use interventions::Intervention;

pub trait PlanningEnvironment {
    type Observation;
    type Error;

    fn reset(&mut self, seed: u64) -> Result<Self::Observation, Self::Error>;
    fn valid_actions(&self) -> Vec<Intervention>;
    fn step(&mut self, action: Intervention) -> Result<Transition<Self::Observation>, Self::Error>;
}

#[derive(Debug, Clone)]
pub struct Transition<T> {
    pub observation: T,
    pub reward: f64,
    pub terminated: bool,
    pub truncated: bool,
}
