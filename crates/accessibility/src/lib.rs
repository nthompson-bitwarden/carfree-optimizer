//! Deterministic E13 scoring over explicit caller-supplied evidence.
//! Synthetic reference scores never establish real-world car independence.
pub mod comparison;
pub mod demand_binding;
pub mod footprints;
pub mod intervals;
pub mod projection;
pub mod scoring;
pub mod spatial;
pub mod upstream;

/// Fraction of destinations reachable within a travel-time threshold.
/// Counts are illustrative inputs; the caller must provide valid observations.
pub fn reachable_share(reachable: usize, total: usize) -> Option<f64> {
    if total == 0 || reachable > total {
        None
    } else {
        Some(reachable as f64 / total as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_valid_share() {
        assert_eq!(reachable_share(3, 4), Some(0.75));
    }

    #[test]
    fn rejects_missing_or_inconsistent_denominator() {
        assert_eq!(reachable_share(0, 0), None);
        assert_eq!(reachable_share(5, 4), None);
    }
}
