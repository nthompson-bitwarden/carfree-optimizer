#!/usr/bin/env python3
"""Measure local queue saturation with the fixed E17 synthetic scenario."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from http.client import HTTPConnection
import json
import math
from pathlib import Path
import resource
import sys
import tempfile
from time import monotonic, sleep

from e20_workspace import require, server

PLAN = "/runs?projects=0,1&width=1900&capital=200&annual=30"


def call(port, method, endpoint):
    headers = {"Origin": f"http://127.0.0.1:{port}", "X-Carfree-Workbench": "1"}
    connection = HTTPConnection("127.0.0.1", port, timeout=8)
    try:
        connection.request(method, "/api/e17/v1" + endpoint, headers=headers)
        response = connection.getresponse()
        return response.status, json.loads(response.read())
    finally:
        connection.close()


def attempt(port):
    start = monotonic()
    code, result = call(port, "POST", PLAN)
    submission_ms = (monotonic() - start) * 1000
    if code == 400 and "queue is full" in result.get("error", ""):
        return submission_ms, None
    require(code == 202, f"unexpected submission status {code}")
    while monotonic() - start < 15:
        code, run = call(port, "GET", f"/run?id={result['id']}")
        require(code == 200, f"unexpected run status {code}")
        status = run["state"]["status"]
        if status == "completed":
            return submission_ms, (monotonic() - start) * 1000
        require(status in {"queued", "running"}, f"unexpected scenario state {status}")
        sleep(0.005)
    raise RuntimeError("scenario timed out")


def p95(values):
    return round(sorted(values)[math.ceil(len(values) * 0.95) - 1], 3)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--clients", type=int, default=1)
    parser.add_argument("--requests", type=int, default=30)
    args = parser.parse_args()
    require(1 <= args.clients <= args.requests <= 64, "expected 1 <= clients <= requests <= 64")
    with tempfile.TemporaryDirectory(prefix="carfree-e20-load-") as scratch:
        with server(args.binary.resolve(strict=True), None, None, Path(scratch) / "workbench.log") as port:
            require(attempt(port)[1] is not None, "warmup failed")
            start = monotonic()
            with ThreadPoolExecutor(max_workers=args.clients) as pool:
                results = list(pool.map(lambda _: attempt(port), range(args.requests)))
            elapsed = monotonic() - start
    completed = [done for _, done in results if done is not None]
    require(completed, "no accepted job completed")
    rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    print(json.dumps({
        "clients": args.clients,
        "submitted": args.requests,
        "completed": len(completed),
        "queue_rejected": args.requests - len(completed),
        "throughput_per_second": round(len(completed) / elapsed, 3),
        "submit_p95_ms": p95([submitted for submitted, _ in results]),
        "completed_p95_ms": p95(completed),
        "peak_rss_bytes": rss if sys.platform == "darwin" else rss * 1024,
    }))


if __name__ == "__main__":
    main()
