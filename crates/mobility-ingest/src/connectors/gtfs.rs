use super::{tables::*, *};
use prost::Message;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

type Ids = BTreeSet<String>;
type Stops = BTreeMap<String, u8>;
const DAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

struct Feed<'a> {
    root: &'a Path,
    directory: &'a Path,
    report: &'a mut Report,
    tables: BTreeMap<&'static str, Vec<Value>>,
}

impl Feed<'_> {
    fn table(
        &mut self,
        name: &'static str,
        required: bool,
        columns: &[&str],
        mut validate: impl FnMut(&Row) -> Result<()>,
    ) -> Result<()> {
        let path = self.directory.join(name);
        if !required && !optional_exists(self.root, &path)? {
            return Ok(());
        }
        let mut rows = Vec::new();
        let count_before = self.report.errors.len();
        let omitted_before = self.report.omitted_errors;
        let result = csv_rows(self.root, &path, columns, self.report, |line, row| {
            validate(&row)?;
            rows.push(json!({"source_row":line,"fields":row}));
            Ok(())
        });
        if let Err(e) = result {
            self.report
                .error(&path.to_string_lossy(), None, e.to_string());
        } else if required
            && rows.is_empty()
            && self.report.errors.len() == count_before
            && self.report.omitted_errors == omitted_before
        {
            self.report.error(
                &path.to_string_lossy(),
                None,
                "required GTFS table contains no records",
            );
        }
        self.tables.insert(name, rows);
        Ok(())
    }
}

fn unique(ids: &mut Ids, value: &str, field: &str) -> Result<()> {
    if !ids.insert(value.to_owned()) {
        return Err(Error::new(format!("duplicate {field} {value}")));
    }
    Ok(())
}

fn reference(ids: &Ids, value: &str, field: &str) -> Result<()> {
    if !ids.contains(value) {
        return Err(Error::new(format!("{field} refers to unknown ID {value}")));
    }
    Ok(())
}

fn optional(row: &Row, field: &str) -> bool {
    row.get(field).is_some_and(|value| !value.is_empty())
}

fn service_time(value: &str) -> Result<u32> {
    let (hour, rest) = value
        .split_once(':')
        .ok_or_else(|| Error::new("expected service time H:MM:SS"))?;
    let (minute, second) = rest
        .split_once(':')
        .ok_or_else(|| Error::new("expected service time H:MM:SS"))?;
    if !(digits(hour, 1) || digits(hour, 2)) || !digits(minute, 2) || !digits(second, 2) {
        return Err(Error::new("expected service time H:MM:SS"));
    }
    let h: u32 = hour.parse().unwrap();
    let m: u32 = minute.parse().unwrap();
    let s: u32 = second.parse().unwrap();
    if m >= 60 || s >= 60 {
        return Err(Error::new("invalid service time minutes or seconds"));
    }
    Ok(h * 3600 + m * 60 + s)
}

fn calendar(feed: &mut Feed<'_>) -> Result<Ids> {
    let mut weekly = Ids::new();
    let mut additions = Ids::new();
    let mut exceptions = BTreeSet::new();
    let mut removed = Vec::new();
    let mut columns = vec!["service_id", "start_date", "end_date"];
    columns.extend(DAYS);
    feed.table("calendar.txt", false, &columns, |row| {
        let id = field(row, "service_id")?;
        let start = field(row, "start_date")?;
        let end = field(row, "end_date")?;
        compact_date(start)?;
        compact_date(end)?;
        if start > end
            || DAYS
                .iter()
                .any(|day| !matches!(field(row, day), Ok("0" | "1")))
        {
            return Err(Error::new("invalid calendar date range or weekday flag"));
        }
        unique(&mut weekly, id, "service_id")
    })?;
    feed.table(
        "calendar_dates.txt",
        false,
        &["service_id", "date", "exception_type"],
        |row| {
            let id = field(row, "service_id")?;
            let date = field(row, "date")?;
            compact_date(date)?;
            match field(row, "exception_type")? {
                "1" => {
                    additions.insert(id.to_owned());
                }
                "2" => removed.push(id.to_owned()),
                _ => return Err(Error::new("exception_type must be 1 or 2")),
            }
            if !exceptions.insert((id.to_owned(), date.to_owned())) {
                return Err(Error::new(format!(
                    "duplicate exception for {id} on {date}"
                )));
            }
            Ok(())
        },
    )?;
    for id in removed {
        if !weekly.contains(&id) {
            feed.report.error(
                "calendar_dates.txt",
                None,
                format!("service {id} has a removal without a weekly calendar"),
            );
        }
    }
    weekly.extend(additions);
    if weekly.is_empty() {
        feed.report.error(
            "calendar.txt / calendar_dates.txt",
            None,
            "no scheduled services",
        );
    }
    Ok(weekly)
}

