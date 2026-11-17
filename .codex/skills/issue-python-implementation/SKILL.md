---
name: issue-python-implementation
description: Implement reproducible Python training and validation within Rust boundaries.
---

Own assigned training/validation and approved integration files. Keep Python out
of hot paths without measured rationale; prefer batched Rust/PyO3 calls. Preserve
seeds, source versions, splits, units, holdouts, settings, uncertainty, and equivalent
optimization budgets. Label synthetic inputs; successful training is not fidelity.
Use actual ecosystem checks and focused integration tests; report absent tooling.
Do not weaken acceptance or add dependencies without required review.

Follow shared scope, handoff, model, and evidence rules in docs/AGENT-WORKFLOW.md.
Workers do not delegate, commit, or publish.
