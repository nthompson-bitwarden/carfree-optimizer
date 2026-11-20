# Contributing

Use Bash, Git, Python 3.11 or newer, a C toolchain, and the Protobuf compiler.
On Ubuntu install `build-essential` and `protobuf-compiler`. On macOS install
the Xcode command-line tools and use `brew install protobuf`. Install Rust
with rustup, then run these commands from the checkout.

```sh
rustup toolchain install 1.97.1 --profile minimal --component rustfmt --component clippy
cargo fetch --locked
./scripts/bootstrap.sh
cargo test --locked --offline -p mobility-api --test e20_import_score
cargo build --locked --offline -p mobility-api --bin spatial-workbench
python3 tests/acceptance/e20_workspace.py target/debug/spatial-workbench
cargo run --locked --offline -p mobility-api --bin spatial-workbench
```

Open the printed loopback URL and follow the [workbench demo](web/README.md).
The scripted demo creates disposable projects and exercises restore. It needs
no account, credentials, or transport-data download.

The [MIT License](LICENSE) covers the repository code and repository-authored
synthetic E04 and workbench fixtures. The [Census fixture manifest](fixtures/geography/README.md)
describes boundary provenance and reproduction. Preserve per-source license,
date, attribution, units, geographic scope and observed or modeled status in
any contribution. Use redistributable fixtures and keep private inputs and
credentials outside the repository.

Pick a story from the [backlog](backlog/car_independence_backlog.md). In a pull
request, name the story, describe how to reproduce the acceptance checks, and
report material limitations. Follow [development checks](docs/DEVELOPMENT.md)
and preserve the [module contracts](docs/MODULE-CONTRACTS.md).
