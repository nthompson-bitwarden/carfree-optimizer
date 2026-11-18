//! Application presentation boundary; no HTTP service or scientific model.
pub mod calibration;
pub mod geography;
pub use geography::render_search_results;

pub mod quality;
pub mod study_area;
