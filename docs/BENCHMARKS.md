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
