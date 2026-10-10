#!/usr/bin/env python3
"""Offline core host preflight. Never executes the candidate or installs files."""
import argparse
import importlib.util
from pathlib import Path
import platform
import re
import struct
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('comparator', ROOT / 'scripts/compare-core-packages.py')
comparator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparator)
package = comparator.package
LOADER = '/lib64/ld-linux-x86-64.so.2'


def command(args):
    # Only absolute system tools, no inherited preload, credentials or lookup paths.
    with tempfile.TemporaryFile() as output:
        try:
            result = subprocess.run(args, stdin=subprocess.DEVNULL, stdout=output,
                                    stderr=subprocess.DEVNULL, timeout=15,
                                    env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'})
        except subprocess.TimeoutExpired as error:
            raise ValueError('system inspection timed out') from error
        size = output.tell()
        if result.returncode or size > 4 * 1024 * 1024:
            raise ValueError('system inspection failed')
        output.seek(0)
        return output.read().decode('utf-8', errors='strict')


def x86_elf(data):
    return (len(data) >= 64 and data[:6] == b'\x7fELF\x02\x01'
            and struct.unpack_from('<H', data, 18)[0] == 62)


def requirements(binary):
    if not x86_elf(binary):
        raise ValueError('expected little-endian ELF64 x86_64 binary')
    with tempfile.TemporaryDirectory(prefix='urdia-host-inspect-') as name:
        path = Path(name) / 'candidate.elf'
        path.write_bytes(binary)
        path.chmod(0o600)  # Data for readelf, never an executable file.
        dynamic = command(['/usr/bin/readelf', '--wide', '--dynamic', str(path)])
        versions = command(['/usr/bin/readelf', '--wide', '--version-info', str(path)])
        segments = command(['/usr/bin/readelf', '--wide', '--program-headers', str(path)])
    needed = sorted(set(re.findall(r'Shared library: \[([^\]]+)\]', dynamic)))
    glibc = sorted(set(re.findall(r'Name: (GLIBC_[0-9.]+)', versions)))
    interpreters = re.findall(r'Requesting program interpreter: ([^\]]+)\]', segments)
    if (not needed or 'libsqlite3.so.0' not in needed or not glibc
            or re.search(r'\((?:RPATH|RUNPATH)\)', dynamic)
            or interpreters != [LOADER]
            or any(not re.fullmatch(r'[A-Za-z0-9_.+-]+', lib) for lib in needed)):
        raise ValueError('unsupported dynamic binary requirements')
    for version in glibc:
        version_tuple(version.removeprefix('GLIBC_'))
    return {'elf_needed': needed, 'glibc_symbol_versions': glibc, 'interpreter': LOADER}


def version_tuple(value):
    if not re.fullmatch(r'[0-9]+\.[0-9]+(?:\.[0-9]+)*', value):
        raise ValueError('invalid glibc version')
    return tuple(int(part) for part in value.split('.'))


def system_elf(path):
    try:
        with Path(path).open('rb') as stream:
            return x86_elf(stream.read(64))
    except OSError:
        return False


def host():
    observed = {'system': platform.system(), 'machine': platform.machine(),
                'glibc': None, 'cached_x86_64_libraries': [], 'interpreter_present': False}
    if observed['system'] != 'Linux' or observed['machine'] != 'x86_64':
        return observed
    libc = command(['/usr/bin/getconf', 'GNU_LIBC_VERSION']).strip()
    if not re.fullmatch(r'glibc [0-9]+\.[0-9]+(?:\.[0-9]+)*', libc):
        raise ValueError('glibc host required')
    observed['glibc'] = libc.split()[1]
    cache = command(['/sbin/ldconfig', '-p'])  # Read cache only; never update it.
    libraries = set()
    for name, flags, path in re.findall(r'^\s*(\S+)\s+\(([^)]+)\)\s+=>\s+(\S+)\s*$', cache, re.M):
        if 'x86-64' in flags and path.startswith('/') and system_elf(path):
            libraries.add(name)
    observed['cached_x86_64_libraries'] = sorted(libraries)
    observed['interpreter_present'] = system_elf(LOADER)
    return observed


def assess(required, observed):
    reasons = []
    floor = max((v.removeprefix('GLIBC_') for v in required['glibc_symbol_versions']),
                key=version_tuple)
    if observed['system'] != 'Linux' or observed['machine'] != 'x86_64':
        reasons.append('unsupported_host_platform')
    if observed['glibc'] is None:
        reasons.append('glibc_not_observed')
    elif version_tuple(observed['glibc']) < version_tuple(floor):
        reasons.append('glibc_below_candidate_floor')
    if not observed['interpreter_present']:
        reasons.append('interpreter_unavailable')
    missing = sorted(set(required['elf_needed']) - set(observed['cached_x86_64_libraries']))
    if missing:
        reasons.append('required_libraries_not_in_verified_cache')
    return floor, missing, reasons


def check(candidate, public_key=None):
    verified = False
    if public_key is not None:
        signing_spec = importlib.util.spec_from_file_location('attest', ROOT / 'scripts/attest-core.py')
        signing = importlib.util.module_from_spec(signing_spec)
        signing_spec.loader.exec_module(signing)
        signing.verify(candidate, public_key)
        verified = True
    evidence, files, hashes = comparator.inspect(candidate)
    required = requirements(files['bin/urdia'][0])
    for field in ('elf_needed', 'glibc_symbol_versions'):
        if required[field] != evidence[field]:
            raise ValueError('ELF requirements differ from candidate evidence')
    observed = host()
    floor, missing, reasons = assess(required, observed)
    if comparator.record_hashes(candidate, evidence['archive']) != hashes:
        raise ValueError('candidate changed during host inspection')
    if public_key is not None:
        signing.verify(candidate, public_key)
    return {'schema': 'urdia.core-host-preflight/v1', 'result': 'blocked' if reasons else 'pass',
            'source_commit': evidence['commit'], 'archive_sha256': evidence['archive_sha256'],
            'binary_sha256': package.sha(files['bin/urdia'][0]), 'requirements': required,
            'glibc_floor': floor,
            'host': {key: value for key, value in observed.items() if key != 'cached_x86_64_libraries'},
            'missing_libraries': missing, 'blocking_reasons': reasons,
            'trusted_signature_verified': verified, 'candidate_executed': False,
            'runtime_compatibility_verified': False, 'publication_allowed': False,
            'scope': 'static platform, glibc floor, interpreter and cached-library preflight only; not a complete ABI or runtime test'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate-dir', type=Path, required=True)
    parser.add_argument('--evidence', type=Path, required=True)
    parser.add_argument('--public-key', type=Path, help='independently trusted Ed25519 public key')
    args = parser.parse_args()
    report = check(args.candidate_dir.absolute(), args.public_key)
    comparator.save(report, args.evidence)
    print('Host preflight: ' + report['result'] + '; private evidence saved')
    return 2 if report['result'] == 'blocked' else 0


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, TypeError, tarfile.TarError) as error:
        print(f'host preflight: {error}', file=sys.stderr)
        raise SystemExit(1)
