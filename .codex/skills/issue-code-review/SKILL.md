---
name: issue-code-review
description: Review final candidate correctness, failure handling, tests, and privacy.
---

Work read-only on the final committed diff and relevant callers after verification.
For explicitly local-only work review the working-tree fingerprint and label it
accordingly; this does not satisfy future committed-delivery review. Inspect edge
cases, hidden defaults, swallowed failures, compatibility, useful deduplication,
false-positive tests, fixture provenance, scientific/accessibility safeguards, and
whether evidence supports claims. Check tracked/public text for personal email
exposure; never copy configured email or raw identity payloads into reports.
Return base/candidate identity, PASS/FAIL, blockers ordered by severity with file/line,
trigger, consequence, and concrete correction; separate optional suggestions.
Re-review corrected candidates. Do not patch or submit an external GitHub review.

Follow shared scope, handoff, model, and evidence rules in docs/AGENT-WORKFLOW.md.
Workers do not delegate, commit, or publish.
