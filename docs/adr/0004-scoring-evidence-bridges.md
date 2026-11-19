# ADR 0004: local evidence bridges for accessibility scoring

- Status: accepted for the bounded E13 reference implementation
- Scope: `accessibility`; existing domain contracts unchanged

## Decision

Keep metric arithmetic deterministic and input/output-neutral. Add local crate
edges to `mobility-graph`, `mobility-routing`, `mobility-sim` and `interventions`
only for explicit opt-in evidence bridges. No external dependency, Python hot path,
storage schema or domain rewrite is introduced. Preserve `reachable_share`.

Reusing E07 physical component timing, E08 actual assignment and E12's opaque
screened snapshot avoids duplicating routing, mode assignment, engineering gates
and risk calculations. E09 reports remain reference-only because their current
public contract does not retain exact input-network bytes. Do not change that
neighboring contract to imply scientific validation. E08 does not expose complete
effective-cost alternatives: publish a separately labeled E06 reference estimate,
actual served mode distribution, and unavailable effective car-required share.

Validated accessibility/trip outputs are immutable. Exact graph bytes bind
cross-module snapshots; projections retain comparable estimands. Typed access
registries and explicit inventories prevent missing evidence from improving a
frontier or becoming a zero total. Geometry disjointness, empirical factor
applicability and sampling designs remain explicit caller responsibilities.

## Alternatives and consequences

Duplicating upstream algorithms would drift from their established semantics.
Accepting bare report IDs or arbitrary scalar scores would permit stale-context
comparisons. Adding a generic serialization/hash/runtime dependency is premature.
The local dependency DAG remains acyclic (interventions depends on graph/routing/
simulation, none depends on accessibility). The arithmetic microbenchmark excludes
bridge execution, while integration tests execute real upstream engines. Graph
snapshot bytes currently duplicate memory across results; optimize to shared
immutable evidence only after a representative benchmark demonstrates need.

## Evidence

Story-linked E13 tests include actual parking/egress, state-limit errors, E12
screened evaluation, risk consistency, denied transit proposals and demand
multiplier 2. The [E13 benchmark](../BENCHMARKS.md#e13-scoring) records synthetic
arithmetic scaling separately from route/simulation costs. These tests and
measurements do not establish regional performance or empirical fidelity.
