# E14 constrained reference planning environment

E14 exposes a versioned, inspectable sequential problem for **caller-supplied,
synthetic/reference evidence**. It does not establish calibration, simulation
fidelity, engineering approval, or a policy-ready car-independent neighborhood.
The original framework-neutral `PlanningEnvironment`/`Transition<T>` remain
source-compatible. The indexed reference environment is additive.

## Episode and scientific contract (S01, S04, S05, S07)

`optimizer_env::reference::Config` binds an immutable E10 opening history, fixed
E06 demand, E08 settings/supply, exact E13 evidence context, door-comparison
samples, E12 budgets/engineering/protected OD inventory, reward configuration,
and an immutable catalog of project payloads. The reference adapter supports
short-run transport changes; it rejects land-use actions rather than inventing
new population or trips. Source metadata and limitations remain in rich Rust
transitions, outside policy features.

`Environment::reset(seed)` returns a version-1 indexed transition with zero
reward. `step(index, expected_state)` stages the candidate and commits only
after screening, assignment, scoring and hard constraints succeed. Every
successful reset or step advances a monotonic state revision. An explicit stop
action follows the catalog entries. Stop terminates; exhausting the configured
project horizon truncates. Invalid, stale, repeated, infeasible and failed
candidates are explicit errors and must leave state unchanged. A mask is not an
authorization token: step revalidates the exact current state.

E12 screens project capital/disruption, gross annual maintenance and transit
operations, engineering/ROW restrictions and fixed protected outbound/return OD
samples before E08 assignment. Cumulative project capital is charged once.
Annual costs are the current portfolio's **per-year commitment**, not a sum of
annual bills across decision steps. Existing transit cost-basis assumptions cannot
be silently rewritten to create budget headroom; changing passenger supply alone
cannot lower an unchanged service's annualization, rates or local cost factors. Currency, price year, scaling, analysis
years and discount rate are explicit. Earlier physical works, maintenance rows
and protected samples persist. The opening path is explicitly reported without
an E12 project gate because E12 has no public empty-project gate; it must not
be relabeled as project-screened.

Reward configuration logs every weight, positive scale, guard tolerance and
convergence requirement. Rich transitions retain raw gains, denominators,
weighted terms, before/after metrics and hard-constraint results. Terms are:

- reduced **E06 reference-network** car-required share;
- increased reference car-free whole-tour feasibility;
- door parity against a **fixed opening car-time comparator**, preventing reward
  solely from slowing cars.

E08 assignment, capacity denial and unserved essential-trip evidence remain
separate. Guards cover trip suppression, no-mode outcomes, essential/noncycling/
step-free losses and nonconvergence. Gross per-tour protected losses cannot be
offset by equal-weight gains elsewhere, even if the affected traveler remains
served by a private car. Zero weights support ablations without
turning off hard constraints. E08 does not expose complete effective-view
alternatives; assigned car trips are **not** car-required trips. Reference
feasibility is not a claim about congested/capacity-constrained car independence.
Door routing includes supplied access, waiting, schedules, transfers and mobility
profiles, but still depends on caller coverage and the router's bounded search.

## Action catalog (S02)

`catalog::Catalog<T>` binds typed payloads to immutable stable IDs, a caller-set
revision, descriptions, prerequisites and symmetric conflicts. It accepts
1–256 projects and rejects unknown/self references, cycles, contradictions,
duplicates and ambiguous TSV fields. Keep the revision unique for the catalog's
actual payload/configuration; revision strings are not cryptographic identities.

`manifest()` returns UTF-8 LF-delimited TSV:

```text
carfree-action-catalog<TAB>1<TAB>catalog-revision
index<TAB>id<TAB>description<TAB>requires<TAB>conflicts
```

Indices are zero-based in stored catalog order. References are sorted,
comma-separated stable IDs. IDs/revisions cannot contain commas or control
characters; descriptions cannot contain control characters. The manifest is a
structural audit view, not a project-payload deserializer or feasibility proof.

