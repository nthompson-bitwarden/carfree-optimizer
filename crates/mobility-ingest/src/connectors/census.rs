use super::{tables::*, *};
use serde_json::json;
use std::collections::BTreeSet;

const GEOGRAPHY: [(&str, usize); 4] = [("state", 2), ("county", 3), ("tract", 6), ("block", 4)];

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CensusLevel {
    State,
    County,
    Tract,
    Block,
}

impl CensusLevel {
    fn parts(self) -> &'static [(&'static str, usize)] {
        let count = match self {
            Self::State => 1,
            Self::County => 2,
            Self::Tract => 3,
            Self::Block => 4,
        };
        &GEOGRAPHY[..count]
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LodesSegment {
    Main,
    Aux,
}

pub(super) fn population(
    root: &Path,
    path: &Path,
    vintage: u16,
    level: CensusLevel,
    report: &mut Report,
) -> Result<Value> {
    if vintage != 2020 {
        return Err(Error::new(
            "the decennial P1_001N adapter requires 2020 geography",
        ));
    }
    let input: Vec<Vec<String>> = serde_json::from_value(json_file(root, path)?)
        .map_err(|e| Error::new(format!("expected Census API array of string arrays: {e}")))?;
    let (headers, records) = input
        .split_first()
        .ok_or_else(|| Error::new("missing Census API header"))?;
    row_limit(records.len())?;
    if records.is_empty() || headers.iter().collect::<BTreeSet<_>>().len() != headers.len() {
        return Err(Error::new(
            "Census response requires unique columns and population rows",
        ));
    }
    let label = path.to_string_lossy();
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, record) in records.iter().enumerate() {
        let line = (index + 2) as u64;
        let parsed = (|| {
            if headers.len() != record.len() {
                return Err(Error::new("row length differs from Census header"));
            }
            let row: Row = headers
                .iter()
                .cloned()
                .zip(record.iter().cloned())
                .collect();
            let mut geoid = String::new();
            for &(key, length) in level.parts() {
                let value = field(&row, key)?;
                if !digits(value, length) {
                    return Err(Error::new(format!("{key} must have {length} digits")));
                }
                geoid.push_str(value);
            }
            let population = number::<u64>(&row, "P1_001N")?;
            if !seen.insert(geoid.clone()) {
                return Err(Error::new(format!("duplicate GEOID {geoid}")));
            }
            Ok(json!({"geoid":geoid, "population":population, "source_row":line}))
        })();
        match parsed {
            Ok(row) => rows.push(row),
            Err(e) => report.error(&label, Some(line), e.to_string()),
        }
    }
    Ok(json!({
        "product":"2020 Decennial Census P1_001N",
        "geography_vintage":vintage, "geography_level":level, "unit":"people",
        "limitations":"Published counts for supplied Census geographies only; privacy protection and enumeration error apply.",
        "rows":rows
    }))
}

pub(super) fn lodes(
    root: &Path,
    path: &Path,
    vintage: u16,
    year: u16,
    job_type: &str,
    segment: LodesSegment,
    report: &mut Report,
) -> Result<Value> {
    if !matches!(vintage, 2010 | 2020)
        || !(2002..=9999).contains(&year)
        || !matches!(
            job_type,
            "JT00" | "JT01" | "JT02" | "JT03" | "JT04" | "JT05"
        )
    {
        return Err(Error::new(
            "LODES needs a 2010/2020 block vintage, reference year and JT00–JT05 job type",
        ));
    }
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    csv_rows(
        root,
        path,
        &["w_geocode", "h_geocode", "S000", "createdate"],
        report,
        |line, row| {
            let work = field(&row, "w_geocode")?;
            let home = field(&row, "h_geocode")?;
            if !digits(work, 15) || !digits(home, 15) {
                return Err(Error::new("home and work GEOIDs must each have 15 digits"));
            }
            let across_state = work[..2] != home[..2];
            if across_state != matches!(segment, LodesSegment::Aux) {
                return Err(Error::new(
                    "block state prefixes disagree with the main/aux segment",
                ));
            }
            let jobs = number::<u64>(&row, "S000")?;
            let created = field(&row, "createdate")?;
            compact_date(created)?;
            if !seen.insert((home.to_owned(), work.to_owned())) {
                return Err(Error::new("duplicate home/work GEOID pair"));
            }
            rows.push(json!({"home_geoid":home,"work_geoid":work,"jobs":jobs,"created_on":created,"source_row":line}));
            Ok(())
        },
    )?;
    if rows.is_empty() && report.errors.is_empty() {
        return Err(Error::new("LODES input has no OD rows"));
    }
    Ok(json!({
        "product":"LEHD LODES OD", "geography_vintage":vintage, "geography_level":"block",
        "reference_year":year, "unit":"jobs", "job_type":job_type, "segment":segment,
        "limitations":"Published privacy-protected covered jobs only; file scope depends on job type and main/aux segment. Counts do not measure actual daily trips.",
        "rows":rows
    }))
}
