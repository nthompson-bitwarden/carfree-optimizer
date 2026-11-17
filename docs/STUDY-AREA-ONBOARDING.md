# Study-area onboarding

E02-S02–S06 build on [city/metro search](GEOGRAPHY-SEARCH.md). The offline
`study-area` CLI saves repeatable geographic project inputs and makes data gaps
visible. The bundled catalogue supports Portland, ME (`place:2360545`), Portland,
OR (`place:4159000`), and Portland-Vancouver-Hillsboro, OR-WA Metro Area
(`cbsa:38900`). Drawn boundaries can cover other supported polygon shapes, but
do not expand the source-data catalogue.

Creating a project imports and validates the available local boundary metadata.
It does not create a transportation network: roads, transit, population and
demand adapters remain absent. No trip feasibility, mobility-constraint, car
independence, calibration or policy result is produced. Census boundaries retain
their [source attribution, vintage and limitations](../fixtures/geography/README.md).
Drawn geometry is user input; envelopes are derived geometry. Neither is an
observed transportation measurement.

## Choose, save and inspect a boundary

Run from the repository root with the pinned Rust toolchain. Every output path is
explicit; there is no default output directory. Use new filenames for `init`,
`draw` and `preview`, which refuse to overwrite existing files.

```sh
cargo run -p mobility-api --bin study-area -- init cbsa:38900 2026-11-17 metro.tsv
cargo run -p mobility-api --bin study-area -- preview metro.tsv metro.html
```

Open `metro.html` in a browser. The standalone report includes an accessible
labelled SVG, original core, CRS/region, snapshot date, baseline, import count,
source/version/year/extent/status table, missing required inputs and scenario
status. Dynamic text is HTML-escaped. The map is a schematic longitude/latitude
preview, not a surveyed boundary. Source accuracy confidence intervals are not
supplied by the pilot.

`init` accepts a canonical ID, not a search query; use `city-search` to resolve an
ambiguous name first. Its project ID replaces the canonical colon with a hyphen
(for example, `cbsa-38900`). Project and scenario IDs contain 1–128 ASCII letters,
digits, hyphens or underscores. Invalid dates, unsupported IDs/CRS, malformed
geometry, missing files and write failures produce an error and nonzero exit.
New projects start at revision 1 with one successful ingestion and no scenarios.

## Supply drawn rings

Create a UTF-8 TSV with one `ring` record per closed ring, no header. Vertices
are tab-separated `longitude,latitude` pairs in degrees; repeat the first vertex
at the end. This example is synthetic, not a measured city boundary:

```sh
python3 - <<'PY'
from pathlib import Path
Path("drawn-rings.tsv").write_text(
    "ring\t-123,45\t-122.9,45\t-122.9,45.1\t-123,45.1\t-123,45\n",
    encoding="utf-8",
)
PY
cargo run -p mobility-api --bin study-area -- draw synthetic-area 'Synthetic core' OR 2024-02-29 drawn-rings.tsv drawn.tsv
cargo run -p mobility-api --bin study-area -- preview drawn.tsv drawn.html
```

The CLI fixes CRS to `EPSG:4326`, serialization order longitude then latitude,
and country to `US`. The states argument is comma-separated (for example,
`OR,WA`). Region metadata is user supplied: state codes are checked for distinct
uppercase two-letter syntax, not geographic intersection with the polygon.
Metadata must be nonempty and free of control characters. A drawn area retains
user provenance even if its ID resembles a canonical Census ID.

Supported geometry consists of disjoint shells and nested rings interpreted by
even-odd containment: nesting depth alternates shell, hole, island, and so on.
Ring order and winding do not encode shell/hole roles. Every ring is preserved;
the report uses SVG `fill-rule="evenodd"`. Rings need at least four vertices
including exact closure, nonzero area and finite coordinates strictly inside
longitude `(-180,180)` and latitude `(-90,90)`. The total is limited to 10,000
vertices, including closures.

Validation rejects self-crossing/touching rings, overlapping edges, duplicate
adjacent vertices, and antimeridian-crossing edges. Separate rings may meet at
one existing shared vertex when containment remains unambiguous; multiple
contacts per pair, more than two rings at one contact, and contact cycles are
unsupported. Predicates use exact comparisons of `f64` calculations, without
snapping or a robust GIS arithmetic library. Numerically difficult inputs can
require external GIS review; successful validation is not legal/engineering
certification.

## Preview dataset coverage

Every accepted project report contains exactly one entry for each required kind:
`Boundary`, `Roads`, `Transit`, `Population`, and `Demand`. An entry records source,
version, year, bounding extent `[west,south,east,north]` in degrees, status and
details. Unknown metadata is absent (`Unknown` in HTML), not invented.

