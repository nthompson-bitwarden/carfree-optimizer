"""E14-S07/S08 acceptance: replay, held-out isolation, equal compute and audits."""
import os
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
EXPERIMENT = Path(os.environ.get("E14_EXPERIMENT", ROOT / "target/debug/examples/e14_experiments"))


class ExperimentTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.output = subprocess.check_output([str(EXPERIMENT)], text=True, timeout=120)
        cls.rows = [row.split("\t") for row in cls.output.splitlines()]

    def test_e14_s08_deterministic_replay(self):
        again = subprocess.check_output([str(EXPERIMENT)], text=True, timeout=120)
        self.assertEqual(self.output, again)

    def test_e14_s03_s08_heldout_data_cannot_change_fitted_policy_or_scaling(self):
        alternate = subprocess.check_output([str(EXPERIMENT), "--heldout-alt"], text=True, timeout=120)
        frozen = [row for row in self.output.splitlines() if row.startswith(("normalizer\t", "policy\t", "compute\t"))]
        other = [row for row in alternate.splitlines() if row.startswith(("normalizer\t", "policy\t", "compute\t"))]
        self.assertEqual(frozen, other)
        self.assertIn("split\ttrain\t0,1", self.output)
        self.assertIn("split\theldout\t4,7", self.output)
        self.assertIn("split\theldout\t5,8", alternate)

    def test_e14_s07_decomposition_and_trivial_baselines_are_logged(self):
        self.assertTrue(any(r[0] == "component" for r in self.rows))
        self.assertTrue(any(r[0] == "constraint" for r in self.rows))
        self.assertTrue(any(r[0] == "config" for r in self.rows))
        failures = [r for r in self.rows if r[0] == "diagnostic-constraint" and r[-1] == "false"]
        self.assertTrue(any(r[2] == "newly_unserved_noncycling" for r in failures))
        self.assertTrue(any(r[2] == "newly_unserved_step_free" for r in failures))
        self.assertIn("diagnostic\tknown-capacity-denial\trejected-without-mutation", self.output)
        self.assertTrue(any(r[0] == "diagnostic-component" for r in self.rows))
        outcomes = [r for r in self.rows if r[0] == "outcome" and r[3] == "heldout"]
        self.assertEqual(len(outcomes), 4 * 2 * 2 * 3)
        self.assertTrue(all(len(r) == 15 and float(r[6]) > 0 for r in outcomes))
        self.assertIn("deterministic replay spread only, not stochastic uncertainty", self.output)
        ablations = {r[1] for r in self.rows if r[0] == "policy"}
        self.assertEqual(ablations, {"full", "no-car-required", "no-parity", "feasibility-only"})
        baseline = [r for r in self.rows if r[0] == "episode" and r[3] == "stop-baseline"]
        self.assertEqual(len(baseline), 4 * 2 * 2 * 3)
        self.assertTrue(all(float(r[6]) == 0 and r[8] == "true" for r in baseline))
        for row in self.rows:
            if row[0] == "component":
                raw, denominator, weight, scale, weighted = map(float, row[8:13])
                self.assertGreater(denominator, 0)
                self.assertGreater(scale, 0)
                # Stop gains are zero; all non-stop rows use this exact formula.
                self.assertAlmostEqual(weighted, raw / scale * weight, places=10)

    def test_e14_s08_representations_share_heldout_seeds_and_budgets(self):
        by_representation = {}
        for row in self.rows:
            if row[0] == "episode" and row[3] == "heldout":
                by_representation.setdefault(row[2], set()).add((row[1], row[4], row[5]))
        self.assertEqual(set(by_representation), {"vector-linear", "graph-message-linear"})
        self.assertEqual(by_representation["vector-linear"], by_representation["graph-message-linear"])
        summaries = [r for r in self.rows if r[0] == "summary"]
        self.assertEqual(len(summaries), 4 * 2 * 2)
        self.assertTrue(all("n=3" in r and r[-1] == "no-confidence-interval" for r in summaries))
        self.assertIn("parameters=12\tcandidates=8", self.output)
        self.assertIn("synthetic-only", self.output)


if __name__ == "__main__":
    unittest.main()
