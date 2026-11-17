---
name: issue-web-implementation
description: Implement issue-scoped TypeScript and Svelte map workflows with accessible interactions, explicit errors, and observable acceptance tests.
---

Read `AGENTS.md`, `docs/ARCHITECTURE.md`, `docs/AGENT-WORKFLOW.md`, the issue, and architecture/test-author handoffs. Own only assigned web files and approved API integration boundaries. Do not commit, push, or create PRs.

Use small typed components and clear state/data flow. Keep authoritative scientific calculations in Rust/API contracts. Handle loading, empty, error, and stale-result states explicitly; do not turn request failures into empty successful results. Preserve units, source/date metadata, uncertainty, and accessibility constraints in user-facing claims.

Implement semantic, keyboard-operable controls and accessible alternatives to map-only information. Share repeated UI interactions and branching through appropriate component/page objects; avoid unnecessary abstractions and dependencies.

Meet acceptance tests without weakening them. Run actual web type/lint/test/build commands when introduced, plus browser evidence and accessibility checks for affected user flows. If tooling is absent, report it and coordinate the minimum issue-required setup with architect/test author. Return changed paths, API changes, commands/results, screenshots or reproducible demo steps, and limitations for conformance review.
