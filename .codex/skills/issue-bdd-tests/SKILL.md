---
name: issue-bdd-tests
description: Write deterministic acceptance scenarios and meaningful failing evidence.
---

Primary owns tests by default; optional test_author owns assigned fixtures/tests.
Map criteria to observable outcomes at the lowest useful layer. Use Given/When/Then
names/comments and story IDs without adding a BDD framework. Label synthetic data,
seeds, units, geography/date metadata. Assert independent expected results; avoid
self-comparisons, swallowed exceptions, blanket skips, and implementation mirrors.
UI branching belongs in page/component objects. Establish red evidence where
behavior is new; compilation failure is BLOCKED behavioral evidence. Existing
behavior may start green. For config/docs use parser/discovery/scenario checks.
Never relax assertions just to pass; return contract concerns to the primary.

Follow shared scope, handoff, model, and evidence rules in docs/AGENT-WORKFLOW.md.
Workers do not delegate, commit, or publish.
