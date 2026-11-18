# Baseline calibration and observed-data validation (E09)

E09-S01–S06 depend on E03 observation/provenance ingestion, E06 demand and E08
simulation. This implementation supplies **comparison and bounded calibration
machinery**, not a calibrated city or historical accuracy claim. All bundled E09
targets are repository-authored synthetic fixtures, marked `Modeled`; none are
observed traffic, ridership or surveys. Source-to-network conversion and the
choice/validation of geographic crosswalks remain caller responsibilities.

## Public contracts and use

`mobility_sim::calibration::{run, Request, Observation, Target, Report}` is an
additive in-memory v1 contract. `run(&dyn simulation::Adapter, &Network, &Demand,
&simulation::Scenario, &Request)` returns an immutable calibration envelope.
Existing simulation/demand/graph contracts are unchanged. No external dependency,
Python hot path, fitting library or optimizer coupling is added.

`mobility_api::calibration::{render, export_tsv}` produces an escaped static HTML
results page and audit TSV from **the same domain envelope**, including project ID,
fitness banner and all limitations. This is not an HTTP endpoint, saved-project
schema migration or production input importer. The project ID associates an
artifact with its caller's project; input revisions must match that project's
baseline before the caller attaches the result. Legacy search/onboarding pages
are not simulation results and do not assert calibration fitness.

Run the deterministic demonstration (requires the existing `protoc` build tool):

```sh
cargo run -p mobility-api --example e09_calibration > /tmp/e09-calibration.html
cargo run -p mobility-api --example e09_calibration -- tsv > /tmp/e09-calibration.tsv
```

Open the HTML file in a browser. It deliberately displays LIMITED FITNESS, a
separate holdout section, source coverage, target values, per-seed modeled values,
residuals, error scales, candidate parameters and limitations. File redirection is
shell-controlled; the library does not write or overwrite files itself.

## Evidence and exact matching

Each observation stores an ID, stable original source record ID, complete source
and crosswalk provenance (source/version/license/geography/civil date/basis/
uncertainty), split, explicit target binding with exact network and demand
revision IDs, half-open UTC interval, value or
missing reason, and positive error scale. Units are determined by the typed
metric, not guessed from an input string. Preserve the source's quantity definition,
cohort and time zone when converting to this contract. There is no implicit unit
conversion, temporal prorating, station snapping or imputation. Stale crosswalk
revision bindings are flagged rather than matching reused edge/trip/group IDs.
A real zero is
`Some(0.0)`; absence is `None` plus a nonempty reason.

- **E09-S01 road vehicles:** directed station cutline edges and direction label;
  sum vehicles entering those edges in whole simulation entry periods whose
  endpoints exactly cover the observation interval. Never supply consecutive
  links along a path as a count-station crosswalk: that would count a vehicle
  repeatedly. Unknown car edges, missing model cells, partial entry periods and
  intervals outside model/network coverage have distinct flags. Each comparison
  retains station, interval, source and signed residual (`model - target`).
- **E09-S02 transit person-boardings:** explicit versioned trip-to-route or
  trip-to-service-area mapping. Derive boardings from served tour journeys, not
  `TransitResult.served_passengers` segment loads. Consecutive segments on the
  same `(trip, service_date, ordinal)` count once; an intervening non-run step or
  a different run counts another boarding. Denied tours count no served boardings.
  Assign boarding to its first ride-segment departure in the requested interval;
  a continuing ride that boarded before the interval is not a new boarding.
  Include companions and expansion weights. Missing ridership, unmapped trips and
  incomplete temporal coverage are not a covered zero. Every mapped scheduled
  segment/run departing in the interval must have an explicit supply and output
  cell, even when no traveler used it. An unsupplied scheduled run is missing model
  coverage, not zero ridership. This metric is NOT unique
  riders, passenger-km, fare validations or segment occupancy; incompatible
  ridership sources need an explicit upstream conversion, not relabeling.
- **E09-S03 surveys:** explicit group cohort, optional purpose and area label.
  Select eligible legs by **requested** departure time, independent of fitted
  journey durations. The entire survey interval must fit the explicit demand
  environment's coverage as well as the graph/simulation coverage; otherwise a
  partial-demand coverage flag makes the comparison unusable. Weight each by expansion × demand multiplier × (1 +
  companions), in person trips. Mode-share denominator includes every eligible
  trip, including unserved trips; reported served-mode shares can sum below one.
  Mean commute time requires explicit `Purpose::Commute`, and uses elapsed journey
  seconds for served commute legs only, including waiting. It is not all-purpose
  `person_seconds / trip_weight`. Eligible and unserved weights are reported;
  any unserved weight flags conditional coverage and makes that fit unusable.
  Unknown groups and empty denominators are explicit missingness. A household,
  worker-only, vehicle-trip or tour-based survey must be converted to a compatible
  person-trip cohort first. There is no silent default denominator.

All adapter outputs must identify the requested scenario, network/demand revisions,
engine and seed, and preserve complete ordered tour/leg alignment. Errors remain
in candidate run records rather than disappearing from the objective. Numeric
comparison/score failures are retained as unusable rows or failed run scores,
including holdout overflow after a successful fit; the artifact remains available
with limitations. Invalid structural inputs still return an error. Adapters
are trusted to implement their stated journey and capacity contracts; calibration
checks do not independently validate alternate engines' internal physics.

## Fitting, holdout and audit

