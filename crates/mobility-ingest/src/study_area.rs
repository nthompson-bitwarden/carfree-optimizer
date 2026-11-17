//! E02 study-area onboarding. Geometry is degrees; envelope distances are km.
mod geometry;
mod project;
use crate::{Catalogue, Coordinate, Geography};
pub use project::*;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardingError(pub String);
impl fmt::Display for OnboardingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for OnboardingError {}
fn err(s: impl Into<String>) -> OnboardingError {
    OnboardingError(s.into())
}
fn text(s: &str) -> Result<(), OnboardingError> {
    if s.trim().is_empty() || s.chars().any(char::is_control) {
        Err(err(
            "metadata must be nonempty and contain no control characters",
        ))
    } else {
        Ok(())
    }
}
pub fn validate_project_id(s: &str) -> Result<(), OnboardingError> {
    if s.is_empty()
        || s.len() > 128
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        Err(err(
            "project/scenario ID must contain 1–128 ASCII letters, digits, hyphens or underscores",
        ))
    } else {
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub country: String,
    pub states: Vec<String>,
}
#[derive(Debug, Clone, PartialEq)]
struct Source {
    url: String,
    version: String,
    retrieved: String,
    license: String,
}
impl Source {
    fn validate(&self) -> Result<(), OnboardingError> {
        for field in [&self.url, &self.version, &self.retrieved, &self.license] {
            text(field)?;
        }
        if self.version.len() != 4
            || !self.version.bytes().all(|b| b.is_ascii_digit())
            || self.version == "0000"
        {
            return Err(err("source vintage must be a four-digit nonzero year"));
        }
        validate_date(&self.retrieved)?;
        let host = self
            .url
            .strip_prefix("https://")
            .and_then(|s| s.split('/').next());
        if host.is_none_or(str::is_empty) || self.url.chars().any(char::is_whitespace) {
            return Err(err(
                "source URL requires https and a nonempty host without whitespace",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct StudyArea {
    id: String,
    name: String,
    region: Region,
    crs: String,
    rings: Vec<Vec<Coordinate>>,
    core_rings: Vec<Vec<Coordinate>>,
    buffer_km: f64,
    source: Option<Source>,
}
impl StudyArea {
    pub fn drawn(
        id: &str,
        name: &str,
        region: Region,
        crs: &str,
        rings: Vec<Vec<Coordinate>>,
    ) -> Result<Self, OnboardingError> {
        text(id)?;
        if id.contains(['/', '\\']) || id == "." || id == ".." {
            return Err(err("invalid study-area ID"));
        }
        text(name)?;
        if region.country != "US"
            || region.states.is_empty()
            || region
                .states
                .iter()
                .any(|s| s.len() != 2 || !s.bytes().all(|b| b.is_ascii_uppercase()))
        {
            return Err(err(
                "region requires country US and uppercase two-letter states",
            ));
        }
        let mut states = region.states.clone();
        states.sort();
        states.dedup();
        if states.len() != region.states.len() {
            return Err(err("duplicate region state"));
        }
        if crs != "EPSG:4326" {
            return Err(err(
                "only EPSG:4326 longitude/latitude degrees is supported",
            ));
        }
        geometry::validate(&rings)?;
        Ok(Self {
            id: id.into(),
            name: name.into(),
            region,
            crs: crs.into(),
            core_rings: rings.clone(),
            rings,
            buffer_km: 0.0,
            source: None,
        })
    }
    pub fn from_geography(g: &Geography) -> Result<Self, OnboardingError> {
        let mut a = Self::drawn(
            &g.id,
            &g.name,
            Region {
                country: "US".into(),
                states: g.states.clone(),
            },
            "EPSG:4326",
            g.rings.clone(),
        )?;
        let source = Source {
            url: g.source_url.clone(),
            version: g.vintage.clone(),
            retrieved: g.retrieved_date.clone(),
            license: g.license.clone(),
        };
        source.validate()?;
        a.source = Some(source);
        Ok(a)
    }
    pub fn buffer_envelope(&self, km: f64) -> Result<Self, OnboardingError> {
        if !km.is_finite() || km < 0.0 {
            return Err(err(
                "buffer must be a finite nonnegative number of kilometres",
            ));
        }
        let mut a = self.clone();
        a.buffer_km = km;
        if km == 0.0 {
            a.rings = a.core_rings.clone();
            return Ok(a);
        }
        let [w, s, e, n] = geometry::extent(&self.core_rings);
        let delta = km / 6371.0088;
        let lat = delta.to_degrees();
        if s - lat <= -90.0 || n + lat >= 90.0 {
            return Err(err(
                "buffer reaches unsupported polar latitude (90 degrees)",
            ));
        }
        // Longitude of a spherical cap at the most poleward core latitude.
        let ratio = delta.sin() / s.abs().max(n.abs()).to_radians().cos();
        if delta >= std::f64::consts::FRAC_PI_2 || ratio.abs() >= 1.0 {
            return Err(err("buffer too large for spherical envelope"));
        }
        let lon = ratio.asin().to_degrees();
        if w - lon <= -180.0 || e + lon >= 180.0 {
            return Err(err("buffer crosses unsupported antimeridian"));
        }
        a.rings = vec![vec![
            Coordinate {
                longitude: w - lon,
                latitude: s - lat,
            },
            Coordinate {
                longitude: e + lon,
                latitude: s - lat,
            },
            Coordinate {
                longitude: e + lon,
                latitude: n + lat,
            },
            Coordinate {
                longitude: w - lon,
                latitude: n + lat,
            },
            Coordinate {
                longitude: w - lon,
                latitude: s - lat,
            },
        ]];
        geometry::validate(&a.rings)?;
        Ok(a)
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn region(&self) -> &Region {
        &self.region
    }
    pub fn crs(&self) -> &str {
        &self.crs
    }
    pub fn rings(&self) -> &[Vec<Coordinate>] {
        &self.rings
    }
    pub fn core_rings(&self) -> &[Vec<Coordinate>] {
        &self.core_rings
    }
    pub fn buffer_km(&self) -> f64 {
        self.buffer_km
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetKind {
    Boundary,
    Roads,
    Transit,
    Population,
    Demand,
}
impl DatasetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Boundary => "Boundary",
            Self::Roads => "Roads",
            Self::Transit => "Transit",
            Self::Population => "Population",
            Self::Demand => "Demand",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetStatus {
    Available,
    Missing,
    Outdated,
    Partial,
    Failed,
}
impl DatasetStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "Available",
            Self::Missing => "Missing",
            Self::Outdated => "Outdated",
            Self::Partial => "Partial",
            Self::Failed => "Failed",
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct CoverageEntry {
    pub kind: DatasetKind,
    pub source: Option<String>,
    pub version: Option<String>,
    pub year: Option<u16>,
    pub extent: Option<[f64; 4]>,
    pub status: DatasetStatus,
    pub detail: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CoverageReport {
    pub entries: Vec<CoverageEntry>,
    pub missing_required: Vec<DatasetKind>,
}
pub trait Importer {
    fn ingest(
        &mut self,
        area: &StudyArea,
        snapshot_date: &str,
    ) -> Result<CoverageReport, OnboardingError>;
}
pub struct CatalogueImporter<'a> {
    catalogue: &'a Catalogue,
}
impl<'a> CatalogueImporter<'a> {
    pub fn new(catalogue: &'a Catalogue) -> Self {
        Self { catalogue }
    }
}
impl Importer for CatalogueImporter<'_> {
    fn ingest(&mut self, area: &StudyArea, date: &str) -> Result<CoverageReport, OnboardingError> {
        validate_date(date)?;
        let mut entries = Vec::new();
        // Revalidate the catalogue source on every ingestion. No transport download is implied.
        let boundary = if area.source.is_some() {
            let g = self
                .catalogue
                .get(area.id())
                .ok_or_else(|| err("original boundary source is unavailable in this catalogue"))?;
            let source_area = StudyArea::from_geography(g)?;
            if area.source != source_area.source
                || area.core_rings != source_area.core_rings
                || area.name != source_area.name
                || area.region != source_area.region
                || area.crs != source_area.crs
            {
                return Err(err("original boundary geometry or provenance differs from the current catalogue; select an explicit new source version"));
            }
            let ex = geometry::extent(source_area.rings());
            let year = g
                .vintage
                .parse::<u16>()
                .map_err(|_| err("source vintage must be a year"))?;
            let status = if year > date[..4].parse::<u16>().map_err(|_| err("invalid year"))? {
                DatasetStatus::Outdated
            } else if area.rings == source_area.rings {
                DatasetStatus::Available
            } else {
                DatasetStatus::Partial
            };
            CoverageEntry{kind:DatasetKind::Boundary,source:Some(g.source_url.clone()),version:Some(g.vintage.clone()),year:Some(year),extent:Some(ex),status,detail:format!("Generalized Census boundary; retrieved {}; {}. Coverage is for the source geography only; transport adapters are absent.",g.retrieved_date,g.license)}
        } else {
            CoverageEntry {
                kind: DatasetKind::Boundary,
                source: Some("User drawn boundary".into()),
                version: Some("drawn-v1".into()),
                year: Some(date[..4].parse().map_err(|_| err("invalid year"))?),
                extent: Some(geometry::extent(area.rings())),
                status: DatasetStatus::Available,
                detail: "User-supplied geometry, not an observed transportation input".into(),
            }
        };
        entries.push(boundary);
        for kind in [
            DatasetKind::Roads,
            DatasetKind::Transit,
            DatasetKind::Population,
            DatasetKind::Demand,
        ] {
            entries.push(CoverageEntry {
                kind,
                source: None,
                version: None,
                year: None,
                extent: None,
                status: DatasetStatus::Missing,
                detail: "No source imported; required transportation/model input is missing".into(),
            })
        }
        let missing_required = entries
            .iter()
            .filter(|e| e.status != DatasetStatus::Available)
            .map(|e| e.kind)
            .collect();
        Ok(CoverageReport {
            entries,
            missing_required,
        })
    }
}
