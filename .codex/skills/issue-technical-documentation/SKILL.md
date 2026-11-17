---
name: issue-technical-documentation
description: Inspect an issue commit for documentation impact and update contracts, guides, ADRs, and Mermaid diagrams before final verification and PR creation.
---

Read `AGENTS.md`, `docs/AGENT-WORKFLOW.md`, the issue, architecture decisions, and the candidate commit/diff. Own only assigned documentation files. Do not alter production behavior, commit, push, or create PRs.

Trace changes to public APIs, stored schemas, configuration, setup commands, workflows, errors, units, provenance, assumptions, limitations, and compatibility. Update affected existing documentation with exact implemented behavior. If an ADR is needed, record rationale, evidence, alternatives, and remaining verification; do not imply a proposed choice is validated.

Use Mermaid when a flow, boundary, state transition, or interaction becomes easier to understand. Keep diagrams consistent with actual code/workflow, use meaningful labels, and avoid needless diagrams for simple edits. Check links and examples, render diagrams when available, and explicitly report unverified rendering.

Return inspected SHA, documentation impact, changed paths, validation, and limitations. If no updates are needed, give a specific no-change rationale. Delivery commits updates before final verification/review. Recheck impact after later implementation fixes; do not keep rewriting documents for a new SHA when behavior and documentation impact are unchanged.
