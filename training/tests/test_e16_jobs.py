"""E16 Given/When/Then durable orchestration acceptance (stdlib only)."""
import hashlib
from io import StringIO
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import secrets
import tempfile
import threading
import unittest
from unittest.mock import Mock, patch
from pathlib import Path

from experiment_jobs import Store, Client, serve, specification, worker


class Jobs(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.clock = [1000.0]
        self.store = Store(Path(self.tmp.name) / "jobs.sqlite", capacity=2,
                           clock=lambda: self.clock[0])
        self.spec = specification("a" * 64, [7, 19], episodes=2)

    def submit(self):
        return self.store.submit(self.spec)

    def claim(self, worker, request_id=None, sha=None):
        return self.store.claim(worker, sha or self.spec["evaluator_sha256"],
                                request_id or secrets.token_hex(32))

    def result(self, lease, score=1.0):
        return dict(token=lease["token"], score=score, scenarios=2, failures=0,
                    log="finished\n", wall_seconds=0.2, cpu_seconds=0.1, peak_rss_bytes=1000)

    def test_s01_durable_hashed_submission(self):
        job = self.submit()
        self.assertEqual(job, self.submit())
        restarted = Store(self.store.path, capacity=2, clock=lambda: self.clock[0])
        status = restarted.status(job)
        self.assertEqual(status["spec"], self.spec)
        self.assertEqual(status["state"], "queued")
        self.assertEqual(job, hashlib.sha256(json.dumps(self.spec, sort_keys=True,
                         separators=(",", ":"), allow_nan=False).encode()).hexdigest())
        changed = dict(self.spec, topology=3)
        self.assertNotEqual(job, restarted.submit(changed))

    def test_s02_partial_progress_and_failures(self):
        job = self.submit()
        lease = self.claim("worker-a")
        self.store.progress(lease["token"], 1, 0, 0.5, "episode 1\n")
        status = self.store.status(job)
        self.assertEqual(status["scenarios"], 1)
        self.assertEqual(status["best"], 0.5)
        self.assertIn("episode 1", status["attempts"][0]["log"])
        self.store.finish(**self.result(lease))
        other = self.claim("worker-b")
        self.store.finish(**dict(self.result(other), score=None, failures=1))
        self.assertEqual(self.store.status(job)["attempt_failures"], 1)

    def test_s03_cancel_reap_resume_retains_finalized_seed(self):
        job = self.submit()
        a = self.claim("a")
        self.store.finish(**self.result(a))
        b = self.claim("b")
        self.store.cancel(job)
        self.assertEqual(self.store.status(job)["state"], "stopping")
        self.assertFalse(self.store.progress(b["token"], 0, 0, None, ""))
        with self.assertRaises(ValueError):
            self.store.resume(job, "b" * 64)
        self.store.finish(**self.result(b))
        self.assertEqual(self.store.status(job)["state"], "cancelled")
        self.store.resume(job, self.spec["evaluator_sha256"])
        retry = self.claim("b")
        self.assertEqual(retry["seed"], b["seed"])
        self.store.finish(**self.result(retry))
        self.assertEqual(self.store.status(job)["state"], "completed")
        self.assertEqual(len(self.store.status(job)["results"]), 2)

    def test_s04_capacity_not_freed_by_expired_lease(self):
        self.submit()
        a, b = self.claim("a"), self.claim("b")
        self.assertIsNone(self.claim("c"))
        self.clock[0] += self.spec["wall_seconds"] + 15
        self.assertIsNone(self.claim("c"))  # unknown live process retains slot
        self.assertFalse(self.store.finish(**self.result(a)))  # reaped, stale result fenced
        retry = self.claim("c")
        self.assertIsNotNone(retry)
        self.assertNotEqual(retry["token"], a["token"])
        self.assertFalse(self.store.finish(**self.result(b)))

    def test_s05_s07_sorted_distribution_all_attempt_usage_and_spend(self):
        self.spec["rates"] = dict(currency="USD", cpu_hour=2.0, gib_hour=0.1,
                                   storage_gib_month=0.02, retention_hours=24.0)
        job = self.submit()
        a, b = self.claim("a"), self.claim("b")
        self.store.finish(**self.result(b, 3.0))
        self.store.finish(**self.result(a, 1.0))
        status = self.store.status(job)
        self.assertEqual([r["seed"] for r in status["results"]], [7, 19])
        self.assertEqual(status["distribution"]["mean"], 2.0)
        self.assertEqual(status["distribution"]["sample_variance"], 2.0)
        self.assertAlmostEqual(status["usage"]["cpu_seconds"], 0.2)
        self.assertGreater(status["usage"]["stored_bytes"], 0)
        self.assertGreater(status["usage"]["estimated_spend"], 0)
        self.assertEqual(status["usage"]["currency"], "USD")

    def test_s06_fenced_idempotent_completion(self):
        job = self.submit()
        lease = self.claim("a")
        result = self.result(lease)
        self.assertTrue(self.store.finish(**result))
        self.assertTrue(self.store.finish(**result))
        with self.assertRaises(ValueError):
            self.store.finish(**dict(result, score=9))
        self.assertEqual(len(self.store.status(job)["results"]), 1)

    def test_s06_atomic_concurrent_claims(self):
        self.submit()
        claims = []
        threads = [threading.Thread(target=lambda: claims.append(self.claim("a")))
                   for _ in range(8)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        tokens = [c["token"] for c in claims if c]
        self.assertEqual(len(tokens), 2)
        self.assertEqual(len(set(tokens)), 2)

    def test_s06_http_two_clients_restart_and_auth(self):
        server = serve(self.store, "secret", port=0)
        self.addCleanup(server.server_close)
        t = threading.Thread(target=server.serve_forever, daemon=True)
        t.start()
        self.addCleanup(server.shutdown)
        client = Client(f"http://127.0.0.1:{server.server_port}", "secret")
        job = client.call("submit", spec=self.spec)
        leases = [client.call("claim", worker=w, evaluator_sha256=self.spec["evaluator_sha256"],
                              request_id=secrets.token_hex(32)) for w in ("host-a", "host-b")]
        for lease in leases:
            self.assertTrue(client.call("finish", **self.result(lease)))
        self.assertEqual(client.call("status", job=job)["state"], "completed")
        bad = Client(client.url, "wrong")
        with self.assertRaises(Exception):
            bad.call("status", job=job)

    def test_e20_s04_s06_job_trace_and_attempt_log_exclude_capabilities(self):
        secret = secrets.token_hex(32)
        server = serve(self.store, secret, port=0)
        self.addCleanup(server.server_close)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.shutdown)
        client = Client(f"http://127.0.0.1:{server.server_port}", secret)
        stderr = StringIO()
        self.spec["max_attempts"] = 1
        self.spec["seeds"] = [7]
        with patch("experiment_jobs.sys.stderr", stderr):
            job = client.call("submit", spec=self.spec)
            lease = client.call("claim", worker="fixture", request_id=secrets.token_hex(32),
                                evaluator_sha256=self.spec["evaluator_sha256"])
            log = f"fixture diagnostics {secret} {lease['token']}"
            self.assertTrue(client.call("progress", token=lease["token"], scenarios=1,
                                        failures=0, score=None, log=log))
            completion = dict(self.result(lease), score=None, log=log)
            self.assertFalse(client.call("finish", **completion))
            self.assertFalse(client.call("finish", **completion))
            with self.assertRaises(ValueError):
                client.call("finish", **dict(completion, log=f"fixture diagnostics {'0' * 64} {lease['token']}"))
            status = client.call("status", job=job)
        events = [json.loads(line) for line in stderr.getvalue().splitlines()]
        self.assertEqual([(event["job"], event["status"]) for event in events],
                         [(job, "submitted"), (job, "running"), (job, "attempt_reaped"), (job, "failed")])
        self.assertEqual(events[1]["attempt"], events[2]["attempt"])
        self.assertEqual(events[-1]["attempt_failures"], 1)
        self.assertIn("inspect", events[-1]["action"])
        self.assertEqual(status["attempts"][0]["log"], "fixture diagnostics * *")
        self.assertNotIn("log_sha256", json.dumps(status))
        for capability in (secret, lease["token"]):
            self.assertNotIn(capability, stderr.getvalue())
            self.assertNotIn(capability, json.dumps(status))

    def test_e20_s06_completion_replay_accepts_legacy_log_after_credential_rotation(self):
        job = self.submit()
        lease = self.claim("fixture")
        old_bearer = secrets.token_hex(32)
        completion = dict(self.result(lease), log=f"fixture {old_bearer} {lease['token']}")
        self.assertTrue(self.store.finish(**completion))
        new_bearer = secrets.token_hex(32)
        server = serve(self.store, new_bearer, port=0)
        self.addCleanup(server.server_close)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.shutdown)
        client = Client(f"http://127.0.0.1:{server.server_port}", new_bearer)
        self.assertTrue(client.call("finish", **completion))
        with self.assertRaises(ValueError):
            client.call("finish", **dict(completion, score=0.0))
        status = client.call("status", job=job)
        self.assertEqual(status["results"][0]["log"], "fixture * *")
        self.assertNotIn(old_bearer, json.dumps(status))
        self.assertNotIn(lease["token"], json.dumps(status))

    def test_s06_lost_claim_reply_recovers_same_reservation_and_incompatible_worker_does_not_charge(self):
        job = self.submit()
        request = secrets.token_hex(32)
        first = self.claim("a", request_id=request)
        recovered = self.claim("a", request_id=request)
        self.assertEqual(first, recovered)
        self.assertEqual(len(self.store.status(job)["attempts"]), 1)
        self.assertIsNone(self.claim("wrong-binary", sha="b" * 64))
        self.assertEqual(len(self.store.status(job)["attempts"]), 1)
        self.clock[0] += 60
        expired = self.claim("a", request_id=request)
        self.assertEqual(expired["token"], first["token"])
        self.assertEqual(expired["attempt_state"], "expired")
        self.assertFalse(self.store.finish(**self.result(expired)))
        self.assertIsNotNone(self.claim("b"))

    def test_s04_unsupported_workers_make_no_coordinator_calls(self):
        for platform, width in (("darwin", 2**63 - 1), ("linux", 2**31 - 1)):
            client = Mock()
            with patch("experiment_jobs.sys.platform", platform), patch("experiment_jobs.sys.maxsize", width):
                with self.assertRaises(ValueError):
                    worker(client, "/missing-evaluator", "unsupported", once=True)
            client.call.assert_not_called()

    def test_security_rejects_noncanonical_loopback_authorities(self):
        for url in ("http://127.0.0.1:8765@evil.invalid", "https://127.0.0.1:8765",
                    "http://user:pass@127.0.0.1:8765", "http://127.0.0.1", "http://127.0.0.1:0",
                    "http://127.0.0.1:65536", "http://127.0.0.1:abc", "http://127.0.0.1:8765/",
                    "http://127.0.0.1:8765/path", "http://127.0.0.1:8765?q=x",
                    "http://127.0.0.1:8765#fragment", "http://127.0.0.1:8765?",
                    "http://127.0.0.1:8765\n", "http://localhost:8765"):
            with self.subTest(url=url), self.assertRaises(ValueError):
                Client(url, "synthetic-test-token")

    def http_server(self, handler):
        server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.addCleanup(server.server_close)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.shutdown)
        return f"http://127.0.0.1:{server.server_port}"

    def test_security_environment_proxies_never_receive_bearer(self):
        hits = []
        class Proxy(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_POST(self):
                hits.append(self.headers.get("Authorization"))
                self.send_response(502)
                self.end_headers()
        proxy = self.http_server(Proxy)
        server = serve(self.store, "synthetic-test-token", port=0)
        self.addCleanup(server.server_close)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.shutdown)
        environment = {"http_proxy": proxy, "HTTP_PROXY": proxy, "all_proxy": proxy,
                       "ALL_PROXY": proxy, "no_proxy": "", "NO_PROXY": ""}
        with patch.dict(os.environ, environment):
            client = Client(f"http://127.0.0.1:{server.server_port}", "synthetic-test-token")
            self.assertEqual(client.call("submit", spec=self.spec), self.submit())
        self.assertEqual(hits, [], "no proxy may receive a bearer capability")

    def test_security_redirects_never_forward_bearer(self):
        hits = []
        class Target(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_GET(self):
                hits.append(self.headers.get("Authorization"))
                self.send_response(200)
                self.end_headers()
            do_POST = do_GET
        target = self.http_server(Target)
        codes = [301, 302, 303, 307, 308]
        class Redirect(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_POST(self):
                self.send_response(codes.pop(0))
                self.send_header("Location", target + "/capture")
                self.send_header("Content-Length", "0")
                self.end_headers()
        origin = self.http_server(Redirect)
        client = Client(origin, "synthetic-test-token")
        for _ in range(5):
            with self.assertRaises(ValueError):
                client.call("status", job="x")
        self.assertEqual(hits, [], "redirect targets must never receive the capability")
        self.assertEqual(codes, [])

    def test_validation_and_missing_costs(self):
        for change in (dict(seeds=[7, 7]), dict(cpu_seconds=0), dict(episodes=True),
                       dict(memory_bytes=0), dict(unknown="x")):
            with self.assertRaises(ValueError):
                self.store.submit(dict(self.spec, **change))
        job = self.submit()
        status = self.store.status(job)
        self.assertIsNone(status["usage"]["estimated_spend"])
        self.assertIsNone(status["distribution"]["sample_variance"])
        with self.assertRaises(ValueError):
            self.claim("\n")


if __name__ == "__main__":
    unittest.main()
