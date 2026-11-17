# E01-S03 agent setup validation

Initial local setup, 2026-11-16 (America/New_York). No GitHub issue or remote was supplied. This records setup validation, not an issue delivery receipt or simulation acceptance. The initial scaffold and configuration are authorized together for the first local commit.

Rust checks used rustc 1.98.1 (48a229cea) and cargo 1.98.1 (797e8a9bc) on macOS. Performance benchmarks are not applicable to the agent configuration or formatting/manifest-only scaffold corrections.

## Acceptance and ownership

E01-S03 requires tasks to identify owned files, public APIs, tests, and prohibited cross-module changes; its epic has no dependencies. The workflow's handoff format and role skills provide these fields. Configuration and documentation are owned by this setup. Production API behavior is unchanged. Existing Rust scaffold fixes are limited to rustfmt and moving two local crate dependencies from package metadata into dependency tables. Cargo generated the workspace lockfile.

## Results

| Check | Status | Evidence |
| --- | --- | --- |
| Project configuration and profiles | PASS | Python tomllib parsed project config and ten unique profiles; required fields, role/skill references, ten discovery symlinks, and local documentation links resolved |
| Skill frontmatter/scaffold validator | PASS | Skill creator `quick_validate.py` passed for all ten skills using temporary Python 3.12/PyYAML tooling; no project Python dependency added |
| Fresh Codex configuration load | PASS | Codex CLI 0.169.0 app-server `config/read` loaded the trusted project layer with agents enabled and concurrency 3; isolated temporary CODEX_HOME, no user config changed |
| Fresh skill discovery | PASS | App-server `skills/list` returned all ten enabled project skills without errors at repository root and inside crates/mobility-graph |
| Fresh custom-agent discovery | PASS | `codex debug prompt-input` included all ten custom role names in model-visible collaboration tool definitions using the isolated trusted configuration |
| Workflow forward testing | PASS | Independent read-only agent exercised the six scenarios below; two contract findings were corrected before commit |
| Rust formatting | PASS | `cargo fmt --all --check` |
| Rust lint | PASS | `cargo clippy --workspace --all-targets -- -D warnings` |
| Rust tests | PASS | `cargo test --workspace`: four existing unit tests passed; all workspace doc-test runs passed |
| Live issue implementation / push / PR | NOT RUN | This bootstrap request authorizes a local initial commit; no origin exists |
| Mermaid rendering | NOT RUN | Diagram source inspected; no renderer invoked |

The independent forward test covered failed required verification, self-asserting tests found during review, documentation follow-up commits, overlapping unrelated staged work, missing origin/main, and uncertain PR creation with an existing PR. Contracts route each case to its owner or explicit blocker. These are tabletop exercises, not remote mutation tests. Corrections make verification precede review so its report is available, and distinguish optional suggestions from blocking review findings.

No BDD framework or artificial red production tests were added for this configuration-only workflow. Parser checks, fresh runtime discovery, and scenario exercises provide the relevant evidence. Individual custom roles have not each executed a live issue; discovery does not establish end-to-end delivery success. Postcommit review results belong in the delivery report, avoiding a self-referential commit hash in this file.
