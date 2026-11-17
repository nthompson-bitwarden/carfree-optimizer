//! Offline E03 ingestion. A candidate with validation errors must not be applied.
mod census;
mod gtfs;
mod observations;
mod osm;
mod places;
mod tables;
mod upload;

pub use census::{CensusLevel, LodesSegment};
pub use upload::{
    apply_upload, load_upload, rollback_upload, FieldMapping, FieldType, Mapping, UploadFormat,
};

use crate::study_area::validate_date;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fmt,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub const SCHEMA_VERSION: u32 = 1;
const MAX_INPUT_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ROWS: usize = 500_000;
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema_version: u32,
    pub source: Source,
    pub input: Input,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub version: String,
    pub uri: String,
    pub attribution: String,
    pub license_terms: String,
    pub retrieved_on: String,
    pub geography: String,
    pub coverage: String,
    pub review: LicenseReview,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LicenseReview {
    Pending,
    RedistributionApproved,
    Restricted,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    Osm {
        path: PathBuf,
    },
    Gtfs {
        directory: PathBuf,
        realtime: Option<RealtimeInput>,
    },
    CensusPopulation {
        path: PathBuf,
        geography_vintage: u16,
        level: CensusLevel,
    },
    Lodes {
        path: PathBuf,
        geography_vintage: u16,
        reference_year: u16,
        job_type: String,
        segment: LodesSegment,
    },
    Places {
        path: PathBuf,
    },
    Dot {
        path: PathBuf,
        columns: observations::DotColumns,
    },
    Crashes {
        path: PathBuf,
        columns: observations::CrashColumns,
    },
    Emissions {
        path: PathBuf,
        columns: observations::EmissionColumns,
    },
    Custom {
        path: PathBuf,
        format: UploadFormat,
        fields: Mapping,
    },
}

