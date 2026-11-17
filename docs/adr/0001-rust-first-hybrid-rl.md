# ADR 0001 — Rust-first simulator with a replaceable RL training backend

Status: Proposed

## Proposed decision and scope

Use Rust for graph, routing, scenario transformation, and simulation. Use Python/PyTorch initially for reinforcement learning through batched PyO3/maturin bindings. API will be Rust/Axum and visualization Svelte/MapLibre. Keep simulator independent of optimizer and frontend.

E01-S06 records these as proposals, not installed frameworks or validated engine
selections. The current workspace has domain types and traits, four empty
ingest/routing/bridge/API boundary libraries, and no simulator, training runtime,
HTTP server, or frontend implementation. Rust/Python separation is a contract;
specific engine and framework adoption remains conditional on measurements.

## Rationale

The hypothesis is that Rust's control of layout and parallel execution supports
repeatable engine performance, while Python/PyTorch reduces training integration
work. Batching is intended to amortize language-boundary overhead. These are
selection reasons to test, not measured throughput or FFI claims for this project.

## Alternatives

- Pure Rust training (Burn/Candle): fewer language boundaries but more implementation burden and less mature RL experimentation.
- Entirely Python simulation: rapid prototyping but potential throughput and memory penalties at city scale.
- External microsimulation for every RL step: useful fidelity but potentially prohibitive training costs.

## Benchmark evidence and acceptance protocol

The implemented [primitive benchmark](../BENCHMARKS.md) measures only
`reachable_share` on synthetic counts. Its initial history records actual raw
samples, hardware, toolchain, and dirty-tree status. It gives no comparative
evidence for simulator engines, RL libraries, FFI, API, or UI frameworks.

Before accepting a selection, record versioned candidates, named fixture/data
snapshots with provenance/licensing and geography/date/units, hardware and
toolchains, warmup/repetition methods, raw time and peak-memory measurements,
and correctness/quality checks. Agree workload-specific pass/fail budgets before
running comparisons; none has been established for the future workloads below.
Preserve failed results as well as successful ones.

| Selection | Required comparison and evidence | Current evidence status |
| --- | --- | --- |
| Simulator | Native Rust approach versus measured reuse candidates such as Metropolis2/A-B Street. Time import/build, multimodal queries, scenario apply/reset, evaluation, and scale-out on the same representative city snapshots. Compare calibration and holdout predictions with observed baselines and an appropriate independent detailed simulator. | NOT RUN; no simulator implementation or calibration evidence. |
| RL backend / bridge | Python/PyTorch batched PyO3/maturin versus pure Rust candidates such as Burn/Candle under fixed environment, seeds, reward contracts, compute budgets, and action constraints. Measure transition/FFI throughput, batch-size effects, memory, end-to-end training, and quality distributions across seeds. Compare policies against greedy/evolutionary baselines. | NOT RUN; no bindings or training implementation. |
| API and UI frameworks | Axum and Svelte/MapLibre versus a justified alternative on the same versioned scenario/results contracts. Measure payload handling, map interaction latency/memory, error handling, keyboard/accessibility behavior, integration effort, and portability. | NOT RUN; no HTTP/UI implementation. |

Simulator validity is a separate acceptance gate from speed. Faster execution
cannot justify inaccessible journeys, disconnected essential access, biased
aggregate metrics, or fabricated missing inputs. Stochastic training comparisons
must include spread/confidence information and distinguish algorithmic variation
from timing noise.

## Exit paths and review triggers

- If native simulator implementation or calibration costs exceed the agreed
  budget, place a measured reusable engine behind `ScenarioEvaluator`; retain
  explicit scenario inputs, units, provenance, and error behavior. Revalidate
  calibration and independent checks rather than assuming engine equivalence.
- If FFI dominates training or reproducibility/maintenance fails its budget,
  increase batching first, then compare a pure Rust backend. Keep
  `PlanningEnvironment` framework neutral so observations/actions can be adapted
  without moving simulation into Python or changing scientific rewards silently.
- If PyTorch no longer satisfies measured training requirements, replace the
  training adapter in `optimizer-bridge`, preserving versioned experiment
  definitions and retained checkpoints where feasible; document migration losses.
- If Axum or Svelte/MapLibre misses agreed accessibility, performance, or
  portability criteria, replace transport/presentation adapters while preserving
  engine ownership and versioned result contracts. No current application data
  migration is needed because these components are not implemented.

Add a superseding numbered ADR with comparison artifacts and migration steps
when accepting or replacing a proposal. Do not rewrite an earlier Proposed
record into a claim that unrun benchmarks passed. Dependency additions still
need explicit review and an update to the local-only dependency policy.
