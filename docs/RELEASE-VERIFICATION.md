# Release verification and recovery

E20 applies to the local synthetic workbench and experiment coordinator. Record
the immutable source revision, executable SHA-256, lockfile, build target, tool
versions, model card and test results with each release. The workbench reports
its package version with `spatial-workbench --version`. The operator protocols
retain their existing schema versions. Version future migrations explicitly
and rehearse each supported source-to-target upgrade using a restored copy.

| Story | Reproduction and scope |
| --- | --- |
| E20-S01 / #162 | Workspace tests check directed access, unit conversion, monotone reachability and conservation across varied transit demand and capacity. |
| E20-S02 / #163 | `cargo test --locked --offline -p mobility-api --test e20_import_score` normalizes and replays the public synthetic fixture, builds a reviewed walk network, and checks a connector's accessible-destination share against explicit expected values. The workbench smoke below exercises onboarding, scenario evaluation and exports. |
| E20-S03 / #164 | [Independent comparison](#independent-comparison) uses Eclipse SUMO. Representative urban comparisons still require matched detailed networks, calibrated inputs and independently curated scenarios. |
| E20-S04 / #165 | Structured job traces and metrics are described below. |
| E20-S05 / #166 | [Local upgrade and restore](#local-upgrade-and-restore) rehearses a versioned workspace on a disposable staging fixture. Deployment-specific staging, credential recovery and offsite restore need their own rehearsal. |
| E20-S06 / #167 | [Input and credential controls](#input-and-credential-controls) and CI advisory scans. |
| E20-S07 / #168 | [Model card](MODEL-CARD.md) accompanies this candidate. |
| E20-S08 / #169 | [Supported execution](#supported-execution) covers macOS and Linux smoke and a local container. The E16 quota worker still requires 64-bit Linux. |
| E20-S09 / #170 | [Synthetic load baseline](#synthetic-load-baseline) measures job throughput and saturation plus a larger synthetic demand batch. A representative large-city workload is pending. |
| E20-S10 / #171 | [Contribution and demo guide](../CONTRIBUTING.md), [license](../LICENSE), and [fixture attribution](../fixtures/geography/README.md). |

## Independent comparison