pub use observations::{CrashColumns, DotColumns, EmissionColumns, SeverityCodes};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RealtimeInput {
    pub path: PathBuf,
    pub source: Source,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InventoryEntry {
    pub source: Source,
    pub assessed_on: String,
    pub age_days: u32,
}

impl InventoryEntry {
    fn new(source: &Source, as_of: &str) -> Result<Self> {
        for (name, value) in [
            ("id", &source.id),
            ("version", &source.version),
            ("uri", &source.uri),
            ("attribution", &source.attribution),
            ("license_terms", &source.license_terms),
            ("geography", &source.geography),
            ("coverage", &source.coverage),
        ] {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(Error::new(format!(
                    "source {name} requires nonempty text without control characters"
                )));
            }
        }
        let age_days = date_index(as_of)?
            .checked_sub(date_index(&source.retrieved_on)?)
            .ok_or_else(|| Error::new("source retrieval date is after the assessment date"))?;
        Ok(Self {
            source: source.clone(),
            assessed_on: as_of.to_owned(),
            age_days,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Diagnostic {
    pub input: String,
    pub row: Option<u64>,
    pub message: String,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Report {
    pub errors: Vec<Diagnostic>,
    pub omitted_errors: usize,
    pub warnings: Vec<Diagnostic>,
}

impl Report {
    fn error(&mut self, input: &str, row: Option<u64>, message: impl Into<String>) {
        if self.errors.len() == 100 {
            self.omitted_errors += 1;
            return;
        }
        self.errors.push(Diagnostic {
            input: input.into(),
            row,
            message: message.into(),
        });
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub schema_version: u32,
    pub kind: String,
    pub inventory: InventoryEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realtime_inventory: Option<InventoryEntry>,
    pub report: Report,
    pub data: Value,
}

impl Candidate {
    pub fn is_valid(&self) -> bool {
        self.report.errors.is_empty() && self.report.omitted_errors == 0
    }
}

/// Parse and validate every record. Paths are relative to the supplied directory.
/// An invalid candidate retains diagnostics, with its data set to null.
pub fn inspect(request: &Request, root: &Path, as_of: &str) -> Result<Candidate> {
    let (inventory, realtime_inventory) = request_inventory(request, as_of)?;
    let mut report = Report::default();
    let (kind, result) = match &request.input {
        Input::Osm { path } => ("osm", osm::read(root, path, &mut report)),
        Input::Gtfs {
            directory,
            realtime,
        } => (
            "gtfs",
            gtfs::read(root, directory, realtime.as_ref(), &mut report),
        ),
        Input::CensusPopulation {
            path,
            geography_vintage,
            level,
        } => (
            "census_population",
            census::population(root, path, *geography_vintage, *level, &mut report),
        ),
        Input::Lodes {
            path,
            geography_vintage,
            reference_year,
            job_type,
            segment,
        } => (
            "lodes",
            census::lodes(
                root,
                path,
                *geography_vintage,
                *reference_year,
                job_type,
                *segment,
                &mut report,
            ),
        ),
        Input::Places { path } => ("places", places::read(root, path, &mut report)),
        Input::Dot { path, columns } => {
            ("dot", observations::dot(root, path, columns, &mut report))
        }
        Input::Crashes { path, columns } => (
            "crashes",
            observations::crashes(root, path, columns, &mut report),
        ),
        Input::Emissions { path, columns } => (
            "emissions",
            observations::emissions(root, path, columns, &mut report),
        ),
        Input::Custom {
            path,
            format,
            fields,
        } => (
            "custom",
            upload::read(root, path, *format, fields, &mut report),
        ),
    };
    let data = match result {
        Ok(data) if report.errors.is_empty() => data,
        Ok(_) => Value::Null,
        Err(error) => {
            report.error(kind, None, error.to_string());
            Value::Null
        }
    };
    Ok(Candidate {
        schema_version: SCHEMA_VERSION,
        kind: kind.into(),
        inventory,
        realtime_inventory,
        report,
        data,
    })
}

fn request_inventory(
    request: &Request,
    as_of: &str,
) -> Result<(InventoryEntry, Option<InventoryEntry>)> {
    if request.schema_version != SCHEMA_VERSION {
        return Err(Error::new(format!(
            "unsupported request schema {}",
            request.schema_version
        )));
    }
    let inventory = InventoryEntry::new(&request.source, as_of)?;
    let realtime_inventory = match &request.input {
        Input::Gtfs {
            realtime: Some(rt), ..
        } => {
            if rt.source.id == request.source.id {
                return Err(Error::new(
                    "realtime and schedule sources require distinct IDs",
                ));
            }
            Some(InventoryEntry::new(&rt.source, as_of)?)
        }
        _ => None,
    };
    Ok((inventory, realtime_inventory))
}

/// List source versions in ID/version order, with one entry per identical source.
/// Conflicting metadata for the same identity is rejected.
pub fn inventory(requests: &[Request], as_of: &str) -> Result<Vec<InventoryEntry>> {
    date_index(as_of)?;
    let mut entries: BTreeMap<_, InventoryEntry> = BTreeMap::new();
    for request in requests {
        let (source, realtime) = request_inventory(request, as_of)?;
        for item in std::iter::once(source).chain(realtime) {
            let key = (item.source.id.clone(), item.source.version.clone());
            match entries.entry(key) {
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(item);
                }
                std::collections::btree_map::Entry::Occupied(slot)
                    if slot.get().source != item.source =>
                {
                    return Err(Error::new(format!(
                        "conflicting inventory entries for {} version {}",
                        item.source.id, item.source.version
                    )));
                }
                _ => {}
            }
        }
    }
    Ok(entries.into_values().collect())
}

fn date_index(value: &str) -> Result<u32> {
    validate_date(value).map_err(|e| Error::new(e.to_string()))?;
    let year: u32 = value[..4].parse().unwrap();
    let month: usize = value[5..7].parse().unwrap();
    let day: u32 = value[8..].parse().unwrap();
    let prev = year - 1;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let month_offsets = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    Ok(prev * 365 + prev / 4 - prev / 100
        + prev / 400
        + month_offsets[month - 1]
        + u32::from(leap && month > 2)
        + day)
}

fn resolved(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(Error::new(format!(
            "{} must be a relative path without traversal",
            relative.display()
        )));
    }
    let root = root
        .canonicalize()
        .map_err(|e| Error::new(format!("{}: {e}", root.display())))?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| Error::new(format!("{}: {e}", relative.display())))?;
    if !path.starts_with(root) {
        return Err(Error::new(format!(
            "{} resolves outside the source directory",
            relative.display()
        )));
    }
    Ok(path)
}

fn open(root: &Path, relative: &Path) -> Result<File> {
    let path = resolved(root, relative)?;
    if !path.is_file() {
        return Err(Error::new(format!(
            "{} is not a regular file",
            relative.display()
        )));
    }
    let file = File::open(path).map_err(|e| Error::new(format!("{}: {e}", relative.display())))?;
    let size = file
        .metadata()
        .map_err(|e| Error::new(e.to_string()))?
        .len();
    if size > MAX_INPUT_BYTES {
        return Err(Error::new(format!(
            "{} exceeds the {MAX_INPUT_BYTES}-byte limit",
            relative.display()
        )));
    }
    Ok(file)
}

fn bytes(root: &Path, path: &Path) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    open(root, path)?
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut output)
        .map_err(|e| Error::new(format!("{}: {e}", path.display())))?;
    if output.len() as u64 > MAX_INPUT_BYTES {
        return Err(Error::new(format!(
            "{} exceeds the {MAX_INPUT_BYTES}-byte limit",
            path.display()
        )));
    }
    Ok(output)
}

fn json_file(root: &Path, path: &Path) -> Result<Value> {
    serde_json::from_slice(&bytes(root, path)?)
        .map_err(|e| Error::new(format!("{}: {e}", path.display())))
}

fn row_limit(count: usize) -> Result<()> {
    if count > MAX_ROWS {
        return Err(Error::new(format!(
            "input exceeds the {MAX_ROWS}-record limit"
        )));
    }
    Ok(())
}

fn coordinates(lon: f64, lat: f64) -> Result<[f64; 2]> {
    if !(-180.0..=180.0).contains(&lon) || !(-90.0..=90.0).contains(&lat) {
        return Err(Error::new("longitude/latitude outside WGS84 bounds"));
    }
    Ok([lon, lat])
}
