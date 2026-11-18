# Multimodal network v1 (E05)

E05-S01–S07 (#47–#53, epic #5) add an immutable directed network and a bounded
reference router. E04 is the prerequisite; its screened topology is **not**
automatically converted into usable links. A caller must supply and attest the
permissions, service origins, accessibility evidence, travel times and coverage.
No new dependencies, ingestion mutations, calibrated simulation or policy
recommendations are introduced.

## Run the synthetic demo

```sh
rustup run 1.97.1 cargo run -p mobility-routing --example e05_network
rustup run 1.97.1 cargo test -p mobility-graph -p mobility-routing
```

The developer demo prints a step-free, mobility-aid journey by walking and bus,
including waiting, then an unmet deadline. The repository-authored fixtures are
MIT licensed, use abstract geography and dated **modeled** inputs, and contain
no observations, personal trips or downloaded data. Times, capacity and stress
are arbitrary test assumptions, not measurements or engineering advice.

## Public contracts and units

Existing `Mode`, `NodeId`, `EdgeId`, `Edge` and `Edge::new` remain compatible.
`mobility_graph::network` adds public input DTOs and `Network::new(NetworkData)`.
Only the validated `Network` is used for routing; it has read-only accessors.
Public legacy `Edge` values cannot bypass the new validation: every new `Link`
is independently checked for finite positive distance, references and duration.
The DTOs may be edited and rebuilt; no operation silently deletes links.

- `NetworkData` has a caller-assigned revision, provenance, timezone/tzdb version,
  inclusive UTC query coverage, nodes, links, car turn rules, calendars and
  explicit service days. IDs are scoped to this graph; callers must remap
  source-scoped IDs without losing their original manifest references.
- `Provenance` preserves source, source version, license/attribution reference,
  geography, civil date, observed/imputed/modeled basis, and uncertainty text.
  Network provenance identifies the complete input/assumption manifest, including
  link-specific source references. Capacity and safety assessments additionally
  carry their own provenance. Missing values remain `None`; they are not zeros.
- Street/transit distance is in **metres**. Transfer distance is absent, not an
  invented length. Street/transfer duration and frequency ride duration are
  explicit integer **seconds** (1–172800). Scheduled rides use their departure
  and arrival times. There is no inferred speed, congestion or emissions model.
- Each street link has exactly one allowed mode. Reverse direction and different
  modes require explicit links. Crossings are explicit; barriers can be blocked.
  Disconnected nodes/components survive construction and round trips. A
  geometric crossing does not create an edge or junction.
- Bicycle/e-bike permissions are independent. Infrastructure is `Protected`,
  `Shared` or `Unspecified`. Optional `Safety` carries caller-assessed ordinal
  traffic stress **1 (lowest)–4 (highest)** and provenance. It is not a crash
  probability, a certified LTS assessment or an observed safety outcome. No
  stress is inferred from protection. Profiles can require protected links or
  an inclusive stress ceiling; missing assessments fail an explicit ceiling.
- Car capacity is an optional positive estimate in **vehicles/hour**, never an
  accessibility objective or a travel-time adjustment. Car routes respect link
  direction and pairwise forbidden/only-turn rules. Only-turn destinations must
  be unique; multiple only rules, contradictory rules, non-car or nonadjacent
  turns fail validation. Via-way, conditional and time-dependent turn formats
  are unsupported and must be rejected by future adapters, not discarded.
- Node/link access is `Accessible`, `Steps`, `Impassable` or `Unknown`.
  `Accessible` is the caller's affirmative assertion of all relevant clearance,
  grade, lift and boarding requirements, not a conclusion from topology. The
  router excludes steps for step-free/mobility-aid profiles, impassable links for
  everyone, and unknown access unless explicitly opted in. Mobility-aid profiles
  cannot cycle; permitted modes must be listed explicitly. More detailed aid,
  gradient, lift-outage or adapted-cycle capabilities remain future contracts.

Validation rejects duplicates, dangling endpoints/calendars, nonfinite distances,
invalid dates/weekday combinations, zero durations/headways/capacity, invalid
stress, overlapping transfer windows, conflicting calendar exceptions, and
inconsistent trip chains/run sets. Unsupported data should fail upstream rather
than be coerced into an accessible or legal route.

## Time, calendars and onboard continuity

Queries use integer **UTC seconds** on one caller-declared epoch. Service-day
`date` is Gregorian YYYYMMDD; weekdays are Monday=0. A calendar includes an
inclusive civil-date range, Monday-first weekday bits, and added/removed dates.
Exceptions may extend the range; simultaneous addition/removal is invalid.
An edge is eligible only on active days explicitly present in `days`.

`ServiceDay::origin_utc` is supplied by the caller, not calculated in the graph
crate. For GTFS, use E04's **local noon minus twelve elapsed hours** convention
with the recorded IANA timezone/tzdb version, not a naive UTC or local-midnight
conversion. Offsets are elapsed seconds from that origin and may exceed 24h,
up to 48h. Calendar eligibility follows the **service date**, not the civil date
at arrival. Consecutive service-day origins need not be 86400 seconds apart;
23-/25-hour DST-day origin fixtures test that no modulo-24h conversion occurs.
The graph validates dates/order/overflow, but cannot independently verify timezone
origins or whether the caller supplied every eligible day. Declared coverage
attests completeness, including prior-day service that runs past midnight.
Out-of-coverage requests are errors, never unmet trips.

A scheduled event has departure/arrival offsets and a run ordinal. A frequency
window generates `first + n*headway < end`, with ordinal `first_run+n`. This is
an explicit **modeled phase**, not the claim that unscheduled service departs
punctually or that expected wait equals zero. Every boardable run is considered,
including a later departure that overtakes an earlier one. Day origins and
calendar membership are applied before eligibility.

Run identity is `(trip template, service date, ordinal)`, plus onboard stop index.
Each trip template is one connected chain with consecutive indices, consistent
mode/calendar, identical run sets and nonnegative dwell time. Use distinct trip
IDs for branches, short-turns and variants. Continuing aboard requires the same
run and next index. A different run requires an explicit alight and board; merely
sharing a node, route name or mode is insufficient.

## Transfers and personal vehicles

Transfer links are at one node, with positive configurable seconds, half-open
UTC availability windows, bicycle permission and access. The entire transfer
must finish by the window end. Boarding additionally requires that the actual
vehicle departure is strictly before that window closes; a completed boarding
preparation cannot be used for service after closure. Alighting starts immediately
on scheduled arrival, not after waiting aboard for a facility to open (delayed
alighting during dwell is conservatively unsupported). Other actions may wait
for a window, with that waiting included in the itinerary. Empty windows mean
unavailable. The graph deliberately does not invent
parking facilities or stop/street connections.

Supported actions:

| Action | Mode transition and condition |
| --- | --- |
| Board | Walk → bus/rail/light rail; retains any carried bicycle restriction |
| Alight | Transit → walk; requires a boarded run |
| Park | Car/bicycle/e-bike → walk; records this node as the parked location |
| Retrieve | Walk → personal vehicle; only at that vehicle's parked node |
| Dismount | Bicycle/e-bike → walk; carries that bicycle |
| Mount | Walk → bicycle/e-bike; requires the matching carried bicycle |

One personal vehicle per query is supported; it cannot be conjured, duplicated,
retrieved remotely or switched to another type. Its last car approach edge
survives parking/retrieval, so transfers cannot erase legal turn history. A parked car remains parked
through transit/walking. A dismounted bicycle travels with the traveler, and both
board transfers and ride links must permit its carriage. Leaving it parked can
make a bicycle-prohibited service usable. Park-and-ride/bicycle parking are
explicit actions, not automatic mode switches. Multiple personal vehicles,
shared vehicle inventories and route-pair fare/transfer rules are not modeled.
Schema v1 transfer costs are structural edge durations, retained even when
walking intervenes; there are no hidden route-pair minimum-transfer rules.

## Reference routing

`mobility_routing::earliest_arrival(&Network, &Request)` returns:

- `Outcome::Journey`: deterministic steps, departure/arrival UTC seconds, waiting
  seconds, mode/run identity and graph revision;
- `Outcome::Unmet`: no feasible journey **within the declared deadline**, given
  the supplied inputs/profile (not evidence of citywide car dependence);
- `RoutingError`: invalid endpoints/coverage/profile/vehicle, checked arithmetic
  failure or `StateLimit`. Resource exhaustion is never called unmet.

Requests start walking, cycling or driving with explicit possession. Initial
onboard transit is not supported: board explicitly. A destination node means
arrival at that graph node (possibly aboard transit); require an explicit alight
and a separate street-access destination when evaluating a door-to-door trip.
Equal origin/destination is a valid zero-step journey only if that node is usable.

The reference engine uses time-expanded Dijkstra labels keyed by exact UTC second,
node, current mode, incoming edge, run/stop index, pending boarding window, and
carried/parked vehicle state (including car approach history).
It does not assume that one node's earliest arrival dominates every later arrival.
Transitions enumerate scheduled/frequency runs and transfer windows lazily;
no per-second waiting graph is allocated. Fixed input gives stable ID-order
traversal and queue tie-breaking. `max_states` explicitly bounds accepted labels.
This correctness-first implementation is not the scalable E07 production router,
a fare optimizer, a demand assignment engine or an uncertainty simulator.

### Complexity and measured synthetic benchmark

Let L/N be links/nodes, T turn rules, C/D calendars/service days, E expanded
service events, and S reached exact-time labels. Graph build sorts IDs/sets and
trip chains, validates runs in ordered maps (roughly O((L+E) log(L+E))), checks
calendar exceptions, and checks turn contradictions in O(T²). Canonical decode
also rebuilds and re-encodes, with O(encoded bytes + N + L + E) auxiliary space.
Exceptions use linear membership checks. This is not a streaming importer.

Routing builds an O(L log N) outgoing index per query. For each expanded label
it scans outgoing links, car turn rules and relevant days/events/windows. Let A
be the total transitions examined including filtered service events and calendar
exception scans. Time is O(L log N + A log S), plus endpoint lookup and string
comparison costs. Memory is O(L + S), including exact-state predecessor records
and the queue (and caller-owned graph). S can grow rapidly with horizon, cycles,
vehicle states and timetables: there is no polynomial-in-N performance claim.
`StateLimit` is a deliberate error boundary, not pruning justified as infeasibility.

```sh
rustup run 1.97.1 cargo run --release -p mobility-routing --example e05_network -- --benchmark
/usr/bin/time -l target/release/examples/e05_network --benchmark
```

`e05-corridor-v1` is a directed synthetic corridor with walk/bike/car links,
accessible bus stops every eight nodes, explicit board/alight penalties and six
phase-assumed frequency runs per leg. The benchmark separately measures graph
build, canonical decode, and 100 step-free/mobility-aid queries on one OD pair per
corridor size, departing at
0,5,…,495 seconds. It checks all results are journeys. Fixtures of 64/128/256
nodes exercise increasing route/service state, not a real metropolitan network.
Measured 2026-11-18 on macOS 26.4 arm64, rustc 1.97.1, release profile,
using the implementation/fixture accompanying this document (base `68aa2a2`):

| Nodes / links | Build µs | Decode µs | OD p50 µs | OD p95 µs |
| --- | ---: | ---: | ---: | ---: |
| 64 / 212 | 116 | 89 | 60 | 127 |
| 128 / 428 | 27 | 81 | 352 | 477 |
| 256 / 860 | 49 | 135 | 1116 | 1448 |

A second complete invocation measured OD p50/p95 of 53/67, 290/331 and
1110/1230 µs respectively; `/usr/bin/time -l` reported **6,258,688 bytes**
maximum resident set size for that whole invocation (all three builds and 300
queries, not per-query allocations). Build timings are not monotonic; causes
were not independently measured. Timing variation is expected; two runs are developer
observations, not statistical confidence bounds or a metro-scale performance gate.

## Acceptance evidence and limitations

`crates/mobility-routing/tests/e05_network.rs` links deterministic tests to each
E05 story: crossings/barriers/direction, protected/shared/forbidden cycling and
stress, service calendars/wait/overtaking/frequency identity/after-midnight/DST,
car turns/capacity, full canonical round trips and corruption, vehicle transfer
eligibility and remote-retrieval rejection, step-free alternatives and unmet trips.
An independent exhaustive small timetable oracle compares competing first rides,
same-run continuations and rail transfers across start times/deadlines.
`crates/mobility-graph/tests/e05_schema.rs` pins independently assembled v1 golden
bytes, so encoder and decoder cannot silently drift together.

No observed safety, travel-time, service reliability, emissions, demand or
population values are invented. Feasible synthetic paths do not establish real
accessibility, calibration, adequate schedule coverage or car independence.
Future source adapters must preserve licenses/provenance and reject unsupported
restrictions; no claim is made that the E04 screened output can simply be routed.

## Canonical binary schema v1

`Network::encode` returns bytes, not a filesystem side effect. `decode` returns a
validated immutable graph or actionable error; it rejects unknown schema/enum
values, invalid UTF-8/booleans, malformed/truncated/oversized lengths, duplicates,
invalid graph invariants, trailing bytes and noncanonical ordering. Collection
lengths must fit remaining bytes before allocation; decoding does not reserve
memory directly from an untrusted count. Callers still need an input byte limit
and suitable trust/privacy controls when reading files. This is not an
authenticated format or a concurrent persistence protocol.

All integers are little endian: IDs/seconds/dates/schema are u32, UTC seconds are
i64, lengths/counts are u64, enum tags and booleans are u8 (false=0,true=1), and
distance is IEEE-754 f64. Strings are UTF-8 prefixed by byte length. Vectors have
a count followed by elements; optionals have a boolean and then the value if
present. Boxes have no extra encoding. There are no platform-width fields.

Field order (also pinned in `network/codec.rs`):

- Root: schema=1, revision, provenance, timezone, tzdb, coverage_start,
  coverage_end, nodes, links, turns, calendars, days.
- Provenance: source, source_version, license, geography, date, basis,
  uncertainty. Basis: Observed=0, Imputed=1, Modeled=2.
- Node: id, access. Access: Accessible=0, Steps=1, Impassable=2, Unknown=3.
- Link: id, from, to, optional distance_m, seconds, access, kind.
- Street kind=0: mode, crossing, blocked, cycling, optional safety, optional
  capacity. Cycling: Protected=0, Shared=1, Unspecified=2. Safety: stress_level
  (u8), provenance. Capacity: vehicles_per_hour, provenance.
- Transit kind=1: mode, calendar, trip, stop_index, timetable, bicycle_allowed.
  Timetable: Scheduled=0 then departure vector, or Frequency=1 then first, end,
  headway, first_run. Departure: depart, arrive, run.
- Transfer kind=2: from_mode, to_mode, action, windows, bicycle_allowed.
  A window is (start,end) i64. Actions Board=0, Alight=1, Park=2, Retrieve=3,
  Mount=4, Dismount=5.
- Modes: Walk=0, Bicycle=1, ElectricBicycle=2, Bus=3, LightRail=4, Rail=5, Car=6.
- Turn: incoming, outgoing, only. Calendar: id, first_date, last_date, weekdays
  (u8), added, removed. ServiceDay: date, weekday (u8), origin_utc.

Nodes/links/calendars are sorted by ID; days by date; turns by
(incoming,outgoing,only); exceptions by date; transfer windows by (start,end);
scheduled events by (depart,arrive,run). Sorting does not remove duplicates.
The schema version identifies encoding semantics; the **caller-assigned graph
revision** identifies an input/assumption release, not a cryptographic content
hash. Callers must issue a new revision whenever inputs/assumptions change and
retain their provenance manifest. Canonical bytes may be externally hashed for
content addressing. A format/semantics change requires a schema bump, migration
policy and new golden fixture; changing a revision alone cannot alter v1 layout.
