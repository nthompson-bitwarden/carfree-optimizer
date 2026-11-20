"""E16 durable training experiment control plane; all evaluation remains in Rust.

Trusted operators only. Bind to loopback; remote hosts use SSH tunnels. SQLite is
local to this coordinator, never on a shared/network filesystem. No third-party
Python dependencies. See docs/EXPERIMENT-JOBS.md for the v1 contract.
"""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import hmac
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import math
import os
from pathlib import Path
import secrets
import signal
import shutil
import sqlite3
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import Request, build_opener, ProxyHandler, HTTPRedirectHandler

SCHEMA = "e16-jobs-v1/e14-synthetic-v1"
LOG_BYTES = 32768
MAX_BODY = 262144


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)


def digest(path):
    with open(path, "rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def specification(evaluator_sha256, seeds, *, algorithm="random", topology=4,
                  demand=3, episodes=8, cpu_seconds=10, memory_bytes=1073741824,
                  wall_seconds=20, concurrency=2, max_attempts=2, rates=None):
    return dict(schema=SCHEMA, evaluator_sha256=evaluator_sha256, seeds=sorted(seeds),
                algorithm=algorithm, topology=topology, demand=demand, episodes=episodes,
                cpu_seconds=cpu_seconds, memory_bytes=memory_bytes, wall_seconds=wall_seconds,
                concurrency=concurrency, max_attempts=max_attempts, rates=rates)


def integer(value, lo, hi):
    return type(value) is int and lo <= value <= hi


def number(value):
    return type(value) in (float, int) and math.isfinite(value) and value >= 0


def validate(spec):
    expected = set(specification("", []))
    if not isinstance(spec, dict) or set(spec) != expected or spec["schema"] != SCHEMA:
        raise ValueError("incompatible job schema or unknown parameters")
    sha = spec["evaluator_sha256"]
    if not isinstance(sha, str) or len(sha) != 64 or any(c not in "0123456789abcdef" for c in sha):
        raise ValueError("evaluator SHA-256 must be lowercase hex")
    seeds = spec["seeds"]
    if (not isinstance(seeds, list) or not 1 <= len(seeds) <= 1000
            or any(not integer(s, 0, 2**63 - 1) for s in seeds)
            or sorted(set(seeds)) != seeds):
        raise ValueError("seeds must be unique, sorted signed-64-bit nonnegative integers")
    for key, lo, hi in (("topology", 0, 63), ("demand", 0, 16), ("episodes", 1, 1000),
                        ("cpu_seconds", 1, 3600), ("memory_bytes", 67108864, 2**40),
                        ("wall_seconds", 2, 3600), ("concurrency", 1, 64), ("max_attempts", 1, 10)):
        if not integer(spec[key], lo, hi):
            raise ValueError(f"invalid {key}; expected integer in {lo}..{hi}")
    if spec["algorithm"] not in ("simulation", "random"):
        raise ValueError("algorithm must be simulation (explicit stop) or random optimization")
    rates = spec["rates"]
    if rates is not None:
        if (not isinstance(rates, dict) or set(rates) != {
                "currency", "cpu_hour", "gib_hour", "storage_gib_month", "retention_hours"}
                or not isinstance(rates["currency"], str) or len(rates["currency"]) != 3
                or not rates["currency"].isalpha() or not rates["currency"].isupper()
                or any(not number(v) or v > 1e9 for k, v in rates.items() if k != "currency")):
            raise ValueError("rates require currency and nonnegative CPU-hour/GiB-hour/storage/retention rates")