`mask(state_revision, applied, screen)` returns versioned entries with explicit
`AlreadyApplied`, `MissingPrerequisite`, `ConflictingProject` or `Screening`
reasons. Screening errors propagate as errors, never as fabricated infeasible
journeys. Structurally blocked projects do not invoke expensive screening.
Applied portfolios are checked for unknown projects, missing prerequisites and
conflicts. `resolve` rejects nonexistent/stale indices and reconstructs current
eligibility rather than trusting a mutable caller mask. Geographic feasibility
and assignment remain separate obligations.

## Policy observation v1 (S03, S08)

The rich observation contains local-index nodes/edges, action anchors, a
24-scalar global vector and ten scalars per catalog action plus stop. Flattening
is `global ++ action[0] ++ ... ++ stop`, without topology-dependent padding.
Feature order is authoritative in `observation::{GLOBAL_FEATURES,ACTION_FEATURES}`.
Graph nodes expose accessibility and in/out degree; edges expose local endpoints,
mode/type, accessibility, seconds and optional physical metres. Missing distance
and anchors remain explicit, never zero-imputed without a presence indicator.

Global features describe accepted/remaining steps, graph/mode/accessibility
counts and degrees, total/essential/noncycling/step-free trip weights, remaining
capital/annual budgets, annual commitments, reference car-free share, fixed door
parity, unserved essential weight and denied weight. Action features describe
stop, declared upper capital/maintenance, change count, proposed walking time
with presence, and current anchor degrees with presence. Proposed infrastructure
is a known action input, not an observed or future realized outcome.

Counts and weights retain their declared expansion units; money is major units
in the configured price year; annual commitments are major units/year; link
and proposed walking times are seconds. Shares/parity are dimensionless. No
city name, split label, source node ID, seed, private trip record or future score
is a policy feature. Graph ordering can change under relabeling while local
connectivity and aggregate vector features remain equivariant/invariant.
Normalizers in experiments must be fit only on training observations, then frozen.
These are deliberately lossy policy encodings, not proof of a sufficient Markov
state: complete timetables, source records and private trip rows remain in the
Rust configuration/evidence. The fixed vector assumes a common catalog/action
feature contract; transferring
across arbitrary action catalogs needs an explicit adapter, not silent padding.

## Persistent Python batch binding (S01, S06)

See [ADR 0005](adr/0005-bounded-rl-process-binding.md). No new Rust registry
dependencies or in-process FFI are introduced. Python requires POSIX pipes.
Install the training-only dependencies in an isolated environment:

```sh
python3 -m venv .codex/run/e14-venv
.codex/run/e14-venv/bin/python -m pip install -r training/requirements.txt
cargo build --locked --offline -p optimizer-bridge --examples
PYTHONPATH=training .codex/run/e14-venv/bin/python -m unittest discover -s training/tests -v
```

`training.carfree_env.BatchProcess` (or import `carfree_env` with
`PYTHONPATH=training`) keeps 1–64 Rust slots in one process. The bundled
`e14_batch SLOTS [TOPOLOGY]` executable is a **synthetic integration fixture**,
not a city loader. Production callers construct explicit reference configs;
there is no automatic data import or policy recommendation pipeline.
`GymEnvironment(command)` exposes Gymnasium's `(observation, info)` reset and
`(observation, reward, terminated, truncated, info)` step. Info includes schema
version and boolean action mask. Algorithms must respect the mask; in-space
but infeasible actions are rejected explicitly. Rich scientific audits remain
available through the Rust API, not flattened into policy observations.

One request frame is `E14<TAB>1<TAB>N<LF>`, followed by N rows:

- `reset<TAB>slot<TAB>seed<LF>`
- `step<TAB>slot<TAB>expected_state<TAB>action_index<LF>`

One response frame repeats the header and ordered slot rows:

