# Offline geospatial normalization and quality (E04)

`mobility-ingest` owns normalization, anomaly detection, pedestrian screening,
and immutable replay. `mobility-api` renders the resulting quality dashboard
with no server, JavaScript, network calls, or browser-side scoring. E02 project
TSV and E03 connector and upload schemas remain unchanged.

## Run the synthetic demo

From the repository root, with the [development prerequisites](DEVELOPMENT.md):

```sh
store=$(mktemp -d)
id=$(cargo run --locked --quiet -p mobility-ingest --bin ingest-data -- \
  normalize fixtures/normalization/recipe.json "$store")
cargo run --locked -p mobility-api --bin quality-dashboard -- \
  "$store" "$id" > "$store/quality.html"
cargo run --locked -p mobility-ingest --bin ingest-data -- rebuild "$store" "$id"
```

Open `quality.html` locally. The deliberately synthetic fixture has a missing DOT
partition next to a reported vehicle count of **zero**, a rejected destination
layer with invalid coordinates, road anomalies, and three stops: a coincident
walk node, a reviewed attachment, and a stop on an unanchored footpath.

## Recipe and snapshot v1

Use `fixtures/normalization/recipe.json` as a schema example. A recipe contains
`schema_version: 1`, an `as_of` date (YYYY-MM-DD), an `expected` map of E03 input
kinds to nonempty sets of region IDs, and up to 128 `inputs`. An input binds a
region to one complete [E03 request](PUBLIC-DATA-INGESTION.md); source paths are
relative to the recipe. Partition inputs beforehand: at most one input per
kind and region, and topology does not join networks across partitions. Region
IDs are user-defined import partitions, **not** geographic boundaries. The
recipe author is responsible for geographic attribution and using consistent
region IDs and vintages. An absent input is missing, a failed E03 or timezone
or polygon topology validation is rejected, and a successful partition with
findings is marked for review. If a supplied OSM or GTFS partition is rejected,
its diagnostics remain visible and dependent walk entries or fixes are reported
as unevaluated rather than applied. A stop without a usable walk network and
entry nodes has status `unevaluated`, not `unreachable_in_screened_network`. File/archive I/O and recipe errors fail the command instead of producing
a snapshot. A successful command means the **report** was created, not that its
data passed validation or is usable for routing.

