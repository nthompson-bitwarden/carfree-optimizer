//! E04 offline, deterministic normalization of E03 inputs.
mod geometry;
mod lineage;
mod spatial;
mod temporal;

use super::*;
use serde_json::json;
use std::collections::BTreeSet;

pub use lineage::{build, load, rebuild};
pub use temporal::service_instant;

pub const SNAPSHOT_SCHEMA: u32 = 1;
pub const PROCESSING_VERSION: &str = "e04-normalization-v1";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub schema_version: u32,
    pub as_of: String,
    /// Layer name to expected region IDs. These are import partitions, not polygons.
    pub expected: BTreeMap<String, BTreeSet<String>>,
    pub inputs: Vec<RegionalInput>,
    #[serde(default)]
    pub stop_fixes: Vec<StopFix>,
    /// Independently identified public pedestrian entry nodes, by regional OSM partition.
    #[serde(default)]
    pub walk_entries: BTreeMap<String, BTreeSet<i64>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionalInput {
    pub region: String,
    pub request: Request,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopFix {
    pub region: String,
    pub feed: String,
    pub feed_version: String,
    pub stop: String,
    pub network: String,
    pub network_version: String,
    pub node: i64,
    pub reviewer: String,
    pub reviewed_on: String,
    pub evidence: String,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawFile {
    pub input: usize,
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub processing_version: String,
    pub crs: String,
    pub axis_order: [String; 2],
    pub recipe: Recipe,
    pub raw: Vec<RawFile>,
    pub layers: Vec<Layer>,
    pub quality: BTreeMap<String, Quality>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Layer {
    /// Index into recipe.inputs (and key into raw).
    pub input: usize,
    pub source: Candidate,
    pub normalized: Value,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Finding {
    pub code: String,
    pub record: String,
    pub detail: String,
    pub invalid: bool,
}

fn finding(code: &str, record: impl ToString, detail: impl ToString, invalid: bool) -> Finding {
    Finding {
        code: code.into(),
        record: record.to_string(),
        detail: detail.to_string(),
        invalid,
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Quality {
    pub missing_regions: Vec<String>,
    pub rejected_regions: Vec<String>,
    pub review_regions: Vec<String>,
    pub invalid_findings: usize,
    pub completeness: f64,
    pub uncertainty: f64,
    pub confidence: String,
}

const KINDS: [&str; 9] = [
    "osm",
    "gtfs",
    "census_population",
    "lodes",
    "places",
    "dot",
    "crashes",
    "emissions",
    "custom",
];

fn kind(input: &Input) -> &'static str {
    KINDS[match input {
        Input::Osm { .. } => 0,
        Input::Gtfs { .. } => 1,
        Input::CensusPopulation { .. } => 2,
        Input::Lodes { .. } => 3,
        Input::Places { .. } => 4,
        Input::Dot { .. } => 5,
        Input::Crashes { .. } => 6,
        Input::Emissions { .. } => 7,
        Input::Custom { .. } => 8,
    }]
}

fn label(s: &str) -> bool {
    !s.trim().is_empty() && !s.chars().any(char::is_control)
}

impl Recipe {
    fn validate(&self) -> Result<()> {
        date_index(&self.as_of)?;
        if self.schema_version != SNAPSHOT_SCHEMA
            || self.expected.is_empty()
            || self.inputs.len() > 128
        {
            return Err(Error::new(
                "expected recipe schema 1, nonempty expectations and at most 128 inputs",
            ));
        }
        for (kind, regions) in &self.expected {
            if !KINDS.contains(&kind.as_str())
                || regions.is_empty()
                || !regions.iter().all(|r| label(r))
            {
                return Err(Error::new(format!("invalid expected regions for {kind}")));
            }
        }
        let mut seen = BTreeSet::new();
        let mut metadata = BTreeMap::new();
        for entry in &self.inputs {
            let kind = kind(&entry.request.input);
            if !self
                .expected
                .get(kind)
                .is_some_and(|r| r.contains(&entry.region))
            {
                return Err(Error::new(format!(
                    "unexpected {kind} region {}",
                    entry.region
                )));
            }
            if !seen.insert((kind, &entry.region)) {
                return Err(Error::new(format!(
                    "multiple {kind} inputs for region {}; partition inputs explicitly",
                    entry.region
                )));
            }
            let (source, realtime) = request_inventory(&entry.request, &self.as_of)?;
            for item in std::iter::once(source).chain(realtime) {
                let key = (item.source.id.clone(), item.source.version.clone());
                if metadata
                    .insert(key, item.source.clone())
                    .is_some_and(|prior| prior != item.source)
                {
                    return Err(Error::new(
                        "conflicting metadata for a source ID and version",
                    ));
                }
            }
        }
        let mut fixes = BTreeSet::new();
        for fix in &self.stop_fixes {
            if ![&fix.reviewer, &fix.evidence, &fix.stop]
                .into_iter()
                .all(|s| label(s))
                || date_index(&fix.reviewed_on)? > date_index(&self.as_of)?
                || !fixes.insert((&fix.region, &fix.feed, &fix.stop))
            {
                return Err(Error::new(format!(
                    "invalid or duplicate review for stop {}",
                    fix.stop
                )));
            }
        }
        Ok(())
    }
}

fn summarize(recipe: &Recipe, layers: &[Layer]) -> BTreeMap<String, Quality> {
    let by_partition: BTreeMap<_, _> = layers
        .iter()
        .map(|layer| {
            (
                (&layer.source.kind, &recipe.inputs[layer.input].region),
                layer,
            )
        })
        .collect();
    recipe
        .expected
        .iter()
        .map(|(kind, regions)| {
            let (mut missing, mut rejected, mut review) = (vec![], vec![], vec![]);
            let mut invalid = 0;
            for region in regions {
                let Some(layer) = by_partition.get(&(kind, region)) else {
                    missing.push(region.clone());
                    continue;
                };
                invalid += layer.source.report.errors.len()
                    + layer.source.report.omitted_errors
                    + layer.findings.iter().filter(|f| f.invalid).count();
                if layer.normalized.is_null() {
                    rejected.push(region.clone());
                } else if !layer.findings.is_empty() || !layer.source.report.warnings.is_empty() {
                    review.push(region.clone());
                }
            }
            let absent = missing.len() + rejected.len();
            let confidence = match (absent, review.len()) {
                (n, _) if n == regions.len() => "unavailable",
                (0, 0) => "screened",
                _ => "review_required",
            };
            (
                kind.clone(),
                Quality {
                    completeness: (regions.len() - absent) as f64 / regions.len() as f64,
                    uncertainty: (absent + review.len()) as f64 / regions.len() as f64,
                    confidence: confidence.into(),
                    missing_regions: missing,
                    rejected_regions: rejected,
                    review_regions: review,
                    invalid_findings: invalid,
                },
            )
        })
        .collect()
}

fn defer_fixes(
    fixes: &BTreeMap<(&str, &str, &str), (usize, &StopFix)>,
    region: &str,
    feed: &Source,
    used: &mut BTreeSet<usize>,
    findings: &mut Vec<Finding>,
) {
    for (&(fix_region, fix_feed, stop), &(index, fix)) in fixes {
        if fix_region == region && fix_feed == feed.id && fix.feed_version == feed.version {
            used.insert(index);
            findings.push(finding(
                "stop_fix_unevaluated",
                stop,
                "required GTFS or OSM partition rejected; reviewed fix not applied",
                false,
            ));
        }
    }
}

fn derive(recipe: Recipe, raw: Vec<RawFile>, sources: Vec<Candidate>) -> Result<Snapshot> {
    let mut networks = BTreeMap::new();
    let mut layers = sources
        .into_iter()
        .enumerate()
        .map(|(input, source)| Layer {
            input,
            source,
            normalized: Value::Null,
            findings: vec![],
        })
        .collect::<Vec<_>>();
    for layer in &mut layers {
        if layer.source.is_valid() && layer.source.kind == "osm" {
            let network = spatial::network(&layer.source.data, &mut layer.findings)?;
            layer.normalized = json!(&network);
            networks.insert(
                &recipe.inputs[layer.input].region,
                (layer.source.inventory.source.clone(), network),
            );
        }
    }
    let rejected_networks: BTreeSet<_> = layers
        .iter()
        .filter(|l| l.source.kind == "osm" && !l.source.is_valid())
        .map(|l| &recipe.inputs[l.input].region)
        .collect();
    for (region, entries) in &recipe.walk_entries {
        if !entries.is_empty() && rejected_networks.contains(region) {
            continue;
        }
        if entries.is_empty()
            || !networks
                .get(region)
                .is_some_and(|(_, network)| entries.iter().all(|node| network.is_walk_node(*node)))
        {
            return Err(Error::new(format!(
                "walk entry nodes for {region} must exist on its screened OSM walk network"
            )));
        }
    }
    let mut used = BTreeSet::new();
    let fixes: BTreeMap<_, _> = recipe
        .stop_fixes
        .iter()
        .enumerate()
        .map(|(i, f)| {
            (
                (f.region.as_str(), f.feed.as_str(), f.stop.as_str()),
                (i, f),
            )
        })
        .collect();
    for layer in &mut layers {
        let region = &recipe.inputs[layer.input].region;
        let rejected_network = rejected_networks.contains(region);
        if !layer.source.is_valid() {
            if layer.source.kind == "gtfs" {
                defer_fixes(
                    &fixes,
                    region,
                    &layer.source.inventory.source,
                    &mut used,
                    &mut layer.findings,
                );
            }
            if layer.source.kind == "osm" && recipe.walk_entries.contains_key(region) {
                layer.findings.push(finding(
                    "walk_entries_unevaluated",
                    region,
                    "OSM input rejected",
                    false,
                ));
            }
            continue;
        }
        match layer.source.kind.as_str() {
            "osm" => {}
            "gtfs" => {
                let Some(schedule) = temporal::schedule(&layer.source.data, &mut layer.findings)?
                else {
                    defer_fixes(
                        &fixes,
                        region,
                        &layer.source.inventory.source,
                        &mut used,
                        &mut layer.findings,
                    );
                    continue;
                };
                let empty_fixes = BTreeMap::new();
                if rejected_network {
                    defer_fixes(
                        &fixes,
                        region,
                        &layer.source.inventory.source,
                        &mut used,
                        &mut layer.findings,
                    );
                }
                let attachments = spatial::stops(
                    &layer.source,
                    region,
                    networks
                        .get(region)
                        .zip(recipe.walk_entries.get(region))
                        .map(|((source, network), entries)| (source, network, entries)),
                    if rejected_network {
                        &empty_fixes
                    } else {
                        &fixes
                    },
                    &mut used,
                    &mut layer.findings,
                )?;
                layer.normalized = json!({"service_times": schedule, "stop_access": attachments});
            }
            "dot" => layer.normalized = temporal::dot(&layer.source.data, &mut layer.findings)?,
            "places" => {
                if geometry::screen(&layer.source.data, &mut layer.findings)? {
                    layer.normalized = json!({"representation":"validated_source_data"});
                }
            }
            _ => layer.normalized = json!({"representation":"validated_source_data"}),
        }
    }
    if let Some((_, fix)) = recipe
        .stop_fixes
        .iter()
        .enumerate()
        .find(|(i, _)| !used.contains(i))
    {
        return Err(Error::new(format!(
            "unused stop fix for {} in {}; check region, source and stop",
            fix.stop, fix.region
        )));
    }
    let quality = summarize(&recipe, &layers);
    Ok(Snapshot {
        schema_version: SNAPSHOT_SCHEMA,
        processing_version: PROCESSING_VERSION.into(),
        crs: "EPSG:4326".into(),
        axis_order: ["longitude".into(), "latitude".into()],
        recipe,
        raw,
        layers,
        quality,
    })
}
