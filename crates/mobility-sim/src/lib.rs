//! Scenario evaluation contract. A calibrated simulator must be implemented
//! before providing predictions or optimization rewards.

pub mod calibration;
pub mod demand;
pub mod simulation;

#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioMetrics {
    pub car_free_accessible_trip_share: f64,
    pub person_hours_travelled: f64,
    pub total_vehicle_km: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulationError {
    NotImplemented,
    InvalidScenario,
    MissingInputs,
}

pub trait ScenarioEvaluator {
    type Scenario;
    fn evaluate(&self, scenario: &Self::Scenario) -> Result<ScenarioMetrics, SimulationError>;
}
