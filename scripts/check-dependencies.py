#!/usr/bin/env python3
"""Check the locked, offline dependency policy (E01-S04 and ADR 0002)."""
import argparse
import glob
import json
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PIN = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]


REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
INGEST = ROOT / "crates/mobility-ingest/Cargo.toml"
INGEST_DECODERS = {
    "chrono": {"version": "=0.4.45", "default-features": False, "features": ["std"]},
    "chrono-tz": {"version": "=0.10.4", "default-features": False},
    "csv": "=1.4.0",
    "gtfs-realtime": "=0.2.0",
    "parquet": {
        "version": "=59.3.3", "default-features": False,
        "features": ["snap", "zstd", "flate2-rust_backend"],
    },
    "prost": "=0.14.3",
    "quick-xml": "=0.41.0",
    "serde": {"version": "=1.0.228", "features": ["derive"]},
    "serde_json": "=1.0.149",
    "sha2": "=0.11.1",
    "tempfile": "=3.27.0",
}


API = ROOT / "crates/mobility-api/Cargo.toml"
API_WORKSPACE = {
    "getrandom": "=0.4.3",
    "serde_json": {"version": "=1.0.149", "features": ["raw_value"]},
    "parquet": {"version": "=59.3.3", "default-features": False},
    **{name: INGEST_DECODERS[name] for name in ("serde", "sha2", "tempfile")},
}


def dependency_tables(manifest):
    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
        yield manifest.get(key, {})
    for target in manifest.get("target", {}).values():
        yield from dependency_tables(target)


def check_manifest(item, allow_first_party):
    document = tomllib.loads(item.read_text())
    if "patch" in document or "replace" in document:
        raise ValueError(f"{item}: dependency source overrides are not allowed")
    approved = {INGEST: INGEST_DECODERS, API: API_WORKSPACE}.get(item, {}) if allow_first_party else {}
    direct = document.get("dependencies", {})
    tables = list(dependency_tables(document))
    tables.append(document.get("workspace", {}).get("dependencies", {}))
    for table in tables:
        for name, spec in table.items():
            if table is direct and name in approved and spec == approved[name]:
                continue
            if (
                not isinstance(spec, dict)
                or not ("path" in spec or spec.get("workspace") is True)
                or any(k in spec for k in ("git", "registry", "registry-index"))
            ):
                raise ValueError(
                    f"{item}: dependency {name!r} requires a reviewed ADR and policy update"
                )


def check(path):
    manifest = tomllib.loads(path.read_text())
    allow_ingest = path == ROOT / "Cargo.toml"
    paths = {path}
    for member in manifest.get("workspace", {}).get("members", []):
        paths.update(Path(p).resolve() / "Cargo.toml" for p in glob.glob(str(path.parent / member)))
    for item in sorted(paths):
        check_manifest(item, allow_ingest)

    result = subprocess.run(
        ["rustup", "run", PIN, "cargo", "metadata", "--format-version", "1",
         "--locked", "--offline", "--manifest-path", str(path)],
        text=True, capture_output=True,
    )
    if result.returncode:
        raise ValueError(f"Cargo metadata failed: {result.stderr.strip()}")
    metadata = json.loads(result.stdout)
    lock = {}
    if allow_ingest:
        lock = {
            (item["name"], item["version"], item.get("source")): item.get("checksum", "")
            for item in tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
        }
    local = registry = 0
    for package in metadata["packages"]:
        source = package.get("source")
        if source is None:
            local += 1
            item = Path(package["manifest_path"]).resolve()
            if item not in paths:
                check_manifest(item, allow_ingest)
            continue
        digest = lock.get((package["name"], package["version"], source), "")
        if not allow_ingest or source != REGISTRY or len(digest) != 64 or any(
            c not in "0123456789abcdef" for c in digest
        ):
            raise ValueError(f"dependency {package['name']} has an unapproved source or missing checksum")
        registry += 1
    print(f"Dependency policy passed: {local} local packages, {registry} locked crates.io packages.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest-path", type=Path, default=ROOT / "Cargo.toml")
    args = parser.parse_args()
    try:
        check(args.manifest_path.resolve())
    except (OSError, ValueError, KeyError) as error:
        print(f"Dependency check: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
