#!/usr/bin/env python3
"""Record bounded core license choices and retained notice coverage, offline."""
import argparse
from collections import Counter
import importlib.util
import io
import json
from pathlib import Path
import re
import sys
import tarfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('comparator', ROOT / 'scripts/compare-core-packages.py')
comparator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparator)
package = comparator.package
NOTICE_SIGNAL = re.compile(rb'copyright|spdx-license|licensed under|permission (?:is|to)|redistribution and use|all rights reserved|license agreement', re.I)
SOURCE_SUFFIXES = ('.rs', '.c', '.h', '.S', '.pl', '.py', '.asm', '.inc')


def embedded_coverage(archive, members, prefix, item):
    grouped = {}
    for ref in item.get('embedded_headers', []):
        grouped.setdefault(ref['source_path'], []).append(ref)
    source_files = signals = references = 0
    for path, member in members.items():
        source = path[len(prefix):]
        if not member.isfile() or (not source.endswith(SOURCE_SUFFIXES) and source not in grouped):
            continue
        if member.size > 16 * 1024 * 1024:
            raise ValueError('oversized source notice input')
        data = archive.extractfile(member).read()
        ranges = []
        for ref in grouped.get(source, []):
            start, end = ref['byte_start'], ref['byte_end']
            if (type(start) is not int or type(end) is not int or not 0 <= start < end <= len(data)
                    or package.sha(data) != ref['source_file_sha256']
                    or len(data[start:end]) != ref['size_bytes']
                    or package.sha(data[start:end]) != ref['sha256']):
                raise ValueError('embedded source notice bytes differ: ' + source)
            ranges.append((start, end))
            references += 1
        if source.endswith(SOURCE_SUFFIXES):
            source_files += 1
            for hit in NOTICE_SIGNAL.finditer(data):
                signals += 1
                if not any(start <= hit.start() and hit.end() <= end for start, end in ranges):
                    raise ValueError('unretained source attribution signal: ' + source)
    if references != sum(len(refs) for refs in grouped.values()):
        raise ValueError('source notice file missing from archive')
    return {'source_files_checked': source_files, 'source_signals_checked': signals,
            'embedded_references_verified': references,
            'scope': 'defined byte-pattern coverage in selected source suffixes; not exhaustive legal or inline-source analysis'}


def original_source_coverage(archive, members, prefix, item):
    review = item.get('source_license_review')
    if review is None:
        return None
    vcs_member = members.get(prefix + '.cargo_vcs_info.json')
    if vcs_member is None or not vcs_member.isfile() or vcs_member.size > 65536:
        raise ValueError('source review requires bounded VCS metadata')
    vcs = json.loads(archive.extractfile(vcs_member).read(), object_pairs_hook=package.pairs_unique)
    if (review['revision'] != vcs['git']['sha1'] or review['path_in_vcs'] != vcs['path_in_vcs']
            or review['resolved_license'] != item['declared_license']
            or review['distribution_approval'] is not False):
        raise ValueError('source review identity mismatch')
    excluded = {'Cargo.toml', 'Cargo.lock', '.cargo_vcs_info.json'}
    if set(review['excluded_generated_metadata']) - excluded:
        raise ValueError('source review excludes original files')
    originals = {path[len(prefix):] for path, member in members.items()
                 if member.isfile() and path[len(prefix):] not in excluded}
    refs = {ref['crate_path']: ref for ref in review['matched_files']}
    if set(refs) != originals or len(refs) != len(review['matched_files']):
        raise ValueError('source review original-file scope mismatch')
    for source, ref in refs.items():
        member = members[prefix + source]
        if member.size > 16 * 1024 * 1024 or package.sha(archive.extractfile(member).read()) != ref['sha256']:
            raise ValueError('source review original bytes differ')
    return {'original_files_verified': len(refs), 'revision': review['revision'],
            'scope': 'published originals match recorded upstream comparison hashes; upstream snapshot was verified separately'}


