# Reproducible development

This setup implements E01-S02 and the infrastructure gates for E01-S04. It needs
Bash, Git (for benchmark revision metadata), rustup, and Python 3.11 or newer
with standard-library `tomllib`, the Protocol Buffers compiler (`protoc`),
and a C toolchain for the Parquet Zstd codec. Core bootstrap needs no Python
packages; E14 training checks additionally use pinned Gymnasium/NumPy. The Rust toolchain is pinned to 1.97.1 in `rust-toolchain.toml`;
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

For E14 changes, also install `training/requirements.txt` in a virtual environment,
build the real Rust batch/experiment examples and run the actual Gymnasium,
transport, profiling-schema and held-out-isolation checks (no optional skips):

```sh
python3 -m venv .codex/run/e14-venv
.codex/run/e14-venv/bin/python -m pip install -r training/requirements.txt
rustup run 1.97.1 cargo build --locked --offline -p optimizer-bridge --examples
.codex/run/e14-venv/bin/python -m compileall -q training
PYTHONPATH=training .codex/run/e14-venv/bin/python -m unittest discover -s training/tests -v
rustup run 1.97.1 cargo test --locked --offline -p optimizer-bridge --example e14_experiments
```

E16 additionally requires the actual Linux quota/remote-worker launch-path suite
(no silent skips). On a 64-bit Linux worker, after building the examples:

```sh
PYTHONPATH=training python3 training/tests/e16_linux_acceptance.py
```

On other hosts, run the control-plane tests locally and this suite on Linux.
See [experiment job contracts, demo and limitations](EXPERIMENT-JOBS.md).

See [E14 schemas, experiments and measured profiling](RL-ENVIRONMENT.md) and
[ADR 0005](adr/0005-bounded-rl-process-binding.md). Python remains outside domain
calculations and traveler hot paths.

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
foundation suite, followed by E14 dependency installation, actual Rust-example
build, Python syntax and Gymnasium/experiment checks. Workflow configuration alone does not prove the job passed:
delivery evidence must identify the actual candidate SHA and runner result.
Push events do not trigger CI, avoiding duplicate runs when an open PR is updated.
This also omits automatic post-merge checks on `main`; use manual dispatch when a
run is needed outside a PR.

For a human-reviewable developer demo, run bootstrap and then the two benchmark
commands in [BENCHMARKS.md](BENCHMARKS.md). No UI or running application is
introduced by this milestone. Module ownership is in
[MODULE-CONTRACTS.md](MODULE-CONTRACTS.md); signed issue delivery follows
[AGENT-WORKFLOW.md](AGENT-WORKFLOW.md).

E20 adds a macOS smoke, a Linux container smoke, and independent advisory
scans for the locked Rust and Python training dependencies. See
[release verification and recovery](RELEASE-VERIFICATION.md) for reproduction,
restore checks and scientific evidence.
