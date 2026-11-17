# Agent development guide

## Objective

Build a scientifically defensible, Rust-first multimodal planning application that **reduces dependence on private cars** while preserving or improving everyday travel flexibility for residents, including those unable to cycle.

## Start here

1. Read `README.md`, `docs/ARCHITECTURE.md`, and `backlog/car_independence_backlog.md`.
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

All new project commits must use `nthompson-bitwarden <nathom791@gmail.com>` as both
author and committer and be cryptographically signed with the user's registered
key. Never attribute commits to Codex or other coding agents, including co-author
trailers or generated-by footers. Verify signatures locally and require GitHub
`verified: true` before reporting delivery complete. See the delivery skill for
identity checks and failure handling. Preserve legitimate third-party authorship.

For a request to implement and deliver a GitHub issue, use `orchestrator` and
`$issue-orchestrator`, following [the issue delivery workflow](docs/AGENT-WORKFLOW.md).
This authorizes bounded specialist delegation for that issue. Follow an explicitly
narrower user scope, such as local work only.

Delivery prepares the branch from freshly fetched `origin/main` unless another
base/branch is specified. Architect reviews before and after implementation;
test author establishes BDD-style acceptance tests first; language specialists
implement; delivery commits; technical documentor checks the commit; final
verification and committed-code review precede push and PR creation. Delivery
includes the verifier's proof in the issue-linked PR. Required failed, blocked,
or unrun gates prevent a ready PR. Other agents do not commit or publish.

Every task handoff identifies the issue/story, dependencies, owned files, public
contracts, tests, prohibited cross-module changes, evidence status, and next owner.
Preserve unrelated changes and serialize shared-contract edits. Agent profiles
are in `.codex/agents`; role skills are in `.codex/skills`, discovered through
the repository's `.agents/skills` symlinks. These instructions preserve the parent
session's permissions and do not authorize merging.
