# Car Independence Optimizer — End-to-End Product Backlog

**Vision:** Redesign US cities to progressively reduce dependence on private cars while maintaining or improving equitable, practical door-to-door access through walking, bicycles, e-bikes, buses, trains and light rail.

**Scope:** City ingestion → historical/baseline validation → multimodal graph → travel demand → fast simulation → infrastructure scenarios → car-independence scoring → constrained RL and search → comparison and reporting → deployment and collaboration.

**Primary success outcome:** An increasing proportion of residents can complete typical everyday trips without owning or using a private car, without unacceptable deterioration in reliability, travel time, affordability, safety, accessibility or geographic equity.

**Non-goals of the first release:** automatic executable civil-engineering plans; claiming forecasts are causal proof; using ideal cycling weather for all travelers; assuming every resident can bike; live traffic signal control; autonomous construction decisions.

**Core stack:** Rust simulator, geospatial/routing and Axum API; PyTorch RL via PyO3; Parquet/GeoArrow and PostGIS; Svelte + MapLibre; scientific validation with observed datasets and independent detailed transportation simulation.

**Personas:** transportation planner, municipal engineer, transit planner, accessibility advocate, mobility researcher, resident, data engineer, RL engineer, maintainer, operator and API developer.

## Definition of Done (all stories)

1. Implementation and acceptance criteria verified in automated tests, plus a human-reviewable demo for user-facing changes.
2. Public API and stored data schema changes documented and versioned; all assumptions and units explicit.
3. Determinism or stochastic confidence ranges recorded where relevant; no invented missing input data.
4. Performance impact measured on a representative fixture for simulation-critical features.
5. Data provenance, attribution, licenses, and consent/privacy concerns addressed where applicable.
6. Failures are explicit and actionable; accessibility and security tests included for affected surfaces.

## Scope and prioritization

- **P0:** Core, end-to-end MVP and mandatory scientific infrastructure.
- **P1:** Planning-grade beta features, multi-objective decisions and robustness.
- **P2:** Advanced capabilities and broader ecosystem.

**Total:** 20 epics, 151 stories (86 P0 / 51 P1 / 14 P2).

---

## E01 — Product foundations and agent-friendly engineering

**Outcome:** Reproducible Rust-first monorepo with explicit component contracts, safe agent workflows, and architecture decisions.

**Dependencies:** None

### E01-S01 · P0

As a maintainer, I want a Rust workspace with isolated crates for ingest, graph, routing, simulation, interventions, scoring, optimization bridge, and API, so that teams can build independently.

**Acceptance criteria:** Workspace compiles on supported Linux and macOS runners; crate boundaries and interfaces are documented.

### E01-S02 · P0

As a developer, I want a one-command local development setup, so that I can reproduce builds and demos.

**Acceptance criteria:** Pinned toolchains and lockfiles; bootstrap verifies dependencies and runs a smoke test.

### E01-S03 · P0

As a coding agent, I want an AGENTS.md and module-specific contracts, so that I can implement features without breaking neighboring modules.

**Acceptance criteria:** Tasks identify owned files, public APIs, tests, and prohibited cross-module changes.

### E01-S04 · P0

As a maintainer, I want automated formatting, linting, unit tests, and dependency checks, so that every change has a consistent quality gate.

**Acceptance criteria:** Required CI checks are green on a clean checkout.

### E01-S05 · P1

As a maintainer, I want a benchmark harness with regression history, so that we catch throughput or memory regressions.

**Acceptance criteria:** Named fixtures, hardware metadata, and comparable benchmark outputs are stored.

### E01-S06 · P1

As a research lead, I want versioned architecture decision records, so that technology choices remain evidence-based.

**Acceptance criteria:** Simulator/RL/framework selections include benchmark rationale and an exit path.


## E02 — City discovery and study-area onboarding

**Outcome:** Users can locate and create repeatable transportation study areas for any supported US geography.

**Dependencies:** E01

### E02-S01 · P0

As a planner, I want to search for a US city or metropolitan area, so that I can begin an analysis without GIS expertise.

**Acceptance criteria:** Search resolves canonical geography and displays its boundary.

### E02-S02 · P0

As a planner, I want to choose city, metro, or drawn study-area boundaries, so that I can include commuter sheds beyond city limits.

**Acceptance criteria:** Saved study area is a valid polygon with CRS and region metadata.

### E02-S03 · P0

As a planner, I want a preview of available city datasets and gaps, so that I know whether the model can be initialized.

**Acceptance criteria:** Coverage report lists source, year, spatial extent, status, and missing required data.

### E02-S04 · P0

As a planner, I want to initialize a city project with a snapshot date, so that I can reproduce the starting network.

**Acceptance criteria:** Project records boundary, source versions, baseline ID, and import status.

### E02-S05 · P1

As a planner, I want to expand the buffer around an urban core, so that traffic diversion outside municipal lines is represented.

**Acceptance criteria:** Boundary change re-runs affected ingestion and clearly marks prior scenarios stale.

### E02-S06 · P2

