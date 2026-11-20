//! E14-S01: scalar transport projection for bounded batch callers.

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct WireTransition {
    pub version: u32,
    pub state: u64,
    pub observation: Vec<f64>,
    pub action_mask: Vec<bool>,
    pub reward: f64,
    pub terminated: bool,
    pub truncated: bool,
}

pub trait BatchEnvironment {
    fn reset(&mut self, seed: u64) -> Result<WireTransition, String>;
    fn step(&mut self, action: usize, expected_state: u64) -> Result<WireTransition, String>;
}
