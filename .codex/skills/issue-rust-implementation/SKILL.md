---
name: issue-rust-implementation
description: Implement assigned Rust engines and APIs with stable scientific contracts.
---

Primary implements by default; optional specialist owns only assigned Rust files.
Read relevant module contracts and approved design/tests. Use explicit Result/Option
errors, preserve deterministic seeds, units, provenance, and mobility constraints.
Avoid panic-based external-input handling, hidden defaults, unnecessary cloning,
and coupling domain types to UI/optimizer. No dependencies without ADR/evidence.
Run focused checks; measure complexity and representative benchmarks for hot paths.
Return contract concerns instead of weakening acceptance tests.

Follow shared scope, handoff, model, and evidence rules in docs/AGENT-WORKFLOW.md.
Workers do not delegate, commit, or publish.