As a administrator, I want batch imports of a catalog of cities, so that we can compare regional patterns at scale.

**Acceptance criteria:** Imports are idempotent and report individual successes/failures.


## E03 — Public data connectors and provenance

**Outcome:** Ingest relevant US transportation, population, land-use and safety inputs with traceable licensing and timestamps.

**Dependencies:** E02

### E03-S01 · P0

As a data engineer, I want to ingest OpenStreetMap roads, paths, turn restrictions and tags, so that the physical network is grounded in geography.

**Acceptance criteria:** Import produces segment metadata, provenance, and a validation report.

### E03-S02 · P0

As a data engineer, I want to ingest GTFS transit schedules and optional GTFS-Realtime, so that existing transit routes and service frequency are represented.

**Acceptance criteria:** Service calendars, stops, trips and transfers parse with error reports.

### E03-S03 · P0

As a data engineer, I want to ingest Census population and commuter origin-destination data, so that we can estimate activity locations and commute demand.

**Acceptance criteria:** Each dataset has version, geography mapping, and known coverage limits.

### E03-S04 · P0

As a data engineer, I want to ingest land use and destination points, so that non-commute trips can be modeled.

**Acceptance criteria:** Destinations are categorized and deduplicated with source traceability.

### E03-S05 · P1

As a data engineer, I want to integrate DOT traffic counts and observed speeds, so that the road model can be calibrated.

**Acceptance criteria:** Observations retain location, timestamp, quality flags, and units.

### E03-S06 · P1

As a researcher, I want to ingest crash and emissions-related inputs, so that safety and emissions metrics use defensible evidence.

**Acceptance criteria:** Sources and uncertainty are recorded; fatal crash and nonfatal data are not conflated.

### E03-S07 · P1

As a administrator, I want a licensing and freshness inventory, so that we know what data can legally be published or redistributed.

**Acceptance criteria:** Each source records attribution, license terms, last retrieval and review state.

### E03-S08 · P2

As a data engineer, I want custom CSV/GeoJSON/Parquet uploads with a schema mapper, so that cities with local datasets can improve model fidelity.

**Acceptance criteria:** Preview, schema validation and rollback work without corrupting the baseline.


## E04 — Geospatial normalization and data quality

**Outcome:** Build a consistent spatial model from heterogeneous raw sources and surface uncertainty.

**Dependencies:** E03

### E04-S01 · P0

As a data engineer, I want consistent coordinates, units, time zones and identifiers, so that layers can be overlaid and joined correctly.

**Acceptance criteria:** CRS, units, local-time handling and geometry validity are tested.

### E04-S02 · P0

As a simulator developer, I want snapped intersections and connected edges, so that routes do not fail due to import artifacts.

**Acceptance criteria:** Topology tests detect dangling links, duplicate edges and impossible connections.

### E04-S03 · P0

As a transit modeler, I want stops aligned to reachable walking paths, so that station access times are realistic.

**Acceptance criteria:** Unreachable stops are flagged and manual fixes are auditable.

### E04-S04 · P0

As a researcher, I want a per-layer completeness and uncertainty score, so that I can distinguish data absence from zero demand.

**Acceptance criteria:** Quality dashboard shows missing regions, invalid values and confidence tiers.

### E04-S05 · P1

As a data engineer, I want automated anomaly reports for speeds, lane counts and connectivity, so that input errors do not become optimization results.

**Acceptance criteria:** Outliers create reviewable findings and do not silently overwrite records.

### E04-S06 · P1

As a administrator, I want immutable raw inputs and derived snapshot lineage, so that a scenario can be rebuilt later.

**Acceptance criteria:** Every derived artifact references raw hashes and processing version.


## E05 — Multimodal network representation

**Outcome:** Create a time-aware graph supporting car, walking, conventional bicycle, e-bike, bus, rail and transfers.

**Dependencies:** E04

### E05-S01 · P0

As a traveler modeler, I want walking links with crossings and barriers, so that pedestrian journeys follow actual accessible routes.

**Acceptance criteria:** Paths account for one-way access, crossings and disconnected segments.

### E05-S02 · P0

As a traveler modeler, I want bicycle and e-bike links with protection and safety attributes, so that mode-specific routing reflects infrastructure quality.

**Acceptance criteria:** Routing distinguishes protected lanes, shared roads and forbidden segments.

### E05-S03 · P0

As a transit modeler, I want scheduled bus, train and light-rail journeys, so that transit travel includes service reality.

**Acceptance criteria:** Edges include schedules, headways, waiting, transfers and service calendars.

### E05-S04 · P0

As a simulator developer, I want vehicle roads with capacity, direction and turn rules, so that driving is a valid reference mode.

**Acceptance criteria:** Routes honor legal constraints and configurable capacity estimates.

### E05-S05 · P0

As a system engineer, I want stable graph serialization and version IDs, so that simulations reuse a canonical representation.

**Acceptance criteria:** Round-trip serialization preserves connectivity and attributes.

### E05-S06 · P1

As a traveler modeler, I want park-and-ride and bike-on-transit/parking transfers, so that intermodal journeys are possible.

