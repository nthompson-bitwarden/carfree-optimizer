use super::*;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;

/// Convert GTFS elapsed service seconds using local noon minus 12 elapsed hours.
/// Do not use civil midnight, or wrap values greater than 24 hours.
pub fn service_instant(zone: &str, date: NaiveDate, seconds: u32) -> Result<DateTime<Utc>> {
    let zone: Tz = zone
        .parse()
        .map_err(|_| Error::new("unknown IANA time zone"))?;
    let noon = zone
        .from_local_datetime(&date.and_hms_opt(12, 0, 0).expect("valid local clock time"))
        .single()
        .ok_or_else(|| Error::new("local service noon is missing or ambiguous"))?;
    noon.with_timezone(&Utc)
        .checked_add_signed(chrono::Duration::seconds(i64::from(seconds) - 43200))
        .ok_or_else(|| Error::new("service instant outside supported date range"))
}

pub(super) fn schedule(data: &Value, findings: &mut Vec<Finding>) -> Result<Option<Value>> {
    let schedule = &data["schedule"];
    let agencies = schedule["agency.txt"]
        .as_array()
        .ok_or_else(|| Error::new("GTFS connector lacks agencies"))?;
    let zones: BTreeSet<_> = agencies
        .iter()
        .filter_map(|a| a["fields"]["agency_timezone"].as_str())
        .collect();
    let zone = match zones.iter().copied().collect::<Vec<_>>().as_slice() {
        [zone] if zone.parse::<Tz>().is_ok() => *zone,
        _ => {
            findings.push(finding(
                "invalid_agency_timezone",
                "agency.txt",
                "GTFS requires one shared recognized IANA agency time zone",
                true,
            ));
            return Ok(None);
        }
    };
    let mut tables = BTreeMap::new();
    for (table, keys) in [
        ("stop_times.txt", ["arrival_time", "departure_time"]),
        ("frequencies.txt", ["start_time", "end_time"]),
    ] {
        let Some(rows) = schedule[table].as_array() else {
            continue;
        };
        let mut output = vec![];
        for row in rows {
            let mut times = BTreeMap::new();
            for field in keys {
                if let Some(s) = row["fields"][field].as_str().filter(|s| !s.is_empty()) {
                    times.insert(field, super::super::gtfs::service_time(s)?);
                }
            }
            output.push(json!({"source_row":row["source_row"], "trip_id":row["fields"]["trip_id"], "seconds":times}));
        }
        tables.insert(table, output);
    }
    Ok(Some(
        json!({"timezone":zone,"tzdb":chrono_tz::IANA_TZDB_VERSION,"origin":"local_noon_minus_12_elapsed_hours","tables":tables}),
    ))
}

pub(super) fn dot(data: &Value, findings: &mut Vec<Finding>) -> Result<Value> {
    let rows = data["rows"]
        .as_array()
        .ok_or_else(|| Error::new("DOT connector lacks observation rows"))?;
    let mut output = vec![];
    for row in rows {
        let timestamp = row["timestamp"]
            .as_str()
            .ok_or_else(|| Error::new("DOT row lacks timestamp"))?;
        let utc = DateTime::parse_from_rfc3339(timestamp)
            .map_err(|e| Error::new(e.to_string()))?
            .with_timezone(&Utc);
        let (value, unit) = if row["metric"] == "speed" {
            let value = row["value"]
                .as_f64()
                .ok_or_else(|| Error::new("DOT row lacks speed"))?;
            let si = match row["unit"].as_str() {
                Some("mph") => value * 0.44704,
                Some("km/h") => value / 3.6,
                Some("m/s") => value,
                _ => return Err(Error::new("unexpected DOT speed unit")),
            };
            if !(0.0..=55.0).contains(&si) {
                findings.push(finding(
                    "speed_review",
                    format!("row {}", row["source_row"]),
                    format!("observed speed {si} m/s is outside [0, 55]; observation retained"),
                    true,
                ));
            }
            (json!(si), "m/s")
        } else {
            (row["value"].clone(), "vehicles")
        };
        output.push(json!({"source_row":row["source_row"],"station_id":row["station_id"],"timestamp_utc":utc.to_rfc3339(),"value":value,"unit":unit}));
    }
    Ok(json!({"rows":output}))
}
