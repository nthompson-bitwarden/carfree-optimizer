# Primitive benchmark and regression history

E01-S05 supplies a real Rust release microbenchmark of
`accessibility::reachable_share`, using the versioned eight-row synthetic fixture
`benchmarks/fixtures/reachable-share-v1.csv`. These counts are constructed test
inputs, not observed traffic, population, travel, or ridership data. No seed is
needed: metric inputs and outputs are deterministic; wall-clock timings vary.

From the repository root, create a local history and compare another run:

```sh
python3 scripts/benchmark.py --output /tmp/carfree-benchmark.jsonl
python3 scripts/benchmark.py --output /tmp/carfree-benchmark.jsonl --baseline /tmp/carfree-benchmark.jsonl
```

Without `--output`, results append to `benchmarks/history.jsonl`. Defaults are
`--samples 7 --iterations 100000`; both must be positive. Each sample performs
100,000 traversals of all eight fixture rows in the Rust harness, using
`black_box` on inputs and outputs. Python builds with the pinned Rust compiler
in locked, offline release mode and records the resulting nanosecond timings.
Fixture loading, compiler execution, and Python orchestration are outside the
timed region. No warmup or confidence interval is provided.

Cost is O(1) per metric call, O(iterations × fixture rows) per timed sample, and
O(fixture rows) harness memory. Peak memory is not measured. This small workload
does not establish import, routing, city simulation, FFI, training, or independent
scientific validation performance.

## Routing benchmarks

