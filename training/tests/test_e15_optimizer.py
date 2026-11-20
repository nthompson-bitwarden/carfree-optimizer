"""E15 acceptance over the real Rust fixture (build optimizer-bridge examples first)."""
from dataclasses import replace
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock

from carfree_env import ProtocolError, RejectedAction, Transition
from optimizer_benchmark import (
    DEFAULT_TEST, DEFAULT_TRAIN, Candidate, Evaluator, Policy, Scenario, benchmark, dashboard,
    fit_pair, pareto,
)

SERVER = Path(__file__).resolve().parents[2] / "target/debug/examples/e15_batch"


class OptimizerTests(unittest.TestCase):
    def test_e15_s01_s02_s06_paired_budget_and_dashboard(self):
        report = benchmark(SERVER, DEFAULT_TEST, fit_pair(SERVER, 6), 12)
        self.assertEqual(len(report["runs"]), len(DEFAULT_TEST) * 5)
        for run in report["runs"]:
            self.assertEqual(run["calls"]["resets"], len(run["episodes"]))
            self.assertEqual(run["calls"]["steps"], sum(len(e["actions"]) for e in run["episodes"]))
            spent = run["training_charge"] + sum(run["calls"].values())
            self.assertEqual(run["budget"], 12)
            self.assertEqual(run["training_charge"], 6 if run["method"] == "rl-guided" else 0)
            self.assertLessEqual(spent, 12)
            self.assertLess(12 - spent, 3)
            self.assertEqual(run["baseline"], 0)
            self.assertEqual(run["best"], max(e["reward"] for e in run["episodes"] if e["valid"]))
        html = dashboard(report)
        for heading in ("Objective improvement", "Validity rate", "Variance", "Wall seconds", "Warm-start"):
            self.assertIn(heading, html)
        json.dumps(report, allow_nan=False)

    def test_e15_s01_rejection_charges_call_and_transport_failure_aborts(self):
        process = Mock()
        process.reset.return_value = [Transition(1, 1, (0.0,) * 64, (True, False, False, True),
                                                0.0, False, False)]
        evaluator = Evaluator(process, 7, 4)
        process.step.return_value = [RejectedAction("post-simulation constraint")]
        candidate, _ = evaluator.episode(lambda *_: 0)
        self.assertEqual(candidate, Candidate((0,), None, None))
        self.assertEqual(evaluator.calls(), {"resets": 1, "steps": 1})
        self.assertFalse(evaluator.ready())
        process.step.side_effect = ProtocolError("uncertain outcome")
        with self.assertRaises(ProtocolError):
            Evaluator(process, 7, 3).episode(lambda *_: 0)
        self.assertEqual(process.step.call_count, 2)

    def test_e15_s04_pareto_keeps_tradeoffs_not_failures_or_dominated(self):
        stop = Candidate((3,), 0, 0)
        cheap = Candidate((0, 3), 1, 10)
        strong = Candidate((1, 3), 2, 20)
        self.assertEqual(pareto([strong, cheap, cheap, stop, Candidate((2, 3), 0.5, 12),
                                 Candidate((2,), None, None)]), [stop, cheap, strong])

    def test_e15_s03_s05_s07_frozen_replay_and_disjoint_splits(self):
        fitted = fit_pair(SERVER, 12)
        self.assertTrue(fitted.scratch.values)
        self.assertEqual(set(s.topology for s in DEFAULT_TRAIN) & set(s.topology for s in DEFAULT_TEST), set())
        self.assertEqual(set(s.demand for s in DEFAULT_TRAIN) & set(s.demand for s in DEFAULT_TEST), set())
        self.assertEqual(set(s.seed for s in DEFAULT_TRAIN) & set(s.seed for s in DEFAULT_TEST), set())
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "policy.json"
            fitted.scratch.save(path)
            before = path.read_bytes()
            first = benchmark(SERVER, DEFAULT_TEST, fitted, 18)
            second = benchmark(SERVER, DEFAULT_TEST, replace(fitted, scratch=Policy.load(path)), 18)
            self.assertEqual(first["ablation"]["pairs"], second["ablation"]["pairs"])
            self.assertEqual(first["ablation"]["reserved_credits_per_arm"], 12)
            for run, replay in zip(first["runs"], second["runs"]):
                self.assertEqual(run["episodes"], replay["episodes"])
            benchmark(SERVER, (Scenario(9, 5, 481),), fitted, 18)
            fitted.scratch.save(path)
            self.assertEqual(before, path.read_bytes())
            self.assertEqual(Policy.load(path).values, fitted.scratch.values)
            bad = json.loads(before)
            bad["schema"] = "another-catalog"
            path.write_text(json.dumps(bad))
            with self.assertRaises(ValueError):
                Policy.load(path)
            with self.assertRaises(ValueError):
                benchmark(SERVER, (Scenario(DEFAULT_TRAIN[0].topology, 5, 481),), fitted, 18)
