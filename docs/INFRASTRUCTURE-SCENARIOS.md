# Infrastructure scenarios — E10, in-memory contract v1

E10-S01–S08 extend `interventions` with deterministic, atomic scenario authoring
on caller-supplied E05 networks and explicit E08 evaluation inputs. Dependencies
E05 and E08 remain unchanged. The original descriptive `Intervention` enum and
network binary schema v1 are preserved. [E11 transit service design](TRANSIT-SERVICE-DESIGN.md)
extends this same transaction/history path with complete service and station
changes, recomputed operations and separate fare comparisons. Use E11 rather
than `BusTiming` when changing run identities or calendars. There is no new persisted schema, web
editor, geographic inference, source-to-network adapter or optimizer integration.

## Reviewable demonstration

```sh
rustup run 1.97.1 cargo run --locked --offline -p interventions --example e10_scenarios
rustup run 1.97.1 cargo test --locked --offline -p interventions
```

The text demo shows connected selection and gap preview, a shared lane ledger,
road and transit time comparisons, continuity findings, byte-exact undo/redo,
road diversion and a separately labeled destination extension. It uses the
MIT repository-authored E06/E10 abstract corridor fixture dated 2026-01-05,
seed 42. Every transport, population, timing, permission and destination input
is **arbitrary modeled data**, not an observation or a recommended design.

## Public surface and ownership

- `scenario::History::new(id, network, roads, requirements)` validates the
  initial directional lane inventory and every supplied access requirement.
- `Batch { expected_parent, id, provenance, changes }` pins an atomic transaction
  to its scenario parent. `preview` returns a validated independent `Snapshot`;
  `apply` repeats the same validation and advances history only on success.
- `Snapshot` is opaque and immutable, including clones. Read-only accessors and
  a `Deref` view expose network, lane owners, allocation/crossing sidecars,
  destinations, continuity results, provenance and parent/version identities.
  A standalone view cannot be injected into history or evaluated as a snapshot.
- `undo`/`redo` move between complete stored versions without inverse-edit
  arithmetic. Canonical network bytes, sidecars and IDs are restored exactly.
  Successful edits after undo discard redo versions but never release their IDs.
  Graph revisions and scenario IDs cannot be reused, including the original
  graph revision. Land-use-only versions leave graph revision/bytes unchanged.
- `corridor::preview` checks an ordered selection against a real `Request`,
  including its modes, step-free/mobility-aid constraints, service window and
  state bound. Corridor previews support simple directed paths only: repeated
  nodes are an explicit structural error, not a claim of route infeasibility. It excludes unselected movement but retains transfer links.
  `scenario::select_corridor` selects a feasible connected route.
  `scenario::preview_corridor` is only a **structural** preview: it reports missing
  IDs, reversed/disconnected segments, mode mismatches, blocked/unknown access,
  incompatible nodes and car turn restrictions. Use the profile-aware preview
  before claiming a usable through corridor; adjacency is not service continuity.
- `evaluation::{road_supply, evaluate}` is the explicit E10 → E08 bridge.
  Interventions depends only on the local graph, routing and simulation crates;
  simulation has no dependency on interventions. Existing engine contracts do
  not change. No service, Python, training or external dependency is introduced.

## Lane conservation and edits

`RoadSpace` names one caller-assessed **physical segment and direction**, owned
by one car edge. Segment names and road owners must be unique. The inventory
conserves `general + bike + bus + retired == total` lanes. Counts are integer
lanes; capacity is integer **vehicles/hour**, computed as general lanes times
explicit vehicles/lane/hour with checked overflow. Initial graph capacity must
agree exactly. Parallel bike and bus edges are representations of this same
space, not additional road-capacity supply. Geometry, lane width, costs and
whether two records describe the same physical street require independent review.

