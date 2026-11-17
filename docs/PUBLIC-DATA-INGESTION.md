# Public data connectors and provenance

E03 provides an offline Rust library in `mobility_ingest::connectors` and the
`ingest-data` CLI. Run these commands from the repository root after the
[development setup](DEVELOPMENT.md).

```sh
cargo run --locked -p mobility-ingest --bin ingest-data -- inventory 2026-11-17 fixtures/ingestion/upload.json
cargo run --locked -p mobility-ingest --bin ingest-data -- preview fixtures/ingestion/upload.json 2026-11-17
cargo run --locked -p mobility-ingest --bin ingest-data -- import fixtures/ingestion/upload.json 2026-11-17
```

The fixture contains synthetic facility names and visit counts. Output goes to
stdout. Preview validates the full input and displays counts and up to five
records per collection. Import emits the complete normalized candidate. Both
include validation diagnostics. A data validation failure exits 2 and returns
`data: null`. An invalid manifest, command, or project operation exits 1.
Diagnostics identify the source file and, where available, the CSV starting
line, feature or Parquet row number, OSM object ID, or GTFS entity. The first 100
errors are retained and further errors are counted.

## Request and source inventory

Each JSON request has `schema_version: 1`, a `source` object and an `input`
object. See the [sample request](../fixtures/ingestion/upload.json). Source
metadata includes `id`, `version`, `uri`, `attribution`, `license_terms`,
`retrieved_on` (ISO date), `geography`, `coverage`, and `review`. The review
states are `pending`, `redistribution_approved`, and `restricted`. The caller
records its source review and any license obligations. Import success by itself
does not grant redistribution rights. Keep credentials out of source URIs and
screen uploaded columns and sample output for personal or licensed information.
For restricted sources, keep normalized outputs and sidecars in private storage.

`inventory AS_OF REQUEST...` produces an ordered inventory without reading
dataset files. Identical sources coalesce by ID and version. Conflicting metadata
for the same ID/version is an error. Each listed source has an effective
assessment date and age in calendar days. Supply that date explicitly to make
freshness reproducible. Retrieval dates in the future relative to it are errors.
Age helps a reviewer assess currency against the source's release schedule.

Input filenames are relative to the request directory. Absolute paths, parent
traversal and symlinks resolving outside the source directory are rejected.
The connectors use local UTF-8 text or binary snapshots. Prepare archives with
your normal acquisition tools before running the CLI. It does not fetch
credentials, feeds, or source URLs.

## Source formats and normalized data

The JSON output envelope has `schema_version`, `kind`, `inventory`, optional
`realtime_inventory`, `report` and `data`. Records inherit the enclosing
source identity. The schedule tables and optional realtime payload each use
their own inventory entry. Raw source values and normalized fields have the
following contracts.

