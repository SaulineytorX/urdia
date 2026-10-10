#!/usr/bin/env python3
"""Detached Ed25519 attestation of a private candidate; does not approve publication."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

spec = importlib.util.spec_from_file_location('rehearsal', Path(__file__).with_name('test-core-package.py'))
rehearsal = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rehearsal)
package = rehearsal.package
ATTESTATION = 'candidate-attestation.json'
SIGNATURE = 'candidate-attestation.sig'
ED25519_SPKI = bytes.fromhex('302a300506032b6570032100')


def openssl(args):
    result = subprocess.run(['/usr/bin/openssl', *args], input=b"\n",
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=15,
                            env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'})
    if result.returncode:
        raise ValueError('OpenSSL operation failed; check key type, permissions, or signature')
    return result.stdout


def identity(key, private=False):
    args = ['pkey', '-in', str(key), '-pubout', '-outform', 'DER']
    if private:
        args += ['-passin', 'stdin']
    else:
        args += ['-pubin']
    data = openssl(args)
    if len(data) != 44 or not data.startswith(ED25519_SPKI):
        raise ValueError('Ed25519 key required')
    return package.sha(data)


def statement(candidate, fingerprint):
    evidence, _ = rehearsal.inspect(candidate)
    if (evidence['commit_dirty'] is not False
            or not re.fullmatch(r'[0-9a-f]{40}', evidence['commit'])
            or not re.fullmatch(r'[0-9a-f]{64}', evidence['checkout_input_sha256'])):
        raise ValueError('clean candidate and source identities required')
    names = [evidence['archive'], 'package-evidence.json', 'SHA256SUMS']
    return {'schema': 'urdia.private-candidate-attestation/v1',
            'algorithm': 'Ed25519', 'public_key_spki_sha256': fingerprint,
            'source_commit': evidence['commit'],
            'checkout_input_sha256': evidence['checkout_input_sha256'],
            'files': {name: package.sha(package.regular(candidate, name, 128 * 1024 * 1024))
                      for name in names}, 'publication_allowed': False}


def sign(candidate, key):
    if any((candidate / name).exists() or (candidate / name).is_symlink()
           for name in (ATTESTATION, SIGNATURE)):
        raise ValueError('attestation output must not exist')
    key = key.absolute()
    if (key.is_symlink() or not key.is_file() or key.stat().st_mode & 0o077
            or key.stat().st_uid != os.getuid()):
        raise ValueError('private key must be a regular owner-only file')
    fingerprint = identity(key, private=True)
    data = package.json_bytes(statement(candidate, fingerprint))
    with tempfile.TemporaryDirectory(prefix='urdia-attest-') as raw:
        path = Path(raw) / 'statement'
        path.write_bytes(data)
        sig = openssl(['pkeyutl', '-sign', '-rawin', '-inkey', str(key),
                       '-passin', 'stdin', '-in', str(path)])
        if len(sig) != 64:
            raise ValueError('invalid Ed25519 signature size')
        # Refuse a candidate changed while the signature was being generated.
        if data != package.json_bytes(statement(candidate, fingerprint)):
            raise ValueError('candidate changed during signing')
    created = []
    try:
        for name, value in ((ATTESTATION, data), (SIGNATURE, sig)):
            fd = os.open(candidate / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            created.append(candidate / name)
            with os.fdopen(fd, 'wb') as out:
                out.write(value)
    except BaseException:
        for path in created:
            path.unlink()
        raise


def verify(candidate, key):
    # Trust comes only from the caller's independently obtained public key.
    fingerprint = identity(key.absolute())
    data = package.regular(candidate, ATTESTATION)
    decoded = json.loads(data, object_pairs_hook=package.pairs_unique)
    if data != package.json_bytes(decoded) or decoded != statement(candidate, fingerprint):
        raise ValueError('attestation does not match candidate or trusted key')
    sig = package.regular(candidate, SIGNATURE)
    if len(sig) != 64:
        raise ValueError('invalid signature size')
    with tempfile.TemporaryDirectory(prefix='urdia-verify-') as raw:
        path, signature = Path(raw) / 'statement', Path(raw) / 'signature'
        path.write_bytes(data)
        signature.write_bytes(sig)
        openssl(['pkeyutl', '-verify', '-rawin', '-pubin', '-inkey', str(key.absolute()),
                 '-in', str(path), '-sigfile', str(signature)])
    if data != package.json_bytes(statement(candidate, fingerprint)):
        raise ValueError('candidate changed during verification')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    for command, flag in (('sign', '--private-key'), ('verify', '--public-key')):
        child = commands.add_parser(command)
        child.add_argument('--candidate-dir', type=Path, required=True)
        child.add_argument(flag, type=Path, required=True)
    args = parser.parse_args()
    candidate = args.candidate_dir.absolute()
    if candidate.is_symlink() or not candidate.is_dir():
        raise ValueError('regular candidate directory required')
    if args.command == 'sign':
        sign(candidate, args.private_key)
        print('Private candidate attestation created; publication remains disabled')
    else:
        verify(candidate, args.public_key)
        print('Candidate matches the supplied trusted public key; publication remains disabled')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired) as error:
        print(f'attestation: {error}', file=sys.stderr)
        raise SystemExit(1)
