#!/usr/bin/env python3
"""Prepare and rehearse private candidates; never publish, tag, or sign a release."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('comparison', ROOT / 'scripts/compare-core-packages.py')
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)
package = comparison.package


def environment(work):
    cargo = shutil.which('cargo')
    if not cargo:
        raise ValueError('Cargo unavailable')
    home = work / 'home'
    home.mkdir(mode=0o700)
    return {'PATH': str(Path(cargo).parent) + ':/usr/local/bin:/usr/bin:/bin',
            'HOME': str(home), 'LC_ALL': 'C', 'TMPDIR': str(work),
            'RUSTUP_HOME': os.environ.get('RUSTUP_HOME', str(Path.home() / '.rustup')),
            'CARGO_HOME': str(work / 'cargo'), 'CARGO_TARGET_DIR': str(work / 'target'),
            'CARGO_BUILD_JOBS': '2', 'GIT_CONFIG_NOSYSTEM': '1',
            'GIT_CONFIG_GLOBAL': '/dev/null'}


def prepare(output, expected):
    if sys.platform != 'linux' or not re.fullmatch(r'[0-9a-f]{40}', expected):
        raise ValueError('Linux and an exact lowercase 40-character commit required')
    output = output.absolute()
    if (output.exists() or output.is_symlink() or not output.parent.is_dir()
            or output.resolve() == ROOT or ROOT in output.resolve().parents):
        raise ValueError('new output directory outside checkout required')
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, package.interrupted)
    with tempfile.TemporaryDirectory(prefix='urdia-prepare-', dir='/tmp') as raw:
        work = Path(raw)
        env = environment(work)
        def command(args, timeout=60):
            return package.bounded(args, env, work, timeout)
        if command(['git', 'rev-parse', 'HEAD']).strip() != expected:
            raise ValueError('expected commit differs from checkout')
        if command(['git', 'status', '--porcelain', '--untracked-files=all']).strip():
            raise ValueError('clean checkout required')
        fingerprint = package.checkout_fingerprint(env, work)
        output.mkdir(mode=0o700)
        stages = []
        stage = 'initialization'
        report = {'schema': 'urdia.private-candidate-preparation/v1', 'source_commit': expected,
                  'checkout_input_sha256': fingerprint, 'source_commit_dirty': False,
                  'result': 'fail', 'stages': stages, 'publication_allowed': False,
                  'production_signature_created': False,
                  'scope': 'two separately cloned sources and isolated builds on one host; no cross-host or publisher identity claim'}
        try:
            def run(name, args, timeout=60):
                nonlocal stage
                stage = name
                print('Candidate preparation: ' + name, flush=True)
                start = time.monotonic()
                command(args, timeout)
                stages.append({'name': name, 'result': 'pass',
                               'duration_ms': round((time.monotonic() - start) * 1000)})
            clones = [work / 'source-first', work / 'different-source-second']
            candidates = [output / 'first', output / 'second']
            for index, (clone, candidate) in enumerate(zip(clones, candidates), 1):
                run(f'clone-{index}', ['git', 'clone', '--quiet', '--no-hardlinks',
                                      '--no-checkout', str(ROOT), str(clone)])
                run(f'checkout-{index}', ['git', '-C', str(clone), 'checkout', '--quiet',
                                         '--detach', expected])
                run(f'build-package-{index}', [sys.executable, '-I', '-B',
                    str(clone / 'scripts/package-core.py'), '--output-dir', str(candidate)], 1800)
            run('compare-packages', [sys.executable, '-I', '-B',
                str(ROOT / 'scripts/compare-core-packages.py'), '--first', str(candidates[0]),
                '--second', str(candidates[1]), '--evidence', str(output / 'comparison.json')])
            target = work / 'smoke-target'
            run('clean-source-smoke', ['bash', str(ROOT / 'scripts/smoke-test.sh'),
                '--source-dir', str(clones[0]), '--target-dir', str(target),
                '--evidence-dir', str(output / 'smoke')], 1800)
            fake = target / 'release/urdia-fake-agent'
            for index, candidate in enumerate(candidates, 1):
                run(f'packaged-recovery-{index}', [sys.executable, '-I', '-B',
                    str(ROOT / 'scripts/test-core-package.py'), '--candidate-dir', str(candidate),
                    '--fake-agent', str(fake), '--evidence', str(candidate / 'rehearsal.json')], 120)
            stage = 'final-source-and-evidence-check'
            if (command(['git', 'rev-parse', 'HEAD']).strip() != expected
                    or command(['git', 'status', '--porcelain', '--untracked-files=all']).strip()
                    or package.checkout_fingerprint(env, work) != fingerprint):
                raise ValueError('source changed during preparation')
            compared = json.loads(package.regular(output, 'comparison.json'), object_pairs_hook=package.pairs_unique)
            smoke = json.loads(package.regular(output, 'smoke/smoke-evidence.json'), object_pairs_hook=package.pairs_unique)
            recoveries = [json.loads(package.regular(c, 'rehearsal.json'), object_pairs_hook=package.pairs_unique)
                          for c in candidates]
            if (compared['source_commit'] != expected or compared['checkout_input_sha256'] != fingerprint
                    or compared['schema'] != 'urdia.independent-rebuild-comparison/v1'
                    or compared['result'] != 'pass' or compared['publication_allowed'] is not False
                    or smoke['schema_version'] != 'urdia.smoke-evidence/v1' or smoke['commit'] != expected
                    or smoke['commit_dirty'] is not False or smoke['result'] != 'pass'
                    or smoke['passed'] != 21 or smoke['failed'] != 0
                    or any(r['source_commit'] != expected or r['source_commit_dirty'] is not False
                           or r['schema'] != 'urdia.packaged-rehearsal/v1' or r['publication_allowed'] is not False
                           or r['result'] != 'pass' or r['runtime_removed'] is not True or len(r['checks']) != 10
                           or r['archive_sha256'] != compared['archive_sha256'] for r in recoveries)):
                raise ValueError('evidence does not match the selected candidate')
            if package.INTERRUPTED:
                raise ValueError('interrupted before completion')
            report.update(result='pass', archive_sha256=compared['archive_sha256'],
                          binary_sha256=compared['binary_sha256'], smoke_checks_passed=21,
                          recovery_checks_passed_each=10)
        except BaseException:
            report.update(result='fail', failed_stage=stage)
            comparison.save(report, output / 'preparation-evidence.json')
            raise
    comparison.save(report, output / 'preparation-evidence.json')
    print('Private preparation passed; disposable builds removed; no publication or production signing')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', type=Path, required=True)
    parser.add_argument('--expected-commit', required=True)
    args = parser.parse_args()
    prepare(args.output_dir, args.expected_commit)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f'preparation: {error}', file=sys.stderr)
        raise SystemExit(1)