class Store:
    """Serialized SQLite transactions. Attempt tokens fence every mutable operation."""

    def __init__(self, path, capacity=2, clock=time.time):
        if not integer(capacity, 1, 64):
            raise ValueError("capacity must be in 1..64")
        self.path, self.clock = Path(path), clock
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self.connection() as db:
            db.executescript("""
                CREATE TABLE IF NOT EXISTS config(version TEXT PRIMARY KEY, capacity INTEGER);
                CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY, spec TEXT NOT NULL,
                  state TEXT NOT NULL, created REAL NOT NULL, ended REAL);
                CREATE TABLE IF NOT EXISTS items(job TEXT, seed INTEGER, state TEXT,
                  tries INTEGER, token TEXT, result TEXT, PRIMARY KEY(job,seed));
                CREATE TABLE IF NOT EXISTS attempts(token TEXT PRIMARY KEY, request_id TEXT UNIQUE, job TEXT,
                  seed INTEGER, worker TEXT, state TEXT, started REAL, deadline REAL,
                  scenarios INTEGER DEFAULT 0, failures INTEGER DEFAULT 0, score REAL,
                  log TEXT DEFAULT '', result TEXT);
            """)
            # executescript commits implicitly; serialize configuration separately.
            db.execute("BEGIN IMMEDIATE")
            config = db.execute("SELECT * FROM config").fetchone()
            if config is None:
                db.execute("INSERT INTO config VALUES (?,?)", (SCHEMA, capacity))
            elif tuple(config) != (SCHEMA, capacity):
                raise ValueError("database schema/capacity mismatch; use recorded coordinator capacity")

    @contextlib.contextmanager
    def connection(self):
        db = sqlite3.connect(self.path, timeout=30, isolation_level=None)
        db.row_factory = sqlite3.Row
        try:
            db.execute("PRAGMA synchronous=FULL")
            db.execute("BEGIN IMMEDIATE")
            yield db
            db.commit()
        except BaseException:
            db.rollback()
            raise
        finally:
            db.close()

    def _job(self, db, job):
        row = db.execute("SELECT * FROM jobs WHERE id=?", (job,)).fetchone()
        if row is None:
            raise ValueError("unknown job")
        return row

    def _settle(self, db):
        now = self.clock()
        for a in db.execute("SELECT * FROM attempts WHERE state='running' AND deadline<=?", (now,)).fetchall():
            # Expiration fences publication but deliberately does NOT release capacity.
            db.execute("UPDATE attempts SET state='expired' WHERE token=?", (a["token"],))
            db.execute("UPDATE items SET state='pending',token=NULL WHERE token=?", (a["token"],))
        for j in db.execute("SELECT * FROM jobs WHERE state IN ('queued','running','stopping')").fetchall():
            active = db.execute("SELECT count(*) FROM attempts WHERE job=? AND state IN ('running','expired')",
                                (j["id"],)).fetchone()[0]
            if j["state"] == "stopping":
                if active == 0:
                    db.execute("UPDATE jobs SET state='cancelled',ended=? WHERE id=?", (now, j["id"]))
                continue
            spec = json.loads(j["spec"])
            db.execute("UPDATE items SET state='failed' WHERE job=? AND state='pending' AND tries>=?",
                       (j["id"], spec["max_attempts"]))
            unfinished = db.execute("SELECT count(*) FROM items WHERE job=? AND state IN ('pending','running')",
                                    (j["id"],)).fetchone()[0]
            if unfinished == 0 and active == 0:
                failed = db.execute("SELECT count(*) FROM items WHERE job=? AND state='failed'", (j["id"],)).fetchone()[0]
                db.execute("UPDATE jobs SET state=?,ended=? WHERE id=?",
                           ("failed" if failed else "completed", now, j["id"]))

    def submit(self, spec):
        validate(spec)
        text = encoded(spec)
        job = hashlib.sha256(text.encode()).hexdigest()
        with self.connection() as db:
            if db.execute("SELECT 1 FROM jobs WHERE id=?", (job,)).fetchone() is None:
                db.execute("INSERT INTO jobs VALUES (?,?, 'queued',?,NULL)", (job, text, self.clock()))
                db.executemany("INSERT INTO items VALUES (?,?,'pending',0,NULL,NULL)",
                               [(job, s) for s in spec["seeds"]])
        return job

    def claim(self, worker, evaluator_sha256, request_id):
        if not isinstance(worker, str) or not 1 <= len(worker) <= 64 or any(not (c.isalnum() or c in '-_.') for c in worker):
            raise ValueError("worker must be a short alphanumeric host identifier")
        if (not isinstance(request_id, str) or len(request_id) != 64
                or any(c not in '0123456789abcdef' for c in request_id)):
            raise ValueError("claim requires a durable random 32-byte hex request ID")
        with self.connection() as db:
            self._settle(db)
            previous = db.execute("SELECT * FROM attempts WHERE request_id=?", (request_id,)).fetchone()
            if previous is not None:
                spec = json.loads(self._job(db, previous["job"])["spec"])
                if previous["worker"] != worker or spec["evaluator_sha256"] != evaluator_sha256:
                    raise ValueError("conflicting claim replay")
                return dict(job=previous["job"], seed=previous["seed"], token=previous["token"],
                            spec=spec, deadline=previous["deadline"], attempt_state=previous["state"])
            capacity = db.execute("SELECT capacity FROM config").fetchone()[0]
            active = db.execute("SELECT count(*) FROM attempts WHERE state IN ('running','expired')").fetchone()[0]
            if active >= capacity:
                return None
            for j in db.execute("SELECT * FROM jobs WHERE state IN ('queued','running') ORDER BY created,id").fetchall():
                spec = json.loads(j["spec"])
                if spec["evaluator_sha256"] != evaluator_sha256:
                    continue
                count = db.execute("SELECT count(*) FROM attempts WHERE job=? AND state IN ('running','expired')", (j["id"],)).fetchone()[0]
                if count >= spec["concurrency"]:
                    continue
                item = db.execute("SELECT * FROM items WHERE job=? AND state='pending' ORDER BY seed LIMIT 1", (j["id"],)).fetchone()
                if item is None:
                    continue
                token = secrets.token_hex(32)
                now = self.clock()
                deadline = now + spec["wall_seconds"] + 10  # transport/setup/reap grace
                db.execute("INSERT INTO attempts(token,request_id,job,seed,worker,state,started,deadline) VALUES (?,?,?,?,?,'running',?,?)",
                           (token, request_id, j["id"], item["seed"], worker, now, deadline))
                db.execute("UPDATE items SET state='running',tries=tries+1,token=? WHERE job=? AND seed=?",
                           (token, j["id"], item["seed"]))
                db.execute("UPDATE jobs SET state='running' WHERE id=?", (j["id"],))
                return dict(job=j["id"], seed=item["seed"], token=token, spec=spec,
                            deadline=deadline, attempt_state="running")
        return None

    def progress(self, token, scenarios, failures, score, log):
        self._progress_fields(scenarios, failures, score, log)
        with self.connection() as db:
            self._settle(db)
            a = db.execute("SELECT * FROM attempts WHERE token=?", (token,)).fetchone()
            if a is None or a["state"] != "running" or self._job(db, a["job"])["state"] == "stopping":
                return False
            if scenarios < a["scenarios"] or failures < a["failures"]:
                raise ValueError("progress cannot decrease")
            db.execute("UPDATE attempts SET scenarios=?,failures=?,score=?,log=? WHERE token=?",
                       (scenarios, failures, score, log, token))
            return True

    @staticmethod
    def _progress_fields(scenarios, failures, score, log):
        if (not integer(scenarios, 0, 1000) or not integer(failures, 0, 1000)
                or failures > scenarios or (score is not None and (type(score) not in (int, float) or not math.isfinite(score)))
                or not isinstance(log, str) or len(log.encode()) > LOG_BYTES):
            raise ValueError("invalid/beyond-cap progress, score or log")

    def finish(self, token, score, scenarios, failures, log, wall_seconds, cpu_seconds, peak_rss_bytes):
        self._progress_fields(scenarios, failures, score, log)
        if (not number(wall_seconds) or not number(cpu_seconds) or wall_seconds > 86400
                or cpu_seconds > 86400 or not integer(peak_rss_bytes, 0, 2**50)):
            raise ValueError("invalid attempt usage")
        payload = encoded(dict(score=score, scenarios=scenarios, failures=failures, log=log,
                               wall_seconds=wall_seconds, cpu_seconds=cpu_seconds, peak_rss_bytes=peak_rss_bytes))
        with self.connection() as db:
            self._settle(db)
            a = db.execute("SELECT * FROM attempts WHERE token=?", (token,)).fetchone()
            if a is None:
                raise ValueError("unknown attempt token")
            if a["result"] is not None:
                if payload != a["result"]:
                    raise ValueError("conflicting completion replay")
                return a["state"] == "accepted"
            job = self._job(db, a["job"])
            spec = json.loads(job["spec"])
            accepted = (a["state"] == "running" and job["state"] != "stopping"
                        and score is not None and scenarios == spec["episodes"] and failures == 0)
            db.execute("UPDATE attempts SET state=?,result=?,scenarios=?,failures=?,score=?,log=? WHERE token=?",
                       ("accepted" if accepted else "reaped", payload, scenarios, failures, score, log, token))
            if a["state"] == "running":
                db.execute("UPDATE items SET state=?,result=?,token=NULL WHERE token=?",
                           ("completed" if accepted else "pending", payload if accepted else None, token))
            self._settle(db)
            return accepted

    def cancel(self, job):
        with self.connection() as db:
            j = self._job(db, job)
            if j["state"] in ("queued", "running"):
                db.execute("UPDATE jobs SET state='stopping' WHERE id=?", (job,))
            self._settle(db)
        return True

    def resume(self, job, evaluator_sha256):
        with self.connection() as db:
            self._settle(db)
            j = self._job(db, job)
            if evaluator_sha256 != json.loads(j["spec"])["evaluator_sha256"]:
                raise ValueError("checkpoint/evaluator digest incompatible; submit a new job")
            if j["state"] not in ("cancelled", "failed"):
                raise ValueError("resume requires cancelled/failed job with all attempts reaped")
            db.execute("UPDATE items SET state='pending',tries=0 WHERE job=? AND state!='completed'", (job,))
            db.execute("UPDATE jobs SET state='queued',ended=NULL WHERE id=?", (job,))
        return True

    def status(self, job):
        with self.connection() as db:
            self._settle(db)
            j = dict(self._job(db, job))
            spec = json.loads(j["spec"])
            items = [dict(r) for r in db.execute("SELECT * FROM items WHERE job=? ORDER BY seed", (job,))]
            attempts = [dict(r) for r in db.execute("SELECT * FROM attempts WHERE job=? ORDER BY started,token", (job,))]
            # Tokens are capabilities, not presentation fields.
            for a in attempts:
                a.pop("token")
                a.pop("request_id")
            results = [dict(seed=i["seed"], reference=f"sqlite:{job}/{i['seed']}", **json.loads(i["result"]))
                       for i in items if i["state"] == "completed"]
            scores = [r["score"] for r in results]
            usage = [json.loads(a["result"]) for a in attempts if a["result"] is not None]
            # Canonical per-job artifact export size, not SQLite page allocation.
            stored = len(encoded(dict(spec=spec, attempts=attempts, results=results)).encode())
            cpu = sum(u["cpu_seconds"] for u in usage)
            wall = sum(u["wall_seconds"] for u in usage)
            unknown = sum(a["result"] is None for a in attempts)
            rates = spec["rates"]
            spend = None
            if rates is not None and unknown == 0:
                # Memory billed at reserved virtual quota, NOT measured resident peak.
                spend = (cpu / 3600 * rates["cpu_hour"]
                         + spec["memory_bytes"] / 2**30 * wall / 3600 * rates["gib_hour"]
                         + stored / 2**30 * rates["storage_gib_month"] * rates["retention_hours"] / 730)
            best = [a["score"] for a in attempts if a["score"] is not None]
            return dict(schema=SCHEMA, job=job, spec=spec, state=j["state"],
                        total_seeds=len(items), finalized=len(results),
                        failed_seeds=sum(i["state"] == "failed" for i in items),
                        scenarios=sum(a["scenarios"] for a in attempts),
                        attempt_failures=sum(a["state"] in ("expired", "reaped") for a in attempts),
                        best=max(best) if best else None, metric="cumulative E14 reward (dimensionless; maximize)",
                        attempts=attempts, results=results,
                        distribution=dict(count=len(scores), mean=statistics.mean(scores) if scores else None,
                                          sample_variance=statistics.variance(scores) if len(scores) > 1 else None,
                                          minimum=min(scores) if scores else None, maximum=max(scores) if scores else None),
                        usage=dict(job_wall_seconds=max(0, (j["ended"] or self.clock()) - j["created"]),
                                   attempt_wall_seconds=wall, cpu_seconds=cpu,
                                   peak_rss_bytes=max((u["peak_rss_bytes"] for u in usage), default=None),
                                   stored_bytes=stored, unknown_attempts=unknown,
                                   estimated_spend=spend, currency=rates["currency"] if rates else None))


