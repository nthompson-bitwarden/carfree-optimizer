# Transit service design — E11 in-memory contract v1

E11-S01–S07 extend `interventions` on the existing E05 graph, E07 door router,
E08 reference simulation and E10 immutable scenario history. No external
dependencies, graph binary schema changes, persistence format, web editor,
fare-aware router or calibrated transportation predictions are introduced.

## Reviewable demonstration

```sh
rustup run 1.97.1 cargo run --locked --offline -p interventions --example e11_transit
rustup run 1.97.1 cargo test --locked --offline -p interventions
rustup run 1.97.1 cargo run --release --locked --offline -p interventions --example e11_transit -- --benchmark
```

The repository-authored MIT fixture is an abstract directed corridor dated
2026-01-05. Every duration, distance, service, staffing level, fare, cost and
capacity is **arbitrary modeled input** with explicit uncertainty, not observed
travel behavior. It is deterministic without randomness. The text demo prints
door arrival, actual boarding fare, generalized cost, fleet/operator estimates,
and reduced general-road capacity after priority allocation. A shorter modeled
bus ride does not establish engineering feasibility, reliability or mode shift.

## Authoring and atomicity

Use `scenario::Change::Station(transit::Station)` to stage new station nodes,
walking/cycling access and transfers, then `Change::Transit(transit::Service)`
to add or replace a complete directed trip pattern. `Service` contains its trip
ID, calendar, all ride links and explicit `Operations`. Existing nodes are valid
stop references; unknown nodes and inconsistent trip/calendar references fail.
A changed calendar uses a new ID so unrelated services cannot be silently changed.
Each reverse direction, express variant and short turn needs its own trip ID.
The caller resolves service-day UTC origins, timezone and DST conventions using
the unchanged E05 contract. Frequency means deterministic phase-aligned departures
strictly before `end`, not an assumed average half-headway wait.

Replacements remove every old ride in that trip and rebuild the whole candidate
through `Network::new`: stop order, topology, mode/calendar consistency, run sets,
dwell, calendars, time overflow and access metadata are validated together.
Retained directed trip legs cannot lose known accessible status, even when their
edge IDs or stop indices change. Essential OD/profile requirements remain needed
for reroutes that replace those legs with a different path.
Rerouting an allocated bus ride may not remove its edge ID, change its endpoints
or change it away from bus mode. There is deliberately no implicit release or
relocation of physical bus space. That needs a separately assessed lane plan.

All changes share E10 `Batch` parent/version checks, modeled dated provenance,
immutable snapshots, access requirements, atomic rollback and exact undo/redo.
`Snapshot.transit` contains the operations assumptions, their provenance, and
estimates pinned to the final graph revision. Every later transport edit
recomputes all managed service estimates; failed estimation rolls back too.
Existing E10 services need an explicit E11 service proposal to acquire operations
metadata; baseline costs are never invented. Snapshot additions are in memory;
network binary schema v1 and prior E10 APIs remain otherwise unchanged.

A batch cannot write the same station target twice or mix competing station,
service and timing edits. Service creation/replacement plus an **identical**
`Allocate`/`AssociateBus` target is intentional composition, not an overwrite.
Stage station topology before its services and services before lane association.

## BRT and road-space consistency

BRT is bus service plus explicit caller-assessed running times/frequency and E10
road-space allocation, not a new transport mode or an automatic speed multiplier.
Use `Change::Allocate` with the staged bus ride and a directional car-edge owner.
It debits general lanes exactly once. Other services use `AssociateBus` on that
same physical bus space. Final graph timetables determine physical routing and
operations accounting. Lane conservation, general-road vehicles/hour and closure
access checks remain authoritative in E10. Neither road capacity nor passenger
capacity is inferred from faster service.

For E08 assignment, rebind matched demand to the new graph revision and scenario
ID, project road capacity exactly once with `evaluation::road_supply`, and supply
explicit passenger capacity for **every new segment/date/run identity**. Never
reuse old run ordinals after a frequency edit. `evaluation::evaluate` preserves
E10 demand/revision and nonoverlapping-edit checks; E08 rejects missing/stale
transit supply. The E11 simulation test exercises edited run identities and
road supply on the same snapshot, including rejection of stale passenger supply.

## Station access and mobility constraints

Station links may add off-road walking or protected cycling connectors with
explicit crossing/no-crossing permission, minimum seconds and modeled provenance;
cycling also requires explicit stress assessment. These are abstract proposals,
not proof of geographic feasibility or permission to take road space. Road
conversions must use the lane ledger. Existing street links cannot be replaced
through `Station`. Existing transfer replacements preserve node, action, modes
and known accessible status, but can change explicit duration and opening hours.

New managed services require at least one actual endpoint boarding/alighting
opportunity on an active run. Existing endpoint opportunities and replaced
station boarding/alighting opportunities (including intermediate stations) cannot
be eliminated entirely. Windows detached from every actual run do not count.
Complete closure is unsupported here and fails with guidance for a separate
access plan. Up to one million date/run events may be examined per opportunity
check; exhaustion is an error, not proof of infeasibility. This structural check
is not a guarantee for every run or traveler: provide directed E10 `Requirement`s
for essential OD/time/profile coverage. Those checks run on the complete final
network, including changed service and station access.

Parking, retrieval, boarding, alighting and carriage use unchanged E05/E07
possession semantics. A bicycle parked at one station cannot reappear at another.
Access and egress are actual directed graph movements with positive seconds;
there is no snapping or implied station teleportation. Keep walking and step-free
profiles for people unable to cycle; a new bicycle option does not replace them.

