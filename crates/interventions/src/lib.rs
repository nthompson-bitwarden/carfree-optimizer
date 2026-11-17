//! Candidate infrastructure interventions. Physical feasibility and cost
//! validation must occur before applying an intervention to a city network.
use mobility_graph::EdgeId;

#[derive(Debug, Clone, PartialEq)]
pub enum Intervention {
    ProtectedBikeLane {
        corridor: Vec<EdgeId>,
    },
    PedestrianConnection {
        from: u32,
        to: u32,
    },
    DedicatedBusLane {
        corridor: Vec<EdgeId>,
    },
    TransitFrequencyIncrease {
        route_id: String,
        headway_minutes: f64,
    },
    LightRailCorridor {
        corridor: Vec<EdgeId>,
    },
    CarLaneConversion {
        corridor: Vec<EdgeId>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interventions_are_describable_without_mutating_a_graph() {
        let plan = Intervention::ProtectedBikeLane {
            corridor: vec![EdgeId(1), EdgeId(2)],
        };
        assert!(matches!(plan, Intervention::ProtectedBikeLane { .. }));
    }
}
