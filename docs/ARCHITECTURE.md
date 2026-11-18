# Architecture (initial contract)

## Data flow

1. **Ingest** public and local datasets with metadata, provenance, licenses, and temporal coverage.
2. **Normalize** into a time-dependent, directed multimodal mobility graph and demand model.
3. **Baseline** evaluate routing and access; calibrate against observations and report data limitations.
4. **Intervene** with physically and financially constrained street, transit, and land-use scenarios.
5. **Simulate** travelers selecting modes, routes, departures, and transfers; account for congestion and network re-equilibration.
6. **Score** car-free mobility, accessibility distributions, travel time, reliability, safety, emissions, investment and operations cost.
7. **Search** with reference algorithms, RL policies, and Pareto comparisons under explicit budget and access constraints.
8. **Validate** promising designs using an independent detailed simulator and observed-data baselines.
9. **Present** maps, differences, distributions, provenance, assumptions, uncertainty, and downloadable artifacts.

## Core metric

Car independence means residents can complete representative daily trip chains **without requiring a private car**. Compare best feasible car-free journeys against car journeys across times of day and population groups, but do not treat a single travel-time ratio as sufficient evidence of parity.

## Technical boundaries

These are ownership boundaries, not a claim that the planned pipeline is
implemented. [Module contracts](MODULE-CONTRACTS.md) list the current public
surface, tests, and restrictions for each crate (E01-S01/E01-S03).

- `mobility-ingest`: canonical geography catalogue and deterministic search (E02-S01); validated study-area topology, dataset coverage/import contracts, dated project snapshots and scenario baseline references (E02-S02–S06). E03 adds offline data connectors with a shared provenance and validation envelope and an isolated custom-upload sidecar.
- `mobility-graph`: domain graph and mode concepts; does not depend on RL.
- `mobility-routing`: routing boundary depending on graph types; no router yet.
- `accessibility`: input/output-neutral accessibility measures, separated from simulation assumptions.
- `interventions`: validated descriptions of candidate changes; no hidden mutation.
- `mobility-sim`: candidate scenario evaluation interface and eventually native fast simulator.
- `optimizer-env`: Markov decision process contracts and action feasibility; objective policies interchangeable.
- `optimizer-bridge`: boundary depending on `optimizer-env`; future bindings batch Rust transitions into the training runtime, avoiding per-trip FFI chatter. No bindings yet.
- `mobility-api`: accessible HTML/SVG search and project reports, offline `city-search` and `study-area` CLIs, local file persistence and per-project batch orchestration using `mobility-ingest` (E02). Future service boundary for reproducible experiments and versioned results. No HTTP server or endpoints yet; Axum remains proposed.
- Svelte frontend (future): interfaces with API and MapLibre; no authoritative model calculation in browser.

The [geography search contract](GEOGRAPHY-SEARCH.md) uses only Rust's standard
library and a local crate dependency. A separate Python preparation script
verifies pinned Census archives and extracts the three-area pilot into a
versioned TSV catalogue and source manifest. Runtime loading, search, and
presentation remain Rust and require no network. These source boundaries are
generalized geographic data, not observed transportation behavior or model
outputs.

The [study-area onboarding contract](STUDY-AREA-ONBOARDING.md) adds no runtime
dependencies. Rust owns polygon validation, conservative spherical envelopes,
required-source coverage and the complete `carfree-study-project-v1` snapshot.
The API layer performs explicit local file operations and presentation. A saved
project contains original and current boundaries, CRS/region, source versions,
snapshot date, deterministic baseline identifier, successful import count and
scenario baseline references. Buffer expansion reruns the importer, then commits
the new revision and marks older scenarios stale; failures leave the project
unchanged. The local catalogue importer revalidates boundary geometry/provenance
and reports missing transportation adapters. It does not build a starting
transportation network or claim that a model can run. Saved files can be viewed
without reimporting their source; later expansion requires the recorded source
to match the active catalogue. File updates assume a single writer; atomic
replacement is not a concurrent transaction protocol.

The [E04 normalization contract](GEOSPATIAL-NORMALIZATION.md) adds an offline
quality-screening step after ingestion. It produces a candidate topology,
pedestrian stop attachments, QA findings and an immutable replay manifest;
`mobility-api` renders the report without calculating scores. This is not a
simulation-ready multimodal graph or demand model.

## Scientific safeguards

- Maintain holdout cities and dates for generalization tests.
- Require mobility/access constraints for people who cannot cycle.
- Track displaced traffic across study boundaries and avoid spatially truncated outcome claims.
- Report uncertainty for induced mode shifts, crashes, emissions, and costs.
- Compare RL against simple greedy/evolutionary approaches on fixed budgets and compute constraints.
- Separate citywide averages from neighborhood-level accessibility distributions.

## Performance benchmark plan

Record time and peak memory for: import, network build, multimodal OD queries, scenario apply/reset, scenario evaluation, parallel simulation scale-out, and RL end-to-end training throughput. Measure fidelity as well as speed.

The implemented [benchmark harness](BENCHMARKS.md) measures only
`accessibility::reachable_share` on a versioned synthetic count fixture. It does
not measure the planned pipeline or peak memory. [ADR 0001](adr/0001-rust-first-hybrid-rl.md)
keeps simulator, RL, and application framework selections proposed until the
relevant benchmark and independent validation evidence exists.
