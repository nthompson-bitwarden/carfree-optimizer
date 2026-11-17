---
name: issue-code-review
description: Review a committed issue diff before PR creation for maintainability, silent failures, quality, false positive tests, and useful deduplication.
---

Read `AGENTS.md`, `docs/AGENT-WORKFLOW.md`, the issue, architecture handoff, final candidate diff, and verification report. Review read-only after delivery commits; do not patch, commit, push, or submit an external GitHub review.

Inspect the whole issue diff and relevant callers. Check correctness, clarity, maintainability, failure propagation, hidden defaults, swallowed errors, edge cases, compatibility, and public-contract consistency. Identify duplication only when consolidation clarifies ownership or prevents drift; avoid abstraction for its own sake.

Inspect tests for false positives, assertions coupled to implementation, excessive mocking, skipped paths, fixture provenance, and missing negative cases. Check that evidence actually supports the claimed result and that scientific/accessibility safeguards are preserved.

Return the reviewed base/candidate SHA, PASS or FAIL, and findings ordered by severity. Each finding needs file/line, triggering condition, consequence, and concrete correction or proof needed. Distinguish blockers from optional suggestions. No findings is a review outcome, not proof of complete feature acceptance. Send findings to the orchestrator and delivery; require blocking findings to be resolved and the final committed diff re-reviewed before PR creation.