def archive_coverage(packages, cache):
    rows = []
    for item in packages:
        name = item['name'] + '-' + item['version']
        matches = []
        for path in cache.glob('*/' + name + '.crate'):
            data = package.regular(cache, str(path.relative_to(cache)), 128 * 1024 * 1024)
            if package.sha(data) == item['artifact_sha256']:
                matches.append((path, data))
        if not matches:
            raise ValueError('hash-verified crate archive required for ' + name)
        with tarfile.open(fileobj=io.BytesIO(matches[0][1]), mode='r:gz') as archive:
            prefix = name + '/'
            members = {}
            for member in archive:
                if len(members) >= 20000 or member.size > 128 * 1024 * 1024:
                    raise ValueError('oversized crate archive metadata')
                if (not member.name.startswith(prefix)
                        and not (member.isdir() and member.name == name)):
                    raise ValueError('unexpected crate root')
                if member.name in members:
                    raise ValueError('duplicate crate member')
                members[member.name] = member
            retained = {ref['source_path']: ref for ref in item['files']}
            notice_paths = []
            for path, member in members.items():
                if not member.isfile():
                    continue
                source = path[len(prefix):]
                basename = Path(source).name.upper()
                # Explicit filename heuristic, not an inline license detector.
                if any(basename == label or basename.startswith(tuple(label + suffix for suffix in '._-'))
                       for label in ('LICENSE', 'LICENCE', 'COPYING', 'COPYRIGHT', 'AUTHORS', 'NOTICE')):
                    if source not in retained:
                        raise ValueError('named attribution file missing from inventory: ' + name)
                if basename == 'NOTICE' or basename.startswith(('NOTICE.', 'NOTICE_', 'NOTICE-')):
                    notice_paths.append(source)
            for source, ref in retained.items():
                member = members.get(prefix + source)
                if (member is None or not member.isfile() or member.size != ref['size_bytes']
                        or member.size > 16 * 1024 * 1024
                        or package.sha(archive.extractfile(member).read()) != ref['sha256']):
                    raise ValueError('retained file differs from verified crate: ' + name)
            embedded = embedded_coverage(archive, members, prefix, item) if item['name'] == 'ring' else None
            source_review = original_source_coverage(archive, members, prefix, item)
        rows.append({'name': item['name'], 'version': item['version'],
                     'artifact_sha256': item['artifact_sha256'],
                     'retained_archive_files_verified': len(retained), 'notice_paths': sorted(notice_paths),
                     'embedded_source_check': embedded, 'original_source_check': source_review})
    return {'registry_archives_verified': len(rows), 'packages': rows,
            'scope': 'hash-verified archive candidate bytes and named attribution-file heuristic; not inline source review'}


def review(root, crate_cache=None):
    inputs = [package.PLAN, 'Cargo.lock', 'third-party/inventory.json',
              'third-party/core-license-choices.json', 'docs/core-third-party-notices.md',
              'LICENSE', 'NOTICE']
    before = {name: package.sha(package.regular(root, name)) for name in inputs}
    plan, inventory, packages = package.validate_plan(root)
    ledger = json.loads(package.regular(root, 'third-party/core-license-choices.json'),
                        object_pairs_hook=package.pairs_unique)
    choices = {(p['name'], p['version']): p for p in ledger['registry_packages']}
    archives = archive_coverage(packages, crate_cache) if crate_cache is not None else None
    rows = []
    for item in sorted(packages, key=lambda p: (p['name'], p['version'])):
        refs = item['files'] + item.get('supplemental_upstream', {}).get('files', [])
        rows.append({'name': item['name'], 'version': item['version'],
                     'artifact_sha256': item['artifact_sha256'],
                     'declared_expression': choices[(item['name'], item['version'])]['declared_expression'],
                     'selected_expression': choices[(item['name'], item['version'])]['selected_expression'],
                     'candidate_texts': [{key: ref[key] for key in
                                          ('source_path', 'retained_path', 'sha256')} for ref in refs],
                     'embedded_header_references': len(item.get('embedded_headers', [])),
                     'supplemental_revision': item.get('supplemental_upstream', {}).get('revision'),
                     'source_license_review': item.get('source_license_review'),
                     'text_applicability_approved': False})
    if (package.validate_plan(root) != (plan, inventory, packages)
            or {name: package.sha(package.regular(root, name)) for name in inputs} != before):
        raise ValueError('review inputs changed during inspection')
    return {'schema': 'urdia.core-license-coverage/v1', 'result': 'pass',
            'scope': 'declared candidate policy, retained bytes and notice-index coverage; not legal clearance or complete source attribution review',
            'input_sha256': before, 'target': plan['target'], 'registry_packages': len(rows),
            'selected_expression_counts': dict(sorted(Counter(p['selected_expression'] for p in rows).items())),
            'unique_retained_texts': sum(e['destination'].startswith('third-party/texts/') for e in plan['contents']),
            'embedded_header_references': sum(p['embedded_header_references'] for p in rows),
            'toolchain_notice_references': sum(len(t['files']) for t in inventory['toolchains']),
            'archive_attribution_check': archives,
            'packages': rows, 'distribution_approval': False, 'publication_allowed': False,
            'remaining_reviews': ['component/text applicability and inline source attribution',
                                  'upstream modifications and relevant NOTICE obligations',
                                  'Rust library component applicability',
                                  'final candidate-specific distribution approval']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence', type=Path, required=True)
    parser.add_argument('--crate-cache', type=Path,
                        help='optional Cargo registry/cache directory; requires every selected hash-verified crate')
    args = parser.parse_args()
    comparator.save(review(ROOT, args.crate_cache), args.evidence)
    print('Core license policy and notice coverage pass; private review evidence saved')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, tarfile.TarError) as error:
        print(f'license review: {error}', file=sys.stderr)
        raise SystemExit(1)
