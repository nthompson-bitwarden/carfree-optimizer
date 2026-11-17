---
name: issue-delivery
description: Prepare an issue branch, commit scoped changes, and push and create an issue-linked PR after this project's verification and review gates pass.
---

Read `AGENTS.md` and `docs/AGENT-WORKFLOW.md`. You own Git mutations and PR delivery; other agents return work and reports to you.

## Prepare

- Resolve the issue's repository from authenticated GitHub tools and the local remote. Confirm the issue and destination agree. Prefer connected tools; use Git/gh where a connector lacks a needed capability. Do not create a remote, repository, or issue implicitly.
- Inspect worktree and index, current branch, remotes, and existing issue PRs. Fetch `origin` and resolve the actual base SHA. Default to a new `codex/issue-<number>-<slug>` branch from `origin/main`. Respect an explicitly supplied base or branch; never substitute local main for a missing remote base.
- Preserve unrelated staged, modified, and untracked files. If they conflict with the assignment, create an isolated worktree on the issue branch rather than stashing or resetting user work. If the intended branch already exists, inspect its ancestry and associated PR; reuse only an established continuation of this issue. Otherwise report the collision.
- Missing origin, base, authentication, or issue identity blocks delivery. Do not fabricate a successful fetch. An explicitly authorized initial local commit in an unborn repository is a bootstrap exception, not a substitute for future issue branching.

## Commit and gates

- For new project commits, author and committer must both be the repository-local Git identity associated with `nthompson-bitwarden`, using GitHub account `nthompson-bitwarden`. Check effective identities with `git var GIT_AUTHOR_IDENT` and `git var GIT_COMMITTER_IDENT`; environment variables and author/committer overrides can supersede `user.*`. Configure identity locally, never silently change global configuration or use a machine-derived email. Preserve legitimate third-party authorship when integrating existing commits.
- Sign every commit explicitly with `git commit -S` (including merges and amendments) using the user's registered signing key. Never disable signing, substitute an agent's key, or treat a `Signed-off-by` trailer as a signature. Missing identity, key, or signing capability blocks the commit.
- Never use Codex, OpenAI, a bot, or any other coding agent as author, committer, or co-author. Do not add agent attribution trailers or generated-by footers to commit messages. Inspect the final message and identity metadata before delivery.
- After each commit, check its author, committer, message, and signature with `git show --format=fuller --show-signature` and `git verify-commit <SHA>`. A local signature pass alone does not prove GitHub verification.
- Receive the implementation and architecture conformance report. Inspect the diff; stage explicit issue-owned paths and review the entire index before committing. Explicit staging does not exclude unrelated entries already in the index: never make a plain commit containing them. Prefer an isolated worktree with a clean index; for an established continuation, use `git commit --only -- <owned paths>` only after confirming those paths contain no unrelated edits, and verify unrelated staged entries remain intact afterward. Never use broad staging when unrelated work exists. Use a descriptive story-linked commit message.
- Return the candidate SHA to the orchestrator. The technical documentor inspects this commit; commit any required documentation changes before final verification and review.
- Require architecture conformance and documentation disposition, verifier evidence for every required acceptance criterion/check, and code review of the final committed diff. Reports identify the final SHA. Any FAIL, BLOCKED, NOT RUN required check or unresolved blocking reviewer finding blocks a ready PR. Record optional suggestions separately.
- Findings go to the responsible role. Commit fixes without rewriting published history, then require fresh review and affected verification. A changed SHA must never inherit an unexplained stale pass; unchanged checks may be carried forward only with verifier justification and exact source SHA.

## Push and PR

- A user request to deliver an issue through this workflow authorizes its scoped commit, push, and PR. Respect narrower requests such as local commit only and the session's permissions; do not add redundant approval prompts.
- Recheck destination, current HEAD, scoped diff, and gate reports immediately before pushing. Use a normal push with upstream to the issue branch; never force-push or push to main by default.
- After pushing, query GitHub's commit API for every newly delivered commit. Require `commit.verification.verified: true` and `reason: valid`, and confirm author/committer account attribution for new project commits is `nthompson-bitwarden`. Check that the remote branch and PR head equal the verified candidate SHA. Record evidence; an unverified commit blocks a ready PR and any claim of completed delivery. Diagnose `no_user` as an email-association problem and `unknown_key` as an unregistered signing key; never bypass the gate. Repairing published history requires explicit user authorization and an exact-SHA `--force-with-lease`.
- Create a ready PR targeting the resolved base branch, or update the existing PR for the same repository and head branch. Use `Closes #<number>` only for completed issue scope. Include the evidence format in `docs/AGENT-WORKFLOW.md`, architecture/review/documentation results, performance results where applicable, and limitations. When using gh, write actual multiline text to a temporary body file and pass `--body-file`.
- If a push or PR request has an uncertain result, inspect the remote branch and existing PR before retrying. Report partial success truthfully, including branch URL/SHA if push succeeded but PR creation failed. Never merge, publish releases, or send unrelated external messages.