| Story | Input kind and manifest keys | Output and coverage |
| --- | --- | --- |
| E03-S01 | `osm` with `path` to an OSM 0.6 XML extract | `ways` by OSM ID with tags and ordered segments, `nodes` with WGS84 positions and tags, and `restrictions` with ordered typed members and tags. Each segment uses adjacent OSM node IDs and a zero-based index within the way, in source order. Missing or disconnected restriction references and unresolved way nodes report errors. Import retains access and restriction tags for downstream interpretation. [OSM restriction specification](https://wiki.openstreetmap.org/wiki/Relation:restriction) |
| E03-S02 | `gtfs` with `directory` containing an extracted schedule, plus optional `realtime: {"path":"snapshot.pb","source":{…}}` | `schedule` groups CSV rows by filename and keeps original fields. The adapter reads agency, routes, stops, trips, stop times, weekly calendars, calendar exceptions, transfers, and frequencies. It validates IDs, supported references, service-day times, and frequency windows. Agency timezone stays as supplied. Optional `realtime` holds a timestamped full GTFS-Realtime protobuf snapshot decoded with the feed schema. It leaves dynamic entities separate from the schedule. [Schedule specification](https://gtfs.org/documentation/schedule/reference/) and [Realtime specification](https://gtfs.org/documentation/realtime/reference/) |
| E03-S03 | `census_population` with `path`, `geography_vintage: 2020`, and `level` of `state`, `county`, `tract` or `block` | `rows` with GEOID, population in people, and source-row number from the saved 2020 Decennial Census PL API JSON response containing `P1_001N` and the relevant geography columns. [Census Decennial API](https://www.census.gov/data/developers/data-sets/decennial-census.html) |
| E03-S03 | `lodes` with `path`, `geography_vintage` of 2010 or 2020, `reference_year`, `job_type` of JT00 through JT05, and `segment` of `main` or `aux` | `rows` with home and work block GEOIDs as strings, `S000` jobs, and source creation date from the decompressed LODES OD CSV. Home and work describe employment links. The selected job population and same-state or cross-state scope remain explicit. [Census LODES technical documentation](https://lehd.ces.census.gov/data/lodes/LODES8/LODESTechDoc8.5.pdf) |
| E03-S04 | `places` with `path` to a WGS84 GeoJSON FeatureCollection | `destinations` from Points and `land_use` from Polygons/MultiPolygons. Point categories include education, health, food, food service, civic, retail, recreation and other, derived from OSM-style property values. Land-use categories use the `landuse` property or `other`. Identical payloads with the same feature ID coalesce. A repeated ID with conflicting geometry or properties fails. ID-less identical geometry and properties coalesce. All original feature indices are retained. [GeoJSON specification](https://www.rfc-editor.org/rfc/rfc7946) |
| E03-S05 | `dot` with `path` and `columns` mapping `station`, `longitude`, `latitude`, `timestamp`, `duration_seconds`, `metric`, `value`, `unit`, `quality` to supplied CSV headers | `rows` preserve station ID, WGS84 point, interval start as RFC 3339 with a known offset, interval seconds, source quality and original unit. Metrics are `count` in vehicles or `speed` in mph, km/h or m/s. Supply measured intervals and describe direction, aggregation and screening in source coverage when relevant. Agency-specific raw file layouts may require export before mapping. [FHWA Traffic Monitoring Guide](https://www.fhwa.dot.gov/policyinformation/tmguide/tmg_2022/traffic-data-collection.cfm) |
| E03-S06 | `crashes` with `path` and `columns` mapping `event_id`, `longitude`, `latitude`, `date`, `severity`, `uncertainty` | `rows` preserve event IDs, dates, WGS84 location, original severity code and distinct `fatal`, `nonfatal`, `unknown` classifications. Codes can be mapped using `columns.severity_codes` with those three disjoint arrays. Use uncertainty text to preserve available quality and geolocation limits, or explicitly enter `unknown`. Document population coverage separately for each source, especially fatal-only or sampled data. [NHTSA crash data systems](https://www.nhtsa.gov/data/crash-data-systems) |
| E03-S06 | `emissions` with `path` and `columns` mapping `area_id`, `date`, `pollutant`, `value`, `unit`, `basis`, `uncertainty` | `rows` preserve finite nonnegative quantities or rates without conversion, original unit, `observed`, `modeled` or `imputed` basis, and uncertainty text with its interpretation and units, or `unknown`. State reporting period and coverage in source metadata. [EPA MOVES](https://www.epa.gov/moves) |
| E03-S07 | Every source uses the same inventory contract | Source attribution, license, geography, coverage, retrieval date and review are required. Freshness is computed relative to the supplied date. |
| E03-S08 | `custom` with `path`, `format` of `csv`, `geojson` or `parquet`, and `fields` | `rows` retain source-row numbers and typed mapped fields. Flat Parquet scalar columns support uncompressed, Snappy, Zstd, and gzip data. GeoJSON exposes feature properties plus `@id` and `@geometry`. |

The schedule subset covers fixed-stop service. GTFS Flex location definitions,
realtime differentials, inference of waits, and realtime reconciliation are
outside the adapter's scope. OSM XML files containing export-clipped roads with
missing nodes need the referenced nodes included before import. Geography
mapping retains source identifiers and vintage without crosswalking between
boundary years. GeoJSON geometry checks validate structure and coordinate ranges.
Topological normalization and cross-source deduplication are later stages.

## Map, preview, apply and roll back a custom upload

Each entry in `fields` names a target field and supplies `column`, `type`, and
`required`. Types are `text`, `integer`, `number`, `boolean` and `json`.
CSV converts text to the requested type. GeoJSON and Parquet require compatible
native types. Parquet maps text, integers, finite floating-point values and
booleans. CSV accepts LF and CRLF record separators. Missing optional values become JSON null. GeoJSON geometry can be
mapped as `json`. Mapped JSON values allow up to 32 nested levels. Mapping is declarative and evaluates no formulas.

```sh
demo_dir=$(mktemp -d)
cargo run --locked -p mobility-api --bin study-area -- init cbsa:38900 2026-11-17 "$demo_dir/metro.tsv"
cargo run --locked -p mobility-ingest --bin ingest-data -- apply fixtures/ingestion/upload.json 2026-11-17 "$demo_dir/metro.tsv"
cargo run --locked -p mobility-ingest --bin ingest-data -- active "$demo_dir/metro.tsv"
cargo run --locked -p mobility-ingest --bin ingest-data -- rollback "$demo_dir/metro.tsv"
```

Apply validates the input and rereads the staged file before atomically installing
`metro.tsv.upload.json`. The v1 sidecar records the project ID, baseline ID and
snapshot date with the complete candidate. Apply refuses to overwrite an
existing sidecar. Active reads it only if the baseline identity matches.
Rollback removes a recognized sidecar for the project and succeeds as a no-op
when none exists. The existing TSV project is read-only during these operations.
The advisory file lock serializes ingestion CLI operations against the same
baseline file. Run study-area edits and uploads serially, since E02 project
operations can replace the file. Applying a sidecar does not mark E02 coverage
as complete or change a scenario. Source ingestion supplies evidence for
later normalization and calibration.

Inputs are limited to 128 MiB per file, 500,000 records per table or source
collection, and 128 mapped fields. Sidecars are limited to 128 MiB. Parquet
also checks the declared uncompressed size. Use source extracts within these
limits and locally acquired inputs in a trusted working directory.

## Validation and complexity

The acceptance suite is [`e03_connectors.rs`](../crates/mobility-ingest/tests/e03_connectors.rs).
It uses deterministic synthetic source rows, a compressed Parquet file with
64-bit integers, a protobuf feed, and an E02 project from the bundled catalogue.
CSV validation and parsing take two linear passes over input bytes. Ordered
identity/reference checks add logarithmic work per lookup or insertion. OSM
segment building is linear in the number of adjacent pairs with indexed
node lookup. Restriction validation also checks junction membership between
the referenced ways, building temporary sets for via-way intersections.
Working memory scales with the retained input and normalized output. Preview
bounds the displayed sample while examining the complete source.

On a Darwin arm64 development host with Rust 1.97.1, release-mode preview of
100,000 synthetic CSV rows (1,778,906 bytes) took 0.289, 0.066 and 0.065 seconds.
Peak child RSS across the three runs was 153 MB. The sample contained two mapped
columns per row, and every preview reported a count of 100,000. Reproduce from
the repository root using a new directory:

```sh
cargo build --release --locked -p mobility-ingest --bin ingest-data
bench_dir=$(mktemp -d)
python3 - "$bench_dir" <<'PY'
import json
from pathlib import Path
import sys

directory = Path(sys.argv[1])
request = json.loads(Path("fixtures/ingestion/upload.json").read_text())
request["input"]["path"] = "bench.csv"
(directory / "bench.json").write_text(json.dumps(request))
with (directory / "bench.csv").open("w") as rows:
    rows.write("facility,visits\n")
    for index in range(100_000):
        rows.write(f"Facility {index},{index % 100}\n")
PY
time target/release/ingest-data preview "$bench_dir/bench.json" 2026-11-17
```

This measures input validation and normalized preview with decoded rows held in
memory. Resource use depends on input format and field sizes.