METHODS = {"submit", "claim", "progress", "finish", "cancel", "resume", "status"}


def serve(store, secret, port=8765):
    if not secret:
        raise ValueError("nonempty bearer token required")

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass  # never log bearer capabilities or job details

        def do_POST(self):
            self.connection.settimeout(5)
            if not hmac.compare_digest(self.headers.get("Authorization", ""), f"Bearer {secret}"):
                self.send_error(401)
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 < length <= MAX_BODY or self.path[1:] not in METHODS:
                    raise ValueError("unknown operation or excessive request size")
                data = self.rfile.read(length)
                if len(data) != length:
                    raise ValueError("incomplete body")
                args = json.loads(data)
                result = getattr(store, self.path[1:])(**args)
                body = encoded(dict(ok=True, result=result)).encode()
                self.send_response(200)
            except (ValueError, TypeError, KeyError, json.JSONDecodeError) as e:
                body = encoded(dict(ok=False, error=str(e))).encode()
                self.send_response(400)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    return ThreadingHTTPServer(("127.0.0.1", port), Handler)


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None  # urllib raises HTTPError; Client closes its response handle


class Client:
    def __init__(self, url, secret):
        try:
            parsed = urlsplit(url)
            port = parsed.port
            valid = (parsed.scheme == "http" and parsed.hostname == "127.0.0.1"
                     and port is not None and 1 <= port <= 65535
                     and url == f"http://127.0.0.1:{port}")
        except (ValueError, TypeError, AttributeError):
            valid = False
        if not valid:
            raise ValueError("use exactly http://127.0.0.1:PORT, without credentials, path, query or fragment")
        self.url, self.secret = url, secret
        # Never route bearer capabilities through environment/system proxies or
        # forward them across redirects, including to a different loopback port.
        self.opener = build_opener(ProxyHandler({}), NoRedirect())

    def call(self, method, **args):
        request = Request(self.url + "/" + method, data=encoded(args).encode(),
                          headers={"Authorization": "Bearer " + self.secret,
                                   "Content-Type": "application/json"})
        try:
            with self.opener.open(request, timeout=5) as response:
                value = json.load(response)
        except HTTPError as e:
            with e:
                message = e.read().decode()
            raise ValueError(message) from e
        return value["result"]