**Acceptance criteria:** Transfer penalties, availability and restrictions are configurable.

### E05-S07 · P1

As a accessibility advocate, I want step-free and mobility-aid accessible paths, so that the model does not treat every traveler as able to cycle.

**Acceptance criteria:** Accessible routing excludes impassable infrastructure and identifies unmet trips.


## E06 — Travel demand and traveler diversity

**Outcome:** Construct time-dependent trips for essential and discretionary travel while modeling heterogeneous capabilities and choices.

**Dependencies:** E03, E05

### E06-S01 · P0

As a researcher, I want time-of-day origin-destination demand for commute and non-commute trips, so that scenarios reflect the full day.

**Acceptance criteria:** Trips have origin, destination, departure window, purpose and expansion weights.

### E06-S02 · P0

As a researcher, I want population-weighted synthetic travelers, so that neighborhoods are represented proportionately.

**Acceptance criteria:** Generated totals reconcile to documented source populations within defined tolerance.

### E06-S03 · P0

As a traveler modeler, I want a choice set across walking, biking, e-biking, transit and car, so that mode shifts are possible after investment.

**Acceptance criteria:** Unavailable/infeasible modes are excluded per traveler and trip.

### E06-S04 · P0

As a researcher, I want configurable generalized travel costs, so that choices respond to time, waiting, reliability and expense.

**Acceptance criteria:** Parameters are explicit, unit-tested and versioned.

### E06-S05 · P1

As a researcher, I want trip chaining and return journeys, so that grocery and school-dropoff trips are modeled.

**Acceptance criteria:** Multi-stop itineraries maintain realistic time windows and load constraints.

### E06-S06 · P1

As a accessibility advocate, I want traveler groups with different mobility constraints, so that results do not overstate bike suitability.

**Acceptance criteria:** Groups can include noncyclists, wheelchair users, children and older adults.

### E06-S07 · P1

As a researcher, I want weather, season and daylight profiles, so that cycling and walking availability varies realistically.

**Acceptance criteria:** Scenario assumptions are selectable, labeled and sensitivity-tested.

### E06-S08 · P2

As a researcher, I want long-term induced demand and relocation sensitivity, so that we can study adaptation over years.

**Acceptance criteria:** Long-horizon effects are reported separately from short-term forecasts.


## E07 — Multimodal routing and door-to-door itineraries

**Outcome:** Produce realistic travel-time and flexibility comparisons across all supported modes.

**Dependencies:** E05, E06

### E07-S01 · P0

As a user, I want door-to-door routing for car and car-free modes, so that I can compare equivalent journeys.

**Acceptance criteria:** Itinerary includes access, wait, ride, transfer, egress and arrival time.

### E07-S02 · P0

As a user, I want departure-time-aware transit routing, so that frequency and schedule constraints affect travel.

**Acceptance criteria:** Off-service trips return infeasible instead of fabricated travel times.

### E07-S03 · P0

As a cyclist, I want bike/e-bike route selection based on infrastructure quality, so that unsafe shortcuts are not rewarded.

**Acceptance criteria:** A tunable comfort/safety penalty is applied and inspectable.

### E07-S04 · P0

As a researcher, I want batch origin-destination routing, so that accessibility can be measured citywide.

**Acceptance criteria:** Matrix jobs run deterministically against an immutable network version.

### E07-S05 · P1

As a user, I want multi-stop journeys and last-mile transfers, so that the model measures real errand flexibility.

**Acceptance criteria:** Itineraries support configurable dwell times and transfer constraints.

### E07-S06 · P1

As a researcher, I want travel-time variability across departure windows, so that spontaneity and service gaps are visible.

**Acceptance criteria:** Results include percentile travel times and no-service fractions.

### E07-S07 · P1

As a researcher, I want routing performance measurements, so that we can scale analysis sensibly.

**Acceptance criteria:** Benchmarks report paths per second, latency and memory by graph size.


## E08 — Fast transportation simulation and behavior change

**Outcome:** Evaluate infrastructure changes using a fast Rust simulator that updates mode choice, routing and congestion.

**Dependencies:** E05, E06, E07

### E08-S01 · P0

As a simulation engineer, I want to run a deterministic baseline scenario, so that I can compare proposed networks fairly.

**Acceptance criteria:** Same inputs and seed produce equivalent outputs with documented tolerances.

### E08-S02 · P0

As a simulation engineer, I want to apply network changes without rebuilding source data, so that scenario evaluation is efficient.

**Acceptance criteria:** Simulator accepts immutable baseline plus a reversible intervention set.

### E08-S03 · P0

As a researcher, I want travelers to reconsider mode and route after infrastructure changes, so that removing car lanes need not assume permanent car demand.

**Acceptance criteria:** Mode-choice iteration converges or reports nonconvergence.

### E08-S04 · P0

As a researcher, I want time-dependent capacity and congestion, so that road interventions model diverted car trips.

**Acceptance criteria:** Traffic volumes and travel times respond to changes in supply and demand.

### E08-S05 · P0