- `ok<TAB>slot<TAB>version<TAB>state<TAB>reward<TAB>terminated<TAB>truncated<TAB>comma-separated-floats<TAB>01-mask<LF>`
- `error<TAB>slot<TAB>bounded-printable-diagnostic<LF>`

Terminal flags are 0/1; numbers are finite; versions are exact, not best-effort.
Requests are capped at 65,536 bytes, 64 unique known slots and one operation per
slot. Responses cap observations at 4,096 scalars/slot, masks at 1,024 entries,
and frames at `64 * (4096 * 32 + 1024 + 256)` bytes. Numeric encoding bounds each
observation scalar's text length. Diagnostic rows are at most 512 characters.
Input/frame errors are fatal before execution; valid slots with runtime errors
are reported individually. Successful peers are **not** rolled back. A malformed
output, IO failure or timeout closes the connection because state may have
advanced: discard it and restart/reset, never blindly retry. Stderr is separate
from protocol stdout. The wire interface is intended for trusted local engines,
not an untrusted network service.

## Ablations and unseen-topology comparison (S07, S08)

```sh
cargo run --release --locked --offline -p optimizer-bridge --example e14_experiments
cargo run --release --locked --offline -p optimizer-bridge --example e14_experiments -- --heldout-alt
```

The TSV version-1 experiment records splits, compute, configured rewards/budgets,
frozen normalizers/coefficients, common raw outcomes, decomposed reward terms,
constraint checks, explicit failures, unchanged-metric/trivial flags and stop-only
baselines. A deliberately denied transit-capacity proposal must log failing
noncycling/step-free guards and leave the current transition unchanged.

The vector baseline uses declared action features, anchor degrees/presence and
affordability. The graph representation performs one fixed shared message pass
of neighboring degree features and pools at each action's public anchors; both
fit a 12-coefficient linear head over eight seeded coefficient candidates.
This is a graph-message policy, **not a trained neural GNN or RL algorithm**.
Each representation uses the same training topologies 0/1, seeds 7/19, eligible
catalog, episode budgets and coefficient-search count. All scaling and coefficient
selection uses training only. Held-out topologies 4/7 (alternate 5/8) are evaluated
once with frozen coefficients at seeds 31/47/73; held-out changes must affect
actual routed paths/project outcomes, not just disconnected padding. Tests change
the held-out set and assert identical fitted coefficients and scaling.

Full reward, no-car-required, no-parity and feasibility-only variants retain the
same hard constraints. Common raw trip weights, no-mode/unserved outcomes, fixed
door parity and capital/annual costs allow comparing variants independently of
their differently weighted scalar rewards. Preference noise is disabled in this
fixture: repeated seeds and sample variance describe **deterministic replay**, not
sampling uncertainty or a confidence interval. Equal outcomes are an acceptable
result; no graph superiority or observed unseen-city performance is claimed.

## Complexity and profiling (S06)

With A catalog projects (at most 256), prerequisite references P, and accepted
steps H (at most A), catalog validation has a conservative upper bound
O(A(A+P) log A + A³ log A), including transitive contradiction checks. Structural
mask/resolve work is O(A² log A) plus caller screening. The reference adapter's
mask screens structurally eligible candidates sequentially, so each mask adds up
to A E10 previews/E12 screens. Each accepted step then performs the exact E08
assignment and E13 scoring/door routes, followed by fresh candidate masks. This
is deliberately a bounded reference engine, not an incremental metro optimizer.
See [E08](SIMULATION.md), [E12](FEASIBILITY.md) and [E13](ACCESSIBILITY-SCORING.md)
for the routing, assignment and scoring bounds inside those operations.

History retains H full immutable graph/inventory snapshots per slot; it does not
retain an unbounded stream of old simulation reports. For S slots, F projected
features and fixed input sizes, batch transport adds O(S·F + frame bytes) memory,
with S≤64 and explicit numeric/frame caps above. Underlying graph, demand and
search bounds remain caller-configured; the process binding does not turn an
arbitrary custom engine into a bounded-memory implementation. Rust owns all
slot state and all scientific calculations; Python only serializes batches.

