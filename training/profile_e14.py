"""E14-S06 process-batch profiling; synthetic fixture, never a fidelity claim.

Run one size per invocation so child peak RSS belongs to exactly one Rust server.
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import platform
import resource
import statistics
import subprocess
import time

from carfree_env import BatchProcess, RejectedAction, Transition

ROOT = Path(__file__).resolve().parents[1]


def source_digest():
    digest = hashlib.sha256()
    paths = sorted((ROOT / "crates").rglob("*.rs"))
    paths += sorted((ROOT / "crates").rglob("Cargo.toml"))
    paths += [ROOT / "Cargo.toml", ROOT / "Cargo.lock", ROOT / "rust-toolchain.toml", ROOT / "training/requirements.txt"]
    paths += sorted((ROOT / "training").glob("*.py"))
    for path in paths:
        digest.update(str(path.relative_to(ROOT)).encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
    return digest.hexdigest()


def percentile(values, fraction):
    return sorted(values)[max(0, __import__("math").ceil(len(values) * fraction) - 1)]


def profile(server: Path, size: int, samples: int):
    resets, steps = [], []
    with BatchProcess([str(server), str(size), "0"], size) as batch:
        for iteration in range(samples + 3):
            start = time.perf_counter_ns()
            states = batch.reset([42] * size)
            reset_ns = time.perf_counter_ns() - start
            actions = [next(i for i, allowed in enumerate(t.action_mask) if allowed) for t in states]
            start = time.perf_counter_ns()
            transitions = batch.step(actions)
            step_ns = time.perf_counter_ns() - start
            if any(isinstance(t, RejectedAction) for t in transitions):
                raise RuntimeError("profiling action rejected; do not report it as successful work")
            assert all(isinstance(t, Transition) for t in transitions)
            if iteration >= 3:
                resets.append(reset_ns)
                steps.append(step_ns)
    rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    rss_bytes = int(rss if platform.system() == "Darwin" else rss * 1024)
    # Query Git after RSS so the measurement includes only the one Rust child.
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    return {
        "schema_version": 1, "synthetic": True, "fixture": "e14-reference-topology-0",
        "seed": 42, "batch_size": size, "samples": samples, "warmup_batches": 3,
        "revision": revision, "source_sha256": source_digest(),
        "working_tree_dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)),
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "server_sha256": hashlib.sha256(server.read_bytes()).hexdigest(),
        "platform": platform.system(), "os_release": platform.release(),
        "architecture": platform.machine(), "python": platform.python_version(),
        "rustc": subprocess.check_output(["rustc", "--version"], cwd=ROOT, text=True).strip(),
        "cargo": subprocess.check_output(["cargo", "--version"], cwd=ROOT, text=True).strip(),
        "build_profile": server.parent.parent.name if server.parent.parent.name in ("release", "debug") else "unknown",
        "fixture_nodes": int(states[0].observation[2]), "fixture_edges": int(states[0].observation[3]),
        "observation_scalars": len(states[0].observation), "actions": len(states[0].action_mask),
        "clock_unit": "ns/batch roundtrip",
        "reset_raw_ns": resets, "step_raw_ns": steps,
        "reset_p50_ns": statistics.median(resets), "reset_p95_ns": percentile(resets, .95),
        "step_p50_ns": statistics.median(steps), "step_p95_ns": percentile(steps, .95),
        "successful_step_slots_per_second": size * 1e9 / statistics.mean(steps),
        "rust_process_peak_rss_bytes": rss_bytes,
        "limitations": ["single process sequential slots, no parallel speedup claimed",
                       "RSS includes setup/allocator/retained bounded episode state",
                       "synthetic uncalibrated inputs; no metro-scale or policy claim",
                       "roundtrip timing includes Python parsing and pipe overhead"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server", type=Path, default=ROOT / "target/release/examples/e14_batch")
    parser.add_argument("--size", type=int, required=True, choices=range(1, 65))
    parser.add_argument("--samples", type=int, default=20)
    args = parser.parse_args()
    if args.samples < 1:
        parser.error("samples must be positive")
    print(json.dumps(profile(args.server.resolve(), args.size, args.samples), sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    main()