As a researcher, I want waiting, crowding and capacity effects for transit, so that bus and rail improvements are not treated as infinite-capacity.

**Acceptance criteria:** Boarding delay or denied boarding is modeled or explicitly bounded.

### E08-S06 · P1

As a simulation engineer, I want batched and parallel scenario evaluation, so that optimizers can search many alternatives.

**Acceptance criteria:** Runs exploit multicore execution and report scenarios/second.

### E08-S07 · P1

As a researcher, I want sensitivity analysis over uncertain demand and behavior, so that rankings are not artifacts of a single assumption.

**Acceptance criteria:** Multiple seeds/parameter sets yield distributions, not just a point estimate.

### E08-S08 · P2

As a simulation engineer, I want an abstraction layer for alternate simulators, so that engines can evolve without rewriting the product.

**Acceptance criteria:** Same scenario schema executes through at least two adapters.


## E09 — Baseline calibration and observed-data validation

**Outcome:** Establish trustworthy historical reference conditions before making optimization claims.

**Dependencies:** E03, E06, E08

### E09-S01 · P0

As a research lead, I want to compare modeled flows with observed traffic counts, so that we know whether road demand is credible.

**Acceptance criteria:** Residuals and source coverage are shown by location and time.

### E09-S02 · P0

As a transit analyst, I want to compare modeled transit use with observed ridership where available, so that transit predictions have a reality check.

**Acceptance criteria:** Differences are reported by route or service area with missing-data flags.

### E09-S03 · P0

As a researcher, I want to compare mode shares and commute times against surveys, so that baseline behavior is calibrated.

**Acceptance criteria:** Calibration report includes targets, errors and model parameters.

### E09-S04 · P0

As a research lead, I want a validation holdout dataset, so that tuning does not consume all available evidence.

**Acceptance criteria:** Held-out observations are not used for fitting and have separate scores.

### E09-S05 · P1

As a researcher, I want repeatable calibration runs, so that updates to demand parameters can be audited.

**Acceptance criteria:** Calibration artifacts store seeds, source versions and parameter changes.

### E09-S06 · P1

As a planner, I want an explicit uncertainty/fitness banner, so that I do not mistake weak evidence for confidence.

**Acceptance criteria:** Projects with failing calibration checks display limitations in results and exports.


## E10 — Infrastructure interventions and scenario authoring

**Outcome:** Enable valid, reversible redesign of streets, corridors, intersections, and connections.

**Dependencies:** E05, E08

### E10-S01 · P0

As a planner, I want to convert general-purpose road lanes to protected bike lanes, so that I can test reducing car-dedicated space.

**Acceptance criteria:** Road capacity, bike connectivity and safety attributes update consistently.

### E10-S02 · P0

As a planner, I want to create or improve continuous pedestrian connections, so that I can test walkability changes.

**Acceptance criteria:** Edited links connect to the pedestrian graph and obey crossing constraints.

### E10-S03 · P0

As a planner, I want to add or change dedicated bus lanes, so that I can test transit priority.

**Acceptance criteria:** Road and transit travel times respond without double-counting lane capacity.

### E10-S04 · P0

As a planner, I want to modify a whole corridor rather than isolated segments, so that interventions provide continuous travel utility.

**Acceptance criteria:** Tool selects connected segments and previews gaps or incompatible junctions.

### E10-S05 · P0

As a planner, I want to undo, redo, and version scenario changes, so that I can safely experiment.

**Acceptance criteria:** Reverting restores the exact previous network state.

### E10-S06 · P1

As a planner, I want to remove or convert a road/highway segment, so that I can assess major car-capacity reductions.

**Acceptance criteria:** Regional diversion, access continuity and emergency constraints are evaluated.

### E10-S07 · P1

As a planner, I want to add protected intersections and crossings, so that cycling/walking upgrades remain safe at junctions.

**Acceptance criteria:** Intersection changes affect route comfort and crossing time.

### E10-S08 · P2

As a planner, I want to model land-use or destination changes separately, so that I can explore urban form as an extension.

**Acceptance criteria:** Transport-only and transport-plus-land-use scenarios are clearly distinguished.


## E11 — Transit and intermodal service design

**Outcome:** Allow bus, BRT, tram/light rail and rail service proposals with operational realism.

**Dependencies:** E05, E07, E08, E10

### E11-S01 · P0

As a transit planner, I want to add or reroute bus service, so that I can serve underserved destinations.

**Acceptance criteria:** Routes connect valid stops and have defined service calendars.

### E11-S02 · P0

As a transit planner, I want to change bus frequency, hours and transfer coordination, so that I can improve spontaneity and reliability.

**Acceptance criteria:** Waiting and journey calculations update based on the new timetable.

### E11-S03 · P0

As a transit planner, I want to designate BRT or transit-priority corridors, so that I can test high-frequency service with dedicated capacity.

**Acceptance criteria:** Road capacity and transit speed assumptions remain internally consistent.

### E11-S04 · P1

As a transit planner, I want to propose rail or light-rail lines and stations, so that I can evaluate high-capacity routes.

