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

1. Open **Try an example scenario** and choose Parking approach on the map or
   with its checkbox. Add Clinic approach to see two construction phases.
   Selection order determines phase order.
2. Open **Edit width and budgets**. Set the capital budget to 1 USD and choose
   **Check proposal**. The check explains the budget rejection and keeps the
   selected walkways. Restore the 200 USD budget, check again, and choose
   **Run comparison**. Setting the width to 8,000 mm demonstrates the
   right-of-way warning. The default width is 1,900 mm.
3. **What changed?** counts destinations that at least one sampled departure
   reaches within the time limit. Compare the paired maps and destination lists.
   Open **Comparison filters and map view** to change modes, origin, mobility
   profile or query times. Choose a map metric or zoom both maps around the
   origin. Previous, Next, the phase selector and Play use the same stored
   series. The baseline provides phase zero.
4. Open **Map layers, legend and data source** to compare modeled road links
   with the absent rail layer. Use **Explore a city** to inspect the Census
   boundary and required dataset statuses for one of the three pilot areas.
   Initialize and download the E02 project snapshot to save it. The TSV works
   with `study-area preview`. Transportation model initialization requires the
   missing datasets listed in the city view.

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
schema. The default application writes no server-side project files. Enable
[E19 saved workspaces](../docs/WORKSPACES.md) to persist and collaborate through
the versioned workspace API and export standalone artifacts. E16's durable Linux
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
playback and narrow-viewport page overflow. E17-S08 also covers the independent
city entry, proposal rejection and correction, edits after a passed check,
recovery of an interrupted run, and reachability gains and zero change from the
real endpoint. A substituted comparison response verifies the loss wording.

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

### First-visit usability review (E17-S08)

The scores use the approved plan’s 1–10 rubric. A score of 1 means the
interface obstructs the task, 5 means the task is feasible with avoidable
interpretation, and 10 means the action and its consequence are clear
throughout. The revised scores are heuristic judgments from browser
walkthroughs and matched 1280 × 900 and 375 × 780 screenshots.

| Concept | Before | After | Observed behavior |
| --- | ---: | ---: | --- |
| First-visit orientation | 3 | 9 | The opening shows the walkway choices and the example works without loading the city catalogue. Explore a city opens the Census view independently. |
| Match with user expectations | 4 | 8 | The opening and result heading both identify the synthetic network. The separate city view shows the actual Census boundary and its dataset gaps. The example still has generic node labels. |
| Clarity of the next action | 4 | 9 | Selection enables and highlights Check proposal. A successful check prioritizes Run comparison. An edit makes the check the primary action again. |
| Cognitive load | 4 | 8 | The default walkthrough leaves width, budgets and layer controls collapsed. Results appear before filters and detailed tables. On mobile, inspecting the map from the proposal and comparing the two result maps require scrolling. |
| Feedback and recovery | 6 | 8 | A budget rejection keeps the selected links and offers a correction. The interrupted status request can reconnect to the same submitted run. A failed initial submission can still expose transport-level wording. |
| Understanding results | 5 | 9 | The default one-walkway run shows 1 → 3 reachable destinations and names the two gains. Selecting the baseline phase shows zero change. The summary shows the origin, profile, phase, sampled departure window and time limit. |
| Keyboard and mobile usability | 7 | 8 | Both engines complete the form by keyboard, land focus on the result heading and show the summary in view. The page fits the 375 px viewport. WebKit’s default keyboard preference requires Option+Tab to reach all controls; detailed tables scroll within their containers. |

The walkthrough verifies behavior, keyboard focus and the live status
semantics. First-time-user comprehension and spoken screen-reader output
still need direct observation.

### Workbench performance

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

## Interpretation and reports

After a run completes, open **Comparison filters and map view** and use
**Origin** and **Update comparison** to update the matched routing maps. Open
**Phase outcomes and modeled trip groups** to inspect the related table. Only the abstract
origin at node 1 has modeled trips. Its two primary cohorts are adult and
mobility-aid travelers. The table describes weighted trip legs by home zone.
Map filters for travel mode, mobility profile and time apply to the E07
departure query. They do not change the fixed demand cohorts used in E06 or E08.
Selecting another origin explicitly shows the absence of modeled trip groups.

Use **Report metric** to change the phase chart and sensitivity table together.
The chart and its numeric table show phase-minus-prior and phase-minus-baseline
differences. Percent shares use percentage-point differences. The comparison
identifies the independent E08 assignment and E06 reference-network denominators.
Reference essential trips without a car-free option include car-required trips
and those without any feasible mode. The reference views measure whole-tour
feasibility before assignment and capacity effects. Added feasible options
can appear even when the assigned mode does not change. Cost bands describe
supplied fixture estimates. The displayed fixed-door ratio follows E14 and
uses a fixed baseline car comparator.

