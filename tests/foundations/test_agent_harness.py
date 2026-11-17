"""E01-S03 harness acceptance: isolated synthetic fixtures, no live issue team."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('agent_harness', ROOT / 'scripts/agent-harness.py')
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)


def snapshot(input_tokens, output_tokens, cached=4, reasoning=2):
    return {'type': 'event_msg', 'payload': {'type': 'token_count', 'info': {'total_token_usage': {
        'input_tokens': input_tokens, 'output_tokens': output_tokens,
        'cached_input_tokens': cached, 'reasoning_output_tokens': reasoning}}}}


def response(rid, input_tokens=10, output_tokens=3):
    return {'type': 'response_item', 'payload': {'id': rid, 'usage': {
        'input_tokens': input_tokens, 'output_tokens': output_tokens,
        'input_tokens_details': {'cached_tokens': 4},
        'output_tokens_details': {'reasoning_tokens': 2}}}}


def meta(sid='synthetic-session'):
    return {'type': 'session_meta', 'payload': {'id': sid}}


class HarnessAcceptance(unittest.TestCase):
    def fixture(self, directory, name, records):
        path = Path(directory) / name
        path.write_text('\n'.join(json.dumps(record) for record in records) + '\n')
        return path

    def test_e01_s03_given_profiles_when_validated_then_models_links_and_safeguards_pass(self):
        result = harness.validate(ROOT)
        self.assertEqual(result['status'], 'PASS')
        self.assertEqual(result['profiles'], 10)
        self.assertEqual(result['runtime_models'], 'NOT OBSERVED')

    def test_e01_s03_given_wrong_model_or_broken_discovery_when_validated_then_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for folder in ('.codex/agents', '.codex/skills', '.agents/skills'):
                shutil.copytree(ROOT / folder, root / folder, symlinks=True)
            shutil.copy(ROOT / '.codex/config.toml', root / '.codex/config.toml')
            profile = root / '.codex/agents/verifier.toml'
            profile.write_text(profile.read_text().replace('gpt-6-luna', 'unavailable-model'))
            with self.assertRaisesRegex(AssertionError, 'model policy'):
                harness.validate(root)
            shutil.copy(ROOT / '.codex/agents/verifier.toml', profile)
            (root / '.agents/skills/issue-architecture').unlink()
            with self.assertRaisesRegex(AssertionError, 'discovery symlink'):
                harness.validate(root)

    def test_e01_s03_given_overlapping_sessions_when_usage_read_then_snapshots_not_added(self):
        with tempfile.TemporaryDirectory() as directory:
            first = self.fixture(directory, 'one.jsonl', [meta(), snapshot(10, 3), response('r1'), snapshot(30, 7)])
            second = self.fixture(directory, 'two.jsonl', [meta(), snapshot(30, 7), snapshot(40, 9)])
            result = harness.usage([first, second, first])
        self.assertEqual(result['totals'], {'input_tokens': 40, 'cached_input_tokens': 4,
                                           'output_tokens': 9, 'reasoning_output_tokens': 2})
        self.assertEqual(result['warnings'], [])

    def test_e01_s03_given_response_records_when_repeated_then_ids_deduplicate(self):
        with tempfile.TemporaryDirectory() as directory:
            path = self.fixture(directory, 'responses.jsonl', [meta(), response('a'), response('a'), response('b')])
            result = harness.usage([path, path])
        self.assertEqual(result['totals']['input_tokens'], 20)
        self.assertEqual(result['totals']['cached_input_tokens'], 8)
        self.assertIn('response-only telemetry may be incomplete', result['warnings'])

    def test_e01_s03_given_token_usage_records_when_overlapping_then_response_ids_deduplicate(self):
        record = {'type': 'token_usage_record', 'payload': {
            'session_id': 'synthetic-session', 'response_id': 'r1',
            'usage': {'input_tokens': 10, 'output_tokens': 3,
                      'cached_input_tokens': 4, 'reasoning_output_tokens': 2}}}
        with tempfile.TemporaryDirectory() as directory:
            path = self.fixture(directory, 'records.jsonl', [record, record])
            result = harness.usage([path])
            self.assertEqual(result['totals']['input_tokens'], 10)
            path = self.fixture(directory, 'records.jsonl', [meta(), record, snapshot(30, 7)])
            result = harness.usage([path])
            self.assertEqual(result['totals']['input_tokens'], 30)

    def test_e01_s03_given_distinct_sessions_when_usage_read_then_only_sessions_add(self):
        with tempfile.TemporaryDirectory() as directory:
            path = self.fixture(directory, 'sessions.jsonl', [meta('a'), snapshot(20, 4), meta('b'), snapshot(30, 7)])
            result = harness.usage([path])
        self.assertEqual(result['totals']['input_tokens'], 50)

    def test_e01_s03_given_missing_or_unattributed_usage_then_unknown_not_zero(self):
        for records in ([], [response('a')], [meta(), {'type': 'event_msg', 'payload': {'type': 'token_count'}}]):
            with self.subTest(records=records), tempfile.TemporaryDirectory() as directory:
                path = self.fixture(directory, 'missing.jsonl', records)
                result = harness.usage([path])
                self.assertIsNone(result['totals']['input_tokens'])
                self.assertTrue(result['warnings'])

    def test_e01_s03_given_conflicting_or_crossing_counters_then_ambiguity_is_visible(self):
        cases = ([response('a'), response('a', 20)], [snapshot(30, 4), snapshot(20, 5)],
                 [snapshot(30, 6), snapshot(10, 3)], [snapshot(3, 1, 4, 2)])
        for records in cases:
            with self.subTest(records=records), tempfile.TemporaryDirectory() as directory:
                path = self.fixture(directory, 'conflict.jsonl', [meta(), *records])
                result = harness.usage([path])
                self.assertIsNone(result['totals']['input_tokens'])
                self.assertTrue(result['warnings'])

    def test_e01_s03_given_response_id_in_two_sessions_then_attribution_is_ambiguous(self):
        with tempfile.TemporaryDirectory() as directory:
            path = self.fixture(directory, 'attribution.jsonl', [meta('a'), response('r'), meta('b'), response('r')])
            result = harness.usage([path])
        self.assertIsNone(result['totals']['input_tokens'])
        self.assertIn('response ID attributed to multiple sessions', result['warnings'])

    def test_e01_s03_given_missing_subsets_then_totals_do_not_invent_zero(self):
        with tempfile.TemporaryDirectory() as directory:
            record = snapshot(10, 3)
            del record['payload']['info']['total_token_usage']['cached_input_tokens']
            path = self.fixture(directory, 'subsets.jsonl', [meta(), record])
            result = harness.usage([path])
        self.assertEqual(result['totals']['input_tokens'], 10)
        self.assertIsNone(result['totals']['cached_input_tokens'])


class CandidateScenarios(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.git('init', '-q')
        self.git('config', 'user.name', 'Synthetic fixture')
        self.git('config', 'user.email', 'synthetic@example.invalid')
        (self.root / '.gitignore').write_text('/run/\n')
        (self.root / 'owned.txt').write_text('original\n')
        self.git('add', '.')
        self.git('-c', 'commit.gpgsign=false', 'commit', '-qm', 'synthetic fixture only')

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], stderr=subprocess.STDOUT)

    def checks(self, code):
        # Use a file so shell quoting never changes the fixture command.
        path = self.root / 'check.py'
        path.write_text(code)
        return harness.run_checks(self.root, self.root / 'run', [f'{sys.executable} {path}'])

    def test_e01_s03_given_ordinary_candidate_when_checks_pass_then_evidence_is_current(self):
        record = self.checks('print("synthetic success")')
        self.assertEqual(record['status'], 'PASS')
        self.assertEqual(harness.evidence_status(record, harness.candidate(self.root)), 'PASS')
        self.assertEqual(record['suite'], 'custom')

    def test_e01_s03_given_failed_verification_when_reviewer_corrects_then_old_pass_is_invalid(self):
        record = self.checks('raise SystemExit(3)')
        self.assertEqual(record['status'], 'FAIL')
        self.assertEqual(record['results'][0]['exit_code'], 3)
        corrected = self.checks('print("correction")')
        self.assertEqual(corrected['status'], 'PASS')
        self.assertEqual(harness.evidence_status(record, harness.candidate(self.root)), 'STALE')

    def test_e01_s03_given_dirty_worktree_when_tracked_untracked_or_index_changes_then_invalidated(self):
        baseline = harness.candidate(self.root)
        owned = self.root / 'owned.txt'
        owned.write_text('modified\n')
        dirty = harness.candidate(self.root)
        self.assertNotEqual(dirty, baseline)
        self.git('add', 'owned.txt')
        self.assertNotEqual(harness.candidate(self.root), dirty)
        (self.root / 'untracked.txt').write_text('first')
        first = harness.candidate(self.root)
        (self.root / 'untracked.txt').write_text('second')
        self.assertNotEqual(harness.candidate(self.root), first)
        ignored = self.root / 'run'
        ignored.mkdir()
        before = harness.candidate(self.root)
        (ignored / 'log').write_text('ignored evidence')
        self.assertEqual(harness.candidate(self.root), before)
        self.assertEqual(owned.read_text(), 'modified\n')

    def test_e01_s03_given_candidate_changes_during_check_then_success_is_stale(self):
        record = self.checks('from pathlib import Path\nPath("owned.txt").write_text("changed")')
        self.assertEqual(record['results'][0]['status'], 'PASS')
        self.assertEqual(record['status'], 'STALE')
        self.assertEqual(harness.evidence_status(record, harness.candidate(self.root)), 'STALE')

    def test_e01_s03_given_change_then_restore_between_checks_then_suite_remains_stale(self):
        command = f'{sys.executable} -c '
        record = harness.run_checks(self.root, self.root / 'run', [
            command + '\'from pathlib import Path; Path("owned.txt").write_text("changed")\'',
            command + '\'from pathlib import Path; Path("owned.txt").write_text("original\\n")\''])
        self.assertEqual(record['candidate_before'], record['candidate_after'])
        self.assertEqual(record['status'], 'STALE')
        self.assertEqual(harness.evidence_status(record, harness.candidate(self.root)), 'STALE')

    def test_e01_s03_given_nonstandard_rustup_location_then_pinned_tools_precede_wrong_cargo(self):
        shim = self.root / 'custom-bin'
        tools = self.root / 'pinned-bin'
        shim.mkdir()
        tools.mkdir()
        (self.root / 'rust-toolchain.toml').write_text('[toolchain]\nchannel="1.2.3"\n')
        scripts = {shim / 'rustup': f'#!/bin/sh\ntest "$*" = "which cargo --toolchain 1.2.3" || exit 8\nprintf "%s\\n" "{tools}/cargo"\n',
                   shim / 'cargo': '#!/bin/sh\necho "wrong compiler"\n',
                   tools / 'cargo': '#!/bin/sh\necho "pinned cargo 1.2.3"\n'}
        for path, content in scripts.items():
            path.write_text(content)
            path.chmod(0o755)
        with patch.dict(os.environ, {'PATH': str(shim) + os.pathsep + os.environ['PATH']}):
            env = harness.required_environment(self.root)
            result = subprocess.check_output(['cargo', '--version'], env=env, text=True)
        self.assertEqual(result.strip(), 'pinned cargo 1.2.3')
        (shim / 'rustup').write_text('#!/bin/sh\nexit 7\n')
        with patch.dict(os.environ, {'PATH': str(shim) + os.pathsep + os.environ['PATH']}):
            with self.assertRaises(subprocess.CalledProcessError):
                harness.required_environment(self.root)

    def test_e01_s03_given_missing_tool_or_timeout_then_blocked_evidence(self):
        record = harness.run_checks(self.root, self.root / 'run', ['unavailable-synthetic-model-tool'])
        self.assertEqual(record['results'][0]['status'], 'BLOCKED')
        self.assertEqual(record['status'], 'BLOCKED')
        record = harness.run_checks(
            self.root, self.root / 'run', [f'{sys.executable} -c "import time; time.sleep(2)"'], timeout=0.01)
        self.assertEqual(record['results'][0]['status'], 'BLOCKED')
        self.assertEqual(record['status'], 'BLOCKED')


if __name__ == '__main__':
    unittest.main()
