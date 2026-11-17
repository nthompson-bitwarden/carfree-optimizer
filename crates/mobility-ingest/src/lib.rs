//! Canonical geography catalogue and source ingestion boundary.
pub mod geography;
pub use geography::{Catalogue, CatalogueError, Coordinate, Geography, GeographyKind, SearchError};

pub mod study_area;

pub mod connectors;