def parse_log(text):
    scenarios, failures, score = 0, 0, None
    for line in text.splitlines():
        fields = line.split("\t")
        if len(fields) == 4 and fields[0] in ("progress", "result"):
            scenarios, failures = int(fields[1]), int(fields[2])
            score = None if fields[3] == "none" else float(fields[3])
            Store._progress_fields(scenarios, failures, score, "")
    return scenarios, failures, score


@contextlib.contextmanager
def staged_evaluator(source):
    # Copy once through an open source handle. Rebuilds/symlink swaps of the
    # source cannot change the private, verified artifact subsequently launched.
    with tempfile.TemporaryDirectory(prefix="e16-binary-") as directory:
        target = Path(directory) / "evaluator"
        with open(source, "rb") as src, open(target, "xb") as dst:
            shutil.copyfileobj(src, dst)
        target.chmod(0o500)
        yield target


def execute(lease, evaluator, heartbeat):
    with staged_evaluator(evaluator) as pinned:
        return _execute(lease, pinned, heartbeat)


def _execute(lease, evaluator, heartbeat):
    """One trusted, subprocess-free Rust evaluator; reap before releasing any slot.

    Rust installs CPU/AS limits and a wall watchdog before evaluating. Unsupported
    hosts fail closed. Worker-loss never frees coordinator capacity on a timer.
    """
    spec = lease["spec"]
    started = time.monotonic()
    proc = None
    usage = None
    text = ""
    cancelled = False
    try:
        if sys.platform != "linux":
            raise ValueError("hard AS quota execution is qualified only on Linux; use a Linux worker")
        if digest(evaluator) != spec["evaluator_sha256"]:
            raise ValueError("evaluator SHA-256 mismatch; incompatible checkpoint")
        # Deadline is not renewed. Never start an evaluator after its reservation expires.
        remaining = lease["deadline"] - time.time() - 2
        if remaining < spec["wall_seconds"]:
            raise ValueError("insufficient lease time; check host clocks/network latency")
        args = [str(Path(evaluator).resolve()), str(lease["seed"]), str(spec["topology"]),
                str(spec["demand"]), spec["algorithm"], str(spec["episodes"]),
                str(spec["cpu_seconds"]), str(spec["memory_bytes"]), str(spec["wall_seconds"])]
        with tempfile.TemporaryFile() as output:
            proc = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=output,
                                    stderr=subprocess.STDOUT, start_new_session=True,
                                    env={"PATH": "/usr/bin:/bin"})
            last_heartbeat = 0.0
            while True:
                pid, status, usage = os.wait4(proc.pid, os.WNOHANG)
                if pid:
                    proc.returncode = os.waitstatus_to_exitcode(status)
                    break
                now = time.monotonic()
                if now - started > spec["wall_seconds"] + 1 or now > last_heartbeat + 0.2:
                    text = os.pread(output.fileno(), LOG_BYTES, 0).decode("utf-8", errors="replace")
                    try:
                        scenarios, failures, score = parse_log(text)
                        keep = heartbeat(scenarios, failures, score, text)
                    except (OSError, ValueError):
                        keep = False  # coordinator lost: stop rather than keep spending
                    last_heartbeat = now
                    if not keep or now - started > spec["wall_seconds"] + 1:
                        cancelled = True
                        os.killpg(proc.pid, signal.SIGKILL)
                time.sleep(0.02)
            text = os.pread(output.fileno(), LOG_BYTES, 0).decode("utf-8", errors="replace")
        scenarios, failures, score = parse_log(text)
        if cancelled or proc.returncode != 0 or not text.splitlines() or not text.splitlines()[-1].startswith("result\t"):
            score = None
            text = (text + f"\nattempt exited {proc.returncode}; cancelled={cancelled}\n")[-LOG_BYTES:]
    except (OSError, ValueError) as e:
        scenarios, failures, score = 0, 0, None
        text = f"worker refused/failed attempt: {e}\n"
    finally:
        if proc is not None and proc.returncode is None:
            os.killpg(proc.pid, signal.SIGKILL)
            _, status, usage = os.wait4(proc.pid, 0)
            proc.returncode = os.waitstatus_to_exitcode(status)
    return dict(token=lease["token"], score=score, scenarios=scenarios, failures=failures,
                log=text, wall_seconds=time.monotonic() - started,
                cpu_seconds=(usage.ru_utime + usage.ru_stime) if usage else 0.0,
                peak_rss_bytes=int(usage.ru_maxrss * 1024) if usage else 0)


