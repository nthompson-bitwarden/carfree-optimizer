# Travel demand and traveler diversity v1 (E06)

E06-S01–S08 (#54–#61) add an in-memory Rust demand and feasibility model in
`mobility_sim::demand`. The existing `ScenarioEvaluator` interface is unchanged.
There is no new persisted schema, HTTP API, source adapter, demand assignment,
congestion model, or calibrated simulator. `Demand.version` and `CostModel.version`
must be `demand::VERSION` (1); caller-assigned revisions identify input releases,
not hashes. Keep the graph, demand, cost and environment assumptions together for
replay. Increment the contract version for incompatible semantics changes.

```sh
cargo run --offline --locked -p mobility-sim --example e06_demand
cargo test --offline --locked -p mobility-sim
```

The demo compares the same school/shopping tour in two scenarios. In daylight,
walking, e-biking and car can complete the tour; the bicycle cannot carry the
return load, and transit runs outbound but is unavailable for the return. The
winter storm assumption leaves only car. The demo separately prints a ten-year
what-if calculation. Fixture `e06-corridor-v1` is repository-authored (MIT),
uses abstract geography and dated modeled values, and contains no downloaded or
personal data. Its numbers are illustrative, not observations or uncertainty
bounds. Real inputs must document source/version, license/attribution, geography,
date, evidence basis (observed/imputed/modeled), and uncertainty; use aggregated
or appropriately consented data, not identifiable routes or home addresses.
Demand provenance identifies the manifest for supplied times, loads, essential
flags and traveler capabilities; population, costs and environment have their
own provenance. Validation checks structure, not the truth of attestations.

## Versioned Rust contract

- `Demand` binds to one E05 graph revision. Each `Tour` names an existing
  home-zone/group stratum and holds an ordered, connected itinerary. `Leg`
  records OD node IDs, purpose (commute, school, shopping, healthcare,
  caregiving, leisure or other), essential flag, positive fractional expansion
  weight in trips, half-open departure window `[start,end)`, a selected departure
  and inclusive arrival deadline. Times are UTC integer seconds on the E05
  epoch; neither deadlines nor departure windows can extend beyond graph
  coverage. Callers supply selected departures and must cover the full period
  of interest; v1 neither samples the window nor fills missing time bins.
- `Population::generate` lazily emits synthetic representatives by zone/group
  stratum. Callers supply modeled stratum populations and representative
  counts. Integer quotient/remainder splitting gives exact stratum totals, with
  the first remainder positions receiving one extra person. No RNG is used.
  Modeled sums must match documented source counts within the absolute
  whole-person tolerance **both per zone and study-wide**. Person and trip
  weights are different quantities; v1 does not infer trip frequency from
  persons or count each synthetic representative as a trip.
- `TravelerGroup` declares mode access, cargo grams and companion count limits,
  step-free and mobility-aid requirements, protected cycling and optional E05
  stress ceiling. Noncyclist, wheelchair user, child and older adult labels may
  overlap; age does not imply capabilities. Wheelchair groups require step-free
  and mobility-aid constraints; noncyclists and mobility-aid groups cannot be
  assigned cycling modes in v1. Unknown graph access is not treated as accessible.
- `evaluate` returns each tour's five alternatives as feasible journeys or an
  exclusion with a zero-based leg index where applicable. Transit uses E05
  walking access and requires at least one actual ride and explicit alighting;
  a walk-only shortcut is not a transit alternative. Use actual endpoints;
  substituting a stop for a door overstates access. A tour uses one fixed mode.
  Personal modes follow single-mode graph links; the vehicle moves to each
  successive stop with its traveler. Car turn rules retain the incoming edge
  from the preceding leg. Car legs are selected greedily by earliest arrival;
  another approach to a stop might make a later leg feasible, but this model
  does not jointly search routes over a whole tour. Caller must attest usable curb/parking or
  storage at stops. No inventory, parking search/duration, mixed-mode tours,
  transfers with personal vehicles, or per-run passenger capacity model is
  implied. Per-leg supplied cargo/companions allow different loads after
  drop-off or pickup; each leg must meet its route deadline and the next planned
  departure must leave time for minimum dwell at the prior stop. A declared
  return must end at the initial node. Alternatives that require changing the
  planned departures or switching modes within the tour are outside v1.
- `CostModel` computes equivalent seconds for each feasible earliest-arrival
  journey: `(elapsed − waiting) × non_wait_multiplier + waiting × wait_multiplier
  + reliability_seconds × reliability_multiplier + expense_usd × seconds_per_usd`.
  Wait uses E05 step waiting, including schedule waits and onboard dwell where
  represented as waiting. A coefficient or estimate is finite and nonnegative.
  The caller specifies compatible price-year USD and a reliability buffer in
  seconds per leg/mode; no transit expense or reliability is inferred. Retain
  evidence appropriate to the chosen scenario and routes. Missing evidence is
  explicitly unpriced, not free and not infeasible. `lowest_cost_mode` returns
  no ranking when any feasible alternative is unpriced; otherwise it chooses
  the lowest sum, breaking ties by Walk, Bicycle, ElectricBicycle, Transit, Car.
  It is not a mode-share forecast, and the underlying routes minimize arrival
  time, not generalized cost.
- `Environment` selects named weather/season assumptions with daylight windows
  and explicit daylight/dark mode sets. Availability at the selected departure
  is a scenario assumption, not a live forecast or along-route weather model.
  Transit availability includes the caller's judgment about its walking access.
  Keep the trip table fixed when comparing profiles. A mismatch in coverage,
  invalid inputs or routing exhaustion fails the run, not the traveler.
- `long_horizon` produces a separate `LongHorizonReport` for a named sensitivity
  and positive number of years. Explicit simultaneous whole-person moves
  conserve documented source population; outbound moves cannot draw from
  arrivals during the same scenario. Supplied zone/purpose rates (trips per
  resident per modeled day) yield baseline, post-relocation, and post-relocation
  with induced/suppressed demand (explicit multiplier) results. Omitted rates
  stay absent; no destination choices or network feedback are inferred. Compare
  documented low/high parameter sets rather than present a single case as a
  forecast. The short-term demand and routes are never modified.

`Report.essential` and `essential_by_group` count essential **leg expansion weight**,
not residents. A leg contributes to the shortfall when its **whole tour** lacks a
complete car-free option, including when a non-essential leg prevents the tour.
The report gives total evaluated essential weight, weight lacking a car-free
option and weight lacking any mode; zero essential weight gives no share.
Successful evaluation covers the supplied departures only. Missing kinds of trips,
unknown access, timetable coverage beyond those departures, inaccessible first
and last metres, and restrictions of the fixed-mode model can change the result.
Neither the presence of transit nor this share establishes neighborhood car
independence. Do not use these results for policy recommendations without
observed-data calibration, independent validation and disclosure of uncertainty.

## Acceptance and performance

`crates/mobility-sim/tests/e06_demand.rs` links fixtures/tests to all eight story
IDs. Population validation uses ordered sets: O(P log P) for P zones and strata;
expansion is O(R) time and constant iterator state for R representatives after
validation. Evaluation calls E05 at most five times per input leg. Each call
retains the [E05 routing bounds](MULTIMODAL-NETWORK.md#complexity-and-measured-synthetic-benchmark)
and includes per-query outgoing link indexing. The overlay is linear in legs
and returned steps apart from ID checks/lookups and O(log D) daylight lookup
for D intervals per evaluated mode and leg. Output memory grows with the
retained choices and steps. Long-horizon arithmetic uses ordered maps and is
O((zones + moves + rates) log(zones + rates)) time and O(zones + rates) space.

```sh
cargo build --release --locked --offline -p mobility-sim --example e06_demand
/usr/bin/time -l target/release/examples/e06_demand --benchmark  # macOS
```

On macOS 26.4 arm64, rustc 1.97.1 (release), a two-zone, two-leg synthetic batch
with two mobility profiles and 15 selected outbound departure instants took
median **0.58 / 4.00 / 33.16 ms** for **100 / 1,000 / 10,000 tours** (five runs
per batch). Peak RSS was **97,370,112 bytes** for the whole invocation, including
input data and retained output. This exercises demand batching, actual schedule
waits, load and profile filters, and route failure. It is not evidence of
metro-scale performance, model calibration, or a statistical confidence interval.
