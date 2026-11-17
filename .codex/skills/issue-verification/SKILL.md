---
name: issue-verification
description: Independently verify a committed issue candidate and provide reproducible acceptance evidence for delivery to include in its PR.
---

Read `AGENTS.md`, `docs/AGENT-WORKFLOW.md`, the issue, acceptance mapping, and final candidate diff. Do not edit production code/tests, commit, push, or create PRs. You may run checks and produce evidence artifacts.

- Verify the actual checkout matches the reported candidate SHA and has no unexplained changes affecting results. Prefer a clean worktree. Inspect the tests and execution path rather than trusting implementor summaries.
- Run the required Rust format/clippy/workspace tests and actual ecosystem checks for introduced UI/training code. Exercise observable behavior, error paths, boundary cases, and applicable mobility constraints; capture browser/demo proof for user-facing changes.
- Map every acceptance criterion to test IDs, command/result, and artifact or file reference. Record SHA, base, environment/tool versions, fixtures, seeds, units, uncertainty, and benchmark context as applicable. Use the shared PASS / FAIL / BLOCKED / NOT RUN format. Capture exact failures; missing tooling is never a pass.
- Detect false positive tests: assertions must distinguish correct from incorrect behavior. Where valuable, use an isolated temporary copy for a bounded negative control; restore it or discard the copy and never contaminate the candidate. Report limitations honestly if such a check is not run.
- Separate engine correctness, runtime integration, baseline calibration, independent validation, performance, and external acceptance. Do not claim simulation fidelity from unit tests or scaffolding.
- Return the evidence report to the orchestrator and `delivery`. Required gaps block ready PR delivery. On a changed SHA, rerun affected checks and final required gates; explain any carried-forward evidence with its source SHA and why the change cannot affect it. Do not edit committed proof afterward to imply it verified its own future commit hash.
