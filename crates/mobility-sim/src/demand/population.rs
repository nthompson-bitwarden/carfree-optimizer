use super::{nonnegative, provenance, require, text, Error, Purpose};
use mobility_graph::network::Provenance;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Population {
    pub provenance: Provenance,
    /// Absolute whole-person tolerance, applied per zone and to the study total.
    pub tolerance_persons: u64,
    pub zones: Vec<Zone>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    pub id: String,
    pub source_persons: u64,
    pub strata: Vec<Stratum>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stratum {
    pub group: String,
    pub modeled_persons: u64,
    pub representatives: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntheticTraveler<'a> {
    pub zone: &'a str,
    pub group: &'a str,
    pub ordinal: u32,
    pub persons: u64,
}

fn add_count(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b)
        .ok_or_else(|| Error::Invalid("population count overflow".into()))
}

impl Population {
    pub(super) fn validate(&self) -> Result<(), Error> {
        require(
            provenance(&self.provenance) && !self.zones.is_empty(),
            || "population requires documented provenance and at least one zone".into(),
        )?;
        let mut zones = BTreeSet::new();
        let mut source = 0;
        let mut modeled = 0;
        for zone in &self.zones {
            require(text(&zone.id) && zones.insert(&zone.id), || {
                format!("invalid or duplicate zone {}", zone.id)
            })?;
            let mut seen = BTreeSet::new();
            let mut sum = 0;
            for stratum in &zone.strata {
                require(
                    text(&stratum.group)
                        && seen.insert(&stratum.group)
                        && (1..=stratum.modeled_persons)
                            .contains(&u64::from(stratum.representatives)),
                    || {
                        format!("zone {}, group {}: invalid or duplicate stratum or representative count", zone.id, stratum.group)
                    },
                )?;
                sum = add_count(sum, stratum.modeled_persons)?;
            }
            require(
                zone.source_persons.abs_diff(sum) <= self.tolerance_persons,
                || {
                    format!(
                        "zone {}: modeled {sum} differs from source {} beyond tolerance {}",
                        zone.id, zone.source_persons, self.tolerance_persons
                    )
                },
            )?;
            source = add_count(source, zone.source_persons)?;
            modeled = add_count(modeled, sum)?;
        }
        require(source.abs_diff(modeled) <= self.tolerance_persons, || {
            format!("modeled study population {modeled} differs from source {source} beyond tolerance {}", self.tolerance_persons)
        })
    }

    /// Deterministic lazy expansion in input order, without a random seed or
    /// allocation proportional to the number of synthetic travelers.
    pub fn generate(&self) -> Result<impl Iterator<Item = SyntheticTraveler<'_>>, Error> {
        self.validate()?;
        Ok(self.zones.iter().flat_map(|zone| {
            zone.strata.iter().flat_map(move |stratum| {
                let n = u64::from(stratum.representatives);
                let (base, remainder) = (stratum.modeled_persons / n, stratum.modeled_persons % n);
                (0..stratum.representatives).map(move |ordinal| SyntheticTraveler {
                    zone: &zone.id,
                    group: &stratum.group,
                    ordinal,
                    persons: base + u64::from(u64::from(ordinal) < remainder),
                })
            })
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Move {
    pub from: String,
    pub to: String,
    pub persons: u64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Rate {
    pub zone: String,
    pub purpose: Purpose,
    /// Per source-zone resident per modeled study day, before adaptation.
    pub trips_per_person: f64,
    pub multiplier: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Sensitivity {
    pub revision: String,
    pub years: u32,
    pub provenance: Provenance,
    pub moves: Vec<Move>,
    pub rates: Vec<Rate>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TripSensitivity {
    pub zone: String,
    pub purpose: Purpose,
    pub baseline: f64,
    pub relocation_only: f64,
    pub with_induced_demand: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct LongHorizonReport {
    pub revision: String,
    pub years: u32,
    pub provenance: Provenance,
    pub population_before: BTreeMap<String, u64>,
    pub population_after: BTreeMap<String, u64>,
    pub trips: Vec<TripSensitivity>,
}

/// Separate what-if arithmetic, not a prediction, short-term route mutation,
/// destination forecast or automatically populated rate table.
pub fn long_horizon(
    population: &Population,
    run: &Sensitivity,
) -> Result<LongHorizonReport, Error> {
    population.validate()?;
    require(
        text(&run.revision) && run.years > 0 && provenance(&run.provenance),
        || "long-horizon sensitivity requires a revision, positive years and provenance".into(),
    )?;
    let before: BTreeMap<_, _> = population
        .zones
        .iter()
        .map(|z| (z.id.clone(), z.source_persons))
        .collect();
    let mut departures = BTreeMap::new();
    let mut arrivals = BTreeMap::new();
    for m in &run.moves {
        require(
            m.from != m.to
                && m.persons > 0
                && before.contains_key(&m.from)
                && before.contains_key(&m.to),
            || format!("invalid relocation {} -> {}", m.from, m.to),
        )?;
        let out = departures.entry(&m.from).or_insert(0);
        *out = add_count(*out, m.persons)?;
        require(*out <= before[&m.from], || {
            format!("relocation exceeds baseline population of {}", m.from)
        })?;
        let inc = arrivals.entry(&m.to).or_insert(0);
        *inc = add_count(*inc, m.persons)?;
    }
    let mut after = before.clone();
    for (zone, count) in &mut after {
        *count = add_count(
            *count - departures.get(zone).copied().unwrap_or(0),
            arrivals.get(zone).copied().unwrap_or(0),
        )?;
    }
    let mut trips = Vec::with_capacity(run.rates.len());
    let mut seen = BTreeSet::new();
    for r in &run.rates {
        require(
            before.contains_key(&r.zone)
                && seen.insert((&r.zone, r.purpose))
                && nonnegative(r.trips_per_person)
                && nonnegative(r.multiplier),
            || {
                format!(
                    "invalid or duplicate trip rate for {} {:?}",
                    r.zone, r.purpose
                )
            },
        )?;
        let baseline = before[&r.zone] as f64 * r.trips_per_person;
        let relocation_only = after[&r.zone] as f64 * r.trips_per_person;
        let with_induced_demand = relocation_only * r.multiplier;
        require(
            [baseline, relocation_only, with_induced_demand]
                .into_iter()
                .all(nonnegative),
            || format!("trip rate overflow for {} {:?}", r.zone, r.purpose),
        )?;
        trips.push(TripSensitivity {
            zone: r.zone.clone(),
            purpose: r.purpose,
            baseline,
            relocation_only,
            with_induced_demand,
        });
    }
    Ok(LongHorizonReport {
        revision: run.revision.clone(),
        years: run.years,
        provenance: run.provenance.clone(),
        population_before: before,
        population_after: after,
        trips,
    })
}