**Acceptance criteria:** Routing respects stations, line frequency, travel times and transfers.

### E11-S05 · P1

As a transit planner, I want to add bike parking, bike access and station connections, so that intermodal journeys are practical.

**Acceptance criteria:** Station access/egress changes affect door-to-door accessibility.

### E11-S06 · P1

As a transit planner, I want to estimate fleet, operator and operating requirements, so that plans are not costed as infrastructure alone.

**Acceptance criteria:** Fleet hours and operating cost estimates accompany service changes.

### E11-S07 · P2

As a transit planner, I want to test timed transfers and fare policies, so that network integration can improve without new roads.

**Acceptance criteria:** Fares, transfer timing and generalized travel costs are updated.


## E12 — Feasibility, costs, safety and physical constraints

**Outcome:** Exclude unsafe/impossible proposals and estimate implementation trade-offs with traceable assumptions.

**Dependencies:** E10, E11

### E12-S01 · P0

As a planner, I want a configurable capital and annual operating budget, so that proposals remain financially bounded.

**Acceptance criteria:** Invalid projects are blocked and cost categories are separated.

### E12-S02 · P0

As a engineer, I want right-of-way and lane-width feasibility checks, so that a proposed street cross-section can physically fit.

**Acceptance criteria:** Design violations are flagged with segment-level reasons.

### E12-S03 · P0

As a planner, I want emergency, disability, essential delivery and property-access constraints, so that car independence does not eliminate necessary access.

**Acceptance criteria:** Hard constraints are tested before costly simulation.

### E12-S04 · P0

As a planner, I want to retain regional network connectivity for freight and essential trips, so that road reductions do not create hidden disconnections.

**Acceptance criteria:** Connectivity tests include designated critical origin-destination pairs.

### E12-S05 · P1

As a engineer, I want construction, maintenance and transit operating cost models, so that lifecycle choices are comparable.

**Acceptance criteria:** Unit-cost assumptions, year and local adjustment factors are exposed.

### E12-S06 · P1

As a researcher, I want collision-risk estimates separated from vehicle movement models, so that safety benefits are not invented from simulated crashes.

**Acceptance criteria:** Risk model, exposure, calibration source and uncertainty are reported.

### E12-S07 · P1

As a planner, I want construction phases and disruption costs, so that the order of projects is evaluated realistically.

**Acceptance criteria:** Each phase has a valid network and temporary-service assumptions.

### E12-S08 · P2

As a engineer, I want bridge, slope, utility and terrain restrictions, so that difficult locations are screened early.

**Acceptance criteria:** Known restrictions are flagged; missing engineering evidence is labeled.


## E13 — Car independence, accessibility and equity scoring

**Outcome:** Measure the product goal directly, including all trips, populations, costs and reliability.

**Dependencies:** E07, E08, E09, E12

### E13-S01 · P0

As a planner, I want the share of essential destinations reachable car-free within a time budget, so that I can measure real mobility rather than lane throughput.

**Acceptance criteria:** Scores are computed by origin, destination type, mode set and departure window.

### E13-S02 · P0

As a resident, I want a car-free versus car door-to-door travel-time comparison, so that I can judge practical mobility parity.

**Acceptance criteria:** Includes walking, waiting, transfers, reliability and infeasible trips.

### E13-S03 · P0

As a researcher, I want an estimated car-required trip share and mode distribution, so that the main objective is explicit.

**Acceptance criteria:** Metric definitions, assumptions and confidence intervals are published.

### E13-S04 · P0

As a planner, I want neighborhood-level accessibility maps, so that local improvements and gaps are visible.

**Acceptance criteria:** Maps have population denominators and flag insufficient coverage.

### E13-S05 · P0

As a planner, I want to measure infrastructure space devoted to private cars, so that we can track physical car dependence.

**Acceptance criteria:** Road-space metrics are spatially aggregated without double counting.

### E13-S06 · P0

As a accessibility advocate, I want equity and mobility-needs breakdowns, so that a high citywide score cannot hide isolated groups.

**Acceptance criteria:** Results report outcomes for relevant user groups and neighborhoods.

### E13-S07 · P1

As a researcher, I want estimated vehicle travel and operational emissions, so that environmental outcomes remain visible.

**Acceptance criteria:** Vehicle distance and emissions methodology are documented with uncertainty.

### E13-S08 · P1

As a researcher, I want a Pareto frontier of costs, accessibility, safety and car dependence, so that we can choose among non-dominated designs.

**Acceptance criteria:** Dominance calculations use consistent constraints and units.

### E13-S09 · P1

As a planner, I want scenario rankings across multiple weather and service conditions, so that travel flexibility is not limited to ideal days.

**Acceptance criteria:** Metrics include day/night, service frequency and weather sensitivity.

### E13-S10 · P2

As a policy analyst, I want separate short- and long-term outcomes, so that infrastructure investment is not conflated with immediate behavior.

**Acceptance criteria:** Time horizon and behavioral assumptions are present in every score.


## E14 — RL environment and optimization specification