## Fleet, staffing and operating estimates

`transit::estimate(network, trip, operations)` counts each active run once across
all legs. The modeled duty interval is half-open:

```text
duty_start = first_departure - deadhead_seconds_per_run
duty_end   = last_arrival + turnaround_seconds
```

Run elapsed time includes intermediate dwell. Coincident duty ends precede starts
when counting peak concurrent vehicles. Peak operators multiply that fleet by
explicit `operators_per_vehicle`; operator-seconds multiply total vehicle-seconds
by the same staffing assumption. Distances are metres; costs use:

```text
vehicle_seconds / 3600 * vehicle_minor_per_hour
+ operator_seconds / 3600 * operator_minor_per_hour
+ vehicle_metres / 1000 * minor_per_km
```

All rates and overheads, including explicit zeros, are caller inputs. Currency is
a three-uppercase-letter caller-supplied code, with no currency conversion or
validation against an ISO registry. Output cost is unrounded floating-point minor
units, not an invoice. Distances and cost must remain finite; integer arithmetic
is checked. The fixture's three 130-second duties overlap with peak fleet 2,
390 vehicle-seconds, 780 operator-seconds, 3300 vehicle-metres and 1110 USD minor
units. A two-leg through-run is counted once, not once per edge.

Graph coverage selects complete runs; partially overlapping runs fail rather than
silently truncating cost. Nonoverlapping runs are excluded. Overheads can extend
outside that selection window and are still fully counted. Calendars and service
days are explicit; absent operating dates are not extrapolated. `max_run_instances`
(1–1,000,000) bounds template departures and examined date/run instances, including
out-of-coverage departures on overlapping days. Completely irrelevant days are
skipped before expansion. Exhaustion fails atomically.

Fleet estimates are **per directed pattern**, with no automatic interlining,
return-trip matching, vehicle location balancing, labor breaks, shift rules,
maintenance spares or fuel/energy engineering. Supply separate reverse patterns,
explicit deadhead/turnaround assumptions and appropriate costs. Do not interpret
a per-pattern minimum as an operationally approved fleet or sum peaks as an
optimized shared network fleet. Infrastructure/capital costs remain separate.

## Fares and timed transfers

`transit::evaluate(network, request, fares, currency, value_of_time_minor_per_hour)`
uses the E07 door-complete physical earliest-arrival planner, including actual
alighting before the deadline. Missing service, insufficient connection time,
inaccessible facilities or closed windows yield `Unmet` with absent cost fields;
router resource exhaustion remains an error. Timed connections are explicit
scheduled departures and minimum positive transfer durations, not probability
or guaranteed holding assumptions.

Supply a modeled dated `Fare` for every ridden trip. Charge once on the actual
ride after each boarding; a multi-leg through-run is charged once. Optional
`TransferFare` uses a shared group, discounted minor-unit fare and a half-open
window measured from the last full-price boarding. Discounts do not renew the
window; a different group resets it. Mixed currencies, missing fares, invalid
discounts or nonpositive value of time are explicit errors. Physical seconds and
fare are separate outputs:

```text
generalized_seconds = physical_elapsed_seconds
                    + fare_minor * 3600 / value_of_time_minor_per_hour
```

This scores a physical earliest journey for fare-policy comparison. It does **not**
search for the cheapest path, implement fare-dependent mode choice, price passes
or daily caps, or infer income-dependent value of time. Calibrated demand and
fare-aware search require separately reviewed engine extensions.

## Acceptance and scientific gate

`tests/e11_transit.rs`, `e11_safeguards.rs`, `e11_review_regressions.rs` and
`e11_simulation.rs` provide deterministic story-linked evidence:

| Story | Observable evidence |
| --- | --- |
| S01 | Add/reroute bus; old rides removed; bad stops/calendars/dwell, stale parent and observed proposal provenance reject atomically. |
| S02 | Exact waits 89→9 seconds and door arrivals 211→131; final departure boundary, overnight and calendar exceptions; essential access failure. |
| S03 | Consistent faster bus timing and one lane debit; two services share one bus lane; E08 revised run supply, stale supply rejection and 180 vehicles/hour projection; orphan and over-allocation rejection. |
| S04 | Rail/light rail with real station access, frequency and timed interchange; independent inaccessible-platform and missed-transfer checks; new station nodes. |
| S05 | Real bike-park-transit and new walking-access journeys; wrong-station retrieval infeasible; protected/safety/crossing requirements; step-free/noncyclist access preserved. |
| S06 | Hand-calculated fleet, seconds, distance and operating cost; multi-leg run counted once; touching duties reuse a vehicle; coverage/staffing/expansion failures. |
| S07 | Timed connections, minimum transfer duration, one fare per boarding, discount window boundary, currency/value-of-time validation, separate physical and generalized cost; onboard deadline regression. |

The [benchmark guide](BENCHMARKS.md#e11-transit-service-design) records bounded
scaling and reproduction. No observed traffic, ridership or operating cost inputs
are fabricated. Service existence, route feasibility, synthetic assignment and
unit tests do not establish car independence. Recommendations require realistic
journeys and coverage, mobility constraints, essential-trip infeasibility,
adequate baseline/holdout calibration and disclosed uncertainty; this epic
provides no policy recommendations or planning-grade fidelity claims.