Reproduce one batch size per profiling process (three warmups and twenty samples):

```sh
TOOLBIN="$(dirname "$(rustup which cargo --toolchain 1.97.1)")"
export PATH="$TOOLBIN:$PATH"
cargo build --release --locked --offline -p optimizer-bridge --example e14_batch
PYTHONPATH=training .codex/run/e14-venv/bin/python training/profile_e14.py --size 1
# Repeat --size 8, --size 32, --size 64 in separate invocations.
```

The profiler records compiler/Python versions, build profile, graph/action/feature
sizes, source and executable hashes, raw reset/step roundtrip nanoseconds, median
and nearest-rank p95, successful step-slots/s and Rust-child peak RSS. RSS includes
setup, history and allocator effects. Slots are sequential; do not infer parallel
speedup, PyO3 superiority, calibrated fidelity or city-scale throughput.

### Measured synthetic reference results

[Stored raw profiles](../benchmarks/e14/) use Darwin 25.6 arm64, Rust 1.97.1
(`8bab26f4f`), Python 3.15.2 and release builds. The fixture has five nodes,
17 links, four actions and 64 projected scalars, seed 42; each row is one
process with three warmups and twenty measured reset/first-project batches.
Times include Python/pipe overhead; RSS is whole Rust-child peak bytes.

| Slots | Reset p50 / p95 ms | Step p50 / p95 ms | Successful step-slots/s | Peak RSS bytes |
| --- | --- | --- | --- | --- |
| 1 | 0.619 / 0.762 | 0.579 / 0.984 | 1,655 | 4,390,912 |
| 8 | 2.871 / 3.012 | 2.939 / 3.154 | 2,688 | 5,455,872 |
| 32 | 11.439 / 11.702 | 11.834 / 12.267 | 2,691 | 9,240,576 |
| 64 | 23.185 / 23.998 | 23.923 / 24.456 | 2,687 | 14,188,544 |

These pre-commit records identify base `e206e9d` with `working_tree_dirty: true`;
their reproducible source digest is
`4131c54359f19942f9600faac5ccc93ab4161999167d5ed046f5ce31c4482337`.
The digest covers crate Rust sources/manifests, workspace lock/toolchain and
training code/requirements; executable hashes are recorded separately. It binds
measurements to the implementation, not to documentation or Git metadata.
No confidence interval, parallel scaling or city-scale capacity is inferred.

The [transfer summary](../benchmarks/e14/transfer-summary-v1.tsv) records common
raw outcomes and all four ablations. Full-reward vector/graph scores tie at
1.4634 on held-out topology 4; on topology 7 vector scores 1.4211 and graph
0.4211. On topology 7 the vector achieves six reference car-free trip-weight
units, versus two for graph, with zero unserved trips in both. Thus this tiny
comparison **does not demonstrate graph superiority**. Seed-repeat variance is
zero because preference noise is disabled, not evidence of empirical certainty.
The separate known-denial diagnostic records failing protected constraints and
exact no-mutation behavior. Re-run rather than extrapolate these observations.

## Acceptance and reproducibility

- `crates/optimizer-env/tests/e14_catalog.rs`: structural catalog and invalid-action contracts.
- `crates/optimizer-env/tests/`: reference engine, observations, budgets and safeguards.
- `crates/optimizer-bridge/tests/e14_batch.rs`: bounded framing and per-slot behavior.
- `training/tests/test_e14_env.py`: actual Rust process/Gymnasium integration and failure paths.

All fixtures are deterministic synthetic modeled assumptions. Benchmarks,
ablations and transfer comparisons are developer/research evidence, not empirical
validation, confidence that a neighborhood is car-independent, or approval to
deploy a planning policy.
