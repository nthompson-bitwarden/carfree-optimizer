//! E13-S04/S05/S06/S07: explicit geography and physical footprints, not graph-edge area.
use crate::scoring::{check, nonnegative, text, Context, Error, VERSION};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, PartialEq)]
pub struct Neighborhood {
    pub id: String,
    pub population: f64,
    /// Caller-supplied map-space polygon (not inferred GIS geometry).
    pub ring: Vec<(f64, f64)>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Origin {
    pub id: String,
    pub neighborhood: String,
    pub group: String,
    pub population: f64,
    /// Complete score for the same declared destination/mode/window estimand.
    pub score: Option<crate::projection::Projection>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct AreaScore {
    pub neighborhood: Neighborhood,
    pub represented_population: f64,
    pub total_population: f64,
    pub coverage: Option<f64>,
    pub insufficient_coverage: bool,
    /// Conditional on represented population; never a citywide extrapolation.
    pub represented_accessibility: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct MapScore {
    pub version: u32,
    pub context: Context,
    pub minimum_coverage: f64,
    pub areas: Vec<AreaScore>,
    /// Disjoint primary cohorts, with their own population denominators.
    pub groups: BTreeMap<(String, String), (f64, f64, Option<f64>)>,
}
pub fn neighborhoods(
    context: &Context,
    areas: &[Neighborhood],
    origins: &[Origin],
    minimum_coverage: f64,
) -> Result<MapScore, Error> {
    context.validate()?;
    check(
        nonnegative(minimum_coverage) && minimum_coverage <= 1.0,
        "coverage threshold must be in [0,1]",
    )?;
    let mut index = BTreeMap::new();
    for a in areas {
        check(
            text(&a.id)
                && nonnegative(a.population)
                && a.ring.len() >= 3
                && a.ring.iter().all(|(x, y)| x.is_finite() && y.is_finite())
                && index.insert(a.id.clone(), a).is_none(),
            "invalid/duplicate neighborhood or map polygon",
        )?;
    }
    let mut seen = BTreeSet::new();
    let mut population: BTreeMap<String, f64> = BTreeMap::new();
    let mut represented: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let mut groups: BTreeMap<(String, String), (f64, f64, f64)> = BTreeMap::new();
    let mut estimand = None;
    for o in origins {
        check(
            text(&o.id)
                && text(&o.group)
                && seen.insert(&o.id)
                && index.contains_key(&o.neighborhood)
                && nonnegative(o.population),
            "invalid/duplicate origin, cohort, population or neighborhood",
        )?;
        *population.entry(o.neighborhood.clone()).or_default() += o.population;
        let g = groups
            .entry((o.neighborhood.clone(), o.group.clone()))
            .or_default();
        g.0 += o.population;
        if let Some(score) = &o.score {
            check(
                score.context == *context
                    && score.origin == o.id
                    && score.neighborhood == o.neighborhood
                    && score.group == o.group,
                "map score context/origin/neighborhood/cohort mismatch",
            )?;
            if let Some(key) = estimand {
                check(key == &score.estimand, "map estimands differ")?;
            } else {
                estimand = Some(&score.estimand);
            }
        }
        if let Some(s) = o.score.as_ref().and_then(|s| s.share) {
            let a = represented.entry(o.neighborhood.clone()).or_default();
            a.0 += o.population;
            a.1 += o.population * s;
            g.1 += o.population;
            g.2 += o.population * s;
        }
    }
    let mut scores = Vec::new();
    for a in areas {
        let supplied = population.get(&a.id).copied().unwrap_or(0.0);
        check(
            supplied.is_finite() && supplied <= a.population,
            "origin populations exceed neighborhood denominator",
        )?;
        let (p, value) = represented.get(&a.id).copied().unwrap_or_default();
        let coverage = (a.population > 0.0).then(|| p / a.population);
        scores.push(AreaScore {
            neighborhood: a.clone(),
            represented_population: p,
            total_population: a.population,
            coverage,
            insufficient_coverage: p == 0.0 || coverage.is_none_or(|c| c < minimum_coverage),
            represented_accessibility: (p > 0.0).then(|| value / p),
        });
    }
    Ok(MapScore {
        version: VERSION,
        context: context.clone(),
        minimum_coverage,
        areas: scores,
        groups: groups
            .into_iter()
            .map(|(key, (p, represented, value))| {
                (
                    key,
                    (
                        p,
                        represented,
                        (represented > 0.0).then(|| value / represented),
                    ),
                )
            })
            .collect(),
    })
}
/// Nonoverlapping physical footprint IDs must be established upstream. Multiple
/// directed/parallel graph references to the SAME footprint are deduplicated.
/// Distinct IDs are a caller assertion of disjointness, not geometry validation.
#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    pub id: String,
    pub area_m2: f64,
    /// Explicit policy allocation: shared mixed traffic may be 0..1, never inferred.
    pub private_car_fraction: f64,
    /// Disjoint neighborhood fractions, sum exactly one within 1e-9.
    pub neighborhood_fractions: BTreeMap<String, f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceScore {
    pub completeness_source: String,
    pub version: u32,
    pub context: Context,
    pub total_m2: f64,
    pub private_car_m2: f64,
    pub by_neighborhood_m2: BTreeMap<String, f64>,
}
/// Explicit complete inventory, including a sourced declaration for an empty
/// (evidenced zero) inventory. Missing rows are errors, never successful zeroes.
#[derive(Debug, Clone, PartialEq)]
pub struct Inventory {
    pub ids: BTreeSet<String>,
    pub completeness_source: String,
}
impl Inventory {
    fn verify(&self, ids: BTreeSet<String>) -> Result<(), Error> {
        check(
            text(&self.completeness_source) && self.ids == ids && self.ids.iter().all(|s| text(s)),
            "incomplete inventory or missing completeness evidence",
        )
    }
}
pub fn road_space(
    context: &Context,
    inventory: &Inventory,
    footprints: &[Footprint],
) -> Result<SpaceScore, Error> {
    context.validate()?;
    inventory.verify(footprints.iter().map(|f| f.id.clone()).collect())?;
    let mut unique = BTreeMap::new();
    let mut r = SpaceScore {
        completeness_source: inventory.completeness_source.clone(),
        version: VERSION,
        context: context.clone(),
        total_m2: 0.0,
        private_car_m2: 0.0,
        by_neighborhood_m2: BTreeMap::new(),
    };
    for f in footprints {
        check(
            text(&f.id)
                && nonnegative(f.area_m2)
                && nonnegative(f.private_car_fraction)
                && f.private_car_fraction <= 1.0
                && !f.neighborhood_fractions.is_empty()
                && f.neighborhood_fractions
                    .iter()
                    .all(|(k, v)| text(k) && nonnegative(*v) && *v <= 1.0)
                && (f.neighborhood_fractions.values().sum::<f64>() - 1.0).abs() <= 1e-9,
            "invalid footprint area/allocation/splits",
        )?;
        if let Some(old) = unique.insert(&f.id, f) {
            check(old == f, "conflicting duplicate physical footprint")?;
            continue;
        }
        r.total_m2 += f.area_m2;
        let private = f.area_m2 * f.private_car_fraction;
        r.private_car_m2 += private;
        for (id, fraction) in &f.neighborhood_fractions {
            *r.by_neighborhood_m2.entry(id.clone()).or_default() += private * fraction;
        }
    }
    check(r.total_m2.is_finite(), "area overflow")?;
    Ok(r)
}
/// Deterministic uncertainty bounds, NOT a statistical confidence interval.
#[derive(Debug, Clone, PartialEq)]
pub struct Bounds {
    pub low: f64,
    pub central: f64,
    pub high: f64,
}
impl Bounds {
    pub fn validate(&self) -> Result<(), Error> {
        check(
            nonnegative(self.low)
                && nonnegative(self.central)
                && nonnegative(self.high)
                && self.low <= self.central
                && self.central <= self.high,
            "invalid nonnegative low/central/high bounds",
        )
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct VehicleActivity {
    pub id: String,
    pub vehicle_km: Bounds,
    /// kg CO2e per vehicle-km, operational only. Fleet/period matching required.
    pub operational_kg_per_vehicle_km: Bounds,
    pub methodology_source: String,
    pub fleet: String,
    pub period: String,
    pub factor_revision: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct EmissionsScore {
    pub completeness_source: String,
    pub version: u32,
    pub context: Context,
    pub vehicle_km: Bounds,
    pub operational_kg_co2e: Bounds,
    pub methodology_sources: Vec<String>,
}
pub fn emissions(
    context: &Context,
    inventory: &Inventory,
    rows: &[VehicleActivity],
) -> Result<EmissionsScore, Error> {
    context.validate()?;
    inventory.verify(rows.iter().map(|r| r.id.clone()).collect())?;
    let zero = || Bounds {
        low: 0.0,
        central: 0.0,
        high: 0.0,
    };
    let mut r = EmissionsScore {
        completeness_source: inventory.completeness_source.clone(),
        version: VERSION,
        context: context.clone(),
        vehicle_km: zero(),
        operational_kg_co2e: zero(),
        methodology_sources: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    for a in rows {
        check(
            text(&a.id)
                && seen.insert(&a.id)
                && text(&a.methodology_source)
                && text(&a.fleet)
                && text(&a.factor_revision)
                && a.period == context.period,
            "duplicate activity or missing methodology",
        )?;
        a.vehicle_km.validate()?;
        a.operational_kg_per_vehicle_km.validate()?;
        r.vehicle_km.low += a.vehicle_km.low;
        r.vehicle_km.central += a.vehicle_km.central;
        r.vehicle_km.high += a.vehicle_km.high;
        r.operational_kg_co2e.low += a.vehicle_km.low * a.operational_kg_per_vehicle_km.low;
        r.operational_kg_co2e.central +=
            a.vehicle_km.central * a.operational_kg_per_vehicle_km.central;
        r.operational_kg_co2e.high += a.vehicle_km.high * a.operational_kg_per_vehicle_km.high;
        r.methodology_sources.push(a.methodology_source.clone());
    }
    r.vehicle_km.validate()?;
    r.operational_kg_co2e.validate()?;
    Ok(r)
}
