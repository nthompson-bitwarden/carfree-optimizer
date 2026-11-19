# Feasibility, costs and independent safety — E12, in-memory contract v1

E12-S01–S08 add `interventions::feasibility` over the merged E10/E11 contracts.
No external dependency, graph schema, router, simulator, optimizer or persistence
contract changes. This is a conservative screening API for **caller-supplied
engineering and financial evidence**, not engineering certification, automatic
geographic feasibility, calibrated predictions or policy recommendations.

## Reviewable demonstration

```sh
rustup run 1.97.1 cargo run --locked --offline -p interventions --example e12_feasibility
rustup run 1.97.1 cargo test --locked --offline -p interventions
```

The MIT repository-authored fixture extends E10's abstract corridor, dated
2026-01-05, seed 42. All widths, limits, costs, permissions, exposure and rates
are arbitrary **modeled** inputs, not observations. The text demo shows separated
costs, directed access, budget/missing-engineering rejections, an expiring
construction closure, and an independent collision-risk sensitivity estimate.

## Atomic gate and exact binding

`screen(&History, &Batch, &Inputs) -> Result<Screened, Rejection>` previews an
E10/E11 transaction without changing history or its redo branch. It then checks
costs, engineering and designated access before any assignment adapter is called.
Existing E10 preview validation itself includes bounded routing. Rejections give
specific reasons; engineering findings include segment/ROW IDs and proposed and
minimum/available millimetres. Router exhaustion is a **routing error**, distinct
from an **unmet** designated journey; neither can produce `Screened`.

`Screened` privately owns the exact immutable `Snapshot`, full input audit and
report. Read-only accessors cannot inject another snapshot. Repeated IDs across
independent histories do not transfer approval. `Screened::evaluate` delegates
to the existing E10 evaluation bridge on those exact snapshot contents, retaining
matched demand/revision/land-use binding, single road-supply projection and
rejection of overlapping edits. E08 still validates simulation settings, demand
and transit run/passenger supply. Screening alone is **not simulation readiness**.
The legacy `evaluation::evaluate` remains compatible and **does not imply an E12
screening pass**; callers requiring E12 must use `Screened::evaluate`.

## Budget and lifecycle accounting (S01, S05)

`Budget` defines one currency, explicit minor-units-per-major-unit scale
(e.g. USD=100, JPY=1), real price year, capital cap, shared annual cap,
horizon (1–200 years), and nonnegative real discount rate. Scope is gross costs
of the caller-declared project, **not automatically calculated baseline deltas**.

- Construction and disruption are one-time capital, reported separately.
- Maintenance is annual recurring cost, separate from annual transit operations;
  both consume the annual cap. Neither can be hidden inside capital.
- `Cost` exposes ID, category, quantity, unit, lower/central/upper unit rate,
  positive local adjustment, currency, price year and full provenance. Each of
  the three categories requires a row, including explicit zero quantities.
- Every actual transit trip template, including baseline patterns missing from
  E11's proposal ledger, needs exactly one `TransitCost`. E11 `estimate` recomputes
  full-run covered operations on the actual network. Recorded E11 operations
  must match. Annual major units = covered minor units / explicit currency scale × coverage
  repetitions per year × local factor. The annualization rationale is mandatory;
  a short timetable is never silently interpreted as a representative year.
  Missing, duplicate, stale or incompatible service costs fail.
- Currency and price year must match exactly. The engine performs no exchange,
  inflation or geographic cost inference. Provenance preserves observed/imputed/
  modeled distinctions; calculation outputs are modeled. Real engineering scope
  and whether all costs/quantities were supplied remain caller obligations.
- Budgets compare **upper parameter bounds**, without rounding or tolerance;
  equality passes. Nonfinite, negative, unordered and overflowing inputs fail.
  Money is unrounded `f64` major units, not ledger-grade decimal accounting.
- Lifecycle bounds are `capital + annual × sum((1+r)^(-year), year=1..N)`.
  Bounds propagate supplied assumptions; they are not a confidence interval.

## Physical evidence (S02, S08)

Every candidate directional road ledger entry and every changed/added/removed
link relative to the project baseline requires `Segment` evidence. This includes
pedestrian, rail, station and transfer works outside the lane ledger. Each edge
has one evidence owner; each road and its allocated targets stay together.
Lane counts by general/bike/bus/retired use must match the actual candidate.
Each lane supplies width and a positive assessed minimum, in **millimetres**.
`other_width_mm` explicitly reserves sidewalks, buffers, rail/station footprints
or other non-lane space; it is not an automatically certified design width.

Segments name shared `RightOfWay` groups. All directional footprints and other
reserved space in a group are summed once and compared with its available width.
Opposing directions must share the same group when they share physical ROW;
this physical grouping and shared-sidewalk allocation are explicit caller
assessments, not inferred from directed edge IDs or arbitrary graph geometry.
The fixture uses separate directional envelopes; tests include shared ROW.
Provenance covers the width/standard/ROW and restriction assessment.

Every segment explicitly addresses bridge, slope, utility and terrain:
`Clear`, `Restricted(reason)`, `Missing(reason)`, or justified
`NotApplicable(reason)`. Restricted and missing evidence block. There is no
unknown-equals-safe default. Numeric grades, load limits and utility relocation
feasibility must be assessed upstream; no invented thresholds or exemptions are
bundled. Input metadata is validated using the existing graph provenance rules.

## Protected access registry (S03, S04)

All six classes require nonempty declared OD samples: emergency, disability,
essential delivery, property access, freight and regional essential trips.
Each `AccessSample` includes actual caller-assessed endpoints, outbound **and
reversed return** request, departure/deadline, profile, state bound and source.
Supply multiple properties, facilities, gateways and times of day as appropriate.
A label alone cannot establish completeness of this registry.

