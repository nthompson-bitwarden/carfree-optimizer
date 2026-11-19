# Car-independence scoring (E13, contract v1)

`accessibility` now provides deterministic, in-memory scoring over explicit
caller-supplied evidence. `reachable_share(usize, usize)` is unchanged. There is
no persisted score schema, web service, nationwide map, empirical baseline or
policy recommendation engine. All bundled examples are repository-authored MIT
synthetic assumptions, not observed traffic, population or emissions.

## Run and inspect

```sh
cargo test --locked --offline -p accessibility
cargo run --locked --offline -p accessibility --example e13_scoring > /tmp/e13-scoring.html
cargo run --release --locked --offline -p accessibility --example e13_scoring -- --bench
```

Open the HTML to review the abstract two-neighborhood map, accessible text/table
alternatives, population denominators, low-coverage hatching, mobility groups and
paired feasibility outcomes. Coordinates are arbitrary drawing coordinates, NOT
geographic boundaries. The same fixed 0–100% accessibility interpretation applies
to both neighborhoods. This developer demo is not an interactive GIS editor.

## Identity and scientific safeguards

Every score has `version = 1` and a `scoring::Context`: scenario, graph revision
and exact canonical graph snapshot bytes, demand/destination/geography/service
versions, period, short- or long-run horizon, behavioral assumptions, geographic
and dated source attribution, and explicit limitations. A neutral synthetic
fixture may use a documented snapshot token; upstream and screened adapters
require actual `Network::encode()` bytes. Equal human-assigned revision strings
are not proof of equal networks. Sources must distinguish modeled, observed and
imputed evidence and retain units/license/uncertainty in their attribution.

Long-run scores require explicitly rebuilt demand and behavior assumptions;
the engine never extrapolates a short-run mode share into relocation or induced
demand. Missing metadata is an error. A feasible route or a passed engineering
screen is never sufficient evidence of real-world car independence.

`AccessScore` and `TripScore` are immutable result wrappers (read-only `Deref`,
no `DerefMut`); callers cannot mutate validated fractions or denominators before
using them in a frontier or projection. Compile-fail doctests protect this.
Public input records remain caller-owned evidence, not authenticated observations.

## Metric definitions and acceptance mapping

### E13-S01: access by origin, destination type, modes, profile and window

`scoring::access` consumes a full eligible destination inventory, a stratum
(origin, neighborhood, disjoint primary mobility cohort, destination type,
canonical sorted car-free mode set, profile, strictly increasing UTC departure
samples, inclusive seconds budget) and every destination/departure cell.

The denominator is **eligible destination × sampled departure pairs**, equally
weighted. Reached pairs have feasible physical journeys within the inclusive
budget. Evaluated, missing, reached and eligible counts remain visible. A missing
cell's reason is mandatory and the headline share is unavailable until all cells
are evaluated; missing cells are never removed from the denominator. Empty
inventories have no share. Duplicate, omitted or unknown cells and invalid
journeys fail explicitly. This is not an all-day continuous-time guarantee.

### E13-S02: physical door-to-door parity and reliability

`Journey` sums access, wait, ride, transfer, intermediate walk, egress and dwell
seconds. Preference penalties are not physical time. Scores report both feasible,
car-free-only, car-only, neither and unknown pair counts. A mean car-free-minus-car
difference is conditional on joint feasibility. Car and car-free p50/p95 use
nearest-rank feasible samples; their feasible denominators are retained. These
are deterministic schedule variation, **not confidence intervals**. Infeasible
and missing trips are not encoded as zero or infinite travel times.

`upstream::route_pair` executes actual E07 queries, retaining graph bytes and both
requests in opaque `RoutedPair`. Actual driving, parking and walking egress are
required. Because E07 does not constrain final car mode, this adapter requires a
**dedicated walk-only destination door node**; a driving or transfer arrival at
that node is an actionable topology error before querying. Callers must connect
the parking node to that door upstream, not discard an earlier driving-only
arrival after routing. Requests share doors, departure, budget and hard access
constraints. `routed_access` checks retained bytes, profiles, destinations,
mode sets and budgets before projection. An explicit origin-ID-to-node registry
prevents a result being relabeled under another origin. Routing errors/state exhaustion fail
the job; they are not infeasible trips. Low-level `access` also accepts explicit
manual/survey evidence and does not pretend those records were routed.

### E13-S03 and E13-S06: trip dependence, mode distribution and equity

