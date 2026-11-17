# Agent development guide

## Objective

Build a scientifically defensible, Rust-first multimodal planning application that **reduces dependence on private cars** while preserving or improving everyday travel flexibility for residents, including those unable to cycle.

## Start here

1. At intake, read `README.md`, `docs/ARCHITECTURE.md`, and the relevant story in `backlog/car_independence_backlog.md`. Workers read only assigned contracts and criteria.
2. Pick one story ID (e.g. `E05-S01`); record its acceptance criteria and dependencies.
3. Identify the crate(s) you will own. Prefer small changes that preserve public contracts.
4. Add deterministic fixtures and tests before or alongside the implementation.
5. Record algorithmic complexity and benchmarks for performance-sensitive changes.

## Rules

- Do not optimize private-vehicle speed/throughput as a proxy for accessibility.
- Do not assume everyone can ride a bicycle or use rail; include mobility constraints and essential vehicle access.
- Do not invent observed traffic, safety, emissions, ridership, or population inputs.
- Distinguish observed values, imputed values, model output, and uncertainty. Preserve units, source, geography, and date.
- Keep calculation engines deterministic under a fixed input and seed where feasible; stochastic outputs must include variance/confidence information.
- Never silently disconnect road, pedestrian, transit, emergency, or freight access.
- Separate abstract interventions from geographic feasibility and cost validation.
- Avoid premature dependency additions or architectural rewrites without an ADR and benchmark.
- When implementing a story, update docs and link tests to the story ID.
- Keep Python in training/validation unless a measured reason supports Python in a hot path.
- Do not claim simulation fidelity before verifying baseline calibration and independent checks.

## Validation before proposing completion

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

For UI/training code, add ecosystem-specific checks when those components are introduced. If tooling is unavailable, state the unverified checks explicitly.

## Proposed modular ownership

- Data and geospatial agent: city ingestion, raw source adapters, quality and provenance.
- Graph/routing agent: multimodal graph contracts and route computation.
- Simulation agent: trips, mode choice, assignment, congestion, metrics.
- Optimization agent: Gym-style interface, baseline methods, RL algorithms.
- App agent: API, Svelte/MapLibre visualization, scenario workflows.
- Scientific QA agent: calibration, regression fixtures, uncertainty, safety, performance.

Agents should coordinate changes to shared types through reviewed interface changes rather than simultaneous edits.

## Issue delivery team

All new project commits must use the repository-local Git identity associated with `nthompson-bitwarden` as both
author and committer and be cryptographically signed with the user's registered
key. Require the account’s verified GitHub noreply address for both email fields;
a personal address blocks new commits. Never attribute commits to Codex or other coding agents, including co-author
trailers or generated-by footers. Verify signatures locally and require GitHub
`verified: true` before reporting delivery complete. See the delivery skill for
identity checks and failure handling. Preserve legitimate third-party authorship.

Treat the configured email as private operational data. Do not copy it into PR titles or bodies,
comments, commit messages, documentation, agent reports, or tracked configuration. Report only
that identity and signature checks passed. Read the email from repository-local Git configuration
for commit operations; do not print it in user-facing evidence. Git commit metadata exposes
the configured email: do not change identities or rewrite published history without explicit
user authorization; use a verified GitHub noreply address only when authorized.

For issue delivery, the primary session uses `$issue-orchestrator` and remains the
sole coordinator and default implementer, test author, documentor, and delivery owner.
Follow [the shared workflow](docs/AGENT-WORKFLOW.md) and the complete
[public-copy privacy boundary](docs/AGENT-DELIVERY.md#privacy-of-identity-evidence). Reuse two independent workers
serially: `architect` for design/conformance/code review, `verifier` for final checks.
Additional language specialists need a concrete cross-module reason. No worker delegates.
Narrower user scope overrides delivery defaults; no merging is authorized.

Every handoff identifies issue/story, dependencies, owned files, public contracts,
tests, prohibited cross-module changes, candidate/evidence, and next owner.
Preserve unrelated work; serialize shared-contract edits. Profiles are under
`.codex/agents`; skills are exposed through `.agents/skills` symlinks.
