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

- `mobility-graph`: domain graph and mode concepts; does not depend on RL.
- `accessibility`: input/output-neutral accessibility measures, separated from simulation assumptions.
- `interventions`: validated descriptions of candidate changes; no hidden mutation.
- `mobility-sim`: candidate scenario evaluation interface and eventually native fast simulator.
- `optimizer-env`: Markov decision process contracts and action feasibility; objective policies interchangeable.
- PyO3 bridge (future): batches Rust transitions into the training runtime; avoid per-trip FFI chatter.
- Axum API (future): initiates reproducible experiments and serves map/data assets.
- Svelte frontend (future): interfaces with API and MapLibre; no authoritative model calculation in browser.

## Scientific safeguards

- Maintain holdout cities and dates for generalization tests.
- Require mobility/access constraints for people who cannot cycle.
- Track displaced traffic across study boundaries and avoid spatially truncated outcome claims.
- Report uncertainty for induced mode shifts, crashes, emissions, and costs.
- Compare RL against simple greedy/evolutionary approaches on fixed budgets and compute constraints.
- Separate citywide averages from neighborhood-level accessibility distributions.

## Performance benchmark plan

Record time and peak memory for: import, network build, multimodal OD queries, scenario apply/reset, scenario evaluation, parallel simulation scale-out, and RL end-to-end training throughput. Measure fidelity as well as speed.
