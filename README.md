# Car Independence Optimizer

A Rust-first platform to measure, simulate, and optimize the transition of US cities away from dependence on private cars.

The primary outcome is **car independence**: more residents able to complete daily trips conveniently, reliably, affordably, safely, and accessibly using walking, conventional bikes, e-bikes, buses, and rail. Vehicle throughput is *not* the optimization objective.

> **Project status:** Planning and initial workspace scaffold. The simulation is not yet implemented, and no planning-grade predictions should be inferred from this repository.

## Principles

- **Accessible destinations, not road throughput.** Optimize door-to-door access to daily needs without private automobiles.
- **Multimodal by construction.** Represent walking, bikes, e-bikes, buses, light rail, heavy rail, transfers, and essential vehicle access.
- **Transition-aware.** Optimize sequences of changes and allow travel demand and mode choices to adapt.
- **Equitable and constrained.** Protect mobility for people who cannot cycle, emergency access, freight needs, and underserved areas.
- **Empirical.** Clearly distinguish measured data, modeled assumptions, predictions, and uncertainty.
- **Performance first.** Rust owns hot paths; Python/PyTorch is used where the RL ecosystem is stronger.
- **Reproducible and agent-friendly.** Tests, deterministic fixtures, explicit APIs, and independently owned modules.

## Planned stack

| Area | Choice |
| --- | --- |
| Graph, routing, simulator, scenario engine | Rust |
| Optimization training | PyTorch + Python via PyO3/maturin |
| Data ingestion | OSM, GTFS, Census, local DOT inputs; Rust geospatial + Arrow/Parquet |
| Geographic persistence | PostgreSQL + PostGIS |
| API | Rust + Axum |
| Visualization | Svelte + MapLibre GL JS |
| Simulation validation | MATSim/SUMO as appropriate |

Technology choices are subject to benchmarking, particularly reuse of existing open-source simulation engines.

## Roadmap

1. Reproduce a study area's multimodal network and demand inputs with provenance.
2. Compute baseline car-free accessibility and car-mobility comparison metrics.
3. Add scenario edits: protected lanes, pedestrian links, bus frequency/priority, rail corridors, and selective car-space conversion.
4. Simulate mode choice and network impacts with calibration and uncertainty.
5. Establish greedy/evolutionary baselines, then RL optimization and comparative evaluation.
6. Visualize scenario maps and publish interpretable reports.
7. Extend to other cities and evaluate out-of-distribution performance.

The full product backlog is in [backlog/car_independence_backlog.md](backlog/car_independence_backlog.md) and [backlog/car_independence_backlog.json](backlog/car_independence_backlog.json) (20 epics, 151 stories).

## Workspace

```text
crates/mobility-graph     shared network domain types
crates/mobility-ingest    source-adapter boundary (not implemented)
crates/mobility-routing   routing boundary (not implemented)
crates/accessibility     access and parity metric primitives
crates/interventions     candidate infrastructure change types
crates/mobility-sim      simulation interfaces (not yet implemented)
crates/optimizer-env     planning environment interfaces
crates/optimizer-bridge  training bindings boundary (not implemented)
crates/mobility-api      application service boundary (not implemented)
benchmarks/             synthetic metric fixture, Rust harness, measured history
training/                future PyTorch training integration
web/                     future Svelte/MapLibre app
backlog/                 full prioritized user stories
docs/                    architecture and decisions
```

Install rustup and Python 3.11 or newer, then run:

```sh
rustup toolchain install 1.97.1 --profile minimal --component rustfmt --component clippy
./scripts/bootstrap.sh
```

Bootstrap checks the pinned Rust tools, enforces the current local-only dependency
policy, and runs the locked, offline workspace smoke test. It also works when
invoked by absolute path from another directory. See [development and validation](docs/DEVELOPMENT.md)
for the full quality gates and [benchmark usage](docs/BENCHMARKS.md) for measured
primitive performance history. The [module contracts](docs/MODULE-CONTRACTS.md)
identify ownership, APIs, tests, and prohibited changes for all nine domain crates.

No external input downloads, route computation, HTTP service, simulator, or Python
bindings have been wired up yet. The benchmark is a developer demonstration of an
existing metric primitive; it provides no planning predictions.

## Agent-assisted issue delivery

Project-local Codex agents and skills coordinate architecture, BDD-style tests,
Rust/Python/web implementation, independent verification, documentation, review,
and issue-linked PR delivery. Start a fresh Codex session and use
`$issue-orchestrator` with a GitHub issue URL. See the
[workflow, role ownership, and evidence gates](docs/AGENT-WORKFLOW.md).

## Publish to GitHub

The GitHub repository is [nthompson-bitwarden/carfree-optimizer](https://github.com/nthompson-bitwarden/carfree-optimizer). Deliver issue changes on a scoped branch through the signed-commit and pull-request gates in the [issue workflow](docs/AGENT-WORKFLOW.md).

For initial publication only, `./scripts/publish.sh` creates **a private repository** called `nthompson-bitwarden/carfree-optimizer` using the GitHub CLI, which you must authenticate on your machine. Inspect the script before running.

## Safety and scientific limitations

This project generates *planning scenarios*, not approved engineering or transportation plans. Conclusions must report calibration quality, missing data, costs, accessibility impacts, uncertainty ranges, and known limitations. Historic commuting flows cannot by themselves represent all daily trips.

## License

This project uses the [MIT License](LICENSE).
