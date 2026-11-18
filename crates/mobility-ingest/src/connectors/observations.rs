use super::{tables::*, *};
use serde_json::json;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DotColumns {
    pub station: String,
    pub longitude: String,
    pub latitude: String,
    pub timestamp: String,
    pub duration_seconds: String,
    pub metric: String,
    pub value: String,
    pub unit: String,
    pub quality: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrashColumns {
    pub event_id: String,
    pub longitude: String,
    pub latitude: String,
    pub date: String,
    pub severity: String,
    pub uncertainty: String,
    pub severity_codes: Option<SeverityCodes>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SeverityCodes {
    pub fatal: BTreeSet<String>,
    pub nonfatal: BTreeSet<String>,
    pub unknown: BTreeSet<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EmissionColumns {
    pub area_id: String,
    pub date: String,
    pub pollutant: String,
    pub value: String,
    pub unit: String,
    pub basis: String,
    pub uncertainty: String,
}

fn scalar(row: &Row, field_name: &str) -> Result<f64> {
    let value = number::<f64>(row, field_name)?;
    if !value.is_finite() || value < 0.0 {
        return Err(Error::new(format!(
            "{field_name} must be finite and nonnegative"
        )));
    }
    Ok(value)
}

fn observations(
    root: &Path,
    path: &Path,
    columns: &[&String],
    report: &mut Report,
    mut normalize: impl FnMut(u64, Row) -> Result<Value>,
) -> Result<Value> {
    let required: Vec<_> = columns.iter().map(|column| column.as_str()).collect();
    if required.iter().collect::<BTreeSet<_>>().len() != required.len() {
        return Err(Error::new("mapped columns must be distinct"));
    }
    let mut rows = Vec::new();
    csv_rows(root, path, &required, report, |line, row| {
        rows.push(normalize(line, row)?);
        Ok(())
    })?;
    if rows.is_empty() && report.errors.is_empty() {
        return Err(Error::new("input has no observations"));
    }
    Ok(json!({"rows":rows}))
}

pub(super) fn dot(root: &Path, path: &Path, c: &DotColumns, report: &mut Report) -> Result<Value> {
    let columns = [
        &c.station,
        &c.longitude,
        &c.latitude,
        &c.timestamp,
        &c.duration_seconds,
        &c.metric,
        &c.value,
        &c.unit,
        &c.quality,
    ];
    observations(root, path, &columns, report, |line, row| {
        let location = coordinates(number(&row, &c.longitude)?, number(&row, &c.latitude)?)?;
        let timestamp = field(&row, &c.timestamp)?;
        chrono::DateTime::parse_from_rfc3339(timestamp)
            .map_err(|_| Error::new("timestamp must include an RFC 3339 offset"))?;
        if timestamp.ends_with("-00:00") {
            return Err(Error::new("timestamp must specify a known UTC offset"));
        }
        let duration = number::<u32>(&row, &c.duration_seconds)?;
        if duration == 0 {
            return Err(Error::new("observation interval must be positive"));
        }
        let metric = field(&row, &c.metric)?;
        let value = field(&row, &c.value)?;
        let unit = field(&row, &c.unit)?;
        let value = match (metric, unit) {
            ("count", "vehicles") => json!(value
                .parse::<u64>()
                .map_err(|_| Error::new("vehicle count must be a nonnegative integer"))?),
            ("speed", "mph" | "km/h" | "m/s") => json!(scalar(&row, &c.value)?),
            _ => {
                return Err(Error::new(
                    "expected count in vehicles or speed in mph, km/h or m/s",
                ))
            }
        };
        Ok(json!({
            "source_row":line, "station_id":field(&row, &c.station)?, "coordinates":location,
            "timestamp":timestamp, "duration_seconds":duration, "metric":metric,
            "value":value, "unit":unit, "quality":field(&row, &c.quality)?,
            "basis":"observed"
        }))
    })
}

impl SeverityCodes {
    fn validate(&self) -> Result<()> {
        if !self.fatal.is_disjoint(&self.nonfatal)
            || !self.fatal.is_disjoint(&self.unknown)
            || !self.nonfatal.is_disjoint(&self.unknown)
        {
            return Err(Error::new("severity code groups must be disjoint"));
        }
        Ok(())
    }

    fn classify(&self, value: &str) -> Result<&'static str> {
        if self.fatal.contains(value) {
            Ok("fatal")
        } else if self.nonfatal.contains(value) {
            Ok("nonfatal")
        } else if self.unknown.contains(value) {
            Ok("unknown")
        } else {
            Err(Error::new(format!("unmapped crash severity {value:?}")))
        }
    }
}

pub(super) fn crashes(
    root: &Path,
    path: &Path,
    c: &CrashColumns,
    report: &mut Report,
) -> Result<Value> {
    if let Some(codes) = &c.severity_codes {
        codes.validate()?;
    }
    let columns = [
        &c.event_id,
        &c.longitude,
        &c.latitude,
        &c.date,
        &c.severity,
        &c.uncertainty,
    ];
    let mut ids = BTreeSet::new();
    observations(root, path, &columns, report, |line, row| {
        let coordinates = coordinates(number(&row, &c.longitude)?, number(&row, &c.latitude)?)?;
        let date = field(&row, &c.date)?;
        date_index(date)?;
        let event_id = field(&row, &c.event_id)?;
        if !ids.insert(event_id.to_owned()) {
            return Err(Error::new(format!("duplicate crash event {event_id}")));
        }
        let original = field(&row, &c.severity)?;
        let severity = match &c.severity_codes {
            Some(codes) => codes.classify(original)?,
            None if matches!(original, "fatal" | "nonfatal" | "unknown") => original,
            None => {
                return Err(Error::new(
                    "crash severity requires fatal, nonfatal or unknown, or a code map",
                ))
            }
        };
        Ok(json!({
            "source_row":line, "event_id":event_id, "coordinates":coordinates, "date":date,
            "severity":severity, "source_severity":original, "basis":"observed",
            "uncertainty":field(&row, &c.uncertainty)?
        }))
    })
}

pub(super) fn emissions(
    root: &Path,
    path: &Path,
    c: &EmissionColumns,
    report: &mut Report,
) -> Result<Value> {
    let columns = [
        &c.area_id,
        &c.date,
        &c.pollutant,
        &c.value,
        &c.unit,
        &c.basis,
        &c.uncertainty,
    ];
    observations(root, path, &columns, report, |line, row| {
        let date = field(&row, &c.date)?;
        date_index(date)?;
        let basis = field(&row, &c.basis)?;
        if !matches!(basis, "observed" | "modeled" | "imputed") {
            return Err(Error::new(
                "emission basis must be observed, modeled or imputed",
            ));
        }
        Ok(json!({
            "source_row":line, "area_id":field(&row, &c.area_id)?, "date":date,
            "pollutant":field(&row, &c.pollutant)?, "value":scalar(&row, &c.value)?,
            "unit":field(&row, &c.unit)?, "basis":basis,
            "uncertainty":field(&row, &c.uncertainty)?
        }))
    })
}
