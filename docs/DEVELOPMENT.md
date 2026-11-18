# Reproducible development

This setup implements E01-S02 and the infrastructure gates for E01-S04. It needs
Bash, Git (for benchmark revision metadata), rustup, and Python 3.11 or newer
with standard-library `tomllib`, the Protocol Buffers compiler (`protoc`),
and a C toolchain for the Parquet Zstd codec. No Python packages are required. The Rust toolchain is pinned to 1.97.1 in `rust-toolchain.toml`;
`Cargo.lock` is committed. Install once, then run the one-command setup:

```sh
rustup toolchain install 1.97.1 --profile minimal --component rustfmt --component clippy
rustup run 1.97.1 cargo fetch --locked
./scripts/bootstrap.sh
```

Bootstrap resolves its repository root from its own path, checks rustup/Python,
verifies the pinned compiler and fmt/clippy components, checks dependencies, and
runs `cargo test --workspace --locked --offline` through `rustup run`. Missing
tools fail with installation guidance. It does not install tools, download city
data, start services, run training, or recursively invoke the foundation suite.
Use rustup explicitly if another installed Cargo shadows its toolchain shims.

Run the full local checks from the repository root:

```sh
rustup run 1.97.1 cargo fmt --all --check
rustup run 1.97.1 cargo clippy --workspace --all-targets --locked -- -D warnings
rustup run 1.97.1 cargo test --workspace --locked
python3 scripts/check-dependencies.py
python3 -m unittest discover -s tests/foundations -v
```

The BDD-style foundation scenarios map to E01-S01–06 and check isolation,
bootstrap behavior and missing-tool errors, dependency rejection, benchmark
history/comparison failure handling, and contract/ADR presence. Structural checks
are explicitly infrastructure evidence; they do not validate scientific models.

The dependency policy checks declarations before resolving locked, offline Cargo
metadata. It allows the exact registry dependencies declared in ADR 0002 for
`mobility-ingest` plus their locked, checksummed crates.io resolutions. To
inspect another manifest, use
`python3 scripts/check-dependencies.py --manifest-path /path/to/Cargo.toml`.
External dependencies require reviewed ADR/benchmark rationale and an explicit
policy change. This is a source policy, not a vulnerability database audit.

The `Rust CI` job targets `ubuntu-24.04`, with Python 3.12 and
the Rust repository pin. Pull requests and manual dispatch install `protoc`,
fetch the locked dependencies, then run bootstrap, formatting,
clippy, workspace tests, dependency policy, and the
foundation suite. Workflow configuration alone does not prove the job passed:
delivery evidence must identify the actual candidate SHA and runner result.
Push events do not trigger CI, avoiding duplicate runs when an open PR is updated.
This also omits automatic post-merge checks on `main`; use manual dispatch when a
run is needed outside a PR.

For a human-reviewable developer demo, run bootstrap and then the two benchmark
commands in [BENCHMARKS.md](BENCHMARKS.md). No UI or running application is
introduced by this milestone. Module ownership is in
[MODULE-CONTRACTS.md](MODULE-CONTRACTS.md); signed issue delivery follows
[AGENT-WORKFLOW.md](AGENT-WORKFLOW.md).
