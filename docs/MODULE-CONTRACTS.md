# Module ownership and contracts

E01-S01 and E01-S03 define nine domain crates at version 0.1.0. Existing public
Rust APIs are preserved by the foundations work. E02-S01 adds the versioned
[geography catalogue schema and Rust search/presentation APIs](GEOGRAPHY-SEARCH.md).
No network protocol exists yet. Future responsibilities below remain proposed
where explicitly stated. Shared-contract changes need architecture
review and serialized edits before dependent implementations change.

| Crate / owned paths | Current public API and intended boundary | Tests and prohibited cross-module changes |
| --- | --- | --- |
| `mobility-ingest`, `crates/mobility-ingest/**` | `Catalogue::{from_tsv,pilot,entries,get,search}`, `Geography`, `GeographyKind`, `Coordinate`, `CatalogueError`, and `SearchError` (root reexports and `geography` module). TSV v1 preserves canonical IDs, rings in longitude/latitude degrees, provenance, and dates; validation is structural. Future transport adapters remain unimplemented. | E02-S01 `tests/e02_s01_geography.rs` covers canonical identity, source geometry, search ambiguity/errors, and malformed input. No routing, invented observations, or topology certification; coordinate derived graph types with graph owner. |
| `mobility-graph`, `crates/mobility-graph/**` | `Mode`, `NodeId`, `EdgeId`, `Edge`, `Edge::new`. Directed endpoints and `distance_m` in metres; constructor rejects nonfinite/nonpositive lengths. Public fields remain mutable, so callers must preserve those invariants. | `rejects_invalid_edge_distance`; E01-S01 dependency isolation. No API, RL, or training dependencies; no implicit link deletion or connectivity guarantees. |
| `mobility-routing`, `crates/mobility-routing/**` | Empty library with graph dependency. Future requests must supply traveler constraints, service time, and network inputs; future results must distinguish infeasible journeys from errors. | Workspace compilation and E01-S01 isolation/membership checks. No route algorithm, travel-time predictions, or empty-route success is available; do not modify graph contracts unilaterally. |
| `accessibility`, `crates/accessibility/**` | Scoring owner: `reachable_share(reachable, total) -> Option<f64>`. Unitless fraction; `None` for zero total or reachable above total. Caller supplies defensible counts and reachability threshold. | `reports_valid_share`, `rejects_missing_or_inconsistent_denominator`; synthetic Rust microbenchmark. No inferred demand, routing, or simulation assumptions; a share alone does not prove car independence. |
| `interventions`, `crates/interventions/**` | `Intervention` descriptions referencing graph edges, endpoints, or transit routes. Frequency uses `headway_minutes`; descriptions do not validate geography, cost, or connectivity. | `interventions_are_describable_without_mutating_a_graph`. No hidden graph mutation, automatic physical feasibility, or silent removal of emergency/freight/pedestrian/transit access. |
| `mobility-sim`, `crates/mobility-sim/**` | `ScenarioEvaluator`, `SimulationError`, `ScenarioMetrics`. Evaluation returns share (unitless), person-hours, and total vehicle-kilometres. Trait has no simulator implementation or numeric validation. | Workspace compilation and E01-S01 isolation checks. No calibration claims, invented rewards, optimizer dependency, or use of vehicle throughput as accessibility. Future implementations must validate inputs and report model provenance/uncertainty. |
| `optimizer-env`, `crates/optimizer-env/**` | `PlanningEnvironment` with `reset(seed)`, `valid_actions`, `step`; `Transition<T>` with observation/reward/termination fields. Reward policy and units are unspecified by this trait and must be supplied explicitly by future implementations. | Workspace compilation and E01-S01 membership checks. Keep framework neutral; no Python/runtime coupling, assumed feasible actions, or fabricated environment implementation. |
| `optimizer-bridge`, `crates/optimizer-bridge/**` | Empty library depending on `optimizer-env`. Future training adapters batch transitions across the language boundary. | Workspace compilation and E01-S01 membership checks. No Python binding, training runtime, FFI, or reward policy exists; no per-trip language crossing without measured rationale. |
| `mobility-api`, `crates/mobility-api/**` | `render_search_results(&Catalogue, &str) -> Result<String, SearchError>` (root and `geography` module) and `city-search` CLI. Accessible escaped static HTML/SVG displays every matched boundary, identity, coverage, and provenance; local dependency on `mobility-ingest`. Future experiment services remain unimplemented. | E02-S01 `tests/e02_s01_city_search.rs` covers boundary display, unsupported coverage, escaping, CLI output, and explicit failures. No HTTP server, endpoints, project persistence, or model exists; do not move authoritative scientific calculations into the browser or API transport layer. |

The separate `foundations-benchmark` executable in `benchmarks/harness/**` is
validation tooling, not a tenth domain engine. Its fixture, history schema,
complexity, and limitations are documented in [BENCHMARKS.md](BENCHMARKS.md).
Python scripts orchestrate validation and offline source-fixture preparation;
runtime search and timed metric computation stay Rust. The E02-S01 source
preparation helper and versioned fixtures live in `scripts/prepare-geography.py`
and `fixtures/geography/**`; they do not implement a live download service.

Every task handoff records issue/story and dependencies, owned paths, public
contracts and compatibility, prohibited edits, tests/fixtures/seeds, evidence
status, and next owner using [AGENT-WORKFLOW.md](AGENT-WORKFLOW.md). Tests for
these handoff fields live in `tests/foundations/test_foundations.py`; module
membership and compilation establish boundaries, not implemented functionality.