| Change | Rules and modeled effects |
| --- | --- |
| `Allocate` | Debits explicit general lanes once. Requires matching directed source/target endpoints. A cycling target must be protected, have modeled dated stress provenance and an explicit crossing/no-crossing assessment. New cycling links use existing graph nodes. Bus targets must reference an existing service; full explicit timing replacements preserve every non-timing field and run identity. Capacity becomes the remaining general-lane capacity. |
| `AssociateBus` | Associates another existing service with already allocated bus space, without consuming another lane. Multiple services can use one physical bus lane. Endpoints and service identity must match. |
| `BusTiming` | Changes an allocated/associated bus service's seconds and explicit timetable without changing lane counts, passenger capacity, calendars, run sets, access, bicycle carriage, topology or other non-timing fields. Whole-trip run/dwell consistency is revalidated atomically. |
| `Pedestrian` | Adds or improves a walking link attached to existing usable directed pedestrian infrastructure: an incoming link at its origin and an outgoing link at its destination. Endpoints must exist and have known usable access. Replacement retains topology/mode and cannot downgrade accessible links to steps. Crossing classification, permission and minimum time must be explicitly assessed. |
| `Crossing` | Changes a real walking/cycling crossing's physical seconds, with an explicit modeled assessment. Cycling additionally requires modeled safety and becomes protected. Existing recorded minimum times and classifications cannot be weakened by any later allocation, pedestrian or crossing edit. Blocked or inaccessible crossings cannot silently be reopened. |
| `CloseRoad` | Retires general lanes and marks the car link blocked rather than deleting topology or turn rules. Existing bike, walk and transit links are retained. Requires explicit access coverage and successful directed continuity checks below. |
| `LandUse` | Replaces a separate nonempty, unique-ID modeled destination list (node, category, opportunity count, provenance). Zero opportunities may represent a removed opportunity. It does not mutate transport links, regenerate demand or compute accessibility. Undo can restore the prior destination state. |

Every changed transport graph is rebuilt through `Network::new`. Failed graph,
provenance, lane, timetable, access, stale-parent or duplicate-ID validation leaves
history and the redo branch untouched. Explicit modeled provenance includes
source, source version, license, geography, ISO date and uncertainty. Existing
observed baseline records are retained in the parent snapshot, not relabeled as
observed intervention effects. Stress is ordinal 1–4, not crash risk. Times are
integer seconds and distances metres. A caller supplies each direction separately.

## Crossing and access safeguards

`CrossingAssessment` records crossing/no-crossing classification, permission,
minimum positive physical seconds and modeled provenance. All affected cycling
allocations, pedestrian changes and crossing upgrades require an assessment.
Permission denial, classification mismatch or duration below the minimum rejects
the batch. Successive edits cannot lower a recorded minimum or replace its
classification; a baseline crossing marker cannot be erased by replacement. Assessments are caller inputs, **not** geometric crossing detection,
engineering certification or automatic approval. In particular, legal vehicle
sizes, grades, sightlines, construction feasibility and costs are not inferred.

A `Requirement` names a directed OD, departure/deadline, actual routing profile,
access kind and provenance. Emergency and freight requirements require explicit
assessed permitted-car-edge sets; generic car eligibility is never substituted.
The router sees all nonpermitted car links blocked and retains turn restrictions,
including an `only` destination that becomes blocked. Transit requirements must
include a real ride and final walking access. Walking requirements cannot
substitute cycling. Routing resource exhaustion is an error, not an unmet trip.

Every transaction evaluates supplied requirements on parent and candidate and
records before/after physical journey seconds. Closure (including conversion of
the last general lane) additionally requires **all five** kinds: emergency,
freight, regional gateway, walking and transit. Missing coverage/permissions or
an unmet requirement rejects the transaction. Supply directed return requirements
and the relevant times of day explicitly; one forward OD is not proof of return
access. A transport network with no transit cannot satisfy this conservative
closure gate without explicitly resolving the missing coverage outside this API.