| Story | Contract and test evidence |
| --- | --- |
| #41 / E04-S01 | Input geometries use EPSG:4326, `[longitude, latitude]`. E03 checks WGS84 bounds, supported types/ring closure and OSM references; no automatic reprojection or full GIS topology certification. Unmapped custom fields keep their source semantics. Derived edge lengths use spherical metres, and DOT speeds use m/s (1 mph = 0.44704 m/s). Offset-bearing DOT times have a UTC representation. GTFS times are elapsed service seconds (no modulo 24h), using one IANA agency timezone per feed. `service_instant` uses local noon minus 12 elapsed hours with the pinned tzdb, leaving calendar eligibility to callers. Declared source units, dates, observed/imputed/modeled basis and original values remain in E03 data and raw inputs. IDs are source-scoped: join by layer, source ID/version and entity type where applicable (e.g. OSM way plus segment index); use documented Census GEOID level and vintage. Never join GTFS numeric-looking IDs across feeds by parsing or dropping leading zeroes. |
| #42 / E04-S02 | Build a candidate topology from source node references. With no turn restrictions in the extract, snap only two untagged degree-one nodes at exactly the same coordinates on distinct same-tag, named/ref-tagged surface ways. Alias to the lower node ID and report the inference. Never infer junctions at geometric crossings or across a grade change. Retain segment order, source ways, access and restriction evidence. Detect same-grade coincident edge geometry, dangling endpoints, impossible segments, different-grade through ways at shared nodes and weak components; duplicates stay in the QA output for review. Weak components ignore direction; this is not a directed or turn-aware routing graph. |
| #43 / E04-S03 | `walk_entries` maps each region to independently identified public pedestrian network node IDs. Screen conservative bidirectional surface paths with explicit pedestrian evidence and no screened access/barrier conflicts. A stop/platform (`location_type=0` or omitted) automatically attaches only to a unique coincident eligible node whose pedestrian component reaches a declared entry. No inference from the nearest roadway or an isolated path. Optional `stop_fixes` specify the region, feed ID/version, stop ID, network ID/version, OSM node, reviewer, review date and evidence of a real connection. A fix must reach an entry on that same screened network and be within 100 m; this is a geometric guardrail, not a traversal time or permission. Fixes retain the original coordinates and record before/after, separation, and the recipe review reference. Update source versions when inputs change and recheck fixes. |
| #44 / E04-S04 | Completeness is the fraction of expected regions with successfully normalized inputs. Uncertainty is the fraction missing, rejected or marked for review. Confidence tiers are `unavailable` (no normalized partitions), `screened` (all present and no findings), `review_required` (everything else). These scores are operational indicators, not statistical confidence, observed coverage within polygons, or calibration. Counts are findings, not distinct invalid records. The HTML exposes missing, rejected and review regions, finding details, attribution, and the snapshot reference. |
| #45 / E04-S05 | Review OSM posted speed if unsupported or outside (0, 55] m/s, DOT observed speed outside [0, 55] m/s, and tagged lanes outside integer 1–12. OSM numeric default is km/h; explicit `mph` and `km/h` also parse. Connectivity findings are advisory except physically degenerate segments, which are excluded from the candidate topology only. Thresholds are screening heuristics, not local regulations. Findings reference a source row or entity; source values are never overwritten and missing values are never invented. |
| #46 / E04-S06 | Parse staged copies of exact archived bytes. `raw/<sha256>` contains each unique raw file; `snapshots/<sha256>.json` contains the versioned recipe, manifest (input index, relative path, byte count and raw SHA-256), all connector candidates, derived data, findings and quality. GTFS includes all consumed tables and optional realtime when present. `rebuild` supplies only archived inputs, verifies digests and checks the regenerated snapshot hash. Keep the store and processing release together; record the snapshot hash with any downstream scenario or exported report. The dashboard references this manifest by its digest. |

Land-use polygon screening additionally checks planar ring topology, including
self-intersections and invalid holes, using the study-area polygon validator.
It rejects unsupported polar/dateline polygons and bounds screening to 10,000
vertices per component and a cumulative sum of squared vertex counts of 50
million per partition. Multipart geometries require review for interpart
contacts and overlap; custom uploads retain their E03 geometry contract.

Acceptance tests: `crates/mobility-ingest/tests/e04_normalization.rs` (E04-S01–S06)
and `crates/mobility-api/tests/e04_quality.rs` (E04-S04). The dashboard CLI checks
the snapshot hash; replay separately verifies the raw blobs. Any processing
change that affects replay must bump `processing_version`. Replay is checked
with the same processing release; cross-platform bitwise floating-point
reproducibility is not claimed.

Use a private, trusted store. Immutable no-clobber publication and hash checks
are not authentication, file permission enforcement, or offsite backup. A
failed attempt can leave reusable unreferenced raw blobs but no partially
written snapshot. Raw inputs and snapshots can contain restricted or sensitive
information (including source rows and reviewer metadata); do not publish them
without reviewing license, attribution, consent and redaction requirements.
Derived JSON and total raw inputs are each capped at 512 MiB on disk, not in
memory; partition large imports. The static HTML includes source metadata and
selected input diagnostics; review it separately before sharing.

**Scientific gate:** These are quality and provenance artifacts, not realistic
journeys. Station paths, crossings, barriers, mobility constraints, wait times,
schedule coverage, and infeasible essential trips still require measurement
and calibration. No policy recommendation or car-independence finding follows
from data completeness, an attached stop or the presence of transit.
