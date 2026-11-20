# Spatial workbench

E17 provides a local browser application for study-area onboarding, synthetic
corridor scenarios and matched accessibility comparisons. From the repository
root, with the pinned Rust toolchain and Protobuf compiler installed:

```sh
cargo run --locked --offline -p mobility-api --bin spatial-workbench
```

Open the URL printed by the command. The default is `http://127.0.0.1:8047`.
Pass another port after `--`, or use `0` for an available port.

## Reviewable demo

1. Choose Portland, ME, Portland, OR, or Portland–Vancouver–Hillsboro. Inspect
   the Census boundary and required dataset statuses. Initialize and download
   the E02 project snapshot if you want to save it. The same TSV works with
   `study-area preview`. Transportation model initialization requires the
   missing datasets listed in the study-area panel.
2. In the synthetic network, toggle Road and Rail to compare modeled links and
   an absent mode. Select Parking approach on the map or with its checkbox,
   then Clinic approach. The order of selection sets the construction phases.
3. Set the walkway width to 8,000 mm and choose **Preview and screen**.
   Review the modeled costs and right-of-way warning. Change the width to
   1,900 mm and preview again with the default budgets. Select **Run scenario**.
4. Compare the completed run using walking and bus, a step-free or standard
   profile, a departure window, and a travel-time limit. Switch between time
   bands and departure reachability. Zoom around the origin to move both maps
   together. Previous, Next, the phase selector, and Play operate on the same
   stored series. The baseline provides phase zero.

Road, bicycle, walking, bus and rail toggles filter displayed network lines.
Access unknown on a supplied link uses a dotted line and a legend count.
An empty fixture layer is labeled absent for that fixture. All five transport
layers for the Census geography are unassessed by this application. The
abstract network uses fixed schematic coordinates for its five graph nodes.
It has no geographic relationship to the selected Census polygon.

The fixture uses the repository-authored E14 topology 0, fixed seed 42 and a
180-second service clock. Its published source, version, date, license and
limitations are displayed in the app. Walkway width is an integer in mm and
changes the project footprint passed to E12 screening. Capital and annual
limits are whole 2026 USD and USD/year. Project cost rows are fixture lump sums
and stay fixed across width choices. Preview returns incremental construction,
disruption and maintenance estimates and, when screening succeeds, cumulative
capital and gross annual commitments. Two candidate links must share an endpoint.
Each phase adds one directed walkway and retains the fixture's existing road,
pedestrian and transit infrastructure. The engine evaluates the exact scenario
history against its protected OD samples and E14 guard thresholds.

The maps show isochrone samples at modeled nodes using the E07 door-arrival
router and its real timetable and transfer rules. Each departure is one second
apart in an inclusive window. The travel-time limit applies independently to
each departure. The time view colors the median among feasible departures with
shared fixed breaks at 15 and 30 seconds. The reachability view colors the
fraction of departures within the limit with common breaks at 0, 50 and 100%.
White marks zero reachable samples. Router errors fail the comparison.
The display uses point samples because the fixture has no continuous geographic
surface. Map results describe reference routing. The table separately shows
E08 assignment, E06 reference mode feasibility through the E13 bridge, and E12
costs for the fixture's fixed demand. A step-free map filter changes the matched
routing query. The demand table retains the model's cohort denominators.

These modeled outcomes have arbitrary inputs and no empirical uncertainty
intervals. They support review of the workflow only. Real-world use requires
verified geography-to-network inputs, representative essential trips, schedule
and profile coverage, independent calibration and uncertainty evidence.

## Local protocol v1

`spatial-workbench` embeds its assets and binds IPv4 loopback only. Browser
state and the last 64 runs are ephemeral. One worker executes up to four queued
fixed plans, in order. Rejected plans return a failed run and expose no partial
phase result. Closing a browser leaves the worker running until the local
process exits. Downloaded E02 files use the existing `carfree-study-project-v1`
schema. The application writes no server-side project files. E16's durable Linux
worker protocol remains available for seeded batch experiments through its
[operator interface](../docs/EXPERIMENT-JOBS.md).