fn stops(feed: &mut Feed<'_>) -> Result<Stops> {
    let mut stops = Stops::new();
    let mut parent_links = Vec::new();
    feed.table("stops.txt", true, &["stop_id"], |row| {
        let id = field(row, "stop_id")?;
        let kind = if optional(row, "location_type") {
            number::<u8>(row, "location_type")?
        } else {
            0
        };
        if kind > 4 {
            return Err(Error::new("unknown location_type"));
        }
        if kind <= 2 || optional(row, "stop_lat") || optional(row, "stop_lon") {
            coordinates(number(row, "stop_lon")?, number(row, "stop_lat")?)?;
        }
        if kind <= 2 {
            field(row, "stop_name")?;
        }
        if kind == 1 && optional(row, "parent_station") {
            return Err(Error::new("a station may not have a parent_station"));
        }
        if kind >= 2 || optional(row, "parent_station") {
            parent_links.push((
                id.to_owned(),
                field(row, "parent_station")?.to_owned(),
                u8::from(kind != 4),
            ));
        }
        if stops.insert(id.to_owned(), kind).is_some() {
            return Err(Error::new(format!("duplicate stop_id {id}")));
        }
        Ok(())
    })?;
    for (id, parent, required_kind) in parent_links {
        if stops.get(&parent) != Some(&required_kind) {
            feed.report.error(
                "stops.txt",
                None,
                format!("stop {id} has invalid parent_station {parent}"),
            );
        }
    }
    Ok(stops)
}

fn stop_reference(stops: &Stops, id: &str, field: &str, allow_station: bool) -> Result<()> {
    match stops.get(id) {
        Some(0) => Ok(()),
        Some(1) if allow_station => Ok(()),
        Some(_) => Err(Error::new(format!(
            "{field} refers to an invalid location_type for {id}"
        ))),
        None => Err(Error::new(format!("{field} refers to unknown ID {id}"))),
    }
}

fn routes(feed: &mut Feed<'_>) -> Result<Ids> {
    let mut agencies = Ids::new();
    let mut count = 0;
    feed.table(
        "agency.txt",
        true,
        &["agency_name", "agency_url", "agency_timezone"],
        |row| {
            count += 1;
            for key in ["agency_name", "agency_url", "agency_timezone"] {
                field(row, key)?;
            }
            if optional(row, "agency_id") {
                unique(&mut agencies, field(row, "agency_id")?, "agency_id")?;
            }
            Ok(())
        },
    )?;
    if count > 1 && agencies.len() != count {
        feed.report.error(
            "agency.txt",
            None,
            "multiple agencies require an agency_id for each row",
        );
    }
    let mut routes = Ids::new();
    feed.table("routes.txt", true, &["route_id", "route_type"], |row| {
        if count > 1 || optional(row, "agency_id") {
            reference(&agencies, field(row, "agency_id")?, "agency_id")?;
        }
        number::<u16>(row, "route_type")?;
        if !optional(row, "route_short_name") && !optional(row, "route_long_name") {
            return Err(Error::new("route requires a short or long name"));
        }
        unique(&mut routes, field(row, "route_id")?, "route_id")
    })?;
    Ok(routes)
}

