# Door-node multimodal routing contract v1 (E07)

E07-S01–S07 (#62–#68, epic #7) add `mobility_routing::planner`, using only
standard-library and existing local dependencies. Graph binary v1, E05 entry
points, and E06 demand APIs remain compatible. This is deterministic planning
over **caller-supplied validated inputs**, not calibrated regional simulation.

## Demo and tests

```sh
rustup run 1.97.1 cargo run --locked --offline -p mobility-routing --example e07_routing
rustup run 1.97.1 cargo test --locked --offline -p mobility-routing -p mobility-sim
```

The demo prints the input graph revision, arrival UTC, component durations,
comfort cost and departure-window service gaps. All fixture inputs are
repository-authored, MIT-licensed, modeled values in abstract geography; their
source/version/date and uncalibrated uncertainty are in graph provenance. No
observed traffic, service reliability, crash, demand or population is invented.

## Planning and clocks (S01–S03)

`planner::plan(&Network, &Request, &Options) -> Result<PlannedOutcome, RoutingError>`
uses the existing [network and request contracts](MULTIMODAL-NETWORK.md). The
caller supplies **actual routable door nodes**, including any street-to-stop
access, legal parking and final egress. There is no invented snapping distance,
walking speed, private vehicle inventory or automatic stop connection. Compare
car and car-free requests using the same endpoints and departure. Personal
vehicles require explicit possession and legal park/retrieve/mount edges.

A successful E07 itinerary cannot terminate aboard transit. Set
`Options::require_transit` to require an actual bus/rail/light-rail ride and
final walking mode; this choice requires initial walking with no personal
vehicle. Otherwise allowed modes and explicit possession determine choices.
A walking alternative is not presented as successful transit service. Legacy
`earliest_arrival` retains its documented, possibly-onboard goal semantics.

`Itinerary` retains the full `Journey` (edge IDs, physical UTC departure/arrival,
wait seconds, mode/run, graph revision), ordered stop `visits`, edge comfort
costs and `Times` totals in integer seconds:

- `access_seconds`: walking before the first non-walk street or transit ride;
  a walking-only itinerary places all walking here;
- `ride_seconds`: car/bicycle/e-bike street movement and transit ride duration;
- `transfer_seconds`: actual transfer preparation/action durations;
- `wait_seconds`: waits for transfers and services, including onboard timetable
  dwell between consecutive segments (not presented as moving ride time);
- `egress_seconds`: walking after the last non-walk street or transit ride;
- `intermediate_walk_seconds`: walking between those first and last movements;
- `dwell_seconds`: explicit stop activity outside transit; retaining car/bicycle
  mode is allowed. Leaving a personal vehicle requires an explicit Park transfer.

The totals sum to final arrival minus requested departure. Zero components
remain explicit. Visit departure includes activity at the final destination, if
requested as a stop; each step still ends at its physical arrival before dwell.
Network edge/run references retain original units, access and source evidence.

`Unmet` means no feasible path **within the supplied horizon and restrictions**.
Off-service departures never get fabricated ride times. Coverage gaps/unknown
endpoints/invalid profiles, arithmetic failure and `StateLimit` are errors, not
service gaps. Frequency phases, calendars, explicit origins/DST, overtaking,
boarding windows and run continuity retain E05 semantics. Missing accessibility
is excluded unless the caller explicitly opts in. Mobility-aid users cannot be
silently assigned cycling.

`Comfort` is a caller-selected, nonnegative **modeled preference**, not a safety
prediction. On every bicycle/e-bike street edge, cost in equivalent seconds is:

```text
(stress_level - 1) * stress_seconds_per_level    if an assessment exists
unknown_seconds                               otherwise
+ shared_seconds                              unless facility is Protected
```

Default costs are zero, preserving earliest physical arrival. Unknown stress is
not imputed; `unknown_seconds` explicitly expresses the caller's uncertainty
policy. Unspecified facilities receive the same facility cost as shared ones.
Inspect `edge_penalties`, `comfort_penalty_seconds` and the graph's facility,
`Safety` and provenance fields together. Hard protected-only/stress/access
restrictions apply before preferences and cannot be bought off with a penalty.
The objective is elapsed physical seconds **plus** accumulated preference cost;
penalties never change street speeds, service eligibility, deadlines or physical
timestamps. Equal objective values use stable graph-ID/label insertion order.

## Stops and transfer constraints (S05)

`Options::stops` contains up to 1024 ordered `Stop { node, dwell_seconds }` values.
A stop is fulfilled by an explicit activity transition at its node when it is
next in the ordered list. Legal parking/alighting or other same-node transfers
can precede activity; passing through a node does not automatically fulfill it.
Consecutive stops at one node each apply their dwell, including at origin or
destination. Transit preparation/aboard states cannot fulfill stops: alight first. Stops
allow walking or retained personal-vehicle mode; a car/bike stop alone does not
assert a parking facility. Explicit parking is needed to leave the vehicle.

One joint search preserves car incoming-turn history, parked location, carried
bicycle, run state and waypoint progress. It does **not** greedily route each
leg or reset possession at stops; a later approach that enables the remaining
journey is retained. Every activity affects actual subsequent service eligibility.

`max_boardings: Option<u32>` bounds total Board actions over the **whole**
itinerary, including across stops; zero forbids all boardings. A continuing run
is not a new boarding. `minimum_board_seconds` makes each Board preparation
last `max(edge.seconds, minimum_board_seconds)`; it must fit its availability
window and finish at or before the run departs. The actual run departure must
be strictly before the facility window closes. Structural alight/park/retrieve and
other transfer durations/windows remain in force. Route-pair fare rules and
maximum walking-distance constraints are not modeled.

## Immutable batches and departure windows (S04, S06)

`matrix(&Network, expected_revision, &[Request], &Options)` routes up to 100,000
caller-ordered OD cells. Caller owns row/column labels and flattening; result
positions are unchanged, including duplicate cells. All cells borrow the same
immutable graph, and the supplied revision must match. The revision is a
caller-assigned input release, not a content hash; callers must issue a new
revision for changed assumptions and may externally hash canonical graph bytes.
Any invalid/resource-failed cell fails the job, with no partial matrix returned.
The implementation is serial and rebuilds adjacency per query; it does not claim
citywide throughput or cross-query caching.

`departure_window(network, template, options, departures_utc, budget_seconds)`
requires 1–100,000 strictly increasing explicit sample times. Each query gets
`deadline = departure + budget`, checked against complete declared coverage.
The template deadline is replaced. Output retains the revision and sample grid,
feasible/unmet counts, and `no_route_fraction = unmet / samples`. Use
`require_transit` for a transit no-service-within-budget statistic; without it,
the fraction refers to whatever modes the request permits. This is an
OD/profile/horizon statistic, **not** the fraction of a city without service or
an essential-trip weighted accessibility score.

p50/p95 use **nearest rank** `ceil(p * feasible_count / 100)` on sorted feasible
elapsed durations, including any requested activity. All-unmet windows return
`None` percentiles, never zero duration. Failures invalidate the aggregate rather
than inflating the service-gap fraction. These deterministic schedule-window
percentiles are not observed reliability estimates, stochastic variance or
confidence intervals. A sample grid may miss gaps between samples.

## Complexity and performance evidence (S07)

The extended exact-time Dijkstra engine preserves E05's full continuation state,
adding stop index and, when bounded, boarding count. For the **same** exact
continuation state (including time) only the least accumulated penalty survives;
it does not use a single best label per node or discard a later, cheaper path.
Nonnegative increments allow termination at the first completed objective label.
The maximum number of accepted labels, including replacements, is `max_states`.

With L links, N nodes, S accepted labels and A transition candidates examined,
time is `O(L log N + A log(S+1))`, plus graph endpoint lookups, string comparison,
calendar/event scans and output reconstruction. Memory is `O(L + S)` plus output; each explicit activity label stores one visit.
Each stop is a separate transition, including consecutive stops at one node;
activity transitions are included in A. Exact-time cycles, timetables, vehicle/stop/boarding states can
grow quickly; this is not a polynomial-in-N or production metro-scale guarantee.
A batch/window sums individual query costs and retains outputs. No pruning turns
resource exhaustion into a false no-service result.

Reproduce single-size processes (build before measuring):

```sh
rustup run 1.97.1 cargo build --release --locked --offline -p mobility-routing --example e07_routing
for size in 64 128 256; do
  /usr/bin/time -l target/release/examples/e07_routing --benchmark "$size"
done
# Linux: use /usr/bin/time -v instead of -l
```

The benchmark reuses `e05-corridor-v1`: directed multimodal corridor, accessible
bus stops every eight nodes, six phase-assumed frequency runs per ride edge,
100 transit-required step-free/mobility-aid queries departing 0,5,…,495 seconds.
It reports graph build separately, counts successful paths, queries/s, paths/s
and nearest-rank p50/p95 latency. Query latency includes per-query adjacency
construction, itinerary assembly and search. Each size is a separate process;
RSS includes graph/build, all queries, runtime and output, **not** per-query
allocations. All 100 queries must produce journeys or the benchmark fails.

Measured 2026-11-18 on macOS 26.7.3 arm64, rustc 1.97.1 release profile, this
implementation and fixture (single run; no confidence interval):

| Nodes / links / service events | Build µs | Paths/s (= queries/s) | p50 / p95 µs | Peak RSS bytes |
| --- | ---: | ---: | ---: | ---: |
| 64 / 212 / 42 | 65 | 10131.50 | 103.167 / 139.542 | 2572288 |
| 128 / 428 / 90 | 91 | 2296.02 | 410.417 / 563.209 | 3375104 |
| 256 / 860 / 186 | 113 | 704.87 | 1420.375 / 1712.084 | 5865472 |

These synthetic scaling observations do not establish a regional performance
budget, observed service quality, engineering safety or planning fidelity.

## Acceptance coverage and limitations

`tests/e07_planner.rs` links each behavioral criterion to deterministic scenarios:
component reconciliation and off-service journeys; protected-vs-stressful paths
for bike/e-bike; unknown assessments and hard restrictions; revision/ordering;
non-greedy stops and preserved turn history; dwell timestamps and boarding
constraints; conditional quantiles/all-unmet windows and failure separation.
A small independent timetable oracle checks every start/deadline combination.
E05 tests retain calendar/DST/overtaking/frequency, parked/carrying vehicles,
step-free and state-limit regressions; E06 tests check compatible demand usage.
The human-reviewable example and per-size benchmark cover S07.

No source-to-routing-network adapter, live journey API, geographic door snapping,
shared-vehicle inventory, calibrated travel speeds, fares, observed reliability,
policy recommendation or citywide car-independence claim is added. Essential-trip
shortfalls and travelers unable to cycle must still be evaluated using defensible
population/demand inputs; the existence of one path or transit service is not
sufficient evidence of car independence.
