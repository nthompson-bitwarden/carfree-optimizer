use super::{places::feature_rows, tables::csv_rows, *};
use crate::study_area::CityProject;
use parquet::{
    basic::{ConvertedType, LogicalType, Type as PhysicalType},
    file::reader::{FileReader, SerializedFileReader},
    record::Field,
    schema::types::{ColumnDescriptor, Type},
};
use serde_json::{json, Map};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Seek,
};
use tempfile::NamedTempFile;

pub type Mapping = BTreeMap<String, FieldMapping>;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldMapping {
    pub column: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    Integer,
    Number,
    Boolean,
    Json,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadFormat {
    Csv,
    Geojson,
    Parquet,
}

fn mapped(fields: &Mapping, row: &Map<String, Value>, text_input: bool) -> Result<Value> {
    let mut normalized = Map::new();
    for (target, spec) in fields {
        let raw = row.get(&spec.column).unwrap_or(&Value::Null);
        let value = if raw.is_null() || (text_input && raw.as_str() == Some("")) {
            if spec.required {
                return Err(Error::new(format!(
                    "required field {target} is blank or missing (column {})",
                    spec.column
                )));
            }
            Value::Null
        } else if text_input {
            convert_text(raw.as_str().unwrap(), spec.field_type).ok_or_else(|| {
                Error::new(format!("field {target} must be {:?}", spec.field_type))
            })?
        } else {
            raw.clone()
        };
        if !valid_type(&value, spec) {
            return Err(Error::new(format!(
                "field {target} does not match {:?} or exceeds the JSON nesting limit",
                spec.field_type
            )));
        }
        normalized.insert(target.to_owned(), value);
    }
    Ok(Value::Object(normalized))
}

fn valid_type(value: &Value, spec: &FieldMapping) -> bool {
    if value.is_null() {
        return !spec.required;
    }
    match spec.field_type {
        FieldType::Text => value
            .as_str()
            .is_some_and(|text| !spec.required || !text.trim().is_empty()),
        FieldType::Integer => value.is_i64() || value.is_u64(),
        FieldType::Number => value.is_number(),
        FieldType::Boolean => value.is_boolean(),
        FieldType::Json => within_depth(value, 0),
    }
}

fn within_depth(value: &Value, depth: usize) -> bool {
    depth <= 32
        && match value {
            Value::Array(items) => items.iter().all(|item| within_depth(item, depth + 1)),
            Value::Object(fields) => fields.values().all(|item| within_depth(item, depth + 1)),
            _ => true,
        }
}

fn validate_mapping(fields: &Mapping) -> Result<()> {
    if fields.is_empty()
        || fields.len() > 128
        || fields.iter().any(|(name, spec)| {
            name.trim().is_empty()
                || spec.column.trim().is_empty()
                || name.chars().any(char::is_control)
        })
    {
        return Err(Error::new(
            "mapping needs 1–128 named fields with nonblank source columns",
        ));
    }
    Ok(())
}

fn convert_text(text: &str, kind: FieldType) -> Option<Value> {
    match kind {
        FieldType::Text if !text.trim().is_empty() => Some(json!(text)),
        FieldType::Text => None,
        FieldType::Integer => text
            .parse::<i64>()
            .map(|n| json!(n))
            .or_else(|_| text.parse::<u64>().map(|n| json!(n)))
            .ok(),
        FieldType::Number => {
            serde_json::Number::from_f64(text.parse::<f64>().ok()?).map(Value::Number)
        }
        FieldType::Boolean => text.parse::<bool>().ok().map(Value::Bool),
        FieldType::Json => serde_json::from_str(text).ok(),
    }
}

fn parquet_value(field: &Field) -> Result<Value> {
    let value = match field {
        Field::Null => Value::Null,
        Field::Bool(v) => json!(v),
        Field::Byte(v) => json!(v),
        Field::Short(v) => json!(v),
        Field::Int(v) => json!(v),
        Field::Long(v) => json!(v),
        Field::UByte(v) => json!(v),
        Field::UShort(v) => json!(v),
        Field::UInt(v) => json!(v),
        Field::ULong(v) => json!(v),
        Field::Float16(v) if v.is_finite() => json!(v.to_f32()),
        Field::Float(v) if v.is_finite() => json!(v),
        Field::Double(v) if v.is_finite() => json!(v),
        Field::Str(v) => json!(v),
        _ => return Err(Error::new(
            "unsupported Parquet value; map scalar text, integer, finite number or boolean columns",
        )),
    };
    Ok(value)
}

fn parquet_type(column: &ColumnDescriptor) -> Option<FieldType> {
    use ConvertedType as C;
    use PhysicalType as P;
    match (column.physical_type(), column.converted_type()) {
        (P::BOOLEAN, C::NONE) => Some(FieldType::Boolean),
        (
            P::INT32,
            C::NONE | C::INT_8 | C::INT_16 | C::INT_32 | C::UINT_8 | C::UINT_16 | C::UINT_32,
        )
        | (P::INT64, C::NONE | C::INT_64 | C::UINT_64) => Some(FieldType::Integer),
        (P::FLOAT | P::DOUBLE, C::NONE) => Some(FieldType::Number),
        (P::FIXED_LEN_BYTE_ARRAY, C::NONE)
            if column.logical_type_ref() == Some(&LogicalType::Float16) =>
        {
            Some(FieldType::Number)
        }
        (P::BYTE_ARRAY, C::UTF8 | C::ENUM | C::JSON) => Some(FieldType::Text),
        _ => None,
    }
}

fn parquet_rows(
    root: &Path,
    path: &Path,
    fields: &Mapping,
    selected: &BTreeSet<&str>,
    report: &mut Report,
    mut accept: impl FnMut(u64, Map<String, Value>) -> Result<()>,
) -> Result<()> {
    let label = path.to_string_lossy();
    let parquet_error = |e: parquet::errors::ParquetError| Error::new(format!("{label}: {e}"));
    let reader = SerializedFileReader::new(open(root, path)?).map_err(parquet_error)?;
    let metadata = reader.metadata();
    let schema = metadata.file_metadata().schema_descr();
    let mut columns = BTreeMap::new();
    for column in schema.columns() {
        if column.path().parts().len() != 1
            || column.max_rep_level() != 0
            || columns.insert(column.name(), column).is_some()
        {
            return Err(Error::new(
                "Parquet requires a flat schema with unique column names",
            ));
        }
    }
    for (target, field) in fields {
        let column = columns.get(field.column.as_str()).ok_or_else(|| {
            Error::new(format!("Parquet is missing mapped column {}", field.column))
        })?;
        let kind = parquet_type(column).ok_or_else(|| {
            Error::new(format!(
                "unsupported type for Parquet column {}",
                field.column
            ))
        })?;
        let compatible = field.field_type == kind
            || field.field_type == FieldType::Json
            || (field.field_type == FieldType::Number && kind == FieldType::Integer);
        if !compatible {
            return Err(Error::new(format!(
                "Parquet column {} is {kind:?}, incompatible with field {target} ({:?})",
                field.column, field.field_type
            )));
        }
    }
    let root_type = schema.root_schema();
    let projection = Type::group_type_builder(root_type.name())
        .with_fields(
            root_type
                .get_fields()
                .iter()
                .filter(|column| selected.contains(column.name()))
                .cloned()
                .collect(),
        )
        .build()
        .map_err(parquet_error)?;
    let count = usize::try_from(metadata.file_metadata().num_rows())
        .map_err(|_| Error::new("invalid Parquet row count"))?;
    row_limit(count)?;
    let inflated = metadata
        .row_groups()
        .iter()
        .flat_map(|group| group.columns())
        .try_fold(0_u64, |sum, column| {
            sum.checked_add(u64::try_from(column.uncompressed_size()).ok()?)
        });
    if inflated.is_none_or(|size| size > MAX_INPUT_BYTES) {
        return Err(Error::new(
            "Parquet declared uncompressed size exceeds the input limit",
        ));
    }
    let mut actual_count = 0;
    for (index, record) in reader
        .get_row_iter(Some(projection))
        .map_err(parquet_error)?
        .enumerate()
    {
        actual_count = index + 1;
        row_limit(actual_count)?;
        let line = actual_count as u64;
        let row = record.map_err(parquet_error)?;
        let mapped = row
            .get_column_iter()
            .filter(|(name, _)| selected.contains(name.as_str()))
            .map(|(name, value)| Ok((name.clone(), parquet_value(value)?)))
            .collect::<Result<Map<_, _>>>();
        if let Err(error) = mapped.and_then(|fields| accept(line, fields)) {
            report.error(&label, Some(line), error.to_string());
        }
    }
    if actual_count != count {
        return Err(Error::new(
            "decoded Parquet row count differs from file metadata",
        ));
    }
    Ok(())
}

pub(super) fn read(
    root: &Path,
    path: &Path,
    format: UploadFormat,
    fields: &Mapping,
    report: &mut Report,
) -> Result<Value> {
    validate_mapping(fields)?;
    let selected: BTreeSet<_> = fields.values().map(|field| field.column.as_str()).collect();
    let mut rows = Vec::new();
    let text_input = matches!(format, UploadFormat::Csv);
    let mut accept = |line, row: Map<String, Value>| -> Result<()> {
        rows.push(json!({"source_row":line, "fields":mapped(fields, &row, text_input)?}));
        Ok(())
    };
    match format {
        UploadFormat::Csv => csv_rows(
            root,
            path,
            &selected.iter().copied().collect::<Vec<_>>(),
            report,
            |line, row| {
                let values = row
                    .into_iter()
                    .filter(|(key, _)| selected.contains(key.as_str()))
                    .map(|(key, value)| (key, Value::String(value)))
                    .collect();
                accept(line, values)
            },
        )?,
        UploadFormat::Geojson => feature_rows(root, path, report, accept)?,
        UploadFormat::Parquet => parquet_rows(root, path, fields, &selected, report, accept)?,
    }
    if rows.is_empty() && report.errors.is_empty() {
        return Err(Error::new("upload contains no records"));
    }
    let mut data = json!({"format":format,"mapping":fields});
    data["rows"] = Value::Array(rows);
    Ok(data)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredUploadSchema {
    format: UploadFormat,
    mapping: Mapping,
}

fn validate_stored(candidate: &Candidate) -> Result<()> {
    let expected = InventoryEntry::new(
        &candidate.inventory.source,
        &candidate.inventory.assessed_on,
    )?;
    if expected.age_days != candidate.inventory.age_days || candidate.realtime_inventory.is_some() {
        return Err(Error::new("invalid upload source inventory"));
    }
    let data = candidate
        .data
        .as_object()
        .filter(|data| data.len() == 3)
        .ok_or_else(|| Error::new("stored upload requires format, mapping and rows"))?;
    let schema = StoredUploadSchema::deserialize(&json!({
        "format":data.get("format"), "mapping":data.get("mapping")
    }))
    .map_err(io_error)?;
    validate_mapping(&schema.mapping)?;
    let rows = data
        .get("rows")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty())
        .ok_or_else(|| Error::new("stored upload requires records"))?;
    row_limit(rows.len())?;
    let mut previous = if matches!(schema.format, UploadFormat::Csv) {
        1
    } else {
        0
    };
    for row in rows {
        let line = row["source_row"]
            .as_u64()
            .ok_or_else(|| Error::new("stored upload row is missing its source row number"))?;
        let fields = row["fields"]
            .as_object()
            .ok_or_else(|| Error::new("stored upload row is missing its fields"))?;
        if line <= previous
            || row
                .as_object()
                .ok_or_else(|| Error::new("stored upload row must be an object"))?
                .len()
                != 2
            || fields.len() != schema.mapping.len()
            || schema.mapping.iter().any(|(name, spec)| {
                fields
                    .get(name)
                    .is_none_or(|value| !valid_type(value, spec))
            })
        {
            return Err(Error::new(format!(
                "stored upload row {line} differs from its schema or order"
            )));
        }
        previous = line;
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectIdentity {
    project_id: String,
    baseline_id: String,
    snapshot_date: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Overlay {
    schema_version: u32,
    project: ProjectIdentity,
    candidate: Candidate,
}

fn io_error(error: impl fmt::Display) -> Error {
    Error::new(error.to_string())
}

fn read_project(file: &mut File) -> Result<(String, ProjectIdentity)> {
    const LIMIT: u64 = 16 * 1024 * 1024;
    file.rewind().map_err(io_error)?;
    let mut input = String::new();
    file.take(LIMIT + 1)
        .read_to_string(&mut input)
        .map_err(io_error)?;
    if input.len() as u64 > LIMIT {
        return Err(Error::new("project snapshot exceeds 16 MiB"));
    }
    let project = CityProject::from_tsv(&input)
        .map_err(|e| Error::new(format!("invalid E02 baseline project: {e}")))?;
    Ok((
        input,
        ProjectIdentity {
            project_id: project.id().into(),
            baseline_id: project.baseline_id().into(),
            snapshot_date: project.snapshot_date().into(),
        },
    ))
}

fn paths(project: &Path) -> Result<(PathBuf, PathBuf)> {
    let project = project.canonicalize().map_err(io_error)?;
    if !project.is_file() {
        return Err(Error::new("baseline path needs an existing project file"));
    }
    let mut name = project.file_name().unwrap().to_os_string();
    name.push(".upload.json");
    let overlay = project.with_file_name(name);
    Ok((project, overlay))
}

/// Validate the complete upload before atomically installing a new project sidecar.
/// Failed validation returns diagnostics and installs nothing. The E02 baseline stays read-only.
pub fn apply_upload(
    project: &Path,
    request: &Request,
    root: &Path,
    as_of: &str,
) -> Result<Candidate> {
    if !matches!(request.input, Input::Custom { .. }) {
        return Err(Error::new("apply only accepts a custom upload request"));
    }
    let candidate = inspect(request, root, as_of)?;
    if !candidate.is_valid() {
        return Ok(candidate);
    }
    let (project, target) = paths(project)?;
    let mut file = File::open(&project).map_err(io_error)?;
    file.lock().map_err(io_error)?;
    let (original, identity) = read_project(&mut file)?;
    #[derive(Serialize)]
    struct Staged<'a> {
        schema_version: u32,
        project: ProjectIdentity,
        candidate: &'a Candidate,
    }
    let envelope = Staged {
        schema_version: SCHEMA_VERSION,
        project: identity,
        candidate: &candidate,
    };
    let staged = NamedTempFile::new_in(project.parent().unwrap()).map_err(io_error)?;
    serde_json::to_writer(staged.as_file(), &envelope).map_err(io_error)?;
    if staged.as_file().metadata().map_err(io_error)?.len() > MAX_INPUT_BYTES {
        return Err(Error::new(
            "normalized upload exceeds the sidecar size limit",
        ));
    }
    staged.as_file().sync_all().map_err(io_error)?;
    drop(candidate);
    let candidate = read_overlay(staged.path())?.candidate;
    let (current, _) = read_project(&mut File::open(&project).map_err(io_error)?)?;
    if original != current {
        return Err(Error::new(
            "project changed during upload; preview and retry against the current baseline",
        ));
    }
    staged.persist_noclobber(&target).map_err(|error| {
        Error::new(format!(
            "{}: {} (roll back an existing upload before applying another)",
            target.display(),
            error.error
        ))
    })?;
    Ok(candidate)
}

fn read_overlay(target: &Path) -> Result<Overlay> {
    let metadata = fs::symlink_metadata(target).map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > MAX_INPUT_BYTES {
        return Err(Error::new(
            "upload sidecar must be a regular file within the size limit",
        ));
    }
    let overlay: Overlay = serde_json::from_reader(std::io::BufReader::new(
        File::open(target)
            .map_err(io_error)?
            .take(MAX_INPUT_BYTES + 1),
    ))
    .map_err(io_error)?;
    if overlay.schema_version != SCHEMA_VERSION
        || overlay.candidate.schema_version != SCHEMA_VERSION
        || overlay.candidate.kind != "custom"
        || !overlay.candidate.is_valid()
    {
        return Err(Error::new("unrecognized or invalid upload sidecar"));
    }
    validate_stored(&overlay.candidate)?;
    Ok(overlay)
}

/// Read the active upload only when it matches the current project baseline.
pub fn load_upload(project: &Path) -> Result<Option<Candidate>> {
    let (project, target) = paths(project)?;
    let mut file = File::open(&project).map_err(io_error)?;
    file.lock_shared().map_err(io_error)?;
    let (_, identity) = read_project(&mut file)?;
    match fs::symlink_metadata(&target) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(error)),
        Ok(_) => {
            let overlay = read_overlay(&target)?;
            if overlay.project != identity {
                return Err(Error::new(
                    "upload belongs to a different baseline; roll back before reuse",
                ));
            }
            Ok(Some(overlay.candidate))
        }
    }
}

/// Remove only a recognized sidecar for this project. A missing upload is a successful no-op.
pub fn rollback_upload(project: &Path) -> Result<bool> {
    let (project, target) = paths(project)?;
    let mut file = File::open(&project).map_err(io_error)?;
    file.lock().map_err(io_error)?;
    let (_, identity) = read_project(&mut file)?;
    match fs::symlink_metadata(&target) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(io_error(e)),
        Ok(_) => {}
    }
    if read_overlay(&target)?.project.project_id != identity.project_id {
        return Err(Error::new("upload sidecar belongs to a different project"));
    }
    fs::remove_file(&target).map_err(io_error)?;
    Ok(true)
}