The matched fixture tracks one unimpeded vehicle on a 100 m road. The Rust
reference uses E08 iterative assignment with zero congestion coefficient and
symmetric outbound and return travel times. SUMO integrates an isolated leg
at a 0.1 s step, seed 42, with fixed speed factor 1 and no speed dispersion.
The acceptance tolerance is two integration steps (0.2 s). The 10 m/s and
5 m/s runs completed in 10.0 s and 20.0 s in both engines. The script checks
that SUMO produced a complete 100 m trip and emits both engine times and their
signed error in seconds. This is a boundary-condition comparison of road
traversal time. The [model card](MODEL-CARD.md) defines the broader validation
needed before policy use. [SUMO trip output specification](https://eclipse.dev/sumo/docs/Simulation/Output/TripInfo.html)

```sh
python3 -m venv /tmp/carfree-sumo-1.28
/tmp/carfree-sumo-1.28/bin/python -m pip install eclipse-sumo==1.28.0 sumo-data==1.28.0
cargo build --locked --offline -p mobility-sim --example e08_simulation
PATH="/tmp/carfree-sumo-1.28/bin:$PATH" python3 tests/acceptance/e20_sumo.py target/debug/examples/e08_simulation
```

## Structured job events

The workbench writes JSON lines to stderr with schema `carfree-job-event-v1`.
E17 uses its numeric run ID. E19 uses the project/scenario/revision IDs from
the authenticated workspace API. Events record running or terminal status,
elapsed milliseconds and a fixed recovery action on failure. Retain the
serving process identity with its logs to correlate IDs across processes.
The coordinator uses its job digest, seed and a one-way attempt correlation
ID. It emits claim and completion events, with status, elapsed request time
and finalized and failed-attempt counts on inspection. The
authenticated status API retains additional attempt timing and usage metrics.
No external telemetry service is required. These local events are scoped to
workbench runs and coordinator jobs.

## Local upgrade and restore

Stop the workspace writer before backing up `workspace-v1.json` from its
private directory. Keep the matching executable and SHA-256. Back up the
bootstrap credential separately and protect it as a secret. The storage lock
is recreated on open. Restore into a new private directory, check hashes,
then read a known project and completed evaluation and download its metrics.
Keep the original intact until those checks pass. An executable change makes
old projects read-only with `evaluator_changed`. Existing saved reports
remain available and new evaluations use a fresh project. Preserve the
previous executable where replay of its exact inputs is needed. The
[workspace contract](WORKSPACES.md) describes storage and artifact versions.

```sh
cargo build --locked --offline -p mobility-api --bin spatial-workbench
python3 tests/acceptance/e20_workspace.py target/debug/spatial-workbench
python3 tests/acceptance/e20_workspace.py target/debug/spatial-workbench \
  --previous-binary /path/to/archived/spatial-workbench
```

The harness creates the synthetic data on the old executable, restores the
store on the candidate, checks that immutable results and city metadata
survived and that the stale project rejects new execution, and compares
fresh scenario and report values. Parquet metadata records the exact
evaluator and its encoding can vary with the writer version.

For the separate E16 coordinator, stop the writer and make a SQLite backup
with the SQLite backup API or CLI `.backup`. Verify `PRAGMA integrity_check`
on a separate restored copy. Keep the matching coordinator and evaluator,
the configured capacity, and private completion journals. E16 accepts legacy
completion records without the optional private log digest. Restore the matching
pre-upgrade database and journals if rolling back the coordinator. Establish worker
process state independently before resuming unresolved attempts. See the
[E16 recovery protocol](EXPERIMENT-JOBS.md).

## Input and credential controls

Use private local workspaces and credential files, strict loopback bindings,
and the supported SSH tunnel for remote E16 workers. Backups and authenticated
exports can contain restricted inputs, share capabilities or operator details.
Handle them as private until reviewed. Never place secrets in study-area
metadata, scenario titles, comments, transport uploads, or worker names.
The coordinator removes its bearer and hexadecimal capabilities from worker
logs at the HTTP boundary. A private log digest preserves exact completion
replay after redaction, including completions created by the prior release.
Use generated 64-character hexadecimal tokens when rotating coordinator
credentials so historical capabilities stay redacted in status responses. Structured job events omit request content,
capabilities and free-form diagnostics. Prior private attempt logs retain the
format and access policy of their original release.

Workspace tests cover roles, anonymous access, same-origin controls, request
bounds, and stored-result integrity. Use the pinned dependency checks and the
separate CI vulnerability scan, which executes RustSec and PyPA advisory checks
without exclusions. The allowlist and audits address distinct controls.

## Supported execution

Run the [contributor smoke](../CONTRIBUTING.md) on Linux or macOS. The Debian
container runs the non-root loopback workbench. The smoke needs no published
port and uses temporary storage with a read-only container filesystem.

```sh
docker build -t carfree-local:0.1.0 .
docker run --rm --network none --read-only --tmpfs /tmp carfree-local:0.1.0 \
  python3 /opt/carfree/e20_workspace.py /usr/local/bin/spatial-workbench
```

## Synthetic load baseline

The loopback workload uses the fixed E17 two-project plan at 1,900 mm with
200 USD capital and 30 USD/year limits. One worker serves at most four queued
runs. After a warmup, 36 submissions at each concurrency measure end-to-end
polling latency and the number of bounded queue refusals. Throughput divides
completed jobs by the full measured batch duration, including drain time.

```sh
cargo build --release --locked --offline -p mobility-api --bin spatial-workbench \
  -p mobility-sim --example e08_simulation
python3 tests/acceptance/e20_load.py target/release/spatial-workbench --clients 1 --requests 36
python3 tests/acceptance/e20_load.py target/release/spatial-workbench --clients 12 --requests 36
/usr/bin/time -l target/release/examples/e08_simulation --benchmark 10000 4 1
```

Use `/usr/bin/time -v` on Linux for the last command. The separate E08 workload
uses 10,000 synthetic tours per evaluation on an 11-link graph with eight
scenario evaluations and four batch workers. It checks parallel results
against serial results and reports separately measured single-run latencies.
These workloads bound the measured fixture, API and queue scope. Network scale,
schedule diversity, larger ledgers and actual city-level demand remain untested.

With Rust 1.97.1 release builds on Apple M5 Pro / macOS 26.7.3, one local
run measured the following. Peak RSS is for the workbench child process.

| Clients | Completed / submitted | Queue refusals | Completed per second | Submit p95 | Completion p95 | Peak RSS |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 36 / 36 | 0 | 74.06 | 0.22 ms | 13.65 ms | 13.55 MiB |
| 12 | 5 / 36 | 31 | 76.90 | 1.24 ms | 64.34 ms | 10.72 MiB |

The overloaded batch drains a five-job burst. Its completion throughput
describes only that burst and excludes refused jobs from the numerator.
The E08 batch measured 7.56 scenarios/s, a 422.36 ms single-scenario p95
over eight sequential comparisons, and 1.02 GiB process peak RSS across
serial oracle, parallel batch and sequential checks. The reference engines
and their asymptotic costs are identical to those in the
[simulation](SIMULATION.md) and [workbench](../web/README.md#acceptance)
contracts.