These are bounded access samples at supplied graph travel times, **not exhaustive
citywide continuity or congestion-aware emergency-response guarantees**. Actual
vehicle eligibility is attested by the permission input. E08 matched-demand
simulation separately reports modeled regional diversion, volume shifts,
convergence and unserved essential trips. Regional boundaries must include the
relevant diversion routes; a truncated network cannot establish regional effects.
There is no claim of calibration or planning-grade fidelity.

## Evaluation and separate land use

1. Start with the validated candidate network. For a transport-only comparison,
   explicitly prepare a copy of matched demand bound to its graph revision.
2. Obtain `road_supply(snapshot, period_count)`. It projects constant modeled
   capacity into each explicit E08 period, **replacing**, not adding to, baseline
   supply. Blocked roads are omitted. Missing capacities fail. Time-varying
   observed capacity is not inferred by this bridge.
3. Preserve explicit transit run/passenger supply. Timing changes cannot change
   the run identities to which that supply is attached. E08 evaluates actual
   wait/ride times, capacity denial and fixed/iterative road costs.
4. Supply an E08 scenario whose ID matches the snapshot, and a `DemandBinding`
   naming scenario ID, demand revision and optional land-use revision. All E08
   edits must be empty: the E10 network is already edited. Duplicate/extra road
   supplies, baseline capacity addition or overlapping edits are rejected.
5. `Evaluation` labels `TransportOnly` versus `TransportPlusLandUse` and carries
   the land-use revision alongside the E08 report. A combined evaluation requires
   caller-prepared revised demand and its matching destination-revision binding.
   **This binding is caller attestation**, not validation that destinations caused
   particular trips. The engine does not invent land-use effects. The acceptance
   fixture explicitly changes a tour's purpose to healthcare for a proposed
   clinic. Actual induced demand needs an independently justified demand model.

E08 convergence and essential infeasibility are reported, not suppressed. Route
feasibility, transit existence, unit tests or a converged synthetic assignment do
not establish car independence. Real comparisons need schedule/time coverage,
individual mobility constraints, calibrated baseline/holdout evidence and disclosed
uncertainty. No policy recommendation is supplied by this epic.

## Acceptance map

| Stories | Deterministic executable evidence |
| --- | --- |
| E10-S01 | `tests/e10_scenarios.rs`: reduced capacity, protected low-stress route becomes feasible, noncyclist links preserved; shared inventory over-allocation rolls back. |
| E10-S02 | Walking/step-free crossing, denied/misclassified/too-fast crossings, isolated and wrongly directed attachments; inaccessible downgrades rejected. |
| E10-S03 | Road BPR seconds and bus wait/ride time change without added capacity; `e10_bus_chain.rs` tests atomic multi-leg run/dwell consistency; `e10_safeguards.rs` tests two services on one lane, E08 transit travelers and preserved passenger capacity/carriage. |
| E10-S04 | Connected selection/whole-corridor edits, reversed/gapped/mixed-mode/turn-blocked previews; actual step-free and transit-continuity failures in profile-aware preview. |
| E10-S05 | Exact complete snapshots and canonical bytes on undo/redo; stale parent, failed batches, branching and non-reused IDs; compile-fail test prohibits snapshot tampering. |
| E10-S06 | Legal detour shifts simulated volumes; before/after gateway times; missing permissions/coverage, illegal turns, return loss and excessive diversion reject closure. |
| E10-S07 | Physical crossing time, hard protected/stress feasibility and independently inspected planner comfort penalty; blocked crossing and minimum-time failures. |
| E10-S08 | Separate versioned destinations, unchanged transport bytes/supply, actual revised tour input, labeled combined evaluation and rejected stale bindings. |

All fixtures are deterministic. No stochastic model confidence claim is made.
The [benchmark guide](BENCHMARKS.md#e10-scenario-authoring) records complexity,
scaling, full-snapshot memory limitations and measured reproduction commands.
