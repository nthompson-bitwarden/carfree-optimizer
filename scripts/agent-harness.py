#!/usr/bin/env python3
"""E01-S03 local harness validation, candidate-bound checks, and usage accounting."""
import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import shlex
import subprocess
import sys
import time
import tomllib
import uuid

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_COMMANDS = [
    'python3 scripts/agent-harness.py validate',
    'python3 -m unittest discover -s tests/foundations -v',
    'cargo fmt --all --check',
    'cargo clippy --workspace --all-targets -- -D warnings',
    'cargo test --workspace',
]
FIELDS = ('input_tokens', 'cached_input_tokens', 'output_tokens', 'reasoning_output_tokens')


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args])


def candidate(root):
    """Hash HEAD, index (including modes), and nonignored worktree contents/modes."""
    digest = hashlib.sha256()
    head = git(root, 'rev-parse', 'HEAD').decode().strip()
    digest.update(head.encode())
    digest.update(git(root, 'ls-files', '--stage', '-z'))
    paths = set(git(root, 'ls-files', '--cached', '--others', '--exclude-standard', '-z').split(b'\0'))
    for raw in sorted(paths - {b''}):
        path = root / os.fsdecode(raw)
        digest.update(len(raw).to_bytes(8, 'big') + raw)
        if path.is_symlink():
            content = b'link:' + os.fsencode(os.readlink(path))
        elif path.is_file():
            content = b'file:' + str(path.stat().st_mode & 0o777).encode() + b':' + path.read_bytes()
        elif path.exists():
            raise ValueError(f'Unsupported candidate path (e.g. submodule): {path}')
        else:
            content = b'missing'
        digest.update(hashlib.sha256(content).digest())
    return {'head': head, 'tree_sha256': digest.hexdigest()}


def validate(root):
    config = tomllib.loads((root / '.codex/config.toml').read_text())
    assert (config['model'], config['model_reasoning_effort']) == ('gpt-6-sol', 'medium'), 'primary model policy'
    agents = config['agents']
    assert agents['enabled'] is True and agents['max_concurrent_threads_per_session'] == 2, 'worker cap'
    assert (agents['default_subagent_model'], agents['default_subagent_reasoning_effort']) == ('gpt-6-sol', 'medium'), 'worker defaults'
    names = set()
    for path in sorted((root / '.codex/agents').glob('*.toml')):
        data = tomllib.loads(path.read_text())
        name = data['name']
        assert name not in names and name == path.stem, f'duplicate/mismatched profile: {path}'
        names.add(name)
        for key in ('description', 'developer_instructions'):
            assert isinstance(data[key], str) and data[key].strip(), f'{path}: {key}'
        expected = ('gpt-6-luna', 'low') if name == 'verifier' else ('gpt-6-sol', 'medium')
        assert (data['model'], data['model_reasoning_effort']) == expected, f'{path}: model policy'
        words = data['developer_instructions'].split()
        links = [word.rstrip(';.,') for word in words if word.startswith('.codex/skills/')]
        assert len(links) == 1 and (root / links[0]).is_file(), f'{path}: skill link'
    assert names == {'architect', 'code_reviewer', 'delivery', 'orchestrator', 'python_implementor',
                     'rust_implementor', 'technical_documentor', 'test_author', 'verifier', 'web_implementor'}, 'profile inventory'
    for skill in (root / '.codex/skills').iterdir():
        text = (skill / 'SKILL.md').read_text()
        assert text.startswith('---\n'), f'{skill}: frontmatter'
        front = text.split('---', 2)[1]
        assert f'name: {skill.name}\n' in front and '\ndescription: ' in front, f'{skill}: metadata'
        link = root / '.agents/skills' / skill.name
        assert link.is_symlink() and link.resolve() == skill.resolve(), f'{skill}: discovery symlink'
    assert git(root, 'check-ignore', '.codex/run/probe').strip(), 'run directory must be ignored'
    workflow = (root / 'docs/AGENT-WORKFLOW.md').read_text()
    delivery = (root / 'docs/AGENT-DELIVERY.md').read_text()
    guide = (root / 'AGENTS.md').read_text()
    for value in ('fork_turns="none"', 'one at a time', 'No automatic escalation', 'BLOCKED',
                  'Owned crates and files', 'Public APIs', 'Prohibited cross-module changes', 'Tests / fixtures'):
        assert value in workflow, f'workflow safeguard: {value}'
    for value in ('origin/main', 'git commit -S', 'git verify-commit', 'verified: true',
                  'reason: valid', 'nthompson-bitwarden', 'configured email', 'verified GitHub noreply', 'unrelated', 'force-with-lease',
                  'local paths', 'environment details', 'commit', 'bookkeeping', 'artifact hashes'):
        assert value in delivery, f'delivery safeguard: {value}'
    for value in ('Never silently disconnect', 'Do not invent observed', 'uncertainty', 'unable to cycle'):
        assert value in guide, f'scientific safeguard: {value}'
    return {'status': 'PASS', 'profiles': len(names), 'configured_primary': 'gpt-6-sol/medium',
            'configured_verifier': 'gpt-6-luna/low', 'runtime_models': 'NOT OBSERVED'}