`scoring::trips` uses positive finite leg expansion weights, distinct from person
population. Car-required means the **whole tour** has a car option and no eligible
car-free option under the declared model. Neither-mode trips are separate, NOT
car-required. Unknown feasibility makes the car-required headline unavailable.
All-trip and essential-trip denominators, no-mode, unknown, served-mode weights
and unserved weights are retained. Mode shares divide each served-mode weight by
the all-trip denominator (not merely served demand); modes plus unserved reconcile
to total weight. Disjoint group/neighborhood distributions expose isolated groups.
Overlapping tags must be evaluated separately and not added to the total.

Intervals are explicit `Uncertainty::{Confidence, Sensitivity, Unavailable}`.
Confidence includes level, sampling unit, method and source; sensitivity bounds
must not be called confidence. Every supplied interval binds the exact context,
metric, exact leg-membership/group/neighborhood/essential-status/weight inventory,
trip denominator and sampling/replicate-design revision. All-trip
car-required intervals are supplied to `trips`; `intervals::with_mode_intervals`
attaches independently matched mode-share intervals. Unsupported sampling designs
remain explicitly unavailable. Fractional survey expansion weights do **not**
become independent Bernoulli trials; the engine fabricates neither confidence
intervals nor effective sample sizes. Examples explicitly publish unavailability.

`upstream::trip_bridge` executes E08 on the actual network/demand/scenario, then
separately evaluates E06 reference-network tour feasibility. The **reference**
car-required estimate is labeled as before E08 edits/cost feedback/capacity. E08
does not expose its complete effective-view alternatives, so the **assignment**
car-required/no-mode classifications remain unknown, not inferred from unserved
weight. Denied proposed transit modes count as unserved, and E08's demand
multiplier applies to every leg. Both outputs retain distinct assumptions. The reference result exposes feasibility
only: assignment/mode-share/unserved fields are unavailable, not manufactured as
100% unserved. `screened_trip_bridge` and `screened_simulation_evidence` obtain
results from the same `Screened::evaluate` call, preserving construction-phase,
single-supply and demand/land-use checks; the direct variants remain explicitly
ungated reference execution. The positive regression includes an actual denied transit proposal and multiplier 2.

`simulation_evidence` executes the actual adapter rather than trusting a mutable
report's IDs. It carries status, denial and effective revision. Optional E09
reference reports must match selected parameters, seed, scenario settings/supply,
engine and effective view. E09 does not retain full input-network bytes; therefore
its banner/limitations are reference evidence only and **current fitness always
remains limited**. There is no inheritance of policy readiness from matching IDs.

### E13-S04: neighborhood map and population denominators

`spatial::neighborhoods` accepts versioned caller-supplied map-space polygons,
population denominators and distinct origins with population weights. Immutable
`projection::Projection` values preserve destination inventory/version, type,
mode set, profile, sampled window and budget. Mixed contexts/estimands fail. Projections retain origin, neighborhood and cohort;
reassigning one origin's score to another origin or cohort fails.
Represented population counts only origins with complete scores. Neighborhood
and disjoint cohort outputs retain total and represented population plus
conditional accessibility. Unknown or below-threshold coverage is visibly
insufficient; an uncovered neighborhood never looks like zero accessibility.
No citywide extrapolation or population imputation is performed.

### E13-S05: infrastructure devoted to private cars

`spatial::road_space` uses m² for **disjoint physical footprints**, not graph-edge
counts. Repeated equal footprint IDs (opposing/different graph references) count
once; conflicting duplicates fail. Shared mixed-traffic car attribution is an
explicit fraction. Neighborhood splits sum to one within 1e-9 and conserve area.
A complete sourced expected inventory is mandatory; missing footprints fail.
An explicitly sourced empty inventory represents evidenced zero, not missingness.
`footprints::screened_road_space` binds the inventory to each screened E12 physical
ROW exactly once using the existing segment/ROW ownership crosswalk and exact
snapshot bytes. Neither generic IDs nor this crosswalk independently validate
geometric polygon overlap: distinct ROW IDs must represent disjoint physical
space established by the caller. E12 width checks do not establish segment area;
area evidence is supplied separately. No inferred road dimensions are inserted.

### E13-S07: vehicle travel and operational emissions

The executed E08 bridge sums road edge-period **vehicles × metres / 1000** into
modeled road vehicle-km. It does not use person-trips or passenger boardings as
transit vehicle activity. Transit vehicle-km require separately supplied fleet
operations. `spatial::emissions` accepts complete activity inventories with fleet,
period, factor version and methodology attribution. Operational factors are
**kg CO2e / vehicle-km**, matched to that fleet and period. Missing rows/factors,
invalid bounds, period mismatches or nonfinite calculations fail, not zero-fill.

