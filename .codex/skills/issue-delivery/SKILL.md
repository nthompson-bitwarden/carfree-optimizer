---
name: issue-delivery
description: Prepare scoped signed Git delivery after independent acceptance gates.
---

The primary owns Git mutations and publication. An optional delivery worker returns
read-only advice. Read docs/AGENT-DELIVERY.md when delivery is authorized; it holds
branching, scoped staging, identity, signing, GitHub verification, privacy, and PR
failure handling. Never bypass required gates or narrower local-only scope.
Report identity/signature status without configured email or raw identity payloads.
Public PR copy contains only concise changes, material limitations, and relevant
lint/test results; keep internal gate and identity evidence private.

Follow shared scope, handoff, model, and evidence rules in docs/AGENT-WORKFLOW.md.
Workers do not delegate, commit, or publish.
