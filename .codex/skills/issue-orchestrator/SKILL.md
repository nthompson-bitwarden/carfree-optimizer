---
name: issue-orchestrator
description: Coordinate issue delivery in the primary session with independent review and verification.
---

Use docs/AGENT-WORKFLOW.md as the shared source. Remain in the primary session;
never spawn a coordinator. Own intake, dependency/acceptance mapping, tests,
implementation, documentation, and delivery. Reuse architect and verifier serially,
without conversation history; extra specialists need a concrete cross-module need.
Keep task state/evidence under ignored .codex/run/. Use focused checks during edits;
independent final checks and review gate publication. Respect local-only scope.
Read docs/AGENT-DELIVERY.md only when Git delivery is authorized.

Follow shared scope, handoff, model, and evidence rules in docs/AGENT-WORKFLOW.md.
Workers do not delegate, commit, or publish.
