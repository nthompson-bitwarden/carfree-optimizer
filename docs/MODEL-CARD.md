# Reference model card

This card covers the E20 local workbench candidate, executable version 0.1.0,
workspace schema v1, and scenario report schema v1. Each saved result includes
the exact evaluator SHA-256, the input revisions and provenance, and seed 42.
Use the executable digest and repository revision to identify builds with the
same package version. Refresh this card when the model, inputs or validated
operating range changes.

| Topic | Evidence and limits |
| --- | --- |
| Intended use | Reproduce workflow and regression checks, compare bounded interventions on the supplied synthetic network, and inspect modeled access and essential trips. |
| Transport geography | Repository-authored five-node schematic, E14 topology 0, with adult and mobility-aid cohorts and explicit mode options. No observed transport geography was evaluated by the workbench. Portland, ME, Portland, OR and the Portland–Vancouver–Hillsboro metro Census boundaries support geographic onboarding. |
| E20 import fixture | An abstract rectangle and repository-authored street geometry. Walk links use an explicit 1 m/s modeled speed and whole-second edge costs rounded up. The separate workbench test uses its published five-node topology. |
| Travel and accessibility | E07 door-to-door timing includes access, waits, transfers, timetable coverage and mobility profile constraints. E08 reports assigned and unserved weighted trips. E06/E13 show car-required and no-mode reference essential trips with separate denominators. |
| Interventions and costs | Phase additions are synthetic and checked against declared budgets and modeled right-of-way. Cost ranges come from fixture inputs in 2026 USD and USD/year. Real designs require independent engineering, operating and cost review. |
| Uncertainty | The report varies modeled demand mix, one walk-mode restriction and generalized waiting penalty independently. Ranges describe finite scenario sensitivity. Population sampling error, correlated shocks, inferred mode shifts, collision rates and long-run adaptation remain unquantified. |
| Calibration and fit | No fitted or held-out observed urban data for the workbench. The SUMO comparison checks an isolated free-flow vehicle leg at two configured speeds. Coverage excludes queue interactions, mixed traffic, transit capacity, real departures and clinical or household trip needs. |
| Failure modes | Missing source partitions, absent service, timetable gaps, unknown access and inaccessible transfers can make essential trips infeasible. Distinguish unmet journeys from request errors and resource bounds. Review uncertainty on every comparison. |

Before making real-world accessibility or policy claims, match essential trip
chains to independently reviewed door locations and population coverage. Check
waits, service availability and each supported mobility profile, including
people who cannot cycle. Report essential trips with no feasible car-free
journey. Require observed baseline calibration, independent holdout and
scenario comparison evidence, and disclose uncertainty before deploying any
policy recommendation. See the [calibration contract](CALIBRATION.md) and the
[workbench report schema](../web/README.md#report-schema-v1).