Sensitivity evaluates the submitted order from a matched reference opening
under each one-at-a-time condition, at seed 42. It tests E15 demand mix 2
(adult and mobility-aid weights of 6 and 4, versus 4 and 2 nominally),
a hypothetical storm that removes the walk-only tour mode at departure
(transit access legs remain permitted), and a doubled generalized penalty for
waiting. The waiting penalty changes modeled choices through cost rather than
altering clock times. Each range takes extrema of final-minus-opening values
over the available conditions and nominal run. Failed evaluations are
listed with reasons and excluded from ranges. These are scenario sensitivity
ranges, with no statistical confidence interpretation.

The frontier evaluates the opening, all one-project choices and both orders
of every distinct pair of projects. It uses the submitted width and shared
budgets, the E17 connected-corridor rule, and E12/E14 feasibility gates.
It maximizes cumulative E14 reward and minimizes upper-bound capital, the
two objectives used in the E15 benchmark archive. Exact ties survive.
**Priority** changes the display order of the same frontier. Open **frontier
plans** below the table to see counts for the baseline, screened plans and excluded orders, with the
exclusion reasons. The report retains every screened sequence, including
dominated results. The frontier applies only to the workbench's finite fixture and
declared objectives.

Select **Download full report (JSON)** to save
`carfree-run-<id>-report-v1.json`. It includes the immutable submitted plan,
model and source revisions, ordered projects, scenario snapshots, seed,
calibration quality, uncertainty and causal limits. The synthetic fixture has
no empirical calibration and supports no planning recommendations.

### Report schema v1

A completed `GET /api/e17/v1/run?id=<id>` adds `state.report`.
Existing E17 plan and phase fields are retained. The comparison route adds
`beforeTrace` and `afterTrace` with the same trace schema.

| Field | Meaning |
| --- | --- |
| `version` | Report schema version, currently 1. |
| `methodology` | Human-readable calibration, uncertainty, limitations and assumptions. |
| `metrics` | Ordered `[label, unit]` pairs bound to numeric vectors. |
| `phases` | Baseline first, then ordered project steps. Each row includes `trace`, metric `values`, signed `incremental` and `cumulative` vectors, grouped demand summaries, assignment status, essential denominators in weighted trips and raw E14 step reward. Missing values and changes are JSON null. |
| `trace` | Scenario ID and schema version, graph/effective/demand/service/destination/catalog revisions, ordered applied project IDs, numeric seed, model versions and dated geography/source/license. The containing plan provides width and limits. The containing sensitivity row provides any changed assumptions. |
| `groups` | Origin node, home zone, primary group, weighted assignment totals/unserved/served-by-mode, and weighted reference total, car-required, no-mode and unknown counts. No individual tour IDs. |
| `sensitivity` | Ordered test objects with assumption settings, phase rows and matched final changes or explicit error. `ranges` contains one `[min, max, count]` or null per metric. |
| `tradeoffs` | Baseline and screened alternatives with zero-based project indices, final upper-bound capital in 2026 USD, cumulative reward, frontier flag and phase rows. Rejected sequences include ordered project IDs and explicit errors. |

The download adds the run ID and plan to the report document. Scope and
retention match E17. The server produces the report once in the bounded worker
and never rewrites it in response to metric or priority controls.

### E18 verification

Rust tests cover missing changes, deterministic projection and exact Pareto
dominance. The browser acceptance in `web/tests/e17-browser.cjs` exercises
E18-S01–S07 as well as E17, including shared map and cohort filtering,
percentage-point deltas, real sensitivity runs and byte-equivalent report
data across priority changes. It saves a report screenshot alongside the
desktop and mobile screenshots.

The reference simulation paths are unchanged. Reporting adds a bounded
enumeration of at most nine intervention orders with at most two steps each
and three matched perturbations. Projection is linear in reported phase
and cohort rows, with a quadratic dominance scan over at most ten plans.
Preview retains the nominal single-plan evaluation.

With Rust 1.97.1 on macOS 26.7.3 / Apple M5 Pro, a debug build completed
and retrieved the two-project report in 76.65 ms median and 77.64 ms p95.
The test sent five warmups and 30 sequential submissions over loopback
with 1 ms polling. It used Parking and Clinic approaches at 1,900 mm,
200 USD capital, and 30 USD/year limits. The completed response was
53,947 bytes. These timings include queueing, evaluation, HTTP and JSON
reading. Browser rendering and memory were outside the measurement.
