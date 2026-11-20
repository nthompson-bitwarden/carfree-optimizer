"""Required Linux E16 launch-path acceptance; run explicitly, never silently skip.

E16_EVALUATOR selects the built Rust example. No Python scientific calculation.
"""
import os
from pathlib import Path
import signal
import secrets
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

from experiment_jobs import Store, Client, serve, specification, digest, execute, worker, staged_evaluator

EVALUATOR = Path(os.environ.get("E16_EVALUATOR", "target/debug/examples/e16_evaluate")).resolve()


class LinuxAcceptance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if sys.platform != "linux" or not EVALUATOR.is_file():
            raise RuntimeError("Linux and built e16_evaluate are mandatory for quota acceptance")

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.store = Store(Path(self.tmp.name) / "jobs.db", capacity=2)
        self.spec = specification(digest(EVALUATOR), [7, 19], episodes=100,
                                  cpu_seconds=30, wall_seconds=40)

    def test_s04_hard_cpu_memory_wall_and_setup_failure(self):
        base = [str(EVALUATOR), "--quota-probe"]
        memory = subprocess.run(base + ["memory", "2", str(2**28), "4"], capture_output=True, timeout=6)
        self.assertEqual(memory.returncode, 0, memory.stderr)
        self.assertIn(b"allocation rejected", memory.stdout)
        cpu = subprocess.run(base + ["cpu", "1", str(2**28), "5"], capture_output=True, timeout=7)
        self.assertEqual(cpu.returncode, -signal.SIGKILL)
        wall = subprocess.run(base + ["wall", "5", str(2**28), "2"], capture_output=True, timeout=5)
        self.assertEqual(wall.returncode, 124)
        setup = subprocess.run(base + ["memory", "1", "1", "2"], capture_output=True, timeout=5)
        self.assertNotEqual(setup.returncode, 0)
        self.assertNotIn(b"allocation rejected", setup.stdout)

    def test_s03_s04_cancel_reaps_before_capacity_release_and_incompatible_binary(self):
        job = self.store.submit(self.spec)
        lease = self.store.claim("linux-a", self.spec["evaluator_sha256"], secrets.token_hex(32))
        self.store.cancel(job)
        result = execute(lease, EVALUATOR, lambda *_: False)
        self.assertIsNone(result["score"])
        self.assertFalse(self.store.finish(**result))
        self.assertEqual(self.store.status(job)["state"], "cancelled")
        self.store.resume(job, self.spec["evaluator_sha256"])
        lease = self.store.claim("linux-b", self.spec["evaluator_sha256"], secrets.token_hex(32))
        lease["spec"]["evaluator_sha256"] = "0" * 64
        refused = execute(lease, EVALUATOR, lambda *_: True)
        self.assertIsNone(refused["score"])
        self.assertIn("mismatch", refused["log"])
        self.assertEqual(refused["cpu_seconds"], 0)

    def test_s05_s06_s07_real_parallel_http_workers_and_serial_reproducibility(self):
        server = serve(self.store, "private", port=0)
        self.addCleanup(server.server_close)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.shutdown)
        client = Client(f"http://127.0.0.1:{server.server_port}", "private")
        job = client.call("submit", spec=self.spec)
        threads = [threading.Thread(target=worker, args=(client, EVALUATOR, f"linux-{i}", True),
                                    kwargs={"spool": Path(self.tmp.name) / f"parallel-{i}.json"}) for i in range(2)]
        for t in threads:
            t.start()
        deadline = time.monotonic() + 10
        overlap = False
        while time.monotonic() < deadline:
            status = client.call("status", job=job)
            live = [a for a in status["attempts"] if a["state"] == "running" and a["scenarios"] > 0]
            if len(live) == 2:
                overlap = True
                break
            if status["state"] in ("completed", "failed", "cancelled"):
                break
            time.sleep(0.02)
        for t in threads:
            t.join(timeout=30)
            self.assertFalse(t.is_alive())
        self.assertTrue(overlap, "must observe two live Rust evaluators reporting progress")
        parallel = client.call("status", job=job)
        self.assertEqual(parallel["state"], "completed", parallel)
        self.assertEqual(parallel["distribution"]["count"], 2)
        self.assertGreater(parallel["usage"]["cpu_seconds"], 0)
        self.assertGreater(parallel["usage"]["peak_rss_bytes"], 0)
        serial_spec = dict(self.spec, concurrency=1)
        serial_job = client.call("submit", spec=serial_spec)
        for _ in range(2):
            worker(client, EVALUATOR, "serial", once=True, spool=Path(self.tmp.name) / "serial.json")
        serial = client.call("status", job=serial_job)
        self.assertEqual([(r["seed"], r["score"]) for r in parallel["results"]],
                         [(r["seed"], r["score"]) for r in serial["results"]])

    def test_s02_maximum_episode_output_reserves_terminal_record(self):
        result = subprocess.run([str(EVALUATOR), "7", "4", "3", "random", "1000", "60",
                                 str(2**30), "90"], capture_output=True, timeout=95)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLess(len(result.stdout), 8192)
        self.assertTrue(result.stdout.splitlines()[-1].startswith(b"result\t1000\t0\t"))

    def test_s03_inflight_http_cancel_reaps_and_allows_resume(self):
        server = serve(self.store, "private", port=0)
        self.addCleanup(server.server_close)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.shutdown)
        client = Client(f"http://127.0.0.1:{server.server_port}", "private")
        job = client.call("submit", spec=dict(self.spec, seeds=[7], episodes=1000))
        t = threading.Thread(target=worker, args=(client, EVALUATOR, "cancel-live", True),
                             kwargs={"spool": Path(self.tmp.name) / "cancel-spool"})
        t.start()
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            status = client.call("status", job=job)
            if status["scenarios"] > 0:
                break
            time.sleep(0.02)
        self.assertEqual(status["state"], "running")
        self.assertGreater(status["scenarios"], 0)
        client.call("cancel", job=job)
        self.assertEqual(client.call("status", job=job)["state"], "stopping")
        t.join(timeout=10)
        self.assertFalse(t.is_alive())
        final = client.call("status", job=job)
        self.assertEqual(final["state"], "cancelled")
        self.assertIsNone(final["attempts"][0]["score"])
        self.assertEqual(final["usage"]["unknown_attempts"], 0)
        self.assertTrue(client.call("resume", job=job, evaluator_sha256=self.spec["evaluator_sha256"]))

    def test_s01_verified_private_copy_survives_source_replacement(self):
        source = Path(self.tmp.name) / "source-binary"
        source.write_bytes(EVALUATOR.read_bytes())
        source.chmod(0o700)
        job = self.store.submit(dict(self.spec, seeds=[7], episodes=1))
        lease = self.store.claim("staged", digest(source), secrets.token_hex(32))
        popen = subprocess.Popen
        def replace_then_launch(*args, **kwargs):
            source.write_bytes(b"invalid replacement")
            return popen(*args, **kwargs)
        with patch("experiment_jobs.subprocess.Popen", side_effect=replace_then_launch):
            result = execute(lease, source, lambda *_: True)
        self.assertIsNotNone(result["score"], result)
        self.assertTrue(self.store.finish(**result))
        self.assertEqual(self.store.status(job)["state"], "completed")

    def test_s06_worker_restart_recovers_lost_claim_and_completion_replies(self):
        server = serve(self.store, "private", port=0)
        self.addCleanup(server.server_close)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.shutdown)
        client = Client(f"http://127.0.0.1:{server.server_port}", "private")
        for operation in ("claim", "finish"):
            job = client.call("submit", spec=dict(self.spec, seeds=[7], episodes=1 if operation == "claim" else 2))
            spool = Path(self.tmp.name) / (operation + ".json")
            source = Path(self.tmp.name) / (operation + "-binary")
            source.write_bytes(EVALUATOR.read_bytes())
            source.chmod(0o700)
            original = client.call
            def lost(method, **args):
                value = original(method, **args)
                if method == operation:
                    raise KeyboardInterrupt("simulated lost reply and worker restart")
                return value
            with patch.object(client, "call", side_effect=lost):
                with self.assertRaises(KeyboardInterrupt):
                    worker(client, source, operation, once=True, spool=spool)
            self.assertTrue(spool.exists())
            source.unlink()  # recovery cannot depend on current executable bytes
            with patch("experiment_jobs.staged_evaluator", wraps=staged_evaluator) as stage:
                worker(client, source, operation, once=True, spool=spool)
                self.assertEqual(stage.call_count, 0 if operation == "finish" else 1)
            if operation == "claim":
                pending = client.call("status", job=job)
                self.assertEqual(pending["usage"]["unknown_attempts"], 0)
                self.assertIn("never launched", pending["attempts"][0]["log"])
                source.write_bytes(EVALUATOR.read_bytes())
                source.chmod(0o700)
                with patch("experiment_jobs.staged_evaluator", wraps=staged_evaluator) as stage:
                    worker(client, source, operation, once=True, spool=spool)
                    self.assertEqual(stage.call_count, 1, "must not stage again after claiming")
            status = client.call("status", job=job)
            self.assertEqual(status["state"], "completed", status)
            self.assertEqual(len(status["attempts"]), 2 if operation == "claim" else 1)
            self.assertFalse(spool.exists())

    def test_s04_supervisor_loss_leaves_independent_wall_cutoff(self):
        # The launcher exits without waiting. The evaluator still enforces its
        # own deadline; Linux /proc distinguishes a harmless zombie from live work.
        script = ("import subprocess,sys; p=subprocess.Popen(sys.argv[1:],"
                  "stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL); print(p.pid)")
        parent = subprocess.run([sys.executable, "-c", script, str(EVALUATOR), "--quota-probe", "wall",
                                 "5", str(2**28), "2"], capture_output=True, text=True, timeout=3)
        self.assertEqual(parent.returncode, 0)
        pid = int(parent.stdout)
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            try:
                state = Path(f"/proc/{pid}/stat").read_text().split(") ", 1)[1][0]
            except FileNotFoundError:
                return
            if state == "Z":
                return
            time.sleep(0.1)
        os.kill(pid, signal.SIGKILL)
        self.fail("orphan evaluator did not stop at its own wall deadline")


if __name__ == "__main__":
    unittest.main(verbosity=2)
