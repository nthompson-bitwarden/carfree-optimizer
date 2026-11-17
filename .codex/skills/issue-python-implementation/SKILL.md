---
name: issue-python-implementation
description: Implement issue-scoped Python training and validation code with reproducible experiments and clear Rust integration boundaries.
---

Read `AGENTS.md`, `docs/ARCHITECTURE.md`, `docs/AGENT-WORKFLOW.md`, the issue, and architecture/test-author handoffs. Own only assigned training/validation and approved integration files. Do not commit, push, or create PRs.

Use small typed interfaces, explicit exception handling, validated inputs, and tests of observable outputs. Keep Python in training/validation unless measurements and an approved design support a hot-path exception. Prefer batched Rust/PyO3 calls to per-trip FFI. Preserve public contracts and avoid new dependencies without required review.

Record seeds, dataset/source versions, splits, units, experiment settings, and uncertainty as appropriate. Avoid holdout leakage and compare optimization methods on equivalent budgets and fixtures. Synthetic fixtures must be labeled; never invent observations or claim fidelity from successful training.

Satisfy acceptance tests without weakening them. Use the repository's actual Python checks once introduced; if no framework exists, have the architect and test author define minimal checks for the issue rather than claiming nonexistent checks passed. Run Rust checks when Rust contracts change. Return paths, API changes, commands/results, reproducibility/performance evidence, and limitations for conformance review.
