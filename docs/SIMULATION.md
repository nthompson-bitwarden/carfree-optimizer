# Bounded transportation simulation v1 (E08)

E08-S01–S08 (#69–#76, epic #8) add `mobility_sim::simulation`, a native Rust
**uncalibrated reference model**, not planning-grade predictions. No new external
dependencies, persisted schema, source adapter, optimizer rewards or policy
recommendations are introduced. Existing graph binary v1, E06 demand APIs and
`ScenarioEvaluator` remain unchanged.

## Run and inspect

```sh
export PATH="$(dirname "$(rustup which cargo --toolchain 1.97.1)"):$PATH"
cargo test --locked --offline -p mobility-sim
cargo run --locked --offline -p mobility-sim --example e08_simulation
```

The example reuses the repository-authored E06 two-node corridor: MIT, abstract
west/east geography, modeled 2026-01-05 inputs, no observed population or transport
measurements. It prints model/seed/revisions, convergence, person-seconds,
unserved essential weight and car-free candidate shortfall. Supply and behavioral
numbers are explicit scenario assumptions, not estimated coefficients or empirical
uncertainty bounds. The demo's zero shortfall is **not** evidence of neighborhood
car independence. Real inputs require provenance, adequate observed baseline
calibration and independent validation before policy use.

## Input, edits and replay (S01–S02)

`Adapter::evaluate(&Network, &demand::Demand, &simulation::Scenario)` borrows an
immutable baseline and returns `Result<Report, Error>`. `Scenario.version` must be
`simulation::VERSION` (1), with a nonempty scenario ID and complete provenance.
Retain the entire scenario, graph canonical bytes and E06 inputs to reproduce a
run; revision strings are caller-assigned releases, **not content hashes**.
`Report` includes engine/schema, original graph/demand revisions, seed and a distinct
`effective_network_revision`. The latter deterministically fingerprints the edited
network, scenario ID, UTC period boundaries and all final entry-period costs with FNV-1a (not a cryptographic
hash or security identity). Returned replay journeys use that effective revision;
internal static candidate views and their local demand bindings have distinct
suffixes. Original graph provenance is preserved; scenario provenance describes
supply/edit assumptions separately. Callers must retain full input manifests and
unique scenario IDs, not rely on this finite fingerprint to rule out collisions.

Supply is explicit: contiguous half-open UTC `Period`s must cover graph coverage
(including its inclusive end), positive vehicles/hour for every open car edge in
every period, and nonnegative passengers for every feasible transit segment/run.
No road capacity is inferred from missing graph attributes. Each run identifies
trip, civil service date, ordinal and stop index. Invalid run/calendar/segment
references, duplicates, nonfinite inputs, missing supply, overflow or routing
state exhaustion fail evaluation; they do not become unmet trips. E06 validation
preserves population reconciliation, mobility groups, noncyclist/wheelchair
constraints, environmental access, load, fixed departures, deadlines and dwell.
Every feasible alternative must be priced: missing evidence is an error, not free
travel or a reason to silently exclude that alternative.

Typed edits change positive street durations, one road edge/period capacity, or
one transit run/segment capacity. They cannot remove links, modes, access, turn
rules or emergency/freight permissions. Duplicate targets and unknown targets
fail atomically. Capacity zero is allowed only for transit (explicit denial).
Abstract edits do **not** establish geographic feasibility, safety or cost.
Evaluation derives private cost views; clearing the edit list restores the exact
baseline result without source rebuilding or mutable undo operations.

Within the same executable/architecture, fixed inputs/seed have exactly equal
reports, excluding the separate batch wall clock. IDs, assignments, statuses and
integer timestamps compare exactly. Floating-point cross-platform comparison
uses absolute `1e-9` plus relative `1e-12 * max(abs(a),abs(b))`; this is a numerical
comparison tolerance, not uncertainty. Different libm implementations can change
near-tied choices or iteration count: cross-platform bitwise identity is not
promised. Stable ordered maps/input tour order govern ties and capacity priority.
FNV-1a/SplitMix64-derived uniform tour/mode preference shocks are fixed within a
run; seeds do not resample routes on every iteration. They are modeled preference
sensitivities, not calibrated random utility coefficients.

## Road/mode feedback and limits (S03–S04)

`Iterative` (`iterative-bpr-v1`) begins at free-flow street durations and evaluates
E06 whole-tour, fixed-mode alternatives. At each iteration it derives one static
road-cost view per UTC period **plus the maximum-duration envelope across
periods**, generating a bounded set of legal car route candidates. Car paths are
then replayed with each link's actual entry time selecting its period. Replay
checks each leg deadline and the next fixed departure/dwell. Incoming-turn
continuity is preserved by E06 candidate generation; personal vehicles cannot
teleport. Non-car paths use the edited static network and actual transit schedules.
Each mode retains its least generalized-cost candidate; least cost plus seeded
preference selects the tour's mode (ties use E06 mode order). Unpriced feasible
choices fail rather than biasing mode shares.

This is **not an exact globally optimal time-dependent router**. The finite
candidate set may miss feasible/cheaper paths or combinations across tour legs;
rejected static candidates are not recovered after replay. No candidate means
unserved **within this method, inputs and horizon**, not proof that no real route
exists. There is no departure-time, destination, mixed-mode-tour or parking-search
adaptation, and no modeled boundary inflow unless supplied as demand. Keep these
limitations with all shortfall statistics.

Car volume is leg expansion weight × demand multiplier × explicit
`vehicles_per_trip`, assigned to every traversed link's entry period. Capacity in
vehicles per period is vehicles/hour × period seconds / 3600. For explicit alpha
and positive beta, modeled BPR duration is:

```text
t = ceil(t0 * (1 + alpha * (vehicles / capacity_vehicles)^beta))
```

Values within `1e-9` seconds of an integer round to that integer first, suppressing
floating-point ceiling artifacts. There is no hard road queue, spillback,
within-period propagation, traffic signal model, transit-road delay coupling or
observed travel-time inference. Per-period static functions can be non-FIFO across
boundaries; exact-time replay is a bounded approximation, not a dynamic traffic
assignment claim. Transit timetables are not secretly accelerated by road edits.

Every iteration reroutes and reconsiders mode; the model does not hold car demand
permanently fixed. Full all-or-nothing updates can oscillate. `Converged` requires
identical complete assignments (including journeys/denial) on consecutive
iterations and max relative link-volume residual <= explicit tolerance. Relative
residual is `abs(new-old)/max(1,new,old)` over every supplied edge/period. Otherwise
at `max_iterations` the result is `Nonconverged`, never a successful equilibrium.
Even a no-feedback run needs two iterations to establish stability. Road output
separates the durations **used** for final assignment from those **implied** by
its final volumes, exposing lag in capped runs. Convergence concerns only this
candidate set and is not an empirical validity or uniqueness guarantee.

## Finite transit capacity (S05)

Schedules determine wait and in-vehicle times through the existing router. Chosen
transit tours request passengers = leg expansion weight × demand multiplier ×
(1 + companions) on each occupied segment/run. Capacities are shared across all
periods for that actual run. Deterministic input-tour order reserves **all segments
of a tour atomically**, or denies the whole tour without consuming any capacity.
Segment-specific reservations permit seat reuse downstream after alighting;
there is no arbitrary capacity reset when a road period changes. Fractional
expansion groups are indivisible, so even a residual seat may remain unused.

`TransitResult` reports requested/served/capacity passengers. Denied assignments
retain their proposed mode/journey for inspection but are excluded from served
mode weights and person-seconds. Their full leg weight contributes to
`unserved_trip_weight` and essential weight to `unserved_essential_weight`.
This is a **conservative upper bound on unmet demand due to denial for the chosen
itineraries**, with zero as the lower bound if retries/other routes could serve
all denied travelers. It is not an observed denied-boarding rate, crowding
utility model, or an estimate of waiting for the next service. No partial group
boarding, later-run retry or post-denial mode substitution is claimed. Missing
capacity for any feasible candidate is an error, not an infinite-capacity default.

`essential_without_car_free_candidate` separately counts essential leg weight in
tours without any complete car-free candidate **before capacity**. It must not be
misread as capacity-aware car-free accessibility. Essential outcomes concern the
supplied trip weights and departures, not residents or full schedule coverage.
`person_seconds` counts served travelers and companions; never interpret falling
travel time caused by denied trips as an accessibility improvement. Report
shortfalls alongside travel time, disaggregate profiles when making comparisons,
and do not optimize private-vehicle throughput as an accessibility proxy.

## Batches, uncertainty and alternate adapters (S06–S08)

`batch` uses bounded standard-library scoped workers across immutable scenarios
(1–256 requested workers), retains caller order and propagates errors/panics
without partial success. It reports actual workers used, elapsed seconds and
scenarios/second; these timing fields are intentionally outside deterministic
`Report`. Do not edit inputs while measuring. Small workloads may not speed up.

`sensitivity` crosses distinct seeds and distinctly labeled parameter sets
(parameter-major order, at most 100,000 cells). Each set specifies BPR alpha,
demand multiplier and preference shock scale; all other settings/supply remain
in the template. Use the **same seeds** for baseline and intervention comparisons.
Output retains every report/label/seed, counts nonconverged cells, and reports
count, mean, sample variance (`None` for one sample), min/max, nearest-rank p50/p95
for person-seconds and unserved essential weight. Any invalid cell fails the job;
capped cells remain explicitly nonconverged in the distribution and must not be
silently presented as equilibria. These are scenario/seed distributions, **not
confidence intervals**, learned parameter uncertainty or policy ranking guarantees.
Inspect per-run mode weights/shortfalls rather than ranking on travel time alone.

The same `Scenario` executes through two adapters: `Iterative` above and
`FixedCost` (`fixed-cost-v1`), a single-pass reference with no congestion feedback.
Both enforce supply, E06 restrictions and transit capacity. `FixedCost` returns
`FixedCosts`, not `Converged`; alpha is deliberately inactive. Their assignments
and served travel totals agree on zero-feedback fixtures; engine/status/iteration
metadata and volume residuals can differ. They diverge under positive congestion. This is an extensible adapter boundary, not integration
with or independent validation against MATSim/SUMO.

## Complexity, benchmarks and acceptance

For B periods (at most 128), I iterations, E road edges, T tours and K total
returned candidate steps, each iteration performs B+1 E06 evaluations (each at
most five route queries per leg), including graph/view validation. See the E05/E06
routing state bounds; exact-time routing is resource-bounded, not polynomial in
network size. Additional replay, selection and ledger work is
`O(K log(E*B + K) + E*B log(E*B))`, plus candidate graph clones and E06 input
validation. Memory retains current candidates, two assignments and cost/volume
maps, plus a cloned local demand binding and one E06 report at a time and the
router's bounded state. Fingerprinting also encodes the edited graph once per iteration. Parallel
batches multiply active scenario working memory by workers and retain all
reports; sensitivity also retains its cloned scenario grid. There is no
city-scale throughput or memory budget claim.

```sh
export PATH="$(dirname "$(rustup which cargo --toolchain 1.97.1)"):$PATH"
cargo build --release --locked --offline -p mobility-sim --example e08_simulation
for tours in 10 100 1000; do
  for workers in 1 4; do
    /usr/bin/time -l target/release/examples/e08_simulation --benchmark "$tours" "$workers" 3
  done
done
# Linux: /usr/bin/time -v for process peak RSS.
```

Measured 2026-11-18, Mac17,9 / 15 logical CPUs, macOS 26.7.3 arm64,
rustc 1.97.1, release. Each process uses 2 nodes/11 links/2 periods, eight
identical scenarios, three timed batch repetitions, fixed seed 42, two legs per
tour, three iterations and explicit capacity scaled with tour count. All runs
must converge and equal a serial oracle. Throughput = 24 / total timed batch
seconds, includes worker creation and output construction but excludes fixture
construction, serial oracle and separate latency measurements. p50/p95 are
nearest-rank single-scenario serial latency across 24 separate calls, **not**
parallel per-scenario latency. RSS covers the entire process including inputs,
serial oracle, retained output, workers and latency calls. No confidence intervals.

| Tours | Workers | Scenarios/s | Serial p50 / p95 ms | Peak RSS bytes |
| ---: | ---: | ---: | ---: | ---: |
| 10 | 1 | 2274.54 | 0.387 / 0.596 | 3,096,576 |
| 10 | 4 | 3653.04 | 0.349 / 0.414 | 3,833,856 |
| 100 | 1 | 322.44 | 2.937 / 3.311 | 5,963,776 |
| 100 | 4 | 706.83 | 2.947 / 3.074 | 10,977,280 |
| 1000 | 1 | 34.03 | 29.375 / 30.021 | 30,375,936 |
| 1000 | 4 | 89.77 | 29.513 / 30.860 | 93,224,960 |

`tests/e08_simulation.rs` links story IDs to deterministic independent BPR and
weighted-statistic oracles, route/mode response, entry-bin crossing, immutable
edit/reversal, explicit nonconvergence, finite transit/atomic rollback,
serial-parallel equality, both adapters, crossed sensitivity grids, mobility
constraints and explicit error separation. Existing E05–E07 tests remain required.
Unit tests and these tiny synthetic benchmarks establish reference semantics,
not calibration, regional realism, safety, emissions, ridership or observed
reliability. No policy deployment is supported without independent scientific
validation and disclosed uncertainty.