**Outcome:** Expose a constrained, inspectable and reproducible RL problem over sequential city redesign.

**Dependencies:** E08, E10, E12, E13

### E14-S01 · P0

As a RL engineer, I want a Gymnasium-compatible interface to the Rust simulator, so that learning algorithms can use a common environment.

**Acceptance criteria:** reset/step return versioned observations, actions, rewards and terminal flags.

### E14-S02 · P0

As a RL engineer, I want a machine-readable action catalog and feasibility mask, so that the agent cannot choose nonexistent projects.

**Acceptance criteria:** Invalid actions are excluded or rejected with explicit explanations.

### E14-S03 · P0

As a RL engineer, I want an observation encoding of network, demand, budget and accessibility, so that policies receive decision-relevant state.

**Acceptance criteria:** Schema is documented and invariance tests detect accidental data leakage.

### E14-S04 · P0

As a research lead, I want a reward anchored in car independence and mobility parity, so that optimization advances the primary goal.

**Acceptance criteria:** Weights and hard constraints are user-configurable and logged.

### E14-S05 · P0

As a RL engineer, I want sequential episodes with construction and operating budgets, so that ordering matters.

**Acceptance criteria:** Each step updates cumulative network, costs and dependent metrics.

### E14-S06 · P1

As a RL engineer, I want batched Rust-Python bindings, so that training avoids excessive per-step overhead.

**Acceptance criteria:** Batch APIs use bounded memory and have profiling tests.

### E14-S07 · P1

As a research lead, I want reward ablations and failure diagnostics, so that we detect gaming of a single metric.

**Acceptance criteria:** Experiments log decomposed rewards, constraint failures and trivial solutions.

### E14-S08 · P2

As a ML engineer, I want graph-based policy observations, so that models can transfer across city topologies.

**Acceptance criteria:** Transfer experiments compare unseen-city results with simpler representations.


## E15 — Optimization algorithms and benchmarks

**Outcome:** Find strong redesign sequences and prove whether RL outperforms simpler search.

**Dependencies:** E14

### E15-S01 · P0

As a researcher, I want a random-search reference optimizer, so that we have a minimum performance baseline.

**Acceptance criteria:** Fixed-budget runs use identical scenario evaluation and constraints.

### E15-S02 · P0

As a researcher, I want greedy and local-search optimizers, so that RL can be compared against practical alternatives.

**Acceptance criteria:** Algorithms share objective scoring, compute budgets and stopping conditions.

### E15-S03 · P0

As a RL engineer, I want a first trainable constrained policy, so that the system can learn a sequence of projects.

**Acceptance criteria:** Checkpoint save/load and deterministic evaluation work.

### E15-S04 · P1

As a researcher, I want an evolutionary or multi-objective optimizer, so that we can explore trade-offs beyond one reward.

**Acceptance criteria:** Output is a feasible non-dominated set using the same simulator.

### E15-S05 · P1

As a researcher, I want held-out maps and demand scenarios, so that we can test generalization.

**Acceptance criteria:** Evaluation data and seeds are isolated from training.

### E15-S06 · P1

As a research lead, I want a fair comparison dashboard, so that we can assess whether RL earns its complexity.

**Acceptance criteria:** Results include objective improvement, validity rate, compute time and variance.

### E15-S07 · P2

As a ML engineer, I want warm-start, curriculum or transfer training, so that policies improve across cities.

**Acceptance criteria:** Ablations document benefit over training from scratch.


## E16 — Experiment orchestration and compute management

**Outcome:** Execute large experiments reliably, with transparent costs, reproducibility and interruptibility.

**Dependencies:** E14, E15

### E16-S01 · P0

As a researcher, I want to submit a simulation or optimization job, so that I can run work without holding the UI open.

**Acceptance criteria:** Jobs persist with input hashes, parameters, status and result references.

### E16-S02 · P0

As a researcher, I want progress and partial logs, so that I can diagnose long-running experiments.

**Acceptance criteria:** Status shows scenario count, failures and current best metrics.

### E16-S03 · P0

As a researcher, I want to stop and resume jobs where supported, so that compute is not wasted.

**Acceptance criteria:** Cancellation releases resources; resumption uses compatible checkpoints.

### E16-S04 · P0

As a operator, I want isolated worker processes with quotas, so that one job cannot exhaust the service.

**Acceptance criteria:** CPU, memory and concurrency limits are enforced.

### E16-S05 · P1

As a researcher, I want seed sweeps and parallel evaluations, so that uncertainty can be evaluated efficiently.

**Acceptance criteria:** Artifacts identify seeds and produce an aggregate distribution.

### E16-S06 · P1

As a operator, I want distributed execution on multiple workers, so that metropolitan experiments can scale.

**Acceptance criteria:** Idempotent work items retry safely and do not duplicate finalized results.

### E16-S07 · P1

As a operator, I want compute and storage usage reports, so that we can budget expensive runs.

**Acceptance criteria:** Each run reports wall time, resource usage and estimated spend.


## E17 — Interactive application and spatial editing