fn stop_times(feed: &mut Feed<'_>, trips: &Ids, stops: &Stops) -> Result<()> {
    let mut times: BTreeMap<String, BTreeMap<u32, Option<(u32, u32)>>> = BTreeMap::new();
    feed.table(
        "stop_times.txt",
        true,
        &[
            "trip_id",
            "stop_id",
            "stop_sequence",
            "arrival_time",
            "departure_time",
        ],
        |row| {
            let trip = field(row, "trip_id")?;
            reference(trips, trip, "trip_id")?;
            stop_reference(stops, field(row, "stop_id")?, "stop_id", false)?;
            let sequence = number::<u32>(row, "stop_sequence")?;
            let time = if optional(row, "arrival_time") || optional(row, "departure_time") {
                let arrival = service_time(field(row, "arrival_time")?)?;
                let departure = service_time(field(row, "departure_time")?)?;
                if arrival > departure {
                    return Err(Error::new("arrival time is later than departure time"));
                }
                Some((arrival, departure))
            } else {
                if matches!(field(row, "timepoint"), Ok("1")) {
                    return Err(Error::new("exact timepoint requires arrival and departure"));
                }
                None
            };
            if times
                .entry(trip.to_owned())
                .or_default()
                .insert(sequence, time)
                .is_some()
            {
                return Err(Error::new(format!(
                    "duplicate stop_sequence {sequence} for {trip}"
                )));
            }
            Ok(())
        },
    )?;
    for trip in trips {
        let Some(stops) = times.get(trip) else {
            feed.report.error(
                "stop_times.txt",
                None,
                format!("trip {trip} has no stop times"),
            );
            continue;
        };
        if stops.len() < 2
            || stops.first_key_value().unwrap().1.is_none()
            || stops.last_key_value().unwrap().1.is_none()
        {
            feed.report.error(
                "stop_times.txt",
                None,
                format!("trip {trip} needs at least two stops and timed endpoints"),
            );
        }
        let mut previous = 0;
        for &(arrival, departure) in stops.values().flatten() {
            if arrival < previous {
                feed.report.error(
                    "stop_times.txt",
                    None,
                    format!("trip {trip} goes backwards in service time"),
                );
                break;
            }
            previous = departure;
        }
    }
    Ok(())
}

fn transfers(feed: &mut Feed<'_>, stops: &Stops, routes: &Ids, trips: &Ids) -> Result<()> {
    let mut keys = BTreeSet::new();
    feed.table("transfers.txt", false, &[], |row| {
        let kind = row
            .get("transfer_type")
            .filter(|kind| !kind.is_empty())
            .map_or("0", String::as_str);
        if !matches!(kind, "0" | "1" | "2" | "3" | "4" | "5") {
            return Err(Error::new("transfer_type must be 0 through 5"));
        }
        let mut key = Vec::new();
        let trip_only = matches!(kind, "4" | "5");
        for name in ["from_stop_id", "to_stop_id"] {
            if !trip_only || optional(row, name) {
                let id = field(row, name)?;
                stop_reference(stops, id, name, !trip_only)?;
                key.push(id.to_owned());
            } else {
                key.push(String::new());
            }
        }
        for (name, known, required) in [
            ("from_route_id", routes, false),
            ("to_route_id", routes, false),
            ("from_trip_id", trips, matches!(kind, "4" | "5")),
            ("to_trip_id", trips, matches!(kind, "4" | "5")),
        ] {
            let id = if required || optional(row, name) {
                let id = field(row, name)?;
                reference(known, id, name)?;
                id
            } else {
                ""
            };
            key.push(id.to_owned());
        }
        if kind == "2" || optional(row, "min_transfer_time") {
            number::<u32>(row, "min_transfer_time")?;
        }
        if !keys.insert(key) {
            return Err(Error::new("duplicate transfer selectors"));
        }
        Ok(())
    })
}

