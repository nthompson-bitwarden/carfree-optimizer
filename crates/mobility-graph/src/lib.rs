//! Core types for a directed multimodal transportation network.
//! E05 adds a validated immutable representation in [`network`].

pub mod network;

/// Supported movement modes. Network and routing contracts keep them explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mode {
    Walk,
    Bicycle,
    ElectricBicycle,
    Bus,
    LightRail,
    Rail,
    Car,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    pub distance_m: f64,
}

impl Edge {
    pub fn new(
        id: EdgeId,
        from: NodeId,
        to: NodeId,
        distance_m: f64,
    ) -> Result<Self, &'static str> {
        if !distance_m.is_finite() || distance_m <= 0.0 {
            return Err("distance_m must be finite and positive");
        }
        Ok(Self {
            id,
            from,
            to,
            distance_m,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_edge_distance() {
        assert!(Edge::new(EdgeId(1), NodeId(0), NodeId(1), -1.0).is_err());
        assert!(Edge::new(EdgeId(1), NodeId(0), NodeId(1), f64::NAN).is_err());
    }
}