def evidence_status(record, current):
    if record['candidate_before'] != current or record['candidate_after'] != current:
        return 'STALE'
    if record.get('status') != 'PASS':
        return record.get('status', 'BLOCKED')
    if not record.get('results') or any(r['status'] != 'PASS' for r in record['results']):
        return 'FAIL'
    return 'PASS'


def required_environment(root):
    """Resolve the installed pin through rustup, independent of shim layout."""
    env = os.environ.copy()
    pin = tomllib.loads((root / 'rust-toolchain.toml').read_text())['toolchain']['channel']
    tool = subprocess.check_output(['rustup', 'which', 'cargo', '--toolchain', pin],
                                   cwd=root, env=env, text=True, stderr=subprocess.STDOUT).strip()
    if not Path(tool).is_file():
        raise ValueError(f'rustup did not resolve installed cargo for {pin}')
    env['PATH'] = str(Path(tool).parent) + os.pathsep + env.get('PATH', '')
    return env


def run_checks(root, output, commands, timeout=300):
    output.mkdir(parents=True, exist_ok=True)
    before = candidate(root)
    run_id = uuid.uuid4().hex[:12]
    env = required_environment(root) if commands == DEFAULT_COMMANDS else os.environ.copy()
    versions = {'python': sys.version.split()[0], 'platform': platform.platform()}
    if commands == DEFAULT_COMMANDS:
        for tool in ('cargo', 'rustc'):
            try:
                versions[tool] = subprocess.check_output([tool, '--version'], env=env, cwd=root, text=True, stderr=subprocess.STDOUT).strip()
            except (OSError, subprocess.SubprocessError) as error:
                versions[tool] = f'UNAVAILABLE: {error}'
    results = []
    changed = False
    for index, command in enumerate(commands):
        log = output / f'{run_id}-{index}.log'
        command_before = candidate(root)
        changed |= command_before != before
        start = time.monotonic()
        status, code = 'BLOCKED', None
        with log.open('wb') as stream:
            try:
                proc = subprocess.run(shlex.split(command), cwd=root, stdout=stream,
                                      stderr=subprocess.STDOUT, timeout=timeout, env=env)
                code = proc.returncode
                status = 'PASS' if code == 0 else 'FAIL'
            except (OSError, subprocess.TimeoutExpired) as error:
                stream.write(str(error).encode())
        command_after = candidate(root)
        changed |= command_after != before
        results.append({'command': command, 'status': status, 'exit_code': code,
                        'candidate_before': command_before, 'candidate_after': command_after,
                        'seconds': round(time.monotonic() - start, 3), 'log': str(log.resolve())})
        print(f'{status}: {command} ({log})')
        if status != 'PASS':
            with log.open('rb') as stream:
                stream.seek(max(0, log.stat().st_size - 2000))
                print(stream.read().decode(errors='replace'))
    after = candidate(root)
    statuses = {r['status'] for r in results}
    status = ('STALE' if changed or before != after else 'FAIL' if 'FAIL' in statuses
              else 'BLOCKED' if 'BLOCKED' in statuses else 'PASS')
    record = {'schema_version': 1, 'candidate_before': before, 'candidate_after': after,
              'suite': 'required' if commands == DEFAULT_COMMANDS else 'custom',
              'status': status, 'results': results, 'versions': versions}
    path = output / f'{run_id}-checks.json'
    serialized = json.dumps(record, indent=2) + '\n'
    path.write_text(serialized)
    (output / 'checks.json').write_text(serialized)
    print(f'{status}: {record["suite"]} suite; evidence {path}')
    return record