Vehicle samples use car-only routing with an explicit nonempty assessed permitted
edge set. This is a permission representation for essential vehicles, **not an
assumption that a generic car fits a fire truck**. All original turn rules are
retained when nonpermitted links are blocked. Disability samples require
step-free, mobility-aid, walk-only, known-access journeys, never cycling
substitution. Property samples use explicit walking or permitted-car access.
Door-node planning rejects inaccessible endpoints and does not finish aboard
transit. This conservative disability gate requires a walking connection; it
does not assume everyone can use rail or has unlimited walking ability—supply
individual deadlines and essential vehicle samples too.

Only supplied directed OD/time/profile samples are established. Network coverage,
return service, comprehensive emergency response, all properties and regional
boundary completeness are **not inferred**. Screened access at fixed graph times
is not a congestion-aware response-time guarantee or proof of car independence.

## Construction schedule (S07)

`screen_plan(history, phases, final_inputs)` operates on a cloned history:

1. Each permanent milestone is an ordinary atomic E10/E11 batch whose parent is
   the preceding **permanent** milestone.
2. An optional temporary batch is previewed on an isolated milestone branch.
   It is the effective phase snapshot but does not become the next milestone's
   parent. Temporary closures therefore expire without pretending E10 can reopen
   a permanently retired lane. Temporary works must be explicitly repeated when
   needed again; IDs are unique across the schedule.
3. Phase intervals are positive, contiguous, ordered, inside graph coverage;
   every protected request must fit its interval. Each effective snapshot and the
   final permanent snapshot is screened against the **original project baseline**,
   retaining evidence for early off-road works and later temporary changes.
4. All phases use the same budget and protected registry as final inputs. IDs,
   classes, endpoints, vehicles, profiles and provenance cannot be dropped or
   replaced; departure/deadline, state bound and assessed permissions may vary.
5. A structured temporary-service map must exactly name changed trip patterns,
   calendars/service days, and patterns incident to changed transfer facilities.
   `Operates` requires active complete runs inside the interval; partial runs or
   exhausted enumeration fail. `Suspended` requires no overlapping complete run
   span, including between-leg dwell.
   Explicit reasons/provenance are retained. An empty map means no affected
   temporary patterns, not an unexamined temporary timetable. Capacity/matched
   run supply is validated later by the simulation adapter for each phase.
6. Each phase supplies incremental construction and disruption line items for
   that interval and the full recurring costs for its effective network. Capital
   accumulates against the fixed project cap; annual costs are bounded in each
   phase and final state. Final construction/disruption must be zero to avoid
   charging the same milestones twice. Project lifecycle is all incremental
   capital at time zero plus discounted **final-state** annual costs; it is not
   a construction cash-flow or temporary operating-cost forecast. Disruption
   quantities/costs and annualization are supplied, not inferred from closures.

Any failed intermediate state blocks the entire plan even if final access would
be restored. The original history remains unchanged. Phase `Screened` retains
its interval and rejects demand departures, deadlines and windows outside it
before adapter invocation. E08's full graph-period coverage contract is unchanged.
Evaluate individual phase `Screened` values with explicitly matched E08 inputs; no phase demand
or passenger supply is invented.

## Independent collision-risk estimates (S06)

`Screened::risk(RiskInput)` is a separate rate-times-exposure estimator, not part
of movement simulation or a budget/access approval condition. Inputs/report name
model and version, mode/severity stratum, exposure unit and quantity, explicit
exposure scenario ID and graph revision (required to match the screened snapshot), exposure
provenance, calibration provenance, applicability, period, rate bounds per
exposure unit and uncertainty method. The single declared unit/stratum applies
to both rate and exposure; caller attests model applicability. Vehicle-km cannot
be assigned to a non-motorized stratum. Geography must match local calibration
and scenario; periods must fit graph coverage. Incompatible/invalid inputs fail.
Missing calibration returns an explicitly **unavailable** estimate, not zero.

Output is modeled expected collision counts with scaled parameter bounds, not
simulated crashes, automatic confidence intervals or causal safety benefits.
Observed/imputed/modeled provenance remains visible, including the demo's
uncalibrated synthetic calibration source. Cycling stress is never a crash rate.
Real scientific use requires independently justified exposure, calibration and
holdout validity; passing these tests establishes none of them.

## Acceptance evidence and performance

- `tests/e12_feasibility.rs`: S01/S05 category arithmetic, upper-bound budget
  boundaries, invalid costs/year/currency/annualization; S02/S08 segment widths,
  lane inventory, shared ROW, missing/restricted evidence and off-road coverage;
  S03/S04 each protected class, return paths, permissions, illegal turns and
  resource exhaustion; S06 independent exposure counts, unavailable calibration
  and stale exposure scenario/revision rejection.
- `tests/e12_phases.rs`: S07 temporary closure expiry, intermediate failure,
  ordered parents/time windows, fixed policy/registry, cumulative capital,
  retained off-road evidence and temporary-service mismatch/exhaustion.
- `tests/e12_evaluation.rs`: zero adapter calls on hard failures, successful
  real E08 evaluation, same-ID history isolation, double supply, stale inputs
  and rejection of out-of-phase demand before the adapter.
- `tests/e12_review_regressions.rs`: explicit zero-decimal currency accounting
  and through-run dwell suspension.
- Compile-fail example prevents direct `Screened` mutation. Existing E10/E11
  regression suites and graph binary schema remain unchanged.

See [benchmark reproduction and complexity](BENCHMARKS.md#e12-feasibility-screening).
