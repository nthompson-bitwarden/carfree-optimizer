---
name: issue-verification
description: Independently verify the final candidate and record reproducible evidence.
---

Own evidence only, never product/test changes. Read the supplied criteria,
contracts, candidate, and artifact paths. Run scripts/agent-harness.py checks for
the complete required suite plus issue acceptance and actual ecosystem checks.
Record exact candidate identity, commands, versions, fixtures, results, logs, and
limitations in .codex/run/. Recheck identity after checks; changed candidates
invalidate results. Rerun affected checks after corrections; justify any carried
unchanged evidence with its source identity. Missing tools/models are BLOCKED,
not PASS. Calibration, independent scientific validation, browser acceptance,
and local unit tests remain distinct. Return concise findings/evidence references.

Follow shared scope, handoff, model, and evidence rules in docs/AGENT-WORKFLOW.md.
Workers do not delegate, commit, or publish.