**Outcome:** Make baseline conditions, redesign tools, and results usable in a responsive map-based web interface.

**Dependencies:** E02, E10, E13, E16

### E17-S01 · P0

As a planner, I want a map-based study-area wizard, so that I can select and initialize a city visually.

**Acceptance criteria:** Boundary, dataset availability and initialization feedback are visible.

### E17-S02 · P0

As a planner, I want to switch among road, bike, walking, bus and rail layers, so that I can inspect the multimodal network.

**Acceptance criteria:** Layers have legends and distinguish unknown data from absent infrastructure.

### E17-S03 · P0

As a planner, I want to select connected corridors and propose conversions, so that I can edit a scenario without code.

**Acceptance criteria:** Map selection produces a preview and feasibility warnings.

### E17-S04 · P0

As a planner, I want to edit intervention parameters and budgets, so that I can test realistic variants.

**Acceptance criteria:** Forms validate units and show estimated costs before applying.

### E17-S05 · P0

As a planner, I want to submit and follow scenario runs, so that I can connect edits to results.

**Acceptance criteria:** UI reflects queued, running, failed and completed jobs.

### E17-S06 · P0

As a resident, I want before/after travel isochrones and accessibility maps, so that I can see which destinations become car-free reachable.

**Acceptance criteria:** Maps let me choose origin, mode set, time window and metric.

### E17-S07 · P1

As a planner, I want linked side-by-side map and metric comparisons, so that I can inspect geographic trade-offs.

**Acceptance criteria:** Map extents and legends stay synchronized between variants.

### E17-S08 · P1

As a resident, I want a responsive and keyboard-accessible interface, so that the app works across devices and needs.

**Acceptance criteria:** Core flows have keyboard navigation, accessible labels and mobile layouts.

### E17-S09 · P2

As a planner, I want an animated sequence of infrastructure phases, so that I can understand how the city evolves.

**Acceptance criteria:** Timeline shows reversible per-phase changes and related outcomes.


## E18 — Interpretation, explainability and reports

**Outcome:** Show why a design was selected and where it helps or harms residents.

**Dependencies:** E13, E15, E17

### E18-S01 · P0

As a planner, I want to compare baseline and scenario results for core metrics, so that I can judge the effect of redesign.

**Acceptance criteria:** Comparisons include differences, units, assumptions and uncertainty status.

### E18-S02 · P0

As a planner, I want to inspect effects by neighborhood, so that local regressions are visible.

**Acceptance criteria:** Users can filter maps/tables to an area and see affected trip groups.

### E18-S03 · P0

As a researcher, I want to trace each optimization result to interventions, so that a result is auditable.

**Acceptance criteria:** Every output references scenario version, projects, seeds and model version.

### E18-S04 · P1

As a planner, I want a step-by-step improvement chart, so that I can understand intervention order.

**Acceptance criteria:** Each step displays incremental and cumulative metric changes.

### E18-S05 · P1

As a researcher, I want sensitivity and robustness comparisons, so that uncertain assumptions are not concealed.

**Acceptance criteria:** Reports show ranges under tested demand, weather and behavior assumptions.

### E18-S06 · P1

As a planner, I want to view trade-offs across Pareto-optimal scenarios, so that I can select a policy preference explicitly.

**Acceptance criteria:** Changing a priority does not silently overwrite prior scenario results.

### E18-S07 · P1

As a researcher, I want exportable methodology notes and known limitations, so that results are interpreted responsibly.

**Acceptance criteria:** Report records calibration quality, causal limits and models used.


## E19 — Workspaces, APIs, sharing and export

**Outcome:** Persist projects and enable programmatic and collaborative analysis with explicit sharing controls.

**Dependencies:** E01, E16, E17, E18

### E19-S01 · P0

As a user, I want a saved project containing city, scenarios and results, so that I can return to my work.

**Acceptance criteria:** Projects reopen against exact versioned inputs and show stale status.

### E19-S02 · P0

As a developer, I want a documented Rust API for cities, scenarios, evaluations and jobs, so that tools and agents can automate workflows.

**Acceptance criteria:** Versioned endpoints have OpenAPI schema, validation and error contracts.

### E19-S03 · P0

As a researcher, I want machine-readable scenario and metric exports, so that I can reproduce analysis outside the UI.

**Acceptance criteria:** Exports include JSON/Parquet and a versioned manifest.

### E19-S04 · P1

As a team member, I want workspace roles for owner, editor and viewer, so that collaboration is controlled.

**Acceptance criteria:** Authorization tests cover read and write actions.

### E19-S05 · P1

As a planner, I want shareable read-only scenario links, so that colleagues can inspect results.

**Acceptance criteria:** Public links reveal only explicitly published data.

### E19-S06 · P1

As a researcher, I want map and report exports, so that I can distribute results without the app.

**Acceptance criteria:** Exports include legend, units, caveats, data attribution and date.

### E19-S07 · P2

As a external developer, I want a stable plugin/connector contract, so that additional datasets and optimizers can be added.

**Acceptance criteria:** Extension manifest and compatibility tests are published.

