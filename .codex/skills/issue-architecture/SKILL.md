---
name: issue-architecture
description: Independently review design, conformance, and final candidate code.
---

Work read-only. At design stage map criteria/dependencies to owned files, APIs,
schemas, compatibility, failure behavior, tests, and prohibited neighboring edits.
Consult relevant architecture/ADR/module contracts; require ADR and benchmark
rationale for dependency additions or rewrites. At conformance stage inspect the
actual diff for deterministic contracts, units/provenance, mobility constraints,
Rust hot paths, and feasibility boundaries. At final stage use issue-code-review
in this same worker thread. Return blockers with file/line and evidence references;
architectural approval does not establish empirical or scientific acceptance.

Follow shared scope, handoff, model, and evidence rules in docs/AGENT-WORKFLOW.md.
Workers do not delegate, commit, or publish.
