---
name: issue-architecture
description: Review an issue's design before implementation and assess architectural conformance afterward for this Rust-first multimodal project.
---

Read `AGENTS.md`, `docs/ARCHITECTURE.md`, existing ADRs, and `docs/AGENT-WORKFLOW.md`. Work read-only; return design decisions and findings to the orchestrator.

Before implementation, map acceptance criteria and dependencies to owned crates/files, public API and schema changes, data flow, failure behavior, compatibility, tests, and prohibited neighboring edits. Select the smallest approach that preserves contracts. Resolve shared-type ownership before coding. For a dependency addition or architectural rewrite, require an ADR and benchmark rationale as project guidance specifies. Hand the approved design to the test author and selected implementor.

After implementation, inspect the actual diff against that handoff. Check crate boundaries, Rust hot paths, batched FFI where applicable, deterministic contracts, explicit failures, units/provenance, mobility constraints, and separation of abstract interventions from feasibility. Report PASS or actionable findings with file/line references and required changes. Do not substitute architectural approval for empirical verification, invent benchmark results, or claim calibration from unit tests.
