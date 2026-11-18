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