[E07 routing measurements](ROUTING.md#complexity-and-performance-evidence-s07)
report paths/s, queries/s, nearest-rank p50/p95 latency and process peak RSS at
64/128/256 nodes, with graph build separate. Reproduce each size in its own
process using `e07_routing --benchmark SIZE`; no metro-scale claim is implied.
The [E05 reference benchmark](MULTIMODAL-NETWORK.md#complexity-and-measured-synthetic-benchmark)
retains its original workload and history separately from the primitive schema.

## Stored schema and provenance

Each line is a schema-version-1 JSON object. Existing valid history is retained
byte-for-byte when appending. Fields include:

| Field | Meaning |
| --- | --- |
| `schema_version`, `fixture`, `fixture_sha256`, `synthetic` | Record version, named fixture identity/content digest, and explicit synthetic status. |
| `hardware` | OS/release, architecture, CPU, model, CPU count, memory where available; unknown metadata is explicit. macOS collection selects model fields and omits serial numbers/UUIDs. |
| `toolchain`, `build_profile`, `harness_sha256` | Full compiler version, release build, and Rust harness source digest. |
| `revision`, `working_tree_dirty`, `timestamp_utc` | Git HEAD, whether uncommitted changes existed, and UTC recording time. Dirty state means the revision alone cannot reproduce the measured code. |
| `iterations`, `unit`, `samples`, `median` | Workload size; `ns/sample`; positive raw timings and their median. A sample covers the complete iteration workload, not one call. |
| `baseline_median`, `median_ratio` | Added only for comparisons; current median divided by baseline median. A ratio above 1 means a slower median, with no automatic pass/fail threshold. |

The stored initial record identifies Apple M5 Pro, Mac17,9, 15 CPUs, 24 GB,
Darwin 25.4.0 and Rust 1.97.1. Seven samples ×100,000 iterations ×eight rows
yielded a median of **1,071,042 ns/sample**. Its HEAD is
`270005c486b7928071990f453bba23e72fc318c7` with `working_tree_dirty: true`:
the harness was measured before the foundations commit. Treat that record as
an initial working-tree measurement, not proof of performance for that base
commit or the final candidate.

`--baseline` reads the last record from that history. Comparison requires equal
schema, fixture identifier/digest, hardware metadata, compiler, build profile,
iterations, unit, harness digest, and synthetic status. Unknown CPU identity
blocks comparison. Revisions and raw timings may differ by design; metric
implementation changes are the subject of comparison. These checks cannot
guarantee equal power settings, background load, or thermal conditions. Review
raw samples and rerun under controlled conditions before diagnosing a regression.

Zero run sizes, invalid history schema/timings/median, an empty baseline,
incompatible metadata, or a history lacking its final newline produce explicit
errors without appending. History validation is not a cryptographic attestation
of a measurement; preserve source changes and environment evidence in reviews.
Store accepted new runs by committing the selected JSONL additions through the
normal signed issue workflow. Do not replace old measurements to conceal slower
results or reinterpret this primitive benchmark as scientific calibration.

## E04 normalization

Separately measured end-to-end `ingest-data normalize` in release mode on
macOS arm64 (Rust 1.98.1). Use the OSM request from the
[synthetic recipe](../fixtures/normalization/recipe.json), keeping only its OSM
input and expectation, with no walk entries or stop fixes. Generate 50,001 OSM
nodes (`i = 0..50000`, ID `i+1`, longitude `-122 + (i % 250) * .0001`,
latitude `45 + (i / 250 rounded down) * .0001`, four decimal places). Each
successive pair forms a separate `highway=footway` way. The fixture is a synthetic
import-scale chain, not a real city's anatomy. Run `/usr/bin/time -lp` on
normalization with a fresh store for each run, after building with
`cargo build --release --locked -p mobility-ingest --bin ingest-data`.

A 6.4 MiB extract produced a 19 MiB snapshot. Three runs took 0.58, 0.34, and
0.34 s (0.34 s median); peak RSS was 524–526 MiB. This includes staging, parsing,
QA, hashing and writing, but not routing or browser rendering. Memory overhead
is substantial because the source and derived representations coexist in memory.

For V nodes, E segments and S stops in a partition, ordered indexes and weak
components cost O((V + E) log(V + E)) time and O(V + E) space; matching stops
against indexed nodes costs O(S log V) time plus walk-entry indexing and does
not scan all edges per stop. Total archive copying and hashing are linear in
bytes read/written. This benchmark does not establish stop-matching throughput,
city-scale memory requirements or calibration.

## E10 scenario authoring

The [E10 scenario contract](INFRASTRUCTURE-SCENARIOS.md) uses deterministic full
snapshots, not an incremental editor or metro-scale performance claim. Reproduce:

```sh
TOOLBIN="$(dirname "$(rustup which cargo --toolchain 1.97.1)")"
export PATH="$TOOLBIN:$PATH"
cargo build --release --locked --offline -p interventions --example e10_scenarios
/usr/bin/time -l target/release/examples/e10_scenarios --benchmark
```

The synthetic E06/E10 fixture (MIT, modeled 2026-01-05, seed 42) is replicated
1/16/64 times with disjoint node/edge/trip IDs. Exactly nine directed access
requirements query the first copy. Graph construction is outside timing. One
preview warmup precedes 100 previews. Apply averages 20 validated transactions
including batch construction and retaining all versions; serialization is outside
timing. Undo/redo averages 10,000 pairs with compiler barriers around mutable
history access. Each row is one run's mean, **not** a percentile or confidence
interval. No performance regression threshold is asserted.

Measured on Apple M5 Pro, macOS arm64, Rust 1.97.1
(`8bab26f4f`, release), with this implementation before its delivery commit:

| Copies | Nodes / links | Preview µs | Apply µs | Undo+redo ns | Retained canonical network bytes (21 versions) |
| --- | --- | --- | --- | --- | --- |
| 1 | 3 / 19 | 65.229 | 76.094 | 1.575 | 57,471 |
| 16 | 48 / 304 | 587.005 | 537.752 | 1.058 | 809,503 |
| 64 | 192 / 1,216 | 2,236.883 | 2,185.498 | 0.975 | 3,216,607 |

Whole-process peak RSS across the three workloads was 11,452,416 bytes; elapsed
0.35 s. RSS includes setup, snapshots and allocator effects, not a per-row peak.
Encoded bytes are an exact **lower bound**, excluding in-memory allocation,
provenance, requirement and crossing/destination sidecar overhead. Tiny hot-cache
undo/redo timings are not an end-to-end UI latency guarantee. Wall time varies;
rerun under controlled conditions before comparing implementations.

Let B be edits, R access requirements, L links, N nodes, H history depth, P
expanded timetable events, and G the E05 full-graph validation cost (including
O(turns²) contradiction checks and calendar scans). Provenance checks deliberately
revalidate a cloned graph, and each edit scans links: preview/apply cost roughly
O((B+R+1)·G + B·L + R·Q), with graph/string/sidecar cloning costs additionally
linear in their bytes. Q is a bounded E05 route query: O(L log N + A log S), where
A includes examined transitions/events and S reached exact-time states. Before
and after queries, including permission-filtered graph clones, are performed for
each requirement. State exhaustion fails explicitly; S can grow rapidly with
horizon, cycles and schedules. Snapshot retention is O(H·(N+L+P+sidecars)) space;
undo/redo is O(1). ID tombstones also grow with every committed history version,
even discarded redo branches. Structural corridor preview costs
O(k·(log L + log k + turns)) for k selected links; profile-aware preview additionally
clones/revalidates the graph and routes the restricted selection. Road-supply
projection is O(L + active_roads·periods) time/space, not another capacity addition.

This workload measures editor validation and snapshot overhead. Its disconnected
replicas and small OD sample do not establish regional simulation throughput,
large-city routing, engineering feasibility, observed diversion or calibration.

## E11 transit service design

```sh
rustup run 1.97.1 cargo run --release --locked --offline -p interventions --example e11_transit -- --benchmark
/usr/bin/time -l target/release/examples/e11_transit --benchmark
```

Measured on Apple M5 Pro arm64, Rust 1.97.1 release profile. The repository-authored
MIT abstract E11 fixture dated 2026-01-05 uses modeled values only, no randomness,
one service day, one directed pattern, seven nodes and explicit frequency runs.
Each row averages 50 previews and 50 physical queries (500-second deadline).
Compilation is excluded. Timings are illustrative, not acceptance thresholds.

| Runs per template | Preview mean µs | Route mean µs |
| ---: | ---: | ---: |
| 1 | 8.571 | 2.483 |
| 32 | 10.611 | 9.877 |
| 128 | 18.593 | 10.866 |
| 512 | 56.608 | 11.539 |

A separate complete process run reported 2,883,584 bytes maximum resident set
(macOS `time -l`), including the demo and all sizes, not per-preview heap usage.
Queries scan timetable entries even beyond their deadline, but only retain legal
states inside it. These results do not establish metro-scale performance.

Beyond the E10 graph rebuild/history and access-query costs above, each managed
pattern scans E links, sorts its k legs and expands r final-leg departures into
an O(r) map. It examines at most B date/run instances (including excluded ones on
overlapping days), sorts 2a duty events for a included runs, and uses
O(E + k log k + r log r + D + B log r + a log a) time and O(k+r+a) auxiliary
space. D is the number of supplied service days; completely irrelevant days are
skipped. B is the explicit `max_run_instances`, capped at one million; template
expansion is bounded by it too. Endpoint/station opportunity scans have a
one-million examination cap per check and include transfer-window lookups.
Across T patterns and A affected station/mode/action checks these costs repeat;
there is no shared service index. E05 full-graph validation still expands full
trip timetables independently of the E11 estimator bound. Fare scoring adds one
pass over journey steps plus fare-provenance validation on boardings; physical
routing remains the bounded E07 search, not a polynomial metro-routing claim.
History retains full snapshots and sidecars as in E10. Fleet estimates omit
interlining and labor constraints; synthetic accounting performance says nothing
about calibration, physical feasibility or operating commitments.

## E12 feasibility screening

```sh
rustup run 1.97.1 cargo run --locked --offline --release -p interventions --example e12_feasibility -- --bench
```

Measured on arm64 with rustc 1.97.1, optimized profile, 20 sequential repetitions,
synthetic MIT abstract corridor dated 2026-01-05, seed 42. Timings include preview,
all gates, input audit cloning and result disposal; exclude fixture preparation.
Run-to-run/cache noise is visible, so these are not throughput guarantees.

| Declared OD samples (two routes each), 19 baseline links | Mean µs |
| ---: | ---: |
| 6 | 287.671 |
| 24 | 473.419 |
| 96 | 1059.323 |
| 384 | 2462.113 |

| Assessed segments / baseline links, 6 samples | Mean µs |
| ---: | ---: |
| 6 / 19 | 108.915 |
| 38 / 51 | 829.621 |
| 134 / 147 | 7037.419 |

| Construction phases, 6 samples per phase plus final | Mean µs |
| ---: | ---: |
| 1 | 207.967 |
| 2 | 324.806 |
| 4 | 578.848 |

Network scaling adds disconnected synthetic car segments with explicit lane/ROW
records; it does not approximate an urban topology. Phases add permanent works
and a first-phase temporary closure. No peak-memory measurement or metro-scale
performance claim is made.

Let V/E be graph nodes/links, I input evidence rows, Q protected requests, L lane
records, P phases, and G the existing E05 network-revalidation cost (including
timetable expansion). Existing E10 preview/rebuild/continuity and E11 operations
costs still apply. Provenance reuses the existing graph validator, cloning and
validating the graph per provenance row: O(I*G), potentially quadratic as evidence
and graph grow together. This deliberately simple reference path is not a hot-path
metro engine. Engineering coverage adds O(E log E + L + I log I), plus allocation
membership checks (worst O(E*I)) and memory O(E+I+L). Each vehicle OD clones and
revalidates its permission-filtered graph, then performs bounded E07 routing;
Q requests therefore cost O(Q*(G + routing)), bounded by each `max_states`, not
an unbounded reachability promise. The full input/snapshot audit is retained.

Plans repeat screening for P effective snapshots plus final and retain those
snapshots, inputs, and cloned permanent history: memory scales with P*(V+E+I),
including E10 full-snapshot history. Temporary service comparison scans trip
links and transfer incidents (worst quadratic in E); active-run validation has
explicit E11 bounds, and suspension enumeration caps examined events at one
million. Bounds exhaustion rejects rather than implying a suspended service.
Risk estimation multiplies three supplied rates by exposure and validates fixed
metadata, using existing provenance graph validation; it never runs simulation.

## E13 scoring

```sh
cargo build --release --locked --offline -p accessibility --example e13_scoring
/usr/bin/time -l target/release/examples/e13_scoring --bench
```

E13-v1 is the MIT repository-authored abstract fixture in
`crates/accessibility/tests/support/mod.rs`, dated 2026-11-19. The example generates
100/1,000/10,000 uniquely labeled destinations, one departure, identical fixed
60-second car-free / 40-second car journeys and explicit metadata. It measures
50 full `access` calls per size, with `black_box` barriers. Fixture construction
and compilation are outside timing; result allocation, validation, inventory
copy and percentile sort are inside. No random seed, warmup, CI or performance
threshold is claimed. Actual E07 routing and E08 simulation are **not** timed;
use their separate benchmark commands above and in SIMULATION.md.

Measured in release profile on macOS arm64, Rust 1.97.1 before the E13 delivery
commit: mean 23.56 µs (100 cells), 232.74 µs (1,000), 2,145.39 µs (10,000).
Whole-process peak RSS was 5,914,624 bytes across all sizes, elapsed 0.26 s.
These are one run's means, not percentiles or confidence bounds. Memory includes
setup/allocator effects and is not per-size RSS. Context graph tokens are tiny
synthetic bindings, not a regional graph; exact graph bytes in real outputs add
memory proportional to graph size per retained context. No metro-scale claim.

Scoring is O(P log P) time/O(P) working memory for P destination/departure cells;
trip, space and emissions ordered-map rollups are O(N log N). Pairwise Pareto is
O(C² M), M=4, plus binding-key validation; it is not measured by this arithmetic
fixture. A separate reference frontier workload exercises all-pairs dominance:

```sh
cargo build --release --locked --offline -p accessibility --example e13_frontier
/usr/bin/time -l target/release/examples/e13_frontier
```

`E13-frontier-v1` constructs 16/64/256 actual E12-screened synthetic candidates
outside timing. All four objectives tie, so all candidates remain nondominated
and no dominance search exits early. One correctness warmup precedes 20 timed
full `pareto` calls per size. On the same release toolchain, measured means were
30.87/82.60/360.70 µs respectively; whole-process peak RSS 12,697,600 bytes,
elapsed 0.28 s including setup. Snapshot/key validation, cloning returned contexts,
allocation and comparisons are timed together. This is not an isolated SIMD
comparison benchmark or proof of quadratic growth at these small sizes.

The [scoring contract](ACCESSIBILITY-SCORING.md) documents bounds and upstream
bridge execution separately. No observed inputs or scientific fidelity are
inferred from these timings.
