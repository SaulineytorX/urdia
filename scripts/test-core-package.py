#!/usr/bin/env python3
"""Offline installed-package rehearsal with disposable data; no model or service."""
import argparse
from contextlib import closing
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import sqlite3
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('package_core', ROOT / 'scripts/package-core.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def inspect(candidate):
    ev = json.loads(package.regular(candidate, 'package-evidence.json'), object_pairs_hook=package.pairs_unique)
    if ev['schema'] != 'urdia.private-package-evidence/v1' or ev['publication_allowed'] is not False:
        raise ValueError('unsupported candidate evidence')
    name = ev['archive']
    package.relative(name)
    if '/' in name or not name.endswith('.tar.gz'):
        raise ValueError('invalid archive name')
    root = name[:-7]
    archive = candidate / name
    if package.sha(package.regular(candidate, name, 128 * 1024 * 1024)) != ev['archive_sha256']:
        raise ValueError('archive checksum mismatch')
    if package.regular(candidate, 'SHA256SUMS') != f"{ev['archive_sha256']}  {name}\n".encode():
        raise ValueError('checksum record mismatch')
    records = {}
    for record in ev['files']:
        path = record['path']
        package.relative(path)
        if (path in records or type(record['size_bytes']) is not int
                or not 0 <= record['size_bytes'] <= 128 * 1024 * 1024):
            raise ValueError('invalid file manifest')
        if record['mode'] != ('0755' if path == 'bin/urdia' else '0644'):
            raise ValueError('invalid mode')
        records[path] = record
    if len(records) > 1000 or sum(r['size_bytes'] for r in records.values()) > 256 * 1024 * 1024:
        raise ValueError('oversized manifest')
    files = {}
    with tarfile.open(archive, 'r:gz') as tar:
        for member in tar:
            if member.isdir():
                continue
            if not member.name.startswith(root + '/'):
                raise ValueError('archive root mismatch')
            path = member.name[len(root) + 1:]
            if path not in records or not member.isfile() or path in files:
                raise ValueError('unexpected file')
            record = records[path]
            if member.size != record['size_bytes']:
                raise ValueError('size mismatch')
            data = tar.extractfile(member).read(member.size + 1)
            if package.sha(data) != record['sha256']:
                raise ValueError('file hash mismatch')
            files[path] = (data, int(record['mode'], 8))
    if set(files) != set(records) or 'bin/urdia' not in files:
        raise ValueError('missing file')
    package.verify_archive(archive, root, files, ev['archive_epoch'])
    package.validate_link_observations(candidate, ev, files["bin/urdia"][0])
    return ev, files


def install(files, prefix):
    prefix.mkdir()
    for name, (data, mode) in files.items():
        path = prefix / package.relative(name)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        path.chmod(mode)


def row_count(database):
    with closing(sqlite3.connect(database.as_uri() + '?mode=ro', uri=True)) as db:
        if db.execute('PRAGMA integrity_check').fetchone()[0] != 'ok':
            raise ValueError('database integrity failure')
        return db.execute('SELECT count(*) FROM events').fetchone()[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate-dir', type=Path, required=True)
    parser.add_argument('--fake-agent', type=Path, required=True)
    parser.add_argument('--evidence', type=Path, required=True)
    args = parser.parse_args()
    evidence = args.evidence.absolute()
    if evidence.exists() or evidence.is_symlink() or ROOT in evidence.resolve().parents:
        raise ValueError('evidence must be a new file outside checkout')
    fake = args.fake_agent.absolute()
    if fake.is_symlink() or not fake.is_file() or not os.access(fake, os.X_OK):
        raise ValueError('explicit fake-agent executable required')
    ev, files = inspect(args.candidate_dir.resolve())
    checks = ['archive-hashes-paths-modes-and-exact-files']
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, package.interrupted)
    with tempfile.TemporaryDirectory(prefix='urdia-package-rehearsal-') as raw:
        work = Path(raw)
        home, workspace, data, backup = [work / p for p in ('home', 'workspace', 'data', 'backup')]
        for p in (home, workspace, data, backup):
            p.mkdir(mode=0o700)
        prefix = work / 'installed'
        install(files, prefix)
        checks.append('install-entire-package-in-private-prefix')
        env = {'PATH': '/usr/bin:/bin', 'HOME': str(home), 'LC_ALL': 'C', 'TMPDIR': str(work)}
        config = work / 'providers.toml'
        config.write_text('[[provider]]\nid = "package-agent"\nname = "Package Agent"\ncommand = '
                          + json.dumps(str(fake)) + '\nargs = ["--stream"]\n')
        binary = prefix / 'bin/urdia'
        package.bounded([str(binary), 'doctor'], env, work)
        package.bounded([str(binary), '--config', str(config), 'providers'], env, work)
        checks.append('installed-doctor-and-provider-registration')
        database = data / 'events.sqlite'
        def run(prompt):
            result = package.bounded([str(binary), '--config', str(config), 'run', '--provider',
                                      'package-agent', '--workspace', str(workspace), '--prompt', prompt,
                                      '--database', str(database)], env, work)
            if 'fake-agent: ' + prompt not in result:
                raise ValueError('installed session failed')
        run('package baseline sentinel')
        baseline = row_count(database)
        if baseline < 1 or b'package baseline sentinel' in database.read_bytes():
            raise ValueError('content persistence or event failure')
        checks.append('installed-ACP-session-default-content-privacy')
        # Every bounded command has finished; no writer is active during copying.
        names = ('events.sqlite', 'events.sqlite-wal', 'events.sqlite-shm')
        for name in names:
            if (data / name).exists():
                shutil.copy2(data / name, backup / name)
        original_hash = package.sha((backup / names[0]).read_bytes())
        checks.append('quiescent-database-and-sidecar-backup')
        staged, previous = work / 'staged', work / 'previous'
        install(files, staged)
        prefix.rename(previous)
        staged.rename(prefix)
        if row_count(database) != baseline:
            raise ValueError('replacement altered data')
        run('package replacement sentinel')
        advanced = row_count(database)
        if advanced <= baseline:
            raise ValueError('replacement session did not append events')
        checks.append('same-version-package-replacement-preserves-and-appends-data')
        prefix.rename(work / 'replaced')
        previous.rename(prefix)
        for name in names:
            (data / name).unlink(missing_ok=True)
            if (backup / name).exists():
                shutil.copy2(backup / name, data / name)
        if package.sha(database.read_bytes()) != original_hash or row_count(database) != baseline:
            raise ValueError('restore differs from backup')
        checks.append('installation-rollback-and-exact-database-restore')
        timeline = work / 'timeline.html'
        package.bounded([str(binary), 'timeline', '--database', str(database), '--output', str(timeline)], env, work)
        if not timeline.is_file():
            raise ValueError('restored database not readable')
        run('package restored sentinel')
        if row_count(database) <= baseline:
            raise ValueError('restored database not writable')
        checks.append('restored-history-export-and-new-session')
        shutil.rmtree(prefix)
        if prefix.exists() or not database.exists():
            raise ValueError('uninstall affected data')
        checks.append('uninstall-prefix-preserves-user-data')
    checks.append('disposable-runtime-cleaned')
    report = {'schema': 'urdia.packaged-rehearsal/v1', 'result': 'pass', 'checks': checks,
              'archive_sha256': ev['archive_sha256'], 'source_commit': ev['commit'],
              'source_commit_dirty': ev['commit_dirty'], 'baseline_events': baseline,
              'replacement_events': advanced, 'runtime_removed': True,
              'upgrade_scope': 'same 0.1.0 candidate reinstalled; no cross-version/schema compatibility claim',
              'publication_allowed': False}
    with evidence.open('xb') as stream:
        stream.write(package.json_bytes(report))
    evidence.chmod(0o600)
    print(f'Packaged rehearsal: {len(checks)} checks passed; temporary data removed')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, sqlite3.Error) as error:
        print(f'package rehearsal: {error}', file=sys.stderr)
        raise SystemExit(1)
