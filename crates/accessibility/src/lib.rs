//! Car-free accessibility metric primitives. No demand or routing model yet.

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