fn frequencies(feed: &mut Feed<'_>, trips: &Ids) -> Result<()> {
    let mut windows: BTreeMap<String, Vec<(u32, u32)>> = BTreeMap::new();
    feed.table(
        "frequencies.txt",
        false,
        &["trip_id", "start_time", "end_time", "headway_secs"],
        |row| {
            let id = field(row, "trip_id")?;
            reference(trips, id, "trip_id")?;
            let start = service_time(field(row, "start_time")?)?;
            let end = service_time(field(row, "end_time")?)?;
            if start >= end
                || number::<u32>(row, "headway_secs")? == 0
                || (optional(row, "exact_times")
                    && !matches!(field(row, "exact_times"), Ok("0" | "1")))
            {
                return Err(Error::new(
                    "invalid frequency window, headway or exact_times",
                ));
            }
            windows.entry(id.to_owned()).or_default().push((start, end));
            Ok(())
        },
    )?;
    for (trip, ranges) in &mut windows {
        ranges.sort_unstable();
        if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            feed.report.error(
                "frequencies.txt",
                None,
                format!("overlapping frequency windows for {trip}"),
            );
        }
    }
    Ok(())
}

fn snapshot(root: &Path, rt: &RealtimeInput, report: &mut Report) -> Result<Value> {
    let data =
        gtfs_realtime::FeedMessage::decode(bytes(root, &rt.path)?.as_slice()).map_err(|e| {
            Error::new(format!(
                "{}: malformed GTFS-Realtime protobuf: {e}",
                rt.path.display()
            ))
        })?;
    if !matches!(data.header.gtfs_realtime_version.as_str(), "2.0" | "1.0")
        || data.header.incrementality.unwrap_or_default() != 0
        || data.header.timestamp.is_none()
    {
        return Err(Error::new(
            "GTFS-Realtime requires a timestamped FULL_DATASET snapshot in version 1.0 or 2.0",
        ));
    }
    row_limit(data.entity.len())?;
    let mut ids = Ids::new();
    for (index, entity) in data.entity.iter().enumerate() {
        let payloads = [
            entity.trip_update.is_some(),
            entity.vehicle.is_some(),
            entity.alert.is_some(),
            entity.shape.is_some(),
            entity.stop.is_some(),
            entity.trip_modifications.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();
        if entity.id.trim().is_empty()
            || !ids.insert(entity.id.clone())
            || entity.is_deleted.unwrap_or(false)
            || payloads != 1
        {
            report.error(
                &rt.path.to_string_lossy(),
                Some((index + 1) as u64),
                "entity needs a unique ID and exactly one payload in a full snapshot",
            );
        }
    }
    serde_json::to_value(data).map_err(|e| Error::new(e.to_string()))
}

pub(super) fn read(
    root: &Path,
    directory: &Path,
    realtime: Option<&RealtimeInput>,
    report: &mut Report,
) -> Result<Value> {
    if !resolved(root, directory)?.is_dir() {
        return Err(Error::new(
            "GTFS schedule requires an extracted feed directory",
        ));
    }
    let mut feed = Feed {
        root,
        directory,
        report,
        tables: BTreeMap::new(),
    };
    let routes = routes(&mut feed)?;
    let stops = stops(&mut feed)?;
    let services = calendar(&mut feed)?;
    let mut trips = Ids::new();
    feed.table(
        "trips.txt",
        true,
        &["trip_id", "route_id", "service_id"],
        |row| {
            reference(&routes, field(row, "route_id")?, "route_id")?;
            reference(&services, field(row, "service_id")?, "service_id")?;
            unique(&mut trips, field(row, "trip_id")?, "trip_id")
        },
    )?;
    stop_times(&mut feed, &trips, &stops)?;
    transfers(&mut feed, &stops, &routes, &trips)?;
    frequencies(&mut feed, &trips)?;
    let realtime = realtime
        .map(|rt| snapshot(root, rt, feed.report))
        .transpose()?;
    Ok(json!({"schedule":feed.tables,"realtime":realtime}))
}
