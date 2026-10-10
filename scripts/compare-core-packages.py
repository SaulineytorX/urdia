#!/usr/bin/env python3
"""Compare two private clean core candidates without executing their binaries."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('rehearsal', ROOT / 'scripts/test-core-package.py')
rehearsal = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rehearsal)
package = rehearsal.package
IDENTITY_FIELDS = ('commit', 'checkout_input_sha256', 'plan_sha256', 'source_inventory_sha256',
                   'target', 'rustc', 'c_compiler', 'archive', 'archive_epoch',
                   'registry_packages', 'elf_needed', 'glibc_symbol_versions', 'build_normalization')


def record_hashes(candidate, name):
    evidence = json.loads(package.regular(candidate, 'package-evidence.json'), object_pairs_hook=package.pairs_unique)
    paths = [name, 'package-evidence.json', 'SHA256SUMS']
    if 'link_observations' in evidence:
        paths.append('link-observations.json')
    return {path: package.sha(package.regular(candidate, path, 128 * 1024 * 1024))
            for path in paths}


def inspect(candidate):
    if candidate.is_symlink() or not candidate.is_dir():
        raise ValueError('regular candidate directory required')
    original = package.regular(candidate, 'package-evidence.json')
    evidence = json.loads(original, object_pairs_hook=package.pairs_unique)
    name = evidence['archive']
    package.relative(name)
    if '/' in name or not name.endswith('.tar.gz'):
        raise ValueError('invalid archive name')
    before = record_hashes(candidate, name)
    loaded, files = rehearsal.inspect(candidate)
    if (loaded != evidence or package.regular(candidate, 'package-evidence.json') != original
            or record_hashes(candidate, name) != before):
        raise ValueError('candidate changed during inspection')
    if evidence['commit_dirty'] is not False:
        raise ValueError('clean candidates required')
    if not re.fullmatch(r'[0-9a-f]{40}', evidence['commit']):
        raise ValueError('invalid source commit')
    for field in ('checkout_input_sha256', 'plan_sha256', 'source_inventory_sha256'):
        if not re.fullmatch(r'[0-9a-f]{64}', evidence[field]):
            raise ValueError('invalid source hash')
    if evidence['target'] != package.TARGET:
        raise ValueError('unsupported target')
    if (type(evidence['registry_packages']) is not int or evidence['registry_packages'] < 1
            or type(evidence['archive_epoch']) is not int or evidence['archive_epoch'] < 0):
        raise ValueError('invalid candidate metadata')
    for field in IDENTITY_FIELDS:
        if field not in evidence:
            raise ValueError('missing build identity')
    return evidence, files, before


def compare(first, second):
    first, second = first.absolute(), second.absolute()
    if first.resolve() == second.resolve():
        raise ValueError('two distinct candidate directories required')
    a, a_files, a_hashes = inspect(first)
    b, b_files, b_hashes = inspect(second)
    for field in IDENTITY_FIELDS:
        if a[field] != b[field]:
            raise ValueError('candidate identities differ: ' + field)
    if a.get('link_observations') != b.get('link_observations'):
        raise ValueError('link observations differ')
    if a['files'] != b['files'] or a_files != b_files:
        raise ValueError('candidate file contents or manifests differ')
    if a['archive_sha256'] != b['archive_sha256']:
        raise ValueError('archive bytes differ')
    if (record_hashes(first, a['archive']) != a_hashes
            or record_hashes(second, b['archive']) != b_hashes):
        raise ValueError('candidate changed during comparison')
    return {'schema': 'urdia.independent-rebuild-comparison/v1', 'result': 'pass',
            'source_commit': a['commit'], 'source_commit_dirty': False,
            'checkout_input_sha256': a['checkout_input_sha256'],
            'archive_sha256': a['archive_sha256'],
            'binary_sha256': package.sha(a_files['bin/urdia'][0]),
            'file_count': len(a_files), 'registry_packages': a['registry_packages'],
            'input_record_hashes': {'first': a_hashes, 'second': b_hashes},
            'compared_identity_fields': list(IDENTITY_FIELDS),
            'scope': 'validated matching candidate bytes and declared build identities; build independence must be recorded separately',
            'build_independence_verified': False,
            'different_host_reproducibility_verified': False,
            'publisher_identity_verified': False, 'publication_allowed': False}


def save(report, output):
    output = output.absolute()
    if output.resolve() == ROOT or ROOT in output.resolve().parents:
        raise ValueError('evidence must be outside checkout')
    # Publish a complete private report atomically without replacing any output.
    with tempfile.NamedTemporaryFile(prefix='.urdia-comparison-', dir=output.parent,
                                     delete=False) as stream:
        temporary = Path(stream.name)
        try:
            stream.write(package.json_bytes(report))
            stream.flush()
            os.fsync(stream.fileno())
        except BaseException:
            temporary.unlink()
            raise
    try:
        os.link(temporary, output)  # Same filesystem; fails if the name exists.
    finally:
        temporary.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--first', type=Path, required=True)
    parser.add_argument('--second', type=Path, required=True)
    parser.add_argument('--evidence', type=Path, required=True)
    args = parser.parse_args()
    report = compare(args.first, args.second)
    save(report, args.evidence)
    print('Candidate bytes and declared identities match; private evidence saved')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, tarfile.TarError) as error:
        print(f'comparison: {error}', file=sys.stderr)
        raise SystemExit(1)
