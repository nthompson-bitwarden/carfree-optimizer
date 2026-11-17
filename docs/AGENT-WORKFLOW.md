# Issue workflow

E01-S03 (dependencies: none) requires handoffs naming owned files, public APIs,
tests, and prohibited cross-module changes. `AGENTS.md` supplies scientific rules;
read `ARCHITECTURE.md` and `MODULE-CONTRACTS.md` only for affected boundaries.

## Ownership and sequence

The primary session is the sole coordinator and default implementer, test author,
documentor, and delivery owner. `$issue-orchestrator` is its workflow entrypoint;
do not spawn an orchestrator. Default to two reused independent workers, scheduled
one at a time: `architect` for design, conformance, and code review; `verifier` for
final checks. Only the primary delegates. Additional language specialists require
a concrete cross-module need and explicit nonoverlapping ownership. Workers never
commit or publish. Preserve unrelated changes and serialize shared-type edits.

1. Primary records issue/story criteria, dependency status, scope, and owned files.
   For issue delivery, prepare from freshly fetched `origin/main` unless a base or
   branch was specified. For local-only work, preserve the current branch.
2. Architect independently reviews design, contracts, failure behavior, acceptance
   mapping, and prohibited edits before implementation.
3. Primary establishes deterministic Given/When/Then tests, then implements with
   focused checks. Missing-symbol compilation is BLOCKED behavioral evidence;
   rerun when the interface exists. Config/docs work uses parser/discovery and
   scenario checks instead of artificial red production tests. Never weaken tests.
4. Reuse architect for conformance. Primary records documentation disposition,
   prepares the candidate, and inspects documentation impact after any commit.
   Commit documentation changes before final verification/review when authorized.
5. Verifier independently runs the complete required suite on the final candidate.
   Reuse architect for final committed-code review, including verification evidence,
   hidden failures and misleading tests. Local-only requests use a working-tree
   fingerprint and working-tree review; this does not satisfy committed delivery.
6. Primary corrects findings, reassesses docs/design as relevant, and renews affected
   verification and review. FAIL, BLOCKED, NOT RUN required gates or unresolved
   blocking findings prevent a ready PR. After two unsuccessful correction cycles,
   checkpoint unresolved findings; do not automatically expand the team or model.

For Git mutations and publication, read [delivery procedure](AGENT-DELIVERY.md).
A request to deliver an issue authorizes scoped commits, push, and PR; narrower
scope wins. No merge, release, repository creation, or unrelated message is implied.
Connected tools are preferred; Git/gh cover unsupported operations. Inspect remote
state before retrying uncertain publication. Keep credentials out of the repo.
Public PRs contain a concise change summary, material limitations, and relevant
lint/test commands and results. Keep identity, signing, environment, and internal
agent gate evidence in private handoffs. Never publish configured email addresses.

## Context and models

Fresh sessions use `gpt-6.2-sol` / `medium` for the primary, fallback workers,
`architect`, `code_reviewer`, `test_author`, and Python, Rust, and web implementors.
The bounded `delivery`, `orchestrator`, and `technical_documentor` profiles use
`gpt-6.2-sol` / `low`; mechanical `verifier` uses `gpt-6-luna` / `low`. Every optional
profile sets model and effort explicitly. No automatic escalation: an unavailable model
is an explicit limitation and blocks its dependent gate until resolved within user
scope. Configuration is not evidence of the active conversation's runtime model.

Spawn with `fork_turns="none"`, supplying only issue criteria, worktree, owned
scope, contracts, candidate, and evidence paths. Reuse the same worker threads for
corrections. Wait up to 60 seconds; send substantive corrections, never status-only
requests. Keep user updates brief and do not restate unchanged gates.

Store compact task state and detailed logs under ignored `.codex/run/`. Return
status, findings, candidate identity, and evidence references instead of full chat
reports. Read shared guidance once per worker; do not reload the whole backlog or
all role guides. Optional profiles retain their named role skills; the primary
uses those skills directly when applicable.

## Handoff and evidence

```text
Issue/story, acceptance criteria, dependencies and status:
Worktree, base ref/SHA, candidate SHA or local fingerprint:
Owned crates and files:
Public APIs / stored schemas / compatibility:
Prohibited cross-module changes:
Tests / fixtures / seeds / expected behavior:
Findings / decisions / limitations, evidence paths:
Status (PASS / FAIL / BLOCKED / NOT RUN) and next owner:
```

Evidence records commands, results, candidate/tree identity, logs, relevant tool
and fixture versions, units, provenance, uncertainty, and benchmarks. PASS means
that check ran and met expectations; FAIL means it contradicted them; BLOCKED
means a missing prerequisite; NOT RUN has no execution evidence. Never silently
reuse evidence across candidates. After relevant changes rerun checks; any carried
unchanged evidence needs exact source identity and verifier justification.

The independent verifier runs `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`,
plus foundation and actual ecosystem checks. UI features require reviewable demos;
simulation-critical edits require complexity/benchmark evidence. Calibration,
independent scientific validation, browser acceptance, and unit tests prove
separate things. Do not claim fidelity from a successful build.

## Harness commands and discovery

Python 3.11+, standard library only, from any working directory:

```sh
python3 scripts/agent-harness.py validate
python3 scripts/agent-harness.py checks
python3 scripts/agent-harness.py checks --verify .codex/run/checks.json
python3 scripts/agent-harness.py usage /path/to/session.jsonl
```

Default evidence directory: `.codex/run/`; `checks --output` selects an ignored
subdirectory there. `checks --command '...'` replaces the default suite for a
focused run and is labeled custom, never complete-suite acceptance. The required suite uses `rustup which cargo --toolchain <pin>` to locate the
installed toolchain and records effective compiler versions. Missing pins block
the suite rather than falling back to another compiler. Nonzero exit,
missing tools, timeout, or a candidate changed during checks prevents PASS. Logs
are private; console failure excerpts are bounded. Candidate identity covers HEAD,
index, tracked worktree content/modes, and nonignored untracked content/symlinks.
Ignored local inputs are outside that identity; relevant external fixtures and
environment must be recorded separately. Run no edits while final checks execute.

Usage accepts Codex JSONL session_meta, token_count snapshots, and response records
with usage. It deduplicates overlapping sessions and response IDs; cumulative
snapshots take precedence over responses from the same session. Conflicts,
resets, unattributed records, and unknown telemetry are flagged rather than added
as trustworthy totals. Cached input is part of input; reasoning is part of output.
No daily-limit percentages, costs, or achieved savings can be inferred. Compare
like-for-like measured runs before claiming savings.

`validate` checks TOML, model policy, skill links/frontmatter, discovery symlinks,
and safeguards. Start a fresh client to check configuration and skill discovery
without launching an issue team; record configured versus observed models
separately. Static parsing does not prove live model availability or GitHub delivery.
Configuration fields follow the [official schema](https://learn.chatgpt.com/docs/agent-configuration/subagents?surface=app).

Exercise ordinary issue, failed verification, reviewer correction, dirty worktree,
and unavailable-model paths with isolated fixtures or read-only tabletop requests.
See [harness validation](AGENT-HARNESS-VALIDATION.md) for current evidence and the
[initial setup record](AGENT-SETUP-VALIDATION.md) for historical evidence only.
