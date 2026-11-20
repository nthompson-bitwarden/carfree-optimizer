# ADR 0005: bounded persistent RL process binding

- Status: accepted for the E14 reference implementation; not a metro-scale performance decision
- Scope: E14-S01/S06, `optimizer-env`, `optimizer-bridge`, `training`

## Context

E14 requires Gymnasium compatibility and batched Rust/Python transitions. The
current dependency policy allows no new Rust registry dependencies in these
crates. PyO3/maturin remain proposed in ADR 0001. There is no measured need to
add them to the bounded synthetic reference environment.

## Decision

Keep scenario, screening, assignment, scoring, reward and episode state in Rust.
Use one persistent POSIX child process per Python batch. Each framed request
contains at most one operation for each of at most 64 slots. All transitions run
inside Rust; Python never receives or evaluates individual traveler records.
Slots run serially; batching amortizes transport and does not imply parallelism.
The transport is a binding, not an in-process extension or stable C ABI.

Add only training/validation dependencies `gymnasium==1.2.3` and
`numpy==2.4.2`, pinned in `training/requirements.txt`. Gymnasium supplies the
standard environment/space contract and its actual checker; NumPy supplies the
required numeric observations. Neither is imported by Rust or required for the
standard-library Python transport. There are no new Rust registry dependencies
and no change to the Rust dependency allowlist.

Reject malformed/oversized/duplicate-slot requests before mutation. Runtime
rejections are per-slot, not cross-slot transactions. Close the connection on
protocol, IO, timeout or invalid-output failures: the client must not retry a
possibly completed action. Catalog/state revisions prevent stale transitions.

## Evidence and consequences

`training/tests/test_e14_env.py` exercises the real Rust server and Gymnasium
checker, scalar/batch equivalence, malformed responses, timeout and byte limits.
`optimizer-bridge/tests/e14_batch.rs` tests framing, allocation limits, ordering,
per-slot failures and pre-mutation validation. `training/profile_e14.py` records
reset/step latency, throughput and Rust process peak RSS for one batch size per
invocation. Measurements are reported in `docs/RL-ENVIRONMENT.md`; they do not
justify a claim that this transport is faster than PyO3 or city-scale ready.

The wire projection intentionally omits private demand and rich evidence logs;
Rust retains the auditable transition. Graph-policy experiments use the rich
Rust graph observation directly. Changing wire features or semantics requires a
schema version change. An in-process binding remains an independently benchmarked
future decision, not an implicit dependency exception.
