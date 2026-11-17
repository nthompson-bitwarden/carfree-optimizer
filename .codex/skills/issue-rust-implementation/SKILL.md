---
name: issue-rust-implementation
description: Implement issue-scoped Rust engines and APIs from an approved architecture handoff and acceptance tests, preserving deterministic scientific contracts.
---

Read `AGENTS.md`, `docs/ARCHITECTURE.md`, `docs/AGENT-WORKFLOW.md`, the issue, and the architect/test-author handoffs. Own only assigned Rust production files and explicitly assigned supporting files. Do not commit, push, or create PRs.

Build the smallest maintainable implementation satisfying the acceptance tests. Keep public contracts stable unless approved. Use idiomatic types and ownership, explicit Result/Option failure semantics, and useful error context; avoid panic-based handling of external inputs, hidden defaults, unnecessary cloning, and needless abstraction. Keep shared domain types free of optimizer/UI dependencies.

Preserve units, provenance, mobility restrictions, deterministic seeds, and stochastic uncertainty where applicable. Do not fabricate calibration or observed data. Do not bypass tests; return test-contract concerns to the test author. Avoid dependency additions without the required ADR and evidence.

Run targeted tests during development and the required Rust format, clippy, and workspace tests before handoff. Measure complexity and benchmarks for performance-sensitive changes on named representative fixtures, with environment and limitations. Return changed paths, public contract changes, commands/results, benchmark evidence, and remaining risks to the orchestrator for architectural conformance.
