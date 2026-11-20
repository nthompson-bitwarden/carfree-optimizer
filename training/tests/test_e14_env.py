"""E14-S01/S06 real Rust integration plus bounded transport failure tests."""
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest

from carfree_env import BatchProcess, GymEnvironment, ProtocolError, RejectedAction, Transition

ROOT = Path(__file__).resolve().parents[2]
SERVER = Path(os.environ.get("E14_SERVER", ROOT / "target/debug/examples/e14_batch"))


def command(slots=1, topology=0):
    return [str(SERVER), str(slots), str(topology)]


def responder(response):
    # Read exactly one reset batch, then return a controlled malformed frame.
    source = "import sys; sys.stdin.buffer.readline(); sys.stdin.buffer.readline(); sys.stdout.buffer.write(" + repr(response) + "); sys.stdout.buffer.flush()"
    return [sys.executable, "-u", "-c", source]


class ProtocolTests(unittest.TestCase):
    def test_e14_s06_malformed_response_closes_connection(self):
        cases = [
            b"BAD\n", b"E14\t2\t1\n", b"E14\t1\t2\n",
            b"E14\t1\t1\nok\t0\t1\t1\t0\t0\t0\tnan\t1\n",
            b"E14\t1\t1\nok\t0\t1\t1\t0\t0\t0\t1\t2\n",
            b"E14\t1\t1\nok\t1\t1\t1\t0\t0\t0\t1\t1\n",
            b"E14\t1\t1\nok\t0\t1\t1\t0\t1\t1\t1\t1\n",
            b"E14\t1\t1\nok\t0\t1\t1\t0\t0\t0\t1\t1",
        ]
        for response in cases:
            with self.subTest(response=response), BatchProcess(responder(response), 1, timeout=2) as batch:
                with self.assertRaises(ProtocolError):
                    batch.reset([42])
                with self.assertRaises(ProtocolError):
                    batch.reset([42])

    def test_e14_s06_trailing_data_cannot_publish_state(self):
        response = b"E14\t1\t1\nok\t0\t1\t1\t0\t0\t0\t1\t1\nBAD\n"
        with BatchProcess(responder(response), 1, timeout=2) as batch:
            with self.assertRaisesRegex(ProtocolError, "trailing"):
                batch.reset([42])
            self.assertEqual(batch._states, {})
            self.assertTrue(batch._closed)

    def test_e14_s06_interrupt_invalidates_uncertain_connection(self):
        with BatchProcess(responder(b"E14\t1\t1\nok\t0\t1\t1\t0\t0\t0\t1\t1\n"), 1) as batch:
            def interrupted(*_):
                raise KeyboardInterrupt
            batch._line = interrupted
            with self.assertRaises(KeyboardInterrupt):
                batch.reset([42])
            self.assertTrue(batch._closed)
            self.assertEqual(batch._states, {})
            self.assertIsNotNone(batch._process.poll())
            with self.assertRaises(ProtocolError):
                batch.reset([42])

    def test_e14_s06_timeout_does_not_retry_unknown_operation(self):
        code = "import sys,time; sys.stdin.buffer.readline(); time.sleep(5)"
        with BatchProcess([sys.executable, "-u", "-c", code], 1, timeout=0.05) as batch:
            with self.assertRaisesRegex(ProtocolError, "timed out"):
                batch.reset([42])
            self.assertTrue(batch._closed)

    def test_e14_s06_validation_precedes_io(self):
        with BatchProcess(responder(b"E14\t1\t1\nok\t0\t1\t1\t0\t0\t0\t1\t1\n"), 1) as batch:
            for operations in [[], [("reset", 0, 1), ("reset", 0, 2)], [("reset", -1, 1)], [("reset", 0, True)], [("step", 0, 1, -1)]]:
                with self.assertRaises(ValueError):
                    batch.request(operations)
            self.assertIsInstance(batch.reset([42])[0], Transition)

    def test_e14_s06_errors_remain_explicit(self):
        with BatchProcess(responder(b"E14\t1\t1\nerror\t0\tmissing calibration input\n"), 1) as batch:
            with self.assertRaisesRegex(RejectedAction, "missing calibration"):
                batch.reset([42])

    def test_e14_s06_oversized_response_is_bounded(self):
        # Avoid OS argv limits for the oversized adversarial output.
        code = "import sys; sys.stdin.buffer.readline(); sys.stdin.buffer.readline(); sys.stdout.buffer.write(b'E14\\t1\\t1\\n'+b'x'*(64*(4096*32+1024+256)+1)); sys.stdout.buffer.flush()"
        with BatchProcess([sys.executable, "-u", "-c", code], 1, timeout=2) as batch:
            with self.assertRaisesRegex(ProtocolError, "byte limit"):
                batch.reset([42])


class RustIntegrationTests(unittest.TestCase):
    def test_e14_s01_seeded_replay_and_stale_actions(self):
        with BatchProcess(command(), 1) as batch:
            first = batch.reset([42])[0]
            second = batch.reset([42])[0]
            self.assertEqual(first.observation, second.observation)
            self.assertGreater(second.state, first.state)
            action = next(i for i, allowed in enumerate(second.action_mask) if allowed)
            stale = batch.request([("step", 0, first.state, action)])[0]
            self.assertIsInstance(stale, RejectedAction)
            good = batch.step([action])[0]
            self.assertIsInstance(good, Transition)
            self.assertGreater(good.state, second.state)

    def test_e14_s06_scalar_batch_equivalence(self):
        with BatchProcess(command(2), 2) as batch, BatchProcess(command(), 1) as scalar:
            b = batch.reset([7, 7])
            s = scalar.reset([7])[0]
            self.assertEqual(b, [s, s])
            action = next(i for i, allowed in enumerate(s.action_mask) if allowed)
            self.assertEqual(batch.step([action, action]), scalar.step([action]) * 2)
            errors = batch.step([999999, 999999])
            self.assertTrue(all(isinstance(e, RejectedAction) for e in errors))

    def test_e14_s06_real_batch_profile_has_bounded_measurement_schema(self):
        for size in (1, 8):
            raw = subprocess.check_output([
                sys.executable, str(ROOT / "training/profile_e14.py"),
                "--server", str(SERVER), "--size", str(size), "--samples", "2",
            ], text=True, timeout=30)
            report = json.loads(raw)
            self.assertEqual(report["batch_size"], size)
            self.assertEqual(len(report["step_raw_ns"]), 2)
            self.assertEqual(len(report["reset_raw_ns"]), 2)
            self.assertGreater(report["rust_process_peak_rss_bytes"], 0)
            self.assertGreater(report["successful_step_slots_per_second"], 0)
            self.assertEqual(len(report["source_sha256"]), 64)
            self.assertTrue(report["synthetic"])

    def test_e14_s01_actual_gymnasium_checker(self):
        from gymnasium.utils.env_checker import check_env
        with self.subTest("synthetic Rust environment"):
            env = GymEnvironment(command())
            try:
                check_env(env, skip_render_check=True)
            finally:
                env.close()


if __name__ == "__main__":
    unittest.main()
