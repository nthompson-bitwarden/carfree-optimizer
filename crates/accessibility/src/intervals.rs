//! Explicit mode-interval attachment; never infers independence from expansion weights.
use crate::scoring::{Error, Mode, ShareMetric, TripScore, Uncertainty};
use std::collections::BTreeMap;
pub fn with_mode_intervals(
    mut score: TripScore,
    intervals: BTreeMap<Mode, Uncertainty>,
) -> Result<TripScore, Error> {
    for (mode, interval) in intervals {
        interval.binding_matches(
            &score.context,
            &ShareMetric::ServedModeAllTrips(mode.clone()),
            score.all.total,
            &score.inventory,
        )?;
        let point = (score.all.total > 0.0)
            .then(|| score.all.served.get(&mode).copied().unwrap_or(0.0) / score.all.total);
        interval.validate(point)?;
        score.0.served_mode_uncertainty.insert(mode, interval);
    }
    Ok(score)
}
