# Local harness validation — E01-S03

Scope: approved harness optimization, including the five preexisting privacy edits.
Initial implementation and validation were local-only in the issue-27 worktree.
Subsequent user authorization permits signed commits, push, and PR creation from
a fresh origin/main branch. Existing remote noreply safeguards remain required.
No product/crate/API change. Dependencies: none. Parser and isolated workflow
fixtures are appropriate acceptance evidence for this configuration/tooling work;
they do not establish scientific calibration or live issue delivery.

## Instruction and coordination accounting

Before/after measurements include `AGENTS.md`, `docs/AGENT-WORKFLOW.md`, all ten
agent profiles and all ten role skills, plus the new conditional delivery reference
in the after total. They exclude helper code, tests, and validation records.

| Measure | Before | After |
| --- | ---: | ---: |
| Instruction bytes (UTF-8) | 48,157 | 35,457 |
| Whitespace-delimited words | 6,453 | 4,550 |
| Prescribed sessions for an ordinary Rust-only issue | 9 | 3 |
| Concurrent subagent configuration cap | 3 | 2 |
| Workers scheduled at a time | up to 3 | 1 |

The prior session count is the primary plus orchestrator, architect, test author,
Rust implementor, delivery, documentor, verifier, and code reviewer. The new count
is primary, reused architect/reviewer, and verifier. Optional specialists remain
available for concrete cross-module needs. This is instruction/coordination
accounting, not measured token or monetary savings. Conditional delivery rules
are loaded only for Git delivery; module contracts and the relevant story are
loaded as needed, not repeatedly by every role.

## Discovery and model evidence

A fresh Codex 0.169.0 app-server was initialized without starting a model turn or
issue team. `config/read` reported the project layer, `gpt-6-sol` / `medium`, the
two-worker cap, and explicit subagent defaults. `skills/list` with force reload
found all ten issue skills at the repo root and `crates/`, with no discovery errors.
The profiles are independently parsed for explicit model/effort policy and skill
links. These checks establish configuration/discovery; they do not prove runtime
model availability. The current conversation's model is unchanged. Runtime
model observations, when present, come from session `turn_context` telemetry.
Private raw discovery evidence is under ignored `.codex/run/discovery.json`.

The bundled skill-creator `quick_validate.py` could not run because PyYAML is
absent. No dependency was added. The standard-library harness validates the
required frontmatter/name/description, role links, TOML fields, and symlinks.

## Reproducible acceptance checks

`tests/foundations/test_agent_harness.py` uses synthetic temporary repositories
and JSONL data, covering:

- Profile parsing/model policy, skill references, discovery symlinks, and safeguards.
- Overlapping cumulative snapshots and unique response IDs, actual
  `token_usage_record` fields, missing telemetry/subsets, resets, and ambiguity.
- Ordinary passing checks; failed verification followed by reviewer correction.
- Dirty tracked/index/untracked content preservation and ignored log exclusion.
- Changed candidates during checks, including change then restore between commands.
- Missing tools and timeouts producing explicit BLOCKED evidence.
- Nonstandard rustup locations selecting the pinned cargo ahead of another compiler;
  missing pinned tools block checks instead of silently selecting another version.

The unavailable configured-model path is also a read-only workflow tabletop:
report the missing model, block dependent independent verification/publication,
checkpoint the limitation, and obtain a user-directed model choice if needed.
Never silently escalate or claim a verifier ran. No live delivery is required.

Run the required suite once on the final candidate:

```sh
python3 scripts/agent-harness.py checks
python3 scripts/agent-harness.py checks --verify .codex/run/checks.json
```

An initial verification exposed Homebrew cargo taking precedence over the installed
pin. The helper now resolves the pin with `rustup which`; affected verification
must be renewed on that corrected candidate.

The independent verifier owns that run and records results plus effective tool
versions in `.codex/run/checks.json` and immutable run-id manifests/logs. Check
candidate freshness before using the evidence. Architecture/conformance and final
working-tree review findings stay in `.codex/run/review.md`; local tree evidence
does not replace future committed-candidate review or GitHub signature gates.

## Usage evidence and limits

```sh
python3 scripts/agent-harness.py usage /path/to/rollout.jsonl
```

The helper consumes cumulative token_count snapshots and per-response usage,
deduplicates overlapping exports, and reports sources and unknown/ambiguous
attribution. Snapshots supersede response records for their session. Cached input
and reasoning output are subsets, never extra tokens added to input/output totals.
Usage from the active editing run is partial and changes as work continues;
private snapshots can be saved in `.codex/run/usage.json`. The approved plan's
76-wait finding is historical input, not a new measurement. No comparable post-
change issue run was launched, so achieved savings, cost, and daily-limit
percentages remain unmeasured.