### E19-S08 · P2

As a collaborator, I want project comments and review states, so that teams can discuss design trade-offs.

**Acceptance criteria:** Comments reference a scenario revision and survive new revisions.


## E20 — Scientific verification, reliability, security and release

**Outcome:** Deliver a trustworthy, observable and maintainable public-facing system with repeatable scientific claims.

**Dependencies:** E01, E09, E12, E15, E19

### E20-S01 · P0

As a tester, I want unit and property tests for graph, geometry, units and metrics, so that numeric errors are caught early.

**Acceptance criteria:** CI includes invariants for connectivity, conservation and monotonicity where applicable.

### E20-S02 · P0

As a tester, I want full end-to-end tests from city import through scenario scoring, so that the core workflow actually works.

**Acceptance criteria:** A small public fixture runs without manual steps and produces a reproducible result.

### E20-S03 · P0

As a research lead, I want independent detailed-simulator comparison, so that the fast engine has measured error.

**Acceptance criteria:** Representative scenarios are cross-checked with SUMO/MATSim or a vetted equivalent.

### E20-S04 · P0

As a operator, I want structured logs, metrics and traces, so that failed data jobs and training runs can be diagnosed.

**Acceptance criteria:** Each job and scenario has correlated identifiers and actionable errors.

### E20-S05 · P0

As a operator, I want versioned releases, migrations and backups, so that deployed results remain available.

**Acceptance criteria:** Upgrade and restore rehearsals succeed on a staging dataset.

### E20-S06 · P0

As a security engineer, I want input validation, secrets management and dependency scanning, so that untrusted imports and APIs do not compromise the system.

**Acceptance criteria:** Threat-model priorities have regression tests and secrets never enter logs.

### E20-S07 · P1

As a research lead, I want uncertainty and model-card documentation, so that users understand forecast limitations.

**Acceptance criteria:** Each release documents assumptions, tested geography and failure modes.

### E20-S08 · P1

As a maintainer, I want Linux/macOS development and container deployment, so that developers and CI share reproducible execution.

**Acceptance criteria:** Smoke tests pass on the supported environments.

### E20-S09 · P1

As a operator, I want load tests with large cities and concurrent experiments, so that performance limits are known.

**Acceptance criteria:** Published baselines cover memory, throughput, API latency and degradation.

### E20-S10 · P2

As a community contributor, I want public contribution guide, licenses and sample projects, so that the platform can be adopted and improved.

**Acceptance criteria:** A new contributor can reproduce the demo from published instructions.

## Delivery plan and dependency gates

| Gate | Scope | Exit criterion |
|---|---|---|
| R0: scientific and engineering foundation | E01–E04 (P0) | Reproducible import of one study area; license/provenance and topological QA pass |
| R1: trustworthy baseline | E05–E09 (P0) | Walking, cycling, e-bike, car, and scheduled transit routes work; real-world calibration and holdout errors are visible |
| R2: manual redesign MVP | E10–E13 and relevant E17/E19/E20 (P0) | A planner can convert a corridor, run a scenario, and compare car-free access and travel time with the baseline |
| R3: optimization MVP | E14–E16 plus E18 (P0) | Greedy, random, and an RL policy evaluate the same feasible project set under identical budget and seeds |
| R4: credible planning beta | P1 stories, especially E12/E18/E20 | Regional diversion, uncertainty, costs, equity, and independent simulator checks are visible in reports |
| R5: broad city support | P2 stories plus multi-city hardening | Multiple cities work without source-specific code changes and held-out-city evaluation is published |

**Gate rule:** Do not claim a neighborhood is car-independent merely because transit service exists. Evaluate realistic journeys, wait times, schedule coverage, individual mobility constraints, and the share of essential trips that remain infeasible. Do not deploy policy recommendations where the baseline model lacks adequate calibration or where uncertainty is not disclosed.

## First vertical slice / initial agent work packets

1. **Bootstrap (E01-S01–S04):** Rust workspace, pinned tools, lint/test CI, AGENTS.md, starter fixture.
2. **Import a pilot area (E02-S01–S04, E03-S01–S04, E04-S01–S04):** OSM/GTFS/Census ingest and city snapshot with data-quality report.
3. **Construct the mobility model (E05-S01–S05, E06-S01–S04, E07-S01–S04):** Multimodal graph, synthetic trips, baseline routing and 30-minute car-free access.
4. **Make changes measurable (E08-S01–S04, E10-S01–S05, E13-S01–S06):** Edit one corridor, reroute demand, report differences and constraint failures.
5. **Enable the app workflow (E17-S01–S06, E19-S01–S03, E20-S01–S04):** Import → map → edit → simulate → compare → export, tested end-to-end.
6. **Add reproducible search (E14-S01–S05, E15-S01–S03, E16-S01–S04):** Valid action masking, greedy/random baselines, first RL policy, repeatable job tracking.

**Important ordering:** The first releasable experience is a trustworthy baseline and manually editable city scenario. RL training follows only after those outputs are reasonably calibrated and reproducible.
