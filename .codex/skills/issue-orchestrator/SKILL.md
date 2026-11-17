---
name: issue-orchestrator
description: Coordinate this project's subagents to implement and deliver a GitHub issue with architecture, tests, verification, review, and documentation gates.
---

Read `AGENTS.md` and `docs/AGENT-WORKFLOW.md`. Use the workflow's handoff and evidence formats.

1. Resolve the issue from the connected GitHub tools, or `gh` when those tools lack the required capability. Read the issue and relevant comments, backlog story, acceptance criteria, and dependencies. Do not infer an issue number from a story ID. Stop dependent work if issue identity or a required dependency is unresolved.
2. Ask `delivery` to prepare the issue branch before any edits. Give `architect` the issue and owned modules for the design gate. Record the public contracts and prohibited cross-module edits in the handoff.
3. Ask `test_author` to establish acceptance scenarios and meaningful failing evidence before assigning an implementor. Select `rust_implementor`, `python_implementor`, or `web_implementor` by module; use multiple specialists only for actual cross-language work.
4. Assign bounded file ownership. Serialize overlapping changes and shared-contract edits. Keep at most three subagents active, release idle threads, and wait for every required result. Do not delegate recursively through orchestrators.
5. Route implementation to `architect` for conformance, then `delivery` for a candidate commit. Route that commit to `technical_documentor`; have delivery commit documentation updates.
6. Send the final candidate SHA and base to `verifier`, then give its report and the committed diff to `code_reviewer`. Route findings to the responsible specialist or test author; never weaken tests to obtain a pass. Have delivery commit fixes and repeat affected checks plus review on the final SHA.
7. Give delivery the final architecture, documentation, verification, and review reports. Only request push and PR creation when all required gates pass. Return PR URL, final SHA, evidence summary, and limitations; if blocked, return the precise failed gate and next action.

Treat implementation, compilation, observed behavior, calibration, and scientific acceptance as distinct evidence. Preserve the project scientific safeguards at every handoff. Do not claim the scenario model is validated because the workflow succeeds.
