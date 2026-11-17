#!/usr/bin/env python3
"""E01-S05: record real Rust primitive timings and compare compatible history.

Python only orchestrates validation/builds; Rust owns the timed computation.
Default output: benchmarks/history.jsonl. This synthetic microbenchmark does not
establish routing, simulation, FFI, training performance, or scientific fidelity.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "benchmarks/fixtures/reachable-share-v1.csv"
PIN = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
COMPATIBILITY = ("schema_version", "fixture", "fixture_sha256", "hardware", "toolchain", "build_profile", "iterations", "unit", "harness_sha256", "synthetic")


def command(*args):
    result = subprocess.run(args, cwd=ROOT, capture_output=True, text=True)
    if result.returncode:
        raise ValueError(f"{' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def positive(value):
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def read_history(path):
    if not path.exists():
        return []
    records = []
    for line_number, line in enumerate(path.read_text().splitlines(), 1):
        try:
            record = json.loads(line)
            if not isinstance(record, dict) or record.get("schema_version") != 1:
                raise ValueError("unsupported schema_version")
            if any(key not in record for key in COMPATIBILITY):
                raise ValueError("missing compatibility metadata")
            samples = record.get("samples")
            if not isinstance(samples, list) or not samples or any(isinstance(x, bool) or not isinstance(x, (int, float)) or not math.isfinite(x) or x <= 0 for x in samples):
                raise ValueError("samples must contain finite positive timings")
            if record.get("median") != statistics.median(samples):
                raise ValueError("median must match raw samples")
            if not isinstance(record.get("iterations"), int) or record["iterations"] <= 0:
                raise ValueError("iterations must be positive")
            records.append(record)
        except (ValueError, TypeError) as error:
            raise ValueError(f"Invalid history {path}:{line_number}: {error}") from error
    return records


def hardware():
    cpu = "unknown"
    model = "unknown"
    memory = "unknown"
    if sys.platform == "darwin":
        try:
            details = json.loads(command("/usr/sbin/system_profiler", "SPHardwareDataType", "-json"))["SPHardwareDataType"][0]
            # Persist only model metadata; never serial numbers or machine UUIDs.
            cpu = details.get("chip_type", details.get("cpu_type", "unknown"))
            model = details.get("machine_model", "unknown")
            memory = details.get("physical_memory", "unknown")
        except ValueError:
            pass  # Unknown identity is explicit and cannot support comparison.
    elif Path("/proc/cpuinfo").exists():
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    return {"os": platform.system(), "os_release": platform.release(), "architecture": platform.machine(), "cpu": cpu, "model": model, "cpu_count": os.cpu_count(), "memory": memory}


def benchmark(args):
    history = read_history(args.output)
    baseline = read_history(args.baseline) if args.baseline else []
    if args.baseline and not baseline:
        raise ValueError(f"Baseline contains no records: {args.baseline}")
    record = {
        "schema_version": 1,
        "fixture": "reachable-share-v1",
        "fixture_sha256": digest(FIXTURE),
        "synthetic": True,
        "hardware": hardware(),
        "toolchain": command("rustup", "run", PIN, "rustc", "-vV"),
        "revision": command("git", "rev-parse", "HEAD"),
        "working_tree_dirty": bool(command("git", "status", "--porcelain")),
        "build_profile": "release",
        "iterations": args.iterations,
        "unit": "ns/sample",
        "harness_sha256": digest(ROOT / "benchmarks/harness/src/main.rs"),
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
    }
    if baseline:
        if record["hardware"]["cpu"] == "unknown":
            raise ValueError("Incompatible baseline: CPU identity unavailable; cannot verify hardware comparability")
        differences = [key for key in COMPATIBILITY if baseline[-1][key] != record[key]]
        if differences:
            raise ValueError("Incompatible baseline: " + ", ".join(differences))
    command("rustup", "run", PIN, "cargo", "build", "--release", "--locked", "--offline", "-p", "foundations-benchmark")
    metadata = json.loads(command("rustup", "run", PIN, "cargo", "metadata", "--no-deps", "--format-version", "1", "--locked", "--offline"))
    binary = Path(metadata["target_directory"]) / "release" / "foundations-benchmark"
    record["samples"] = [int(value) for value in command(str(binary), str(FIXTURE), str(args.samples), str(args.iterations)).splitlines()]
    if len(record["samples"]) != args.samples or any(value <= 0 for value in record["samples"]):
        raise ValueError("Harness returned invalid sample timings")
    record["median"] = statistics.median(record["samples"])
    if baseline:
        record["baseline_median"] = baseline[-1]["median"]
        record["median_ratio"] = record["median"] / baseline[-1]["median"]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    # Validate all existing lines before opening for append; preserve prior bytes.
    if history and not args.output.read_bytes().endswith(b"\n"):
        raise ValueError("History must end in newline before appending")
    with args.output.open("a") as output:
        output.write(json.dumps(record, sort_keys=True, allow_nan=False) + "\n")
    print(f"Recorded {args.samples} samples to {args.output}; median {record['median']} {record['unit']}")
    if baseline:
        print(f"Median ratio to compatible baseline: {record['median_ratio']:.4f} (no pass/fail threshold)")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "benchmarks/history.jsonl")
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--samples", type=positive, default=7)
    parser.add_argument("--iterations", type=positive, default=100000)
    args = parser.parse_args()
    try:
        benchmark(args)
    except (OSError, ValueError, KeyError) as error:
        print(f"Benchmark: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
