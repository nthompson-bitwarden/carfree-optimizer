use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Seek},
};

pub(super) type Row = BTreeMap<String, String>;

#[derive(Clone, Copy)]
enum CsvField {
    Start,
    Plain,
    Quoted,
    QuoteClosed,
}

// csv accepts truncated or misplaced quotes for interoperability. Reject those
// before accepting a candidate because the forgiving parse can change its data.
fn scan_csv(file: &File, label: &str) -> Result<Vec<u64>> {
    use CsvField::*;
    let mut reader = BufReader::new(file.take(MAX_INPUT_BYTES + 1));
    let mut state = Start;
    let mut length = 0_u64;
    let mut pending_cr = false;
    let mut inside_record = false;
    let mut line = 1_u64;
    let mut lines = Vec::new();
    loop {
        let buffer = reader
            .fill_buf()
            .map_err(|e| Error::new(format!("{label}: {e}")))?;
        if buffer.is_empty() {
            break;
        }
        let skip = if length == 0 && buffer.starts_with(&[0xef, 0xbb, 0xbf]) {
            3
        } else {
            0
        };
        for (offset, &byte) in buffer.iter().enumerate().skip(skip) {
            if pending_cr && byte != b'\n' {
                return Err(Error::new(format!(
                    "{label}: bare CR record terminator at byte {}; use LF or CRLF",
                    length + offset as u64
                )));
            }
            if matches!(byte, b'\r' | b'\n') {
                if !matches!(state, Quoted) {
                    inside_record = false;
                }
                line += u64::from(byte == b'\n');
            } else if !inside_record {
                lines.push(line);
                row_limit(lines.len().saturating_sub(1))?;
                inside_record = true;
            }
            pending_cr = byte == b'\r' && !matches!(state, Quoted);
            state = match (state, byte) {
                (Start | QuoteClosed, b'"') => Quoted,
                (Quoted, b'"') => QuoteClosed,
                (Quoted, _) => Quoted,
                (_, b',' | b'\r' | b'\n') => Start,
                (Start | Plain, byte) if byte != b'"' => Plain,
                _ => {
                    return Err(Error::new(format!(
                        "{label}: invalid CSV quoting at byte {}",
                        length + offset as u64 + 1
                    )))
                }
            };
        }
        let read = buffer.len();
        length += read as u64;
        reader.consume(read);
    }
    if length > MAX_INPUT_BYTES {
        return Err(Error::new(format!(
            "{label}: file grew beyond the size limit"
        )));
    }
    if pending_cr {
        return Err(Error::new(format!(
            "{label}: bare CR record terminator at end of input; use LF or CRLF"
        )));
    }
    if matches!(state, Quoted) {
        return Err(Error::new(format!(
            "{label}: unterminated quoted CSV field"
        )));
    }
    Ok(lines)
}

pub(super) fn csv_rows(
    root: &Path,
    path: &Path,
    required: &[&str],
    report: &mut Report,
    mut accept: impl FnMut(u64, Row) -> Result<()>,
) -> Result<()> {
    let label = path.to_string_lossy();
    let mut file = open(root, path)?;
    let lines = scan_csv(&file, &label)?;
    file.rewind()
        .map_err(|e| Error::new(format!("{label}: {e}")))?;
    let mut reader = csv::Reader::from_reader(file.take(MAX_INPUT_BYTES + 1));
    let headers = reader
        .headers()
        .map_err(|e| Error::new(format!("{label}: {e}")))?
        .clone();
    let unique: BTreeSet<_> = headers.iter().collect();
    if unique.len() != headers.len() || headers.is_empty() || unique.contains("") {
        return Err(Error::new(format!(
            "{label}: headers must be unique and nonempty"
        )));
    }
    for field in required {
        if !unique.contains(field) {
            return Err(Error::new(format!("{label}: missing column {field}")));
        }
    }
    let mut row_lines = lines.into_iter().skip(1);
    for record in reader.records() {
        let line = row_lines.next().ok_or_else(|| {
            Error::new(format!("{label}: CSV record count changed during parsing"))
        })?;
        match record {
            Ok(record) => {
                let position = record.position().unwrap();
                if position.byte() >= MAX_INPUT_BYTES {
                    return Err(Error::new(format!(
                        "{label}: file grew beyond the size limit"
                    )));
                }
                let row = headers
                    .iter()
                    .zip(record.iter())
                    .map(|(name, value)| (name.to_owned(), value.to_owned()))
                    .collect();
                if let Err(e) = accept(line, row) {
                    report.error(&label, Some(line), e.to_string());
                }
            }
            Err(e) => report.error(&label, Some(line), e.to_string()),
        }
    }
    if row_lines.next().is_some() {
        return Err(Error::new(format!(
            "{label}: CSV record count changed during parsing"
        )));
    }
    if reader.position().byte() > MAX_INPUT_BYTES {
        return Err(Error::new(format!(
            "{label}: file grew beyond the size limit"
        )));
    }
    Ok(())
}

pub(super) fn field<'a>(row: &'a Row, name: &str) -> Result<&'a str> {
    row.get(name)
        .map(String::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| Error::new(format!("missing value for {name}")))
}

pub(super) fn number<T: std::str::FromStr>(row: &Row, name: &str) -> Result<T> {
    field(row, name)?
        .parse()
        .map_err(|_| Error::new(format!("invalid number in {name}")))
}

pub(super) fn digits(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|byte| byte.is_ascii_digit())
}

pub(super) fn compact_date(value: &str) -> Result<()> {
    if !digits(value, 8) {
        return Err(Error::new("expected date in YYYYMMDD"));
    }
    date_index(&format!("{}-{}-{}", &value[..4], &value[4..6], &value[6..]))?;
    Ok(())
}

pub(super) fn optional_exists(root: &Path, path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(root.join(path)) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(Error::new(format!("{}: {e}", path.display()))),
    }
}
