#!/usr/bin/env python3
"""E01-S04: enforce the current path-only dependency policy, without downloads.

External crates require an ADR, benchmark rationale, and explicit policy update.
This policy check is not a vulnerability audit of future external dependencies.
"""
import argparse
import glob
import json
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
PIN = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]


def dependency_tables(manifest):
    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
        yield manifest.get(key, {})
    for target in manifest.get("target", {}).values():
        yield from dependency_tables(target)


def check(path):
    manifest = tomllib.loads(path.read_text())
    paths = {path}
    for member in manifest.get("workspace", {}).get("members", []):
        paths.update(Path(p) / "Cargo.toml" for p in glob.glob(str(path.parent / member)))
    for item in sorted(paths):
        document = tomllib.loads(item.read_text())
        tables = list(dependency_tables(document))
        tables.append(document.get("workspace", {}).get("dependencies", {}))
        for table in tables:
            for name, spec in table.items():
                if not isinstance(spec, dict) or not ("path" in spec or spec.get("workspace") is True) or any(k in spec for k in ("git", "registry", "registry-index")):
                    raise ValueError(f"{item}: dependency {name!r} violates path-only policy; external dependencies require reviewed ADR and policy update")
    result = subprocess.run(["rustup", "run", PIN, "cargo", "metadata", "--format-version", "1", "--locked", "--offline", "--manifest-path", str(path)], text=True, capture_output=True)
    if result.returncode:
        raise ValueError(f"Cargo metadata failed: {result.stderr.strip()}")
    metadata = json.loads(result.stdout)
    for package in metadata["packages"]:
        if package.get("source") is not None:
            raise ValueError(f"dependency {package['name']} violates path-only policy: {package['source']}")
    print(f"Dependency policy passed: {len(metadata['packages'])} local packages, no registry/git sources.")


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
