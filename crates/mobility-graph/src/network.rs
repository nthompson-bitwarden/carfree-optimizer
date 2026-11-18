//! E05 immutable network contract, independent of ingestion and routing.
use crate::{EdgeId, Mode, NodeId};
use std::collections::{BTreeMap, BTreeSet};

mod codec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Error {}
fn require(ok: bool, message: &str) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error(message.into()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    Observed,
    Imputed,
    Modeled,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub source: String,
    pub source_version: String,
    pub license: String,
    pub geography: String,
    /// ISO civil date, YYYY-MM-DD.
    pub date: String,
    pub basis: Basis,
    pub uncertainty: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Accessible,
    Steps,
    Impassable,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleFacility {
    Protected,
    Shared,
    Unspecified,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capacity {
    pub vehicles_per_hour: u32,
    pub provenance: Provenance,
}
/// Caller-assessed ordinal cycling traffic stress, 1 (lowest) through 4.
/// Not a crash probability, observed safety outcome, or inferred default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Safety {
    pub stress_level: u8,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub access: Access,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    /// Metres; absent only for a transfer at one node.
    pub distance_m: Option<f64>,
    /// Positive modeled street/transfer duration, or frequency ride duration.
    /// Scheduled rides use their explicit departure/arrival times instead.
    pub seconds: u32,
    pub access: Access,
    pub kind: LinkKind,
}
#[derive(Debug, Clone, PartialEq)]
pub enum LinkKind {
    Street {
        mode: Mode,
        crossing: bool,
        blocked: bool,
        cycling: CycleFacility,
        /// Explicit assessed traffic stress; no inference when absent.
        safety: Option<Box<Safety>>,
        capacity: Option<Box<Capacity>>,
    },
    Transit {
        mode: Mode,
        calendar: u32,
        trip: String,
        stop_index: u32,
        timetable: Timetable,
        bicycle_allowed: bool,
    },
    Transfer {
        from: Mode,
        to: Mode,
        action: TransferAction,
        /// Half-open UTC availability windows. Empty means unavailable.
        windows: Vec<(i64, i64)>,
        bicycle_allowed: bool,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferAction {
    Board,
    Alight,
    Park,
    Retrieve,
    Mount,
    Dismount,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Departure {
    /// Elapsed seconds from the explicit service-day UTC origin (can exceed 24h).
    pub depart: u32,
    pub arrive: u32,
    /// Run instance within a trip template and service date.
    pub run: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Timetable {
    Scheduled(Vec<Departure>),
    /// Deterministic phase assumption: first + n*headway, strictly before end.
    Frequency {
        first: u32,
        end: u32,
        headway: u32,
        first_run: u32,
    },
}
fn frequency_count(
    first: u32,
    end: u32,
    headway: u32,
    first_run: u32,
    seconds: u32,
) -> Result<u32, Error> {
    require(
        headway > 0 && first < end && end <= 172800 && seconds > 0,
        "invalid frequency window/duration",
    )?;
    let count = (end - 1 - first) / headway;
    let last_departure = first + count * headway;
    require(
        first_run.checked_add(count).is_some()
            && last_departure
                .checked_add(seconds)
                .is_some_and(|v| v <= 172800),
        "frequency run/time overflow",
    )?;
    Ok(count)
}
impl Timetable {
    /// Enumerate every run, not just the next departure (overtaking is legal).
    /// Frequency expansion has constant auxiliary memory and checks arithmetic.
    pub fn departures(
        &self,
        seconds: u32,
    ) -> Result<Box<dyn Iterator<Item = Departure> + '_>, Error> {
        match self {
            Self::Scheduled(events) => Ok(Box::new(events.iter().cloned())),
            Self::Frequency {
                first,
                end,
                headway,
                first_run,
            } => {
                let count = frequency_count(*first, *end, *headway, *first_run, seconds)?;
                Ok(Box::new((0..=count).map(move |i| {
                    let depart = first + i * headway;
                    Departure {
                        depart,
                        arrive: depart + seconds,
                        run: first_run + i,
                    }
                })))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    pub id: u32,
    /// Gregorian YYYYMMDD, inclusive range. Weekdays Monday=bit 0.
    pub first_date: u32,
    pub last_date: u32,
    pub weekdays: u8,
    pub added: Vec<u32>,
    pub removed: Vec<u32>,
}
impl Calendar {
    pub fn active(&self, day: &ServiceDay) -> bool {
        day.weekday < 7
            && !self.removed.contains(&day.date)
            && (self.added.contains(&day.date)
                || (day.date >= self.first_date
                    && day.date <= self.last_date
                    && self.weekdays & (1 << day.weekday) != 0))
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceDay {
    pub date: u32,
    /// Monday=0; checked against Gregorian date.
    pub weekday: u8,
    /// Caller-resolved UTC origin, including timezone/DST convention.
    pub origin_utc: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRule {
    pub incoming: EdgeId,
    pub outgoing: EdgeId,
    pub only: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkData {
    /// Caller-assigned dataset revision, separate from the binary schema version.
    pub revision: String,
    pub provenance: Provenance,
    pub timezone: String,
    pub tzdb: String,
    /// Inclusive UTC query coverage. Caller attests calendars/origins are complete.
    pub coverage_start: i64,
    pub coverage_end: i64,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub turns: Vec<TurnRule>,
    pub calendars: Vec<Calendar>,
    pub days: Vec<ServiceDay>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Network {
    data: NetworkData,
}

pub fn is_transit(mode: Mode) -> bool {
    matches!(mode, Mode::Bus | Mode::Rail | Mode::LightRail)
}
pub fn is_bicycle(mode: Mode) -> bool {
    matches!(mode, Mode::Bicycle | Mode::ElectricBicycle)
}
pub fn is_vehicle(mode: Mode) -> bool {
    mode == Mode::Car || is_bicycle(mode)
}

/// Gregorian date and Monday-based weekday, no timezone conversion.
pub fn date_weekday(date: u32) -> Option<u8> {
    let y = date / 10000;
    let m = date / 100 % 100;
    let d = date % 100;
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let mdays = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=9999).contains(&y) || !(1..=12).contains(&m) || d == 0 || d > mdays[(m - 1) as usize] {
        return None;
    }
    let prior = y - 1;
    let mut days = 365 * prior + prior / 4 - prior / 100 + prior / 400;
    days += mdays[..(m - 1) as usize].iter().sum::<u32>() + d - 1;
    Some((days % 7) as u8)
}
fn valid_provenance(p: &Provenance) -> bool {
    let b = p.date.as_bytes();
    let valid_date = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
        && p.date
            .replace('-', "")
            .parse()
            .ok()
            .and_then(date_weekday)
            .is_some();
    !p.source.trim().is_empty()
        && !p.source_version.trim().is_empty()
        && !p.license.trim().is_empty()
        && !p.geography.trim().is_empty()
        && valid_date
        && !p.uncertainty.trim().is_empty()
}
type TripLeg<'a> = (&'a Link, u32, Mode, u32, &'a Timetable);

impl Network {
    pub const SCHEMA: u32 = 1;
    pub fn data(&self) -> &NetworkData {
        &self.data
    }
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.data
            .nodes
            .binary_search_by_key(&id.0, |n| n.id.0)
            .ok()
            .map(|i| &self.data.nodes[i])
    }
    pub fn link(&self, id: EdgeId) -> Option<&Link> {
        self.data
            .links
            .binary_search_by_key(&id.0, |e| e.id.0)
            .ok()
            .map(|i| &self.data.links[i])
    }
    pub fn new(mut data: NetworkData) -> Result<Self, Error> {
        require(
            !data.revision.trim().is_empty() && valid_provenance(&data.provenance),
            "revision and complete dated provenance required",
        )?;
        require(
            !data.timezone.trim().is_empty() && !data.tzdb.trim().is_empty(),
            "timezone and tzdb version required",
        )?;
        require(
            data.coverage_start < data.coverage_end
                && data.coverage_end.checked_sub(data.coverage_start).is_some(),
            "invalid UTC coverage",
        )?;
        data.nodes.sort_by_key(|n| n.id.0);
        data.links.sort_by_key(|l| l.id.0);
        data.calendars.sort_by_key(|c| c.id);
        data.days.sort_by_key(|d| d.date);
        data.turns
            .sort_by_key(|t| (t.incoming.0, t.outgoing.0, t.only));
        require(
            !data.nodes.is_empty() && data.nodes.windows(2).all(|w| w[0].id != w[1].id),
            "empty nodes or duplicate node ID",
        )?;
        require(
            data.links.windows(2).all(|w| w[0].id != w[1].id),
            "duplicate edge ID",
        )?;
        require(
            data.calendars.windows(2).all(|w| w[0].id != w[1].id),
            "duplicate calendar ID",
        )?;
        require(
            data.days
                .windows(2)
                .all(|w| w[0].date != w[1].date && w[0].origin_utc < w[1].origin_utc),
            "service days must have unique dates and increasing UTC origins",
        )?;
        for day in &data.days {
            require(
                date_weekday(day.date) == Some(day.weekday),
                "service-day date/weekday mismatch",
            )?;
            require(
                day.origin_utc.checked_add(172800).is_some(),
                "service-day UTC overflow",
            )?;
        }
        for c in &mut data.calendars {
            c.added.sort_unstable();
            c.removed.sort_unstable();
            require(
                date_weekday(c.first_date).is_some()
                    && date_weekday(c.last_date).is_some()
                    && c.first_date <= c.last_date
                    && c.weekdays <= 127,
                "invalid calendar range/weekdays",
            )?;
            for dates in [&c.added, &c.removed] {
                require(
                    dates.iter().all(|d| date_weekday(*d).is_some())
                        && dates.windows(2).all(|w| w[0] != w[1]),
                    "invalid or duplicate calendar exception",
                )?;
            }
            require(
                !c.added.iter().any(|d| c.removed.contains(d)),
                "conflicting calendar exceptions",
            )?;
        }
        let node_ids: BTreeSet<u32> = data.nodes.iter().map(|n| n.id.0).collect();
        let calendar_ids: BTreeSet<u32> = data.calendars.iter().map(|c| c.id).collect();
        for link in &mut data.links {
            require(
                node_ids.contains(&link.from.0) && node_ids.contains(&link.to.0),
                "edge has missing endpoint",
            )?;
            require(
                link.seconds > 0 && link.seconds <= 172800,
                "duration must be 1..172800 seconds",
            )?;
            if let Some(d) = link.distance_m {
                require(
                    d.is_finite() && d > 0.0,
                    "distance must be finite positive metres",
                )?;
            }
            match &mut link.kind {
                LinkKind::Street {
                    mode,
                    safety,
                    capacity,
                    ..
                } => {
                    require(
                        !is_transit(*mode) && link.distance_m.is_some(),
                        "street requires street mode and distance",
                    )?;
                    require(
                        safety.as_ref().is_none_or(|s| {
                            (1..=4).contains(&s.stress_level) && valid_provenance(&s.provenance)
                        }),
                        "invalid safety provenance",
                    )?;
                    require(
                        capacity.as_ref().is_none_or(|c| {
                            *mode == Mode::Car
                                && c.vehicles_per_hour > 0
                                && valid_provenance(&c.provenance)
                        }),
                        "capacity requires car mode, positive vehicles/hour and provenance",
                    )?;
                }
                LinkKind::Transit {
                    mode,
                    calendar,
                    trip,
                    timetable,
                    ..
                } => {
                    require(
                        is_transit(*mode)
                            && link.distance_m.is_some()
                            && calendar_ids.contains(calendar)
                            && !trip.trim().is_empty()
                            && !data.days.is_empty(),
                        "invalid transit mode/calendar/trip/days/distance",
                    )?;
                    match timetable {
                        Timetable::Scheduled(events) => {
                            events.sort_by_key(|e| (e.depart, e.arrive, e.run));
                            let mut runs = BTreeSet::new();
                            require(
                                !events.is_empty()
                                    && events.iter().all(|e| {
                                        e.depart < e.arrive
                                            && e.arrive <= 172800
                                            && runs.insert(e.run)
                                    }),
                                "invalid scheduled times or duplicate run",
                            )?;
                        }
                        Timetable::Frequency {
                            first,
                            end,
                            headway,
                            first_run,
                        } => {
                            frequency_count(*first, *end, *headway, *first_run, link.seconds)?;
                        }
                    }
                }
                LinkKind::Transfer {
                    from,
                    to,
                    action,
                    windows,
                    ..
                } => {
                    require(
                        link.from == link.to && link.distance_m.is_none(),
                        "transfers must be at one node with no distance",
                    )?;
                    let modes_ok = match action {
                        TransferAction::Board => *from == Mode::Walk && is_transit(*to),
                        TransferAction::Alight => is_transit(*from) && *to == Mode::Walk,
                        TransferAction::Park => is_vehicle(*from) && *to == Mode::Walk,
                        TransferAction::Retrieve => *from == Mode::Walk && is_vehicle(*to),
                        TransferAction::Mount => *from == Mode::Walk && is_bicycle(*to),
                        TransferAction::Dismount => is_bicycle(*from) && *to == Mode::Walk,
                    };
                    require(modes_ok, "transfer action incompatible with modes")?;
                    windows.sort_unstable();
                    require(
                        windows
                            .iter()
                            .all(|(a, b)| a < b && b.checked_sub(*a).is_some())
                            && windows.windows(2).all(|w| w[0].1 <= w[1].0),
                        "invalid or overlapping transfer windows",
                    )?;
                }
            }
        }
        // A trip template is an ordered chain, not a branching set of rides.
        // Short-turns/express variants must have distinct trip IDs.
        let mut trips: BTreeMap<&str, Vec<TripLeg<'_>>> = BTreeMap::new();
        for link in &data.links {
            if let LinkKind::Transit {
                trip,
                stop_index,
                mode,
                calendar,
                timetable,
                ..
            } = &link.kind
            {
                trips.entry(trip).or_default().push((
                    link,
                    *stop_index,
                    *mode,
                    *calendar,
                    timetable,
                ));
            }
        }
        for legs in trips.values_mut() {
            legs.sort_by_key(|leg| leg.1);
            for pair in legs.windows(2) {
                let (a, ai, am, ac, at) = pair[0];
                let (b, bi, bm, bc, bt) = pair[1];
                require(ai.checked_add(1) == Some(bi) && a.to == b.from && am == bm && ac == bc, "trip legs require consecutive connected stop indices and consistent mode/calendar")?;
                let previous: BTreeMap<u32, u32> = at
                    .departures(a.seconds)?
                    .map(|e| (e.run, e.arrive))
                    .collect();
                let following: BTreeMap<u32, u32> = bt
                    .departures(b.seconds)?
                    .map(|e| (e.run, e.depart))
                    .collect();
                require(
                    previous.len() == following.len()
                        && previous.iter().all(|(run, arrive)| {
                            following.get(run).is_some_and(|depart| depart >= arrive)
                        }),
                    "trip run sets or dwell times are inconsistent",
                )?;
            }
        }
        let network = Self { data };
        require(
            network.data.turns.windows(2).all(|w| w[0] != w[1]),
            "duplicate turn rule",
        )?;
        let mut only = BTreeSet::new();
        for t in &network.data.turns {
            let a = network
                .link(t.incoming)
                .ok_or_else(|| Error("missing incoming turn edge".into()))?;
            let b = network
                .link(t.outgoing)
                .ok_or_else(|| Error("missing outgoing turn edge".into()))?;
            let car = |l: &Link| {
                matches!(
                    l.kind,
                    LinkKind::Street {
                        mode: Mode::Car,
                        ..
                    }
                )
            };
            require(
                a.to == b.from && car(a) && car(b),
                "turn rule must join car streets",
            )?;
            require(
                !t.only || only.insert(t.incoming.0),
                "multiple only-turn destinations unsupported",
            )?;
            require(
                !network.data.turns.iter().any(|other| {
                    other.incoming == t.incoming
                        && other.outgoing == t.outgoing
                        && other.only != t.only
                }),
                "contradictory turn rule",
            )?;
        }
        Ok(network)
    }
    /// Canonical binary v1. Decoding revalidates the complete graph.
    pub fn encode(&self) -> Vec<u8> {
        codec::encode(&self.data)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let network = Self::new(codec::decode(bytes)?)?;
        require(network.encode() == bytes, "noncanonical network encoding")?;
        Ok(network)
    }
}