The relative API prefix is `/api/e17/v1/`. All responses use JSON and no-cache
headers. Send empty request bodies. POST requires the printed loopback origin
and `X-Carfree-Workbench: 1`. Unknown or duplicate query parameters fail.

| Method and operation | Parameters | Result |
| --- | --- | --- |
| GET `areas` | None | Array of `[id, name]` pairs. |
| GET `study` / POST `initialize` | `area`, `date` in YYYY-MM-DD | Geographic rings in longitude/latitude degrees, coverage entries, baseline ID, missing kinds, and project TSV on initialization. |
| GET `network` | None | Node IDs, links `[id, from, to, layer, access]`, candidate links `[id, from, to]` in selection-index order, and provenance. |
| GET `preview` / POST `runs` | `projects` as comma-separated zero-based indices, `width` in mm, `capital` in 2026 USD and `annual` in 2026 USD/year | Preview contains estimates and any rejection warning. A successful submission returns HTTP 202 with a local run ID and queued status. |
| GET `run` | `id` | Immutable submitted plan and job state. The state is queued, running, failed with reason, or completed with immutable phase views including baseline, network revision, costs and metrics. |
| GET `comparison` | `id`, zero-based `phase`, `origin` node ID, `modes` (`walk` or `walk-bus`), `profile` (`standard` or `step-free`), `start`, `end`, `seconds` | Matched before and after rows `[node, medianSecondsOrNull, reachable, sampled]`. |

Cost bounds use `[lower, central, upper]` in the indicated units. These bounds
describe supplied costs. The run API exposes only the local synthetic fixture,
onboarding catalogue, and versioned projections. Exact Host validation,
same-origin writes, bounded headers and jobs, fixed assets, and content
restrictions support the loopback demo. Public hosting and remote access are
outside this server's scope.

## Acceptance

The Rust tests in `crates/mobility-api/src/bin/spatial-workbench` cover E17
onboarding, exact scenario screening, invalid units and budgets, queue
transitions and capacity, same-origin authorization, matched queries, and
reversible phases. The browser test covers E17-S01–S09 against the actual Rust
application. It exercises pointer and keyboard controls, downloadable project
data, layer visibility, focus, scenario submission, both maps, snapshots,
playback and narrow-viewport page overflow.

```sh
cargo test --locked --offline -p mobility-api --bin spatial-workbench
cargo build --locked --offline -p mobility-api --bin spatial-workbench
node --check web/workbench.js
node web/tests/e17-browser.cjs
E17_BROWSER=webkit node web/tests/e17-browser.cjs
```

The browser test requires Node.js and Playwright with the selected browser
installed. Use `E17_PLAYWRIGHT_MODULE` to point to an existing Playwright
installation. The default screenshots directory is `.codex/run/e17`, override
with `E17_SCREENSHOTS`. Serve the app directly for manual review.

Each preview or run evaluates at most two projects using the unchanged E14
simulation and screening algorithms. A comparison issues
(2 × (N − 1) × D) bounded E07 searches, for (N=5) nodes and (D≤121) departure
samples. The browser draws (O(V + E)) primitives per map and renders matching
table values. A Rust 1.97.1 debug build on macOS 26.7.3 / Apple M5 Pro measured 8.06 ms
median and 8.30 ms p95 for a two-project preview, and 9.17 ms median and 9.28 ms
p95 for the five-node comparison at 121 departures. Each measurement used five
warmups and 30 sequential loopback requests through the v1 protocol. The plan
used Parking and Clinic approaches at 1,900 mm with 200 USD capital and
30 USD/year limits. Comparison used node 1, phase 2, walking and bus, step-free
access, departures 0–120 s and a 60 s limit. Timings include HTTP and JSON, with
browser rendering and memory outside the measurement. City-scale maps and
routing workloads require separate benchmarks.