| Status | Meaning in the local catalogue importer |
| --- | --- |
| `Available` | Exact selected source boundary at a compatible vintage, or supplied drawn geometry. This says nothing about model calibration. |
| `Partial` | A source-backed project's envelope differs from the original source boundary; only the original source extent is available. |
| `Outdated` | The source vintage is later than the requested snapshot year and cannot stand in for historical inputs. The enum name includes this temporal mismatch. |
| `Missing` | No source was imported. Currently used for all four transportation/model input kinds. |
| `Failed` | Available for custom importers to report an explicit dataset failure; the bundled importer does not emit it. |

The missing-required list must exactly match every entry whose status is not
`Available`. Custom importers cannot omit a kind to imply readiness. Even when a
custom importer supplies all five kinds, importing data does not verify
calibration. The bundled importer always reports four missing kinds and the
HTML states that a model cannot be initialized. Vintage compatibility is checked
by year; no within-year historical boundary reconstruction is performed.

Source-backed areas retain original URL, vintage, retrieval date and licence.
Every ingestion revalidates the active catalogue geometry and provenance against
the recorded source. Unavailable or changed sources fail explicitly. Loading an
existing snapshot for viewing does not require reimporting it. No command
downloads data.

## Expand a buffer and track earlier scenarios

```sh
cargo run -p mobility-api --bin study-area -- scenario metro.tsv prior-plan
cargo run -p mobility-api --bin study-area -- buffer metro.tsv 10
cargo run -p mobility-api --bin study-area -- preview metro.tsv expanded.html
```

`scenario` saves a baseline reference only; it runs no simulation. `buffer` takes
the requested total distance in kilometres around the original core, not an
increment around the previous envelope. A positive distance generates a
conservative spherical bounding envelope, not an exact polygon-distance buffer
or an inferred commuter shed. The original rings remain unchanged separately.
For Earth radius `R = 6371.0088 km`, angular distance is `delta = km/R`;
latitude padding is `delta`, and longitude padding is
`asin(sin(delta)/cos(phi))`, where `phi` is the largest absolute core latitude.
The result is an axis-aligned longitude/latitude rectangle. Distances are on the
stated spherical approximation, not an ellipsoidal surveying model.

Nonfinite/negative distances, shrinking a project buffer, pole-reaching or
antimeridian-crossing envelopes, and overly large spherical caps fail explicitly.
The same distance is a no-op. A successful changed envelope invokes the importer
again, stores its coverage, increments revision and successful import count,
recomputes the baseline, and marks existing scenarios stale. Their old baseline
IDs remain visible after saving/reloading. Import or validation failure leaves
the old project unchanged; failed attempts are not added to its saved counter.
A source-backed envelope generally makes boundary coverage `Partial`; the tool
does not fabricate inputs for the expanded area.

File mutation assumes one writer at a time. Updates write and sync a temporary
file next to the project, check that the old content still matches, then rename.
This gives atomic replacement on the supported local filesystem; the optimistic
check is not a lock and does not prevent all concurrent-writer races. Use separate
files or serialize writers. New files use create-new semantics; input symlinks
and directories are rejected. The code syncs file content, not the parent
directory, so it does not claim crash-durable directory transactions.

## Import several cities

Batch input is UTF-8 TSV without a header: `project-id`, `canonical-id`, and
`YYYY-MM-DD` per row. For example:

```sh
python3 - <<'PY'
from pathlib import Path
Path("requests.tsv").write_text(
    "portland-me\tplace:2360545\t2026-11-17\n"
    "portland-metro\tcbsa:38900\t2026-11-17\n",
    encoding="utf-8",
)
PY
cargo run -p mobility-api --bin study-area -- batch requests.tsv projects
cargo run -p mobility-api --bin study-area -- batch requests.tsv projects
```

The output directory is created when absent. Each row prints its number, project
ID and `Created`, `Unchanged` or `Failed`; errors include the reason. Successful
projects are written to `<output-dir>/<project-id>.tsv`. A bad row does not abort
later rows; any failed row makes the overall command exit nonzero. Blank or
malformed rows fail individually. An empty request file produces no rows.

Replaying an identical request after restarting the CLI returns `Unchanged` and
preserves file bytes. Comparison includes the complete canonical project, not
just its ID: boundary/source versions, date, coverage and baseline all matter.
An existing project with a different request, changed buffer or added scenario
is a conflict reported as `Failed`; it is not overwritten or reset. Unsupported
source geographies, traversal IDs and symlink targets fail explicitly.

## Rust contracts

All additions are under `mobility_ingest::study_area`; existing catalogue/search
APIs and schema remain unchanged.

