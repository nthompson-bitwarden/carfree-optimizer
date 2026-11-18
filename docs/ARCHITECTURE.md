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
- `mobility-graph`: immutable validated directed multimodal network, explicit service/transfer/access contracts and canonical binary v1 (E05); does not depend on RL.
- `mobility-routing`: bounded deterministic time-aware reference routing (E05), plus door-node itineraries, cycling preferences, joint multi-stop planning, immutable batches and departure-window statistics (E07) over caller-supplied validated graph inputs. Metro-scale performance and observed fidelity remain unvalidated.
- `accessibility`: input/output-neutral accessibility measures, separated from simulation assumptions.
- `interventions`: preserves descriptive candidates and adds [E10 atomic in-memory scenarios](INFRASTRUCTURE-SCENARIOS.md), a conserved directional lane ledger, assessed crossings, profile-aware corridor previews, permission-aware access samples, immutable version snapshots and separate modeled destinations. Its explicit E08 evaluation bridge depends on local graph/routing/simulation contracts, rejects overlapping edits and double-counted supply, and never mutates the baseline or infers physical feasibility, destination demand or calibrated impacts.
- `mobility-sim`: scenario evaluation interface, E06 travel demand and conditional tour feasibility, plus E08 bounded reference assignment with immutable edits, entry-period road congestion, finite transit-capacity bounds, parallel scenarios, sensitivity distributions and fixed-cost/iterative adapters; E09 adds explicit road/boarding/survey comparisons, fit-only parameter grids, separate holdout scores and immutable fitness envelopes ([calibration contract](CALIBRATION.md)); empirically calibrated simulation remains future work.
- `optimizer-env`: Markov decision process contracts and action feasibility; objective policies interchangeable.
- `optimizer-bridge`: boundary depending on `optimizer-env`; future bindings batch Rust transitions into the training runtime, avoiding per-trip FFI chatter. No bindings yet.
- `mobility-api`: accessible HTML/SVG search and project reports, offline `city-search` and `study-area` CLIs, local file persistence and per-project batch orchestration using `mobility-ingest` (E02). E09 adds static calibration results and versioned audit TSV from `mobility-sim` envelopes; no authoritative calculation moves into presentation. Future service boundary for reproducible experiments and versioned results. No HTTP server or endpoints yet; Axum remains proposed.
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

The [E05 multimodal network contract](MULTIMODAL-NETWORK.md) adds per-mode
permissions, car turn rules/capacity, scheduled and phase-assumed frequency
service, explicit vehicle-aware transfers and mobility constraints. Its bounded
time-expanded reference router distinguishes unmet deadlines from invalid inputs
and resource exhaustion. Graph and routing use only standard-library/local crate
dependencies; no ingestion adapter automatically promotes screened topology to
this contract. Service-day UTC origins and complete coverage are caller
obligations, following E04's documented timezone convention. Synthetic fixtures
and developer benchmarks do not establish observed accessibility or calibration.

The [E06 demand contract](TRAVEL-DEMAND.md) layers population reconciliation,
weighted demand, traveler constraints, cost-based comparisons and explicit
scenario sensitivities on the reference router. It evaluates supplied
itineraries and selected departures, not a calibrated regional baseline or
an optimization reward. Long-horizon sensitivity is reported separately.

The [E07 routing contract](ROUTING.md) adds opt-in door-arrival semantics,
physical component times separate from cycling preference costs, joint stop/dwell
search preserving vehicle/turn state, revision-pinned batch cells and conditional
nearest-rank departure-window summaries. Legacy E05/E06 APIs and graph binary v1
are unchanged. Errors remain distinct from no-route-within-budget outcomes;
all inputs require actual door connections and complete service coverage.

The [E08 simulation contract](SIMULATION.md) preserves existing public APIs and
uses E06 legal whole-tour candidates from per-period cost views and a maximum-cost
envelope. It replays car paths at actual link-entry periods and explicitly reports
nonconvergence, candidate-set limitations, served demand and denied essential-trip
weight. Finite transit reservations are atomic across tours; denied demand is a
conservative bound, not a next-service waiting prediction. The alternate fixed-cost
adapter is a reference comparison, not independent scientific validation.

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
