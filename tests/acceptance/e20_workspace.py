#!/usr/bin/env python3
"""Run E20 local synthetic demo, security checks and offline restore rehearsal."""
import argparse
from contextlib import contextmanager
from http.client import HTTPConnection
import hashlib
import json
from pathlib import Path
from queue import Empty, Queue
import secrets
import shutil
import subprocess
import tempfile
import threading

API = "/api/e19/v1"
PLAN = {"projects": [0, 1], "width": 1900, "capital": 200, "annual": 30}


def digest(path):
    with path.open("rb") as payload:
        return hashlib.file_digest(payload, "sha256").hexdigest()


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


@contextmanager
def server(binary, store, credential, log):
    args = [str(binary), "0"]
    if store is not None:
        args += [str(store), str(credential)]
    with log.open("wb") as stderr:
        with subprocess.Popen(
            args,
            stdout=subprocess.PIPE, stderr=stderr, text=True,
        ) as process:
            lines = Queue()
            reader = threading.Thread(target=lambda: lines.put(process.stdout.readline()), daemon=True)
            reader.start()
            try:
                line = lines.get(timeout=15).strip()
                prefix = "Spatial workbench: http://127.0.0.1:"
                require(line.startswith(prefix), "workbench did not start")
                yield int(line.removeprefix(prefix))
            except Empty as error:
                raise RuntimeError("timed out starting the workbench") from error
            finally:
                if process.poll() is None:
                    process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                reader.join()


def request(port, method, route, token=None, body=None, status=200):
    headers = {}
    if token is not None:
        headers["Authorization"] = "Bearer " + token
    if body is not None:
        body = json.dumps(body)
        headers["Content-Type"] = "application/json"
    connection = HTTPConnection("127.0.0.1", port, timeout=8)
    try:
        connection.request(method, API + route, body, headers)
        response = connection.getresponse()
        raw = response.read()
        require(response.status == status, f"{method} {route}: expected HTTP {status}, got {response.status}")
        if response.headers["Content-Type"].startswith("application/json"):
            return json.loads(raw)
        return raw
    finally:
        connection.close()


def create(port, token):
    project = request(port, "POST", "/projects", token, {
        "title": "E20 public fixture", "area": "place:4159000", "date": "2026-01-05",
    }, 201)
    path = f"/projects/{project['id']}"
    scenario = request(port, "POST", path + "/scenarios", token, {
        "title": "Two walkways", "plan": PLAN,
    }, 201)
    return path, f"{path}/scenarios/{scenario['id']}/revisions/{scenario['revision']}"


def evaluate(port, token, revision):
    job = request(port, "POST", revision + "/jobs", token)
    require(job["status"] == "completed", "synthetic scenario did not complete")
    result = request(port, "GET", revision + "/evaluation", token)
    report = result["report"]
    require(result["manifest"]["seed"] == 42 and len(report["phases"]) == 3,
            "expected the fixed fixture seed and three phases")
    require(result["manifest"]["plan_units"]["width"] == "mm", "incorrect width unit")
    require(report["phases"][2]["values"] != report["phases"][0]["values"],
            "the intervention did not change the score vector")
    return result, metrics(port, token, revision)


def metrics(port, token, revision):
    parquet = request(port, "GET", revision + "/exports/metrics.parquet", token)
    require(parquet[:4] == parquet[-4:] == b"PAR1", "missing Parquet framing")
    return parquet


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--previous-binary", type=Path)
    args = parser.parse_args()
    current = args.binary.resolve(strict=True)
    previous = args.previous_binary.resolve(strict=True) if args.previous_binary else current
    previous_digest = digest(previous)
    current_digest = digest(current)
    depends_on_old_build = current_digest != previous_digest
    if args.previous_binary:
        require(depends_on_old_build, "upgrade rehearsal requires distinct executable digests")
    with tempfile.TemporaryDirectory(prefix="carfree-e20-") as scratch:
        root = Path(scratch)
        token = secrets.token_hex(32)
        foreign = secrets.token_hex(32)
        credential = root / "owner"
        credential.touch(mode=0o600)
        credential.write_text(token)
        store = root / "workspace"
        store.mkdir(mode=0o700)
        original_log = root / "original.log"
        with server(previous, store, credential, original_log) as port:
            project, revision = create(port, token)
            request(port, "GET", project, foreign, status=404)
            result, parquet = evaluate(port, token, revision)
            ledger = request(port, "GET", project, token)
            require("carfree-study-project-v1" in ledger["city"]["projectTsv"], "missing city snapshot")
        # Copy a stopped local store. Credentials, lock files and executable
        # are handled independently of the data snapshot.
        restored = root / "restored"
        restored.mkdir(mode=0o700)
        shutil.copyfile(store / "workspace-v1.json", restored / "workspace-v1.json")
        with server(current, restored, credential, root / "restore.log") as port:
            recovered = request(port, "GET", project, token)
            expect_stale = "evaluator_changed" if depends_on_old_build else None
            require(recovered["stale"] == expect_stale, "upgrade compatibility flag differs")
            require(recovered["city"] == ledger["city"], "restored city snapshot differs")
            require(request(port, "GET", revision + "/evaluation", token) == result,
                    "restored evaluation differs")
            restored_parquet = metrics(port, token, revision)
            if not expect_stale:
                require(restored_parquet == parquet, "restored metrics export differs")
            require(request(port, "GET", revision + "/exports/manifest.json", token) == result["manifest"],
                    "restored metrics manifest differs")
            if expect_stale:
                request(port, "POST", revision + "/jobs", token, status=409)
        replay = root / "replay"
        replay.mkdir(mode=0o700)
        replay_log = root / "replay.log"
        with server(current, replay, credential, replay_log) as port:
            _, replay_revision = create(port, token)
            replayed, replay_parquet = evaluate(port, token, replay_revision)
        for field in ("scenario", "report"):
            require(replayed[field] == result[field], f"scientific replay differs in {field}")
        # The manifest embedded in Parquet includes the evaluator build digest.
        if not expect_stale:
            require(replay_parquet == parquet, "metrics replay differs")
        logs = "\n".join(path.read_text() for path in root.glob("*.log"))
        require(token not in logs and foreign not in logs, "bearer capability entered diagnostic log")
        events = [json.loads(line) for line in replay_log.read_text().splitlines() if line.startswith("{")]
        require([(event["job"], event["status"]) for event in events] ==
                [("1/1/1", "running"), ("1/1/1", "completed")], "unexpected workspace job trace")
        print("E20 fixture, workspace authorization, reproducible scores and backup/restore PASS")
        if args.previous_binary:
            print("E20 previous-to-current build read-only upgrade rehearsal PASS")


if __name__ == "__main__":
    main()