| API | Contract |
| --- | --- |
| `StudyArea::from_geography(&Geography)` | Validate geometry, region and source metadata; preserve source rings. |
| `StudyArea::drawn(id, name, Region, crs, rings)` | Construct user geometry; only `EPSG:4326` is supported. |
| `StudyArea::buffer_envelope(km)` | Return a derived area from the original core. Project mutation additionally rejects shrinking. |
| `StudyArea` getters | `id`, `name`, `region`, `crs`, `rings`, `core_rings`, `buffer_km`; invariant fields remain private. |
| `Importer::ingest(&mut self, &StudyArea, snapshot_date)` | Return `Result<CoverageReport, OnboardingError>`; implementations own source-specific ingestion. |
| `CatalogueImporter::new(&Catalogue)` | Revalidate bundled boundary sources and report explicit absent transportation inputs. |
| `CityProject::initialize(id, area, snapshot_date, &mut importer)` | Validate ID/date, invoke importer and validate all required coverage before returning a project. |
| `CityProject::{expand_buffer,add_scenario}` | Transactional domain update or baseline-reference insertion; duplicate scenario IDs fail. |
| `CityProject::{to_tsv,from_tsv}` | Produce/validate the canonical versioned snapshot below. |
| `CityProject` getters | `id`, `area`, `snapshot_date`, `baseline_id`, `revision`, `import_attempts`, `coverage`, `scenarios`. |
| `validate_date`, `validate_project_id` | Validate the calendar date or conservative filename-safe identifier. |
| `mobility_api::study_area::{render_project,run}` | Escaped static report and offline CLI/file orchestration; no HTTP service. |

`Region`, `CoverageEntry`, `CoverageReport` and `ScenarioRecord` have public data
fields; accepted project state is validated and exposed through shared getters.
`DatasetKind` and `DatasetStatus` provide `as_str`. `OnboardingError` carries an
actionable message. The API layer owns file I/O, not domain calculations.

## Stored schema: `carfree-study-project-v1`

This is a strict, canonical UTF-8 tagged TSV with LF newlines and a final newline.
Fields are not quoted or escaped: control characters in metadata are rejected.
Read using `CityProject::from_tsv`, rather than editing the checksum or assuming
unknown fields are ignored. Records occur in this order:

| Record | Fields following the tag |
| --- | --- |
| Header | `carfree-study-project-v1` alone |
| `project` | project ID, snapshot date, revision, successful import count |
| `area` | area ID, name, CRS, country, comma-separated states, total buffer km |
| `source` | `none`, or URL, vintage, retrieval date, licence |
| `core` | ring count; followed by that many `ring` rows |
| `boundary` | current ring count; followed by that many `ring` rows |
| `ring` | tab-separated `longitude,latitude` vertices including closure |
| `coverage` | dataset count (exactly five) |
| `dataset` | kind, status, optional source, optional version, optional year, optional extent, details |
| `missing` | comma-separated missing-required kinds, or an empty field |
| `baseline` | deterministic baseline identifier |
| `scenarios` | scenario count |
| `scenario` | scenario ID, referenced baseline ID, `true`/`false` stale flag |

Optional source/version strings use `-` for absent and `=<value>` for present;
year and extent use `-` for absent. Extent is four comma-separated numbers.
Original and current boundaries are serialized fully, including holes/islands.
On reload the current boundary is recomputed from core/buffer and compared;
coverage completeness, baseline, scenario uniqueness/staleness and canonical
serialization are checked. Unknown versions/records, extra rows, noncanonical
representations and corrupted relationships fail explicitly.

Baseline IDs use stable FNV-1a over canonical project content (including ID,
snapshot, revision/import count, geometry, source and coverage), plus a revision
suffix. Scenarios are excluded. This is a reproducibility checksum, not a
cryptographic digest or authenticity proof. A new scenario references the current
baseline; a stale one must reference a different baseline. The saved import count
equals revision. Accepted projects are limited to a complete canonical encoding
of 16 MiB and 10,000 scenarios. Initialization, scenario insertion, buffer
expansion and loading enforce these limits. An insertion or expansion that would
exceed either limit fails before changing the saved project state, so a successful
update remains reloadable within the same schema limits.

## Validation and performance scope

Story-linked tests are in `crates/mobility-ingest/tests/e02_onboarding.rs`
(E02-S02–S05) and `crates/mobility-api/tests/e02_onboarding.rs` (E02-S02–S06).
They exercise all three real pilot geometries, synthetic holes/islands and invalid
topology, source provenance, required gaps, calendar dates, saved-file corruption,
actual importer callbacks, persisted stale scenarios, schema-size/scenario limits
and atomic rejection, CLI output protection, escaping and process-restart batch
idempotency. Existing E02-S01 tests remain.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Polygon validation uses `O(V²)` segment-pair comparisons and `O(V)` geometry
storage, bounded to 10,000 vertices. Initialization revalidates the selected local
source; serialization and file I/O scale with snapshot size. Adding a scenario
checks existing IDs in `O(S)` for `S` scenarios and accounts for added encoded
bytes incrementally, without serializing the geometry again. Loading checks
scenario uniqueness with up to `O(S²)` comparisons. A batch processes rows
serially and reinitializes each requested baseline before comparing files.
For a reproducible local timing scope, build the release CLI and time repeated
`target/release/study-area init` invocations with fresh output files and the three
named pilot IDs. This measures process startup, catalogue/source validation,
coverage, serialization and synchronized new-file writes, not isolated geometry
time, simulation throughput or peak memory. Record toolchain, hardware, fixture
version and distributions when comparing measurements. No simulation-critical
algorithm or calibrated planning model is introduced here.
