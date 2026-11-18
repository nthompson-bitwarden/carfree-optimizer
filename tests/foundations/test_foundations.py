"""E01-S01–06 foundations acceptance checks; no third-party test framework.

These are infrastructure/configuration scenarios, not scientific validation.
Run: python3 -m unittest discover -s tests/foundations -v
"""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[2]
CRATES = {
    "mobility-graph", "accessibility", "interventions", "mobility-sim",
    "optimizer-env", "mobility-ingest", "mobility-routing",
    "optimizer-bridge", "mobility-api",
}


def run(*args, cwd=ROOT, env=None):
    return subprocess.run(args, cwd=cwd, env=env, text=True,
                          capture_output=True, timeout=180)


class FoundationsAcceptance(unittest.TestCase):
    def script(self, name):
        path = ROOT / "scripts" / name
        self.assertTrue(path.is_file(), f"Required infrastructure entry point missing: {path}")
        return path

    def benchmark(self, output, *extra):
        return run(sys.executable, str(self.script("benchmark.py")),
                   "--output", str(output), "--samples", "2",
                   "--iterations", "100", *map(str, extra))

    def test_e01_s01_given_workspace_when_inspected_then_all_boundaries_are_members(self):
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
        actual = {tomllib.loads((ROOT / p / "Cargo.toml").read_text())["package"]["name"]
                  for p in workspace["workspace"]["members"]}
        self.assertTrue(CRATES <= actual, f"Missing isolated boundaries: {CRATES - actual}")

    def test_e01_s01_given_core_when_dependencies_inspected_then_no_service_or_training_coupling(self):
        for name in ("mobility-graph", "accessibility", "interventions", "mobility-sim"):
            manifest = tomllib.loads((ROOT / "crates" / name / "Cargo.toml").read_text())
            deps = manifest.get("dependencies", {})
            self.assertFalse({"mobility-api", "optimizer-bridge", "pyo3", "axum"} & set(deps))

    def test_e01_s02_given_setup_when_pin_inspected_then_exact_toolchain_components_and_lock(self):
        pin = ROOT / "rust-toolchain.toml"
        self.assertTrue(pin.is_file(), "A reproducible setup needs an exact Rust toolchain pin")
        toolchain = tomllib.loads(pin.read_text())["toolchain"]
        self.assertRegex(toolchain["channel"], r"^\d+\.\d+\.\d+$")
        self.assertTrue({"clippy", "rustfmt"} <= set(toolchain["components"]))
        self.assertTrue((ROOT / "Cargo.lock").is_file(), "Workspace lockfile must be committed")

    def test_e01_s02_given_any_working_directory_when_bootstrap_runs_then_smoke_succeeds(self):
        script = self.script("bootstrap.sh")
        with tempfile.TemporaryDirectory() as directory:
            result = run("/bin/bash", str(script), cwd=directory)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_e01_s02_given_missing_rustup_when_bootstrap_runs_then_error_is_actionable(self):
        script = self.script("bootstrap.sh")
        env = os.environ.copy()
        env["PATH"] = "/usr/bin:/bin"
        result = run("/bin/bash", str(script), env=env)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("rustup", (result.stdout + result.stderr).lower())

    def test_e01_s03_given_handoff_when_inspected_then_ownership_and_contract_fields_exist(self):
        text = (ROOT / "docs" / "AGENT-WORKFLOW.md").read_text()
        for field in ("Owned crates and files", "Public APIs", "Prohibited cross-module changes", "Tests / fixtures"):
            self.assertIn(field, text)
        contracts = ROOT / "docs" / "MODULE-CONTRACTS.md"
        self.assertTrue(contracts.is_file(), "Module contracts need an explicit ownership source")
        text = contracts.read_text()
        for crate in CRATES:
            self.assertIn(crate, text)

    def test_e01_s04_given_ci_when_inspected_then_linux_runner_and_required_gates_exist(self):
        text = (ROOT / ".github" / "workflows" / "rust-ci.yml").read_text()
        for value in ("runs-on: ubuntu-24.04", "cargo fmt --all --check",
                      "cargo clippy --workspace --all-targets", "cargo test --workspace",
                      "--locked", "check-dependencies.py", "tests/foundations"):
            self.assertIn(value, text)
        self.assertNotIn("macos", text.lower())
        self.assertTrue("workflow_dispatch:" in text or "codex/**" in text,
                        "Candidate CI must be runnable before opening a ready PR")
        self.assertNotIn("rust-toolchain@stable", text, "CI must honor the repository pin")

    def test_e01_s04_given_reviewed_workspace_when_dependency_policy_runs_then_accepts(self):
        result = run(sys.executable, str(self.script("check-dependencies.py")))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_e01_s04_given_registry_or_git_dependency_when_policy_runs_then_rejects(self):
        script = self.script("check-dependencies.py")
        for dependency in ('bad = "1"', 'bad = { git = "https://invalid.example/repo" }'):
            with self.subTest(dependency=dependency), tempfile.TemporaryDirectory() as directory:
                manifest = Path(directory) / "Cargo.toml"
                manifest.write_text('[package]\nname="policy-fixture"\nversion="0.1.0"\n'
                                    '[dependencies]\n' + dependency + '\n')
                result = run(sys.executable, str(script), "--manifest-path", str(manifest))
                self.assertNotEqual(result.returncode, 0, "External dependency must require review")
                self.assertIn("bad", result.stdout + result.stderr)

    def test_e01_s05_given_metric_fixture_when_benchmarked_twice_then_history_and_metadata_persist(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "history.jsonl"
            first = self.benchmark(output)
            self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
            prefix = output.read_bytes()
            second = self.benchmark(output)
            self.assertEqual(second.returncode, 0, second.stdout + second.stderr)
            self.assertTrue(output.read_bytes().startswith(prefix), "History must append without replacing prior runs")
            records = [json.loads(line) for line in output.read_text().splitlines()]
            self.assertEqual(len(records), 2)
            for record in records:
                for field in ("schema_version", "fixture", "fixture_sha256", "hardware", "toolchain",
                              "revision", "build_profile", "iterations", "unit", "samples", "synthetic"):
                    self.assertIn(field, record)
                self.assertTrue(record["synthetic"])
                self.assertEqual(record["iterations"], 100)
                self.assertEqual(record["build_profile"], "release")
                self.assertEqual(len(record["samples"]), 2)
                self.assertTrue(all(isinstance(x, (int, float)) and x > 0 for x in record["samples"]))
                self.assertTrue(record["hardware"])
                self.assertRegex(record["fixture_sha256"], r"^[0-9a-f]{64}$")
                self.assertRegex(record["revision"], r"^[0-9a-f]{40}$")

    def test_e01_s05_given_invalid_run_size_when_benchmarked_then_rejects_without_history(self):
        script = self.script("benchmark.py")
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "history.jsonl"
            result = run(sys.executable, str(script), "--output", str(output), "--samples", "0")
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists(), "Invalid arguments must not create benchmark evidence")

    def test_e01_s05_given_incompatible_baseline_when_compared_then_rejects_without_append(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "history.jsonl"
            result = self.benchmark(output)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            original = output.read_bytes()
            baseline = json.loads(output.read_text().splitlines()[0])
            path = Path(directory) / "baseline.jsonl"
            for field, changed in (("fixture_sha256", "0" * 64),
                                   ("iterations", 101), ("toolchain", "other compiler"),
                                   ("build_profile", "debug"), ("hardware", {"cpu": "other"})):
                with self.subTest(field=field):
                    path.write_text(json.dumps({**baseline, field: changed}) + "\n")
                    result = self.benchmark(output, "--baseline", path)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("incompatible", (result.stdout + result.stderr).lower())
                    self.assertEqual(output.read_bytes(), original)

    def test_e01_s05_given_compatible_baseline_when_compared_then_result_appends(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "history.jsonl"
            first = self.benchmark(output)
            self.assertEqual(first.returncode, 0, first.stdout + first.stderr)
            baseline = Path(directory) / "baseline.jsonl"
            baseline.write_bytes(output.read_bytes())
            result = self.benchmark(output, "--baseline", baseline)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(len(output.read_text().splitlines()), 2)

    def test_e01_s05_given_bad_history_schema_when_appending_then_rejects_without_modification(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "history.jsonl"
            original = '{"schema_version":999}\n'
            output.write_text(original)
            result = self.benchmark(output)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(output.read_text(), original)

    def test_e01_s05_given_repository_when_history_inspected_then_measured_baseline_is_stored(self):
        history = ROOT / "benchmarks" / "history.jsonl"
        self.assertTrue(history.is_file(), "Store an actual baseline, not a fabricated timing fixture")
        records = [json.loads(line) for line in history.read_text().splitlines()]
        self.assertGreater(len(records), 0)
        self.assertTrue(all(record["synthetic"] for record in records))

    def test_e01_s06_given_proposed_adrs_when_inspected_then_benchmark_criteria_and_exit_paths_exist(self):
        text = "\n".join(path.read_text() for path in (ROOT / "docs" / "adr").glob("*.md"))
        self.assertIn("Proposed", text)
        for term in ("benchmark", "exit", "simulat", "RL", "framework"):
            self.assertIn(term.lower(), text.lower())


if __name__ == "__main__":
    unittest.main()