E09-S04/S05 use a predeclared bounded **ordered grid**, not an adaptive optimizer.
Candidates replace E08 `alpha`, `demand_multiplier` and `preference_seconds`; all
other scenario settings are fixed. Every candidate receives the same distinct
seeds. The input order resolves exact objective ties. Fix the grid, seeds, split,
scales, required families, crosswalks and cohorts **before inspecting holdout**.
Repeated human tuning against a holdout invalidates its independence; the API
cannot prevent reuse across separate jobs.

For each seed, the fitting objective is the root mean of squared
`abs(model - target) / scale`, across fitting rows only. It is undefined unless
EVERY expected fitting row has a usable comparison and the simulator converged.
Missing rows and failed/nonconverged seeds cannot improve an objective by being
dropped. Select the candidate with the smallest mean seed objective, then replay
ONLY that candidate for separate per-seed holdout scores. No holdout value enters
candidate scores, tie-breaking, fitting eligibility or candidate selection.
Source-record reuse across splits is rejected even under alternate row IDs,
release versions or geography labels: identity uses stable source namespace and
original record ID, not release/display metadata;
duplicate mapped quantity/cohort/time rows are rejected even with changed source
or location labels. This does not detect semantic leakage in differently encoded,
partially overlapping cutlines or cohorts: source curation must ensure disjoint
validation observations, cities/dates and cohorts where appropriate.

Per-candidate score population variance across declared seeds is retained, alongside
all per-seed predictions/scores; it is not a confidence interval or observational
uncertainty. A single seed has zero seed variance and establishes no robustness.
Source uncertainty is preserved separately; scales are caller-declared tolerances,
not inferred standard errors. Multiple metrics are normalized by explicit scales,
not by adding raw vehicles, fractions and seconds. Each selected comparison with
scaled absolute error above one fails its declared check even if the aggregate
objective is small. Both splits must contain observed targets for every declared
required metric family; callers must not omit unavailable families to suggest
citywide fitness. Coverage describes only the supplied target inventory, not a
claim of spatial or temporal representativeness.

### Audit TSV v1

UTF-8, header `record\tfield\tvalue`, exactly three columns. Values escape
backslash, tab, CR and LF as `\\`, `\t`, `\r`, `\n`; decode left-to-right with
these four escapes. Numeric fields use Rust finite-number display; optional
numbers use literal `missing`. Indexed records preserve input order. Records
include report version/fitness/banner/limitations/project/input identities,
source and crosswalk versions and revision bindings, every target and split, base settings, candidate
effective settings and before/delta values, seeds, per-seed failures, predictions,
residuals, coverage and scores. The `scenario_snapshot` and `mapping` diagnostic
fields use the documented Rust v1 types' Debug representation; they retain full
fixed scenario/cost settings and bindings for inspection, **not a parsable input
schema**. Retain the original network bytes, demand, scenario, request and adapter
implementation for replay; the audit file alone is not a model-input snapshot.
Output has no wall-clock timestamp and replays byte-for-byte for fixed inputs.

## Fitness and limits (E09-S06)

The immutable envelope computes fitness once. Results and exports cannot omit
its banner or limitations. Missing required observed families in either split,
synthetic/imputed targets, modeled/imputed network, scenario or demand provenance, unavailable
holdout, failed/nonconverged selected runs, missing comparisons, unserved survey
trips or an exceeded error scale yield LIMITED FITNESS. If every fitting candidate
is unusable there is no winner and no holdout fitting fallback; all failed
candidates are still reported. `ChecksPassed` means only the declared checks
passed, never independent model validation or planning certification.

No city is demonstrated car-independent here. Realistic journey schedules, wait
times, mobility constraints, infeasible essential trips, representative sampling,
independent validation and uncertainty disclosure remain necessary for policy
use. Observed count agreement alone is not an accessibility outcome.

## Acceptance, complexity and performance

`crates/mobility-sim/tests/e09_calibration.rs` links deterministic Given/When/Then
coverage to E09-S01–S06: location/time residual signs, zero/missing/partial coverage,
real-adapter boardings plus multi-segment/transfer/denial oracle, mixed-purpose
commute and survey denominators, holdout perturbation without winner changes,
leakage rejection, replay/audit metadata, nonconvergence and adapter failure.
The additional `e09_regressions.rs` suites cover stale revision bindings,
uncovered scheduled transit, republished source leakage, holdout arithmetic
failure and demand-basis isolation.
`crates/mobility-api/tests/e09_calibration.rs` checks shared limitations, missing
holdout, export determinism and HTML/TSV injection boundaries. These are software
acceptance tests, not independent transportation validation.

For C candidates, S seeds and simulator cost T, run cost is `(C + 1) S T` when a
winner exists; no holdout run is performed without a winner. With O observations,
E graph edges, A served journey steps, R road output cells, P periods, F mapped
scheduled segment/run events and U transit supply/output cells, the straightforward
comparison scans are bounded by `O(O (E + A + P R + F U))` per run
(up to crosswalk size factors for edge/trip/group membership and calendar/day scans). The implementation
prioritizes explicit matching and inspectability, not metro-scale joins. Retained
comparison memory is `O(C S O)` plus input metadata and one simulator output at a
time. Validation rejects more than 100,000 observations or 10,000 candidate-seed
combinations, but these are safety bounds, not supported production scale.

Reproduce the whole-job benchmark (includes E08 simulation):

```sh
rustup run 1.97.1 cargo run --release -p mobility-sim --example e09_calibration
```

Apple arm64, Rust 1.97.1, release build, two synthetic targets, two seeds, 20 jobs
per grid size: 2 candidates **7.201 ms** total; 8 **14.810 ms**; 32 **46.101 ms**.
These single local samples demonstrate the harness only, not a regression budget,
peak-memory measurement, geographic scalability or empirical fidelity.
