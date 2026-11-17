---
name: issue-bdd-tests
description: Author deterministic Given/When/Then acceptance tests before implementation, with meaningful failing evidence and protection against false positives.
---

Read the issue, approved architecture handoff, `AGENTS.md`, and `docs/AGENT-WORKFLOW.md`. Own assigned test and fixture files; do not implement production behavior or commit changes.

- Map each acceptance criterion to externally observable behavior at the lowest useful test layer. Use Given/When/Then test names or comments in the existing framework; do not add a BDD dependency solely for syntax. Link scenarios to the story ID.
- Cover the happy path, boundary/invalid inputs, explicit errors, and relevant access constraints. Use synthetic fixtures clearly labeled as synthetic; keep seeds, units, geography/date metadata, and expected outputs explicit where relevant. Never present fixtures as observed city inputs.
- Assert outcomes independently of the implementation. Avoid self-comparisons, unasserted mocks, swallowed exceptions, broad success matching, unconditional skips, and implementation-mirroring tests. Shared UI interactions and branching belong in page/component objects when introduced.
- Run the scenarios before implementation. Record the exact command, expected behavior, actual failure, and test IDs. A missing-symbol compilation failure is a BLOCKED behavioral run, not proof that behavior failed. After the interface exists, rerun to establish behavioral red/green evidence. Existing behavior may correctly start green; identify the new regression scenario rather than forcing failure.
- For documentation/configuration-only changes, prefer parser/discovery checks and handoff exercises instead of artificial red production tests. Explain that disposition.
- Return acceptance mapping, fixtures, failing evidence, and implementor command. If a test contract must change, review the issue and architecture decision with the orchestrator; never relax an assertion just to make the implementation pass.