def normalize_usage(value):
    """Normalize Codex counters or Responses API counters without adding subsets."""
    if not isinstance(value, dict):
        raise ValueError('usage is not an object')
    result = {}
    for field in FIELDS:
        raw = value.get(field)
        if raw is None and field == 'cached_input_tokens':
            raw = value.get('input_tokens_details', {}).get('cached_tokens')
        if raw is None and field == 'reasoning_output_tokens':
            raw = value.get('output_tokens_details', {}).get('reasoning_tokens')
        if raw is not None and (type(raw) is not int or raw < 0):
            raise ValueError(f'invalid {field}')
        result[field] = raw
    if result['input_tokens'] is None or result['output_tokens'] is None:
        raise ValueError('missing input/output telemetry')
    for subset, parent in (('cached_input_tokens', 'input_tokens'), ('reasoning_output_tokens', 'output_tokens')):
        if result[subset] is not None and result[subset] > result[parent]:
            raise ValueError(f'{subset} exceeds {parent}')
    return result


def usage(paths):
    sessions, warnings, observed_models = {}, set(), set()
    response_sessions = {}
    def group(session):
        return sessions.setdefault(session, {'snapshots': [], 'responses': {}, 'ambiguous': False})
    for path in paths:
        session = None
        previous_snapshot = None
        for line_number, line in enumerate(path.read_text().splitlines(), 1):
            try:
                record = json.loads(line)
                payload = record.get('payload', {})
                if record.get('type') == 'session_meta':
                    session = payload.get('id')
                    previous_snapshot = None
                    continue
                if record.get('type') == 'turn_context' and payload.get('model'):
                    observed_models.add(payload['model'])
                snapshot = (record.get('type') == 'event_msg' and payload.get('type') == 'token_count')
                response = record.get('response', record)
                if record.get('type') in ('response_item', 'token_usage_record'):
                    response = payload
                raw = payload.get('info', {}).get('total_token_usage') if snapshot else response.get('usage')
                if raw is None:
                    if snapshot:
                        warnings.add('token_count has no cumulative usage')
                    continue
                sid = record.get('session_id') or response.get('session_id') or session
                if not sid:
                    warnings.add('unattributed usage omitted')
                    continue
                data = group(sid)
                counters = normalize_usage(raw)
                if any(value is None for value in counters.values()):
                    warnings.add('some subset telemetry is missing; unknown totals retained')
                if snapshot:
                    if previous_snapshot and any(counters[f] is not None and previous_snapshot[f] is not None and counters[f] < previous_snapshot[f] for f in FIELDS):
                        data['ambiguous'] = True
                        warnings.add('cumulative counters decreased; possible reset')
                    previous_snapshot = counters
                    data['snapshots'].append(counters)
                else:
                    rid = response.get('response_id') or response.get('id')
                    if not rid:
                        warnings.add('response usage without id omitted')
                        data['ambiguous'] = True
                    elif rid in data['responses'] and data['responses'][rid] != counters:
                        warnings.add('conflicting response usage')
                        data['ambiguous'] = True
                    else:
                        if rid in response_sessions and response_sessions[rid] != sid:
                            warnings.add('response ID attributed to multiple sessions')
                            data['ambiguous'] = True
                            group(response_sessions[rid])['ambiguous'] = True
                        response_sessions[rid] = sid
                        data['responses'][rid] = counters
            except (ValueError, TypeError, AttributeError) as error:
                warnings.add(f'{path.name}:{line_number}: {error}')
                if session:
                    group(session)['ambiguous'] = True
    totals = dict.fromkeys(FIELDS, 0)
    sources = {}
    for sid, data in sessions.items():
        snapshots = data['snapshots']
        if snapshots:
            # A cumulative maximum must dominate every prior observation; crossing
            # counters signal resets/conflicts and cannot be summed reliably.
            maximum = max(snapshots, key=lambda x: x['input_tokens'] + x['output_tokens'])
            for sample in snapshots:
                if (sample['input_tokens'], sample['output_tokens']) == (maximum['input_tokens'], maximum['output_tokens']) and sample != maximum:
                    data['ambiguous'] = True
                    warnings.add('conflicting cumulative snapshots')
                if any(sample[f] is not None and maximum[f] is not None and sample[f] > maximum[f] for f in FIELDS):
                    data['ambiguous'] = True
                    warnings.add('crossing cumulative snapshots; possible reset')
            counters, source = maximum, 'cumulative snapshot; response records excluded'
        else:
            counters = {f: (None if any(x[f] is None for x in data['responses'].values()) else
                            sum(x[f] for x in data['responses'].values())) for f in FIELDS}
            source = 'unique response IDs; coverage unknown'
            warnings.add('response-only telemetry may be incomplete')
        if data['ambiguous']:
            warnings.add('ambiguous session omitted from totals')
            sources[sid] = 'AMBIGUOUS'
            continue
        sources[sid] = source
        for field in FIELDS:
            totals[field] = None if totals[field] is None or counters[field] is None else totals[field] + counters[field]
    if not sessions or not any(source != 'AMBIGUOUS' for source in sources.values()):
        totals = dict.fromkeys(FIELDS, None)
        warnings.add('no attributable usage telemetry')
    return {'schema_version': 1, 'totals': totals, 'sessions': sources,
            'observed_models': sorted(observed_models), 'warnings': sorted(warnings),
            'limitations': 'Cached input and reasoning output are subsets. No cost, daily-limit percentage, or savings inferred.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    sub.add_parser('validate')
    checks = sub.add_parser('checks')
    checks.add_argument('--output', type=Path, default=ROOT / '.codex/run')
    checks.add_argument('--command', action='append')
    checks.add_argument('--timeout', type=int, default=300)
    checks.add_argument('--verify', type=Path)
    report = sub.add_parser('usage')
    report.add_argument('paths', type=Path, nargs='+')
    args = parser.parse_args()
    try:
        if args.action == 'validate':
            print(json.dumps(validate(ROOT), indent=2))
        elif args.action == 'usage':
            print(json.dumps(usage(args.paths), indent=2))
        elif args.verify:
            record = json.loads(args.verify.read_text())
            status = evidence_status(record, candidate(ROOT))
            print(f'{status}: {record["suite"]} suite')
            return 0 if status == 'PASS' else 1
        else:
            output = args.output.resolve()
            output.relative_to((ROOT / '.codex/run').resolve())
            if args.timeout < 1:
                raise ValueError('timeout must be positive')
            record = run_checks(ROOT, output, args.command or DEFAULT_COMMANDS, args.timeout)
            return 0 if record['status'] == 'PASS' else 1
    except (OSError, ValueError, KeyError, AssertionError, subprocess.SubprocessError) as error:
        print(f'BLOCKED: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
