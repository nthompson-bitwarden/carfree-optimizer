#!/usr/bin/env bash
# E01-S02: verify prerequisites and run a locked workspace smoke test.
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
for dependency in rustup python3; do
    if ! command -v "$dependency" >/dev/null 2>&1; then
        echo "Missing $dependency; install it before running scripts/bootstrap.sh (Rust: https://rustup.rs; Python >=3.11)." >&2
        exit 1
    fi
done
python3 -c 'import tomllib' || { echo 'Python >=3.11 is required (tomllib).' >&2; exit 1; }
toolchain=$(python3 -c 'import tomllib; print(tomllib.load(open("rust-toolchain.toml", "rb"))["toolchain"]["channel"])')
if ! rustup run "$toolchain" rustc --version; then
    echo "Install the pinned toolchain: rustup toolchain install $toolchain --profile minimal --component rustfmt --component clippy" >&2
    exit 1
fi
rustup run "$toolchain" cargo fmt --version
rustup run "$toolchain" cargo clippy --version
python3 scripts/check-dependencies.py
rustup run "$toolchain" cargo test --workspace --locked --offline
echo 'Bootstrap smoke test passed.'
