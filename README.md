# Car Independence Optimizer

A Rust-first platform to measure, simulate, and optimize the transition of US cities away from dependence on private cars.

The primary outcome is **car independence**: more residents able to complete daily trips conveniently, reliably, affordably, safely, and accessibly using walking, conventional bikes, e-bikes, buses, and rail. Vehicle throughput is *not* the optimization objective.

> **Project status:** Workspace foundations, offline city/metro search, and saved study-area onboarding are implemented. The bundled catalogue covers three pilot areas. Offline public-data ingestion is available. Routing, simulation, and calibration remain future work.

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
crates/mobility-ingest    geography, project snapshots and offline public-data connectors
crates/mobility-routing   routing boundary (not implemented)
crates/accessibility     access and parity metric primitives
crates/interventions     candidate infrastructure change types
crates/mobility-sim      simulation interfaces (not yet implemented)
crates/optimizer-env     planning environment interfaces
crates/optimizer-bridge  training bindings boundary (not implemented)
crates/mobility-api      offline search/onboarding CLIs and static HTML reports
benchmarks/             synthetic metric fixture, Rust harness, measured history
training/                future PyTorch training integration
web/                     future Svelte/MapLibre app
backlog/                 full prioritized user stories
docs/                    architecture and decisions
```

Install rustup, Python 3.11 or newer, and the Protocol Buffers compiler (`protoc`). Then run:

```sh
rustup toolchain install 1.97.1 --profile minimal --component rustfmt --component clippy
rustup run 1.97.1 cargo fetch --locked
./scripts/bootstrap.sh
```

Bootstrap checks the pinned Rust tools, enforces the scoped ingest dependency
policy, and runs the locked, offline workspace smoke test. It also works when
invoked by absolute path from another directory. See [development and validation](docs/DEVELOPMENT.md)
for the full quality gates and [benchmark usage](docs/BENCHMARKS.md) for measured
primitive performance history. The [module contracts](docs/MODULE-CONTRACTS.md)
identify ownership, APIs, tests, and prohibited changes for all nine domain crates.

No runtime external input downloads, route computation, HTTP service, simulator, or Python
bindings have been wired up yet. The benchmark is a developer demonstration of an
existing metric primitive; it provides no planning predictions.

## City and metropolitan area search

E02-S01 provides deterministic offline search and a standalone HTML/SVG boundary
preview. The bundled Census 2024 pilot covers exactly Portland, Maine; Portland,
Oregon; and Portland-Vancouver-Hillsboro, OR-WA Metro Area. It is not a nationwide
catalogue. An ambiguous query displays every supported candidate.

```sh
cargo run -p mobility-api --bin city-search -- 'Portland' --output /tmp/carfree-boundaries.html
```

Open the generated file in a browser. The default output is HTML on stdout;
`--output` requires a new file and refuses to overwrite an existing one. Try
`'Portland ME'`, `'Portland OR'`, or `'cbsa:38900'` for narrower results. See the
[search/API/schema guide](docs/GEOGRAPHY-SEARCH.md) and
[source provenance and reproduction instructions](fixtures/geography/README.md).
The generalized boundaries are a geographic preview, with no routing, demand,
calibration, or accessibility predictions.

## Saved study areas and city projects

E02-S02–S06 add an offline `study-area` CLI. Select a pilot city/metro or supply
drawn polygon rings, save a dated project with source versions and coverage gaps,
expand a conservative buffer envelope, and import several projects in a batch.
For example, use new output filenames:

```sh
cargo run -p mobility-api --bin study-area -- init cbsa:38900 2026-11-17 metro.tsv
cargo run -p mobility-api --bin study-area -- scenario metro.tsv prior-plan
cargo run -p mobility-api --bin study-area -- buffer metro.tsv 10
cargo run -p mobility-api --bin study-area -- preview metro.tsv metro.html
```

Open `metro.html` to inspect the boundary, original core, source coverage, missing
required inputs, and stale scenario reference. There is no default output path:
each command takes explicit file paths; new projects and reports refuse to
overwrite files. Updates support one writer at a time. The buffer is a spherical
bounding envelope in kilometres, with polar/dateline restrictions, rather than
an exact polygon-distance buffer. Roads, transit, population and demand remain
explicitly missing, so creating a project does not initialize a transportation
model. See the [workflow, Rust API and stored schema guide](docs/STUDY-AREA-ONBOARDING.md)
for drawn boundaries, batch requests, validation and limitations.

## Import public or local data

E03 adds offline OSM, GTFS and GTFS-Realtime, Census, land-use, traffic, crash,
and emissions readers. Each import includes source provenance and validation.
The custom upload CLI maps CSV, GeoJSON and Parquet and supports preview and
rollback for a saved E02 project.

```sh
cargo run --locked -p mobility-ingest --bin ingest-data -- preview fixtures/ingestion/upload.json 2026-11-17
```

See the [data formats, licensing inventory and upload workflow](docs/PUBLIC-DATA-INGESTION.md).

## Agent-assisted issue delivery

The primary Codex session coordinates implementation and delivery, reusing two
independent workers serially for review and final verification. Start a fresh
session and use `$issue-orchestrator` with an issue URL. See the
[workflow and evidence gates](docs/AGENT-WORKFLOW.md) and local
[harness commands](docs/AGENT-WORKFLOW.md#harness-commands-and-discovery).

## Publish to GitHub

The GitHub repository is [nthompson-bitwarden/carfree-optimizer](https://github.com/nthompson-bitwarden/carfree-optimizer). Deliver issue changes on a scoped branch through the signed-commit and pull-request gates in the [issue workflow](docs/AGENT-WORKFLOW.md).

For initial publication only, `./scripts/publish.sh` creates **a private repository** called `nthompson-bitwarden/carfree-optimizer` using the GitHub CLI, which you must authenticate on your machine. Inspect the script before running.

## Safety and scientific limitations

This project generates *planning scenarios*, not approved engineering or transportation plans. Conclusions must report calibration quality, missing data, costs, accessibility impacts, uncertainty ranges, and known limitations. Historic commuting flows cannot by themselves represent all daily trips.

## License

This project uses the [MIT License](LICENSE).