def durable_record(path, value):
    temporary = path.with_suffix(".tmp")
    with open(temporary, "w", encoding="utf-8") as f:
        os.chmod(temporary, 0o600)
        f.write(encoded(value))
        f.flush()
        os.fsync(f.fileno())
    os.replace(temporary, path)
    fd = os.open(path.parent, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def worker(client, evaluator, name, once=False, spool=None):
    if sys.platform != "linux" or sys.maxsize < 2**32:
        raise ValueError("worker execution requires 64-bit Linux; no work was claimed")
    if not isinstance(name, str) or not name or any(not (c.isalnum() or c in '-_.') for c in name):
        raise ValueError("invalid worker name")
    spool = Path(spool) if spool else Path(".codex/run/e16-completions") / (name + ".json")
    spool.parent.mkdir(parents=True, exist_ok=True)
    import fcntl  # worker-only Unix facility; coordinator imports remain portable
    with open(spool.with_suffix(".lock"), "a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as e:
            raise ValueError("another supervisor owns this worker journal") from e
        _worker_locked(client, evaluator, name, once, spool)


def _worker_locked(client, evaluator, name, once, spool):
    while True:
        record = json.loads(spool.read_text()) if spool.exists() else None
        if record is not None:
            if record["url"] != client.url:
                raise ValueError("worker journal belongs to a different coordinator")
            if record["phase"] == "launching":
                raise ValueError("interrupted launch: process state unknown; prove exit and reconcile before recovery")
        with contextlib.ExitStack() as artifacts:
            pinned = None
            if record is None:
                pinned = artifacts.enter_context(staged_evaluator(evaluator))
                record = dict(phase="requesting", request_id=secrets.token_hex(32),
                              url=client.url, sha=digest(pinned))
                durable_record(spool, record)  # BEFORE claim: lost replies are recoverable
            if record["phase"] == "requesting":
                lease = client.call("claim", worker=name, evaluator_sha256=record["sha"],
                                    request_id=record["request_id"])
                if lease is None:
                    spool.unlink()
                    if once:
                        return
                    time.sleep(0.5)
                    continue
                record.update(phase="claimed", lease=lease)
                durable_record(spool, record)
            if record["phase"] == "claimed":
                lease = record["lease"]
                # Recovery can establish a never-launched failure without current
                # executable bytes. It must never strand that reservation.
                try:
                    if pinned is None:
                        pinned = artifacts.enter_context(staged_evaluator(evaluator))
                    if digest(pinned) != record["sha"]:
                        raise ValueError("recovered claim evaluator digest changed before launch")
                except (OSError, ValueError) as e:
                    result = dict(token=lease["token"], score=None, scenarios=0, failures=0,
                                  log=f"never launched: {e}\n", wall_seconds=0.0,
                                  cpu_seconds=0.0, peak_rss_bytes=0)
                else:
                    record["phase"] = "launching"
                    durable_record(spool, record)  # unknown launch after crash must fail closed
                    result = _execute(lease, pinned, lambda n, f, s, log: client.call(
                        "progress", token=lease["token"], scenarios=n, failures=f, score=s, log=log))
                record.update(phase="finished", result=result)
                durable_record(spool, record)
            # Replaying a reaped completion needs no executable, staging or hash.
            while True:
                try:
                    client.call("finish", **record["result"])
                    spool.unlink()
                    break
                except OSError:
                    time.sleep(0.5)
            if once:
                return


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:8765")
    parser.add_argument("--token-file", type=Path, required=True,
                        help="private file holding the shared operator bearer token")
    sub = parser.add_subparsers(dest="command", required=True)
    server = sub.add_parser("serve")
    server.add_argument("--database", type=Path, required=True)
    server.add_argument("--capacity", type=int, default=2)
    server.add_argument("--port", type=int, default=8765)
    submit = sub.add_parser("submit")
    submit.add_argument("--spec", type=Path, required=True)
    make = sub.add_parser("spec")
    make.add_argument("--evaluator", type=Path, required=True)
    make.add_argument("--seeds", type=int, nargs="+", default=[7, 19, 31])
    for name in ("status", "cancel", "resume"):
        p = sub.add_parser(name)
        p.add_argument("job")
        if name == "resume":
            p.add_argument("--evaluator", type=Path, required=True)
    work = sub.add_parser("worker")
    work.add_argument("--evaluator", type=Path, required=True)
    work.add_argument("--name", required=True)
    work.add_argument("--once", action="store_true")
    args = parser.parse_args()
    secret = args.token_file.read_text().strip()
    if args.command == "serve":
        serve(Store(args.database, args.capacity), secret, args.port).serve_forever()
        return
    client = Client(args.url, secret)
    if args.command == "spec":
        value = specification(digest(args.evaluator), args.seeds)
        validate(value)
    elif args.command == "worker":
        worker(client, args.evaluator, args.name, args.once)
        return
    elif args.command == "submit":
        value = client.call("submit", spec=json.loads(args.spec.read_text()))
    elif args.command == "resume":
        value = client.call("resume", job=args.job, evaluator_sha256=digest(args.evaluator))
    else:
        value = client.call(args.command, job=args.job)
    print(json.dumps(value, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