Nonnegative low/central/high activity and factor bounds propagate by endpoint
multiplication and summation. These conservative supplied sensitivity bounds
are not confidence intervals, independent draws or lifecycle/embodied emissions.
Zero activity needs an explicit sourced empty inventory or an explicit zero row.

### E13-S08: constrained Pareto frontier

`comparison::Candidate::single` performs a fresh E12 screen from an exact
`SingleProject` (history, batch, inputs and an independently supplied
`ProjectCostInventory`), immutable matched accessibility/trip scores and optional
E12 risk inputs. A fresh engineering screen does **not** prove cost completeness:
the caller must supply a dated/versioned completeness source and the full project's
quantity/rate and transit inventory, including justified zero quantities. The
inventory must match the exact baseline/final snapshots and every screened cost
row; phase-final inputs with omitted capital fail against the full inventory.
The completeness source is retained on candidates and frontiers. This is explicit
caller evidence, not automatic validation of real-world cost coverage. Its
lifecycle is the declared incremental single-batch project cost relative to the
retained exact baseline snapshot;
different baselines cannot mix. `Candidate::screened` accepts a bare opaque gate
for reporting but always leaves it unranked because that gate alone cannot
establish complete project cost scope. Trip results must
first pass `demand_binding::bind_demand`, matching every leg and expanded weight to
the actual supplied `Demand`, multiplier and E12 scenario/demand/land-use binding.
Equal total weights or revision strings alone do not establish this membership. Context graph bytes
and scenario must match the exact gate. Screening failures cannot construct a
candidate. Cost comes from screened lifecycle cost, risk from `Screened::risk`.
Missing complete project cost scope, accessibility/dependence or calibrated risk
leaves a candidate **unranked**. In particular, a staged plan's `final_state()`
gate omits prior-phase capital from its own lifecycle. It cannot enter a frontier
as a cheap recurring-only project. Staged project ranking is intentionally
unavailable until a complete accumulated-project cost bridge is implemented;
E12's plan-level cost report remains available separately.

Minimize lifecycle currency cost, expected collisions and car-required share;
maximize accessibility. Dominance requires no worse in all four dimensions and
strictly better in at least one; exact ties remain nondominated. No weighted sum
hides trade-offs. Comparisons require the same currency/minor units/price year,
cost horizon/discount/budgets, actual typed protected-access registry, population
and demand, destination inventory, stratum/window/budget, geographic period and behavioral horizon, exact demand snapshot (apart from the
candidate network revision), leg inventory and expansion multiplier. Risk model/version, **mode**, exposure unit, severity,
applicability and interval must match. A frontier is reference evidence, not a
policy or engineering recommendation; unavailable objectives never become zero.

### E13-S09 and E13-S10: weather, service, day/night and horizons

`comparison::rank_conditions` requires an explicit weather × day/night × versioned
service grid and positive finite weights. It consumes immutable projections with
a common destination/mode/profile/budget estimand and population/horizon/behavior,
but condition-specific graph snapshots, departure grids and service identities.
Origins, neighborhoods and mobility cohorts must match across every ranked row;
a downtown score cannot compete against a peripheral origin's different demand.
Exact departure grids must match across scenarios **within** each condition;
day/night conditions may use different actual UTC samples. Different timetable graph revisions are valid;
the score's service revision must identify its grid condition. Rankings maximize
weighted accessibility, retain per-condition rankings, use deterministic ID ties,
and leave scenarios with any missing grid cell unranked. No absent poor-weather
condition can improve overall rank. Callers actually rebuild service/network and
mobility restrictions; the scorer never invents a weather penalty or night bus.
Short- and long-run contexts cannot be mixed in a comparison.

## Tests, complexity and performance

Story-linked tests: `crates/accessibility/tests/e13_scoring.rs`,
`e13_upstream.rs`, `e13_regressions.rs`, immutable-result doctests and strict
Pareto dominance unit tests. They test boundaries and failure handling, not
scientific fidelity. See [scoring benchmark](BENCHMARKS.md#e13-scoring).

For P destination/departure pairs, scoring is O(P log P) time and O(P) memory for
inventory validation and sorted percentiles. Trip/neighborhood/footprint/emissions
rollups are O(N log N) with ordered maps. Condition ranking is O(S K log(S K))
including grid validation/sorts, S scenarios and K conditions. Pairwise Pareto is
O(C² M), M=4, plus equality validation of contexts/registries. Context snapshots
add their encoded graph sizes to output memory; a future shared immutable binding
can reduce duplication if measured. At most 1,000,000 access cells and 10,000
Pareto candidates/conditions are supported. Routing and simulation retain their
own upstream bounds; their execution time is **not included** in the arithmetic
benchmark. No new external dependency or neighboring public API change is made.
