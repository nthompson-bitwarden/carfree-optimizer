//! Private, bounded, canonical binary schema v1. No native-width encodings.
use super::*;
trait Bin: Sized {
    fn put(&self, out: &mut Vec<u8>);
    fn get(input: &mut &[u8]) -> Result<Self, Error>;
}
fn take<'a>(input: &mut &'a [u8], n: usize) -> Result<&'a [u8], Error> {
    if input.len() < n {
        return Err(Error("truncated network".into()));
    }
    let (value, rest) = input.split_at(n);
    *input = rest;
    Ok(value)
}
macro_rules! number {
    ($t:ty,$n:expr) => {
        impl Bin for $t {
            fn put(&self, out: &mut Vec<u8>) {
                out.extend(self.to_le_bytes());
            }
            fn get(input: &mut &[u8]) -> Result<Self, Error> {
                let mut a = [0; $n];
                a.copy_from_slice(take(input, $n)?);
                Ok(<$t>::from_le_bytes(a))
            }
        }
    };
}
number!(u32, 4);
number!(i64, 8);
number!(f64, 8);
impl Bin for u8 {
    fn put(&self, out: &mut Vec<u8>) {
        out.push(*self);
    }
    fn get(input: &mut &[u8]) -> Result<Self, Error> {
        Ok(take(input, 1)?[0])
    }
}
impl Bin for bool {
    fn put(&self, out: &mut Vec<u8>) {
        (*self as u8).put(out);
    }
    fn get(input: &mut &[u8]) -> Result<Self, Error> {
        match u8::get(input)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error("invalid boolean".into())),
        }
    }
}
// Length is u64 encoded as two u32s, avoiding platform-dependent length casts.
fn put_len(n: usize, out: &mut Vec<u8>) {
    out.extend((n as u64).to_le_bytes());
}
fn get_len(input: &mut &[u8]) -> Result<usize, Error> {
    let mut a = [0; 8];
    a.copy_from_slice(take(input, 8)?);
    let n = usize::try_from(u64::from_le_bytes(a)).map_err(|_| Error("length overflow".into()))?;
    require(
        n <= input.len(),
        "collection length exceeds remaining bytes",
    )?;
    Ok(n)
}
impl Bin for String {
    fn put(&self, out: &mut Vec<u8>) {
        put_len(self.len(), out);
        out.extend(self.as_bytes());
    }
    fn get(input: &mut &[u8]) -> Result<Self, Error> {
        let n = get_len(input)?;
        String::from_utf8(take(input, n)?.to_vec()).map_err(|_| Error("invalid UTF-8".into()))
    }
}
impl<T: Bin> Bin for Vec<T> {
    fn put(&self, out: &mut Vec<u8>) {
        put_len(self.len(), out);
        for v in self {
            v.put(out);
        }
    }
    fn get(input: &mut &[u8]) -> Result<Self, Error> {
        let n = get_len(input)?;
        let mut v = Vec::new();
        for _ in 0..n {
            v.push(T::get(input)?);
        }
        Ok(v)
    }
}
impl<T: Bin> Bin for Box<T> {
    fn put(&self, out: &mut Vec<u8>) {
        self.as_ref().put(out);
    }
    fn get(input: &mut &[u8]) -> Result<Self, Error> {
        Ok(Box::new(T::get(input)?))
    }
}
impl<T: Bin> Bin for Option<T> {
    fn put(&self, out: &mut Vec<u8>) {
        self.is_some().put(out);
        if let Some(v) = self {
            v.put(out);
        }
    }
    fn get(input: &mut &[u8]) -> Result<Self, Error> {
        if bool::get(input)? {
            Ok(Some(T::get(input)?))
        } else {
            Ok(None)
        }
    }
}
impl Bin for (i64, i64) {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
        self.1.put(out);
    }
    fn get(input: &mut &[u8]) -> Result<Self, Error> {
        Ok((i64::get(input)?, i64::get(input)?))
    }
}
macro_rules! record {
    ($t:ident,$($f:ident),+)=>{
        impl Bin for $t {
            fn put(&self,out:&mut Vec<u8>){$(self.$f.put(out);)+}
            fn get(input:&mut &[u8])->Result<Self,Error>{Ok(Self{$($f:Bin::get(input)?,)+})}
        }
    };
}
macro_rules! tags {
    ($t:ident,$($tag:literal=>$v:ident),+)=>{
        impl Bin for $t {
            fn put(&self,out:&mut Vec<u8>){let tag:u8=match self{$(Self::$v=>$tag,)+};tag.put(out);}
            fn get(input:&mut &[u8])->Result<Self,Error>{match u8::get(input)?{$($tag=>Ok(Self::$v),)+_=>Err(Error(concat!("invalid ",stringify!($t)," tag").into()))}}
        }
    };
}
tags!(Mode,0=>Walk,1=>Bicycle,2=>ElectricBicycle,3=>Bus,4=>LightRail,5=>Rail,6=>Car);
tags!(Access,0=>Accessible,1=>Steps,2=>Impassable,3=>Unknown);
tags!(Basis,0=>Observed,1=>Imputed,2=>Modeled);
tags!(CycleFacility,0=>Protected,1=>Shared,2=>Unspecified);
tags!(TransferAction,0=>Board,1=>Alight,2=>Park,3=>Retrieve,4=>Mount,5=>Dismount);
impl Bin for NodeId {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
    }
    fn get(input: &mut &[u8]) -> Result<Self, Error> {
        Ok(Self(u32::get(input)?))
    }
}
impl Bin for EdgeId {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
    }
    fn get(input: &mut &[u8]) -> Result<Self, Error> {
        Ok(Self(u32::get(input)?))
    }
}
record!(
    Provenance,
    source,
    source_version,
    license,
    geography,
    date,
    basis,
    uncertainty
);
record!(Capacity, vehicles_per_hour, provenance);
record!(Safety, stress_level, provenance);
record!(Node, id, access);
record!(Link, id, from, to, distance_m, seconds, access, kind);
record!(Departure, depart, arrive, run);
record!(Calendar, id, first_date, last_date, weekdays, added, removed);
record!(ServiceDay, date, weekday, origin_utc);
record!(TurnRule, incoming, outgoing, only);
record!(
    NetworkData,
    revision,
    provenance,
    timezone,
    tzdb,
    coverage_start,
    coverage_end,
    nodes,
    links,
    turns,
    calendars,
    days
);
impl Bin for Timetable {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Scheduled(events) => {
                0u8.put(out);
                events.put(out);
            }
            Self::Frequency {
                first,
                end,
                headway,
                first_run,
            } => {
                1u8.put(out);
                first.put(out);
                end.put(out);
                headway.put(out);
                first_run.put(out);
            }
        }
    }
    fn get(input: &mut &[u8]) -> Result<Self, Error> {
        match u8::get(input)? {
            0 => Ok(Self::Scheduled(Bin::get(input)?)),
            1 => Ok(Self::Frequency {
                first: Bin::get(input)?,
                end: Bin::get(input)?,
                headway: Bin::get(input)?,
                first_run: Bin::get(input)?,
            }),
            _ => Err(Error("invalid timetable tag".into())),
        }
    }
}
impl Bin for LinkKind {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Street {
                mode,
                crossing,
                blocked,
                cycling,
                safety,
                capacity,
            } => {
                0u8.put(out);
                mode.put(out);
                crossing.put(out);
                blocked.put(out);
                cycling.put(out);
                safety.put(out);
                capacity.put(out);
            }
            Self::Transit {
                mode,
                calendar,
                trip,
                stop_index,
                timetable,
                bicycle_allowed,
            } => {
                1u8.put(out);
                mode.put(out);
                calendar.put(out);
                trip.put(out);
                stop_index.put(out);
                timetable.put(out);
                bicycle_allowed.put(out);
            }
            Self::Transfer {
                from,
                to,
                action,
                windows,
                bicycle_allowed,
            } => {
                2u8.put(out);
                from.put(out);
                to.put(out);
                action.put(out);
                windows.put(out);
                bicycle_allowed.put(out);
            }
        }
    }
    fn get(input: &mut &[u8]) -> Result<Self, Error> {
        match u8::get(input)? {
            0 => Ok(Self::Street {
                mode: Bin::get(input)?,
                crossing: Bin::get(input)?,
                blocked: Bin::get(input)?,
                cycling: Bin::get(input)?,
                safety: Bin::get(input)?,
                capacity: Bin::get(input)?,
            }),
            1 => Ok(Self::Transit {
                mode: Bin::get(input)?,
                calendar: Bin::get(input)?,
                trip: Bin::get(input)?,
                stop_index: Bin::get(input)?,
                timetable: Bin::get(input)?,
                bicycle_allowed: Bin::get(input)?,
            }),
            2 => Ok(Self::Transfer {
                from: Bin::get(input)?,
                to: Bin::get(input)?,
                action: Bin::get(input)?,
                windows: Bin::get(input)?,
                bicycle_allowed: Bin::get(input)?,
            }),
            _ => Err(Error("invalid link kind tag".into())),
        }
    }
}
pub(super) fn encode(data: &NetworkData) -> Vec<u8> {
    let mut out = Vec::new();
    Network::SCHEMA.put(&mut out);
    data.put(&mut out);
    out
}
pub(super) fn decode(mut input: &[u8]) -> Result<NetworkData, Error> {
    require(
        u32::get(&mut input)? == Network::SCHEMA,
        "unsupported network schema",
    )?;
    let data = NetworkData::get(&mut input)?;
    require(input.is_empty(), "trailing network bytes")?;
    Ok(data)
}
