#!/usr/bin/env python3
"""Offline corruption/path/archive regressions for the private core packager."""
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import shlex
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("package_core", ROOT / "scripts/package-core.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
rehearsal_spec = importlib.util.spec_from_file_location("test_core_package", ROOT / "scripts/test-core-package.py")
rehearsal = importlib.util.module_from_spec(rehearsal_spec)
rehearsal_spec.loader.exec_module(rehearsal)
attest_spec = importlib.util.spec_from_file_location('attest', ROOT / 'scripts/attest-core.py')
attest = importlib.util.module_from_spec(attest_spec)
attest_spec.loader.exec_module(attest)
compare_spec = importlib.util.spec_from_file_location('comparator', ROOT / 'scripts/compare-core-packages.py')
comparator = importlib.util.module_from_spec(compare_spec)
compare_spec.loader.exec_module(comparator)
host_spec = importlib.util.spec_from_file_location('host_check', ROOT / 'scripts/check-core-host.py')
host_check = importlib.util.module_from_spec(host_spec)
host_spec.loader.exec_module(host_check)
review_spec = importlib.util.spec_from_file_location('license_review', ROOT / 'scripts/review-core-licenses.py')
license_review = importlib.util.module_from_spec(review_spec)
review_spec.loader.exec_module(license_review)


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="urdia-package-unit-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / "source"
        self.root.mkdir()
        plan = json.loads((ROOT / package.PLAN).read_text())
        paths = {package.PLAN, *plan["input_sha256"]}
        paths.update(e["source"] for e in plan["contents"] if "source" in e)
        for name in paths:
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, target)

    def rewrite_plan(self, mutate):
        path = self.root / package.PLAN
        value = json.loads(path.read_text())
        mutate(value)
        path.write_bytes(package.json_bytes(value))

    def test_selected_texts_and_filtered_inventory(self):
        plan, inventory, selected = package.validate_plan(self.root)
        files = package.assemble_files(self.root, plan, inventory, selected, b"fixture binary")
        filtered = json.loads(files["third-party/inventory.json"][0])
        self.assertEqual(len(filtered["rust"]), len(plan["registry_packages"]))
        self.assertEqual(filtered["python"], [])
        self.assertEqual(filtered["model_cards"], [])
        self.assertEqual(filtered['counts']['supplemental_upstream_file_references'],
                         sum(len(p.get('supplemental_upstream', {}).get('files', []))
                             for p in filtered['rust']))
        self.assertEqual(inventory['counts']['supplemental_upstream_file_references'],
                         sum(len(p.get('supplemental_upstream', {}).get('files', []))
                             for group in ('rust', 'python') for p in inventory[group]))
        self.assertFalse(filtered["scope"]["distribution_approval"])
        self.assertNotIn(b"](../third-party/", files["THIRD_PARTY_NOTICES.md"][0])
        self.assertEqual(set(files), {e["destination"] for e in plan["contents"]})

    def test_stale_input_and_corrupt_text_refused(self):
        (self.root / "Cargo.lock").write_text("changed input")
        with self.assertRaisesRegex(ValueError, "stale plan"):
            package.validate_plan(self.root)
        shutil.copyfile(ROOT / "Cargo.lock", self.root / "Cargo.lock")
        plan = json.loads((self.root / package.PLAN).read_text())
        text = next(e["source"] for e in plan["contents"] if e.get("source", "").startswith("third-party/texts/"))
        (self.root / text).write_text("tampered license")
        with self.assertRaisesRegex(ValueError, "text mismatch"):
            package.validate_plan(self.root)

    def test_input_symlinks_refused(self):
        path = self.root / "LICENSE"
        path.unlink()
        path.symlink_to(ROOT / "LICENSE")
        with self.assertRaisesRegex(ValueError, "symlink"):
            package.validate_plan(self.root)

    def test_traversal_duplicates_and_extra_content_refused(self):
        original = (self.root / package.PLAN).read_bytes()
        for destination in ("../outside", "/absolute", "bin/./urdia", "bin//urdia", "bin\\urdia"):
            with self.subTest(destination=destination):
                self.rewrite_plan(lambda p: p["contents"][0].update(destination=destination))
                with self.assertRaises(ValueError):
                    package.validate_plan(self.root)
                (self.root / package.PLAN).write_bytes(original)
        self.rewrite_plan(lambda p: p["contents"].append(copy.deepcopy(p["contents"][0])))
        with self.assertRaisesRegex(ValueError, "duplicate destination"):
            package.validate_plan(self.root)
        (self.root / package.PLAN).write_bytes(original)
        self.rewrite_plan(lambda p: p["contents"].append({"destination": "secret", "mode": "0644", "source": "LICENSE"}))
        with self.assertRaisesRegex(ValueError, "unexpected destination"):
            package.validate_plan(self.root)
        with self.assertRaisesRegex(ValueError, "duplicate JSON"):
            json.loads('{"schema": 1, "schema": 2}', object_pairs_hook=package.pairs_unique)

    def test_path_normalization_preserves_spaces_and_argument_boundaries(self):
        flags = package.reproducible_flags('/tmp/source with spaces', '/tmp/build with spaces', '/tmp/rust')
        rust = flags['CARGO_ENCODED_RUSTFLAGS'].split('\x1f')
        self.assertEqual(rust[0], '--remap-path-prefix=/tmp/source with spaces=/urdia/source')
        self.assertEqual(shlex.split(flags['CFLAGS']), [
            '-ffile-prefix-map=/tmp/source with spaces=/urdia/source',
            '-ffile-prefix-map=/tmp/build with spaces=/urdia/build'])
        self.assertEqual(flags['CC_SHELL_ESCAPED_FLAGS'], '1')
        self.assertEqual(flags['CARGO_PROFILE_RELEASE_STRIP'], 'symbols')

    def test_license_choices_cannot_change_publisher_declaration(self):
        path = self.root / 'third-party/core-license-choices.json'
        value = json.loads(path.read_bytes())
        value['registry_packages'][0]['declared_expression'] = 'fabricated grant'
        path.write_bytes(package.json_bytes(value))
        self.rewrite_plan(lambda p: p['input_sha256'].update({
            'third-party/core-license-choices.json': package.sha(path.read_bytes())}))
        with self.assertRaisesRegex(ValueError, 'declaration mismatch'):
            package.validate_plan(self.root)

    def test_license_selection_cannot_invent_grants_or_drop_and_terms(self):
        path = self.root / 'third-party/core-license-choices.json'
        original = json.loads(path.read_bytes())
        for name, selected in [('agent-client-protocol', 'MIT'), ('matchit', 'MIT'),
                               ('ring', 'ISC'), ('icu_properties', 'MIT')]:
            with self.subTest(name=name):
                value = copy.deepcopy(original)
                choice = next(p for p in value['registry_packages'] if p['name'] == name)
                choice['selected_expression'] = selected
                path.write_bytes(package.json_bytes(value))
                self.rewrite_plan(lambda p: p['input_sha256'].update({
                    'third-party/core-license-choices.json': package.sha(path.read_bytes())}))
                with self.assertRaisesRegex(ValueError, 'policy review'):
                    package.validate_plan(self.root)

    def test_notice_index_cannot_drop_one_packages_shared_text_reference(self):
        path = self.root / 'docs/core-third-party-notices.md'
        original = path.read_text()
        # This license is still referenced by other ACP packages: global link
        # coverage alone must not hide a missing per-component attribution.
        begin = original.index('### agent-client-protocol 2.2.0')
        end = original.index('### agent-client-protocol-derive', begin)
        path.write_text(original[:begin] + '### agent-client-protocol 2.2.0\n\n' + original[end:])
        with self.assertRaisesRegex(ValueError, 'package coverage'):
            package.validate_plan(self.root)
        path.write_text(original)
        path.write_text(original + '\n### agent-client-protocol 2.2.0\n\n')
        with self.assertRaisesRegex(ValueError, 'duplicate notice'):
            package.validate_plan(self.root)

    def test_notice_index_cannot_drop_native_or_toolchain_scope(self):
        path = self.root / 'docs/core-third-party-notices.md'
        original = path.read_text()
        for heading in ('ring source headers and fiat AUTHORS', 'Rust 1.98.1 standard-library notice'):
            with self.subTest(heading=heading):
                path.write_text(original.replace('### ' + heading, '### Unrelated section'))
                with self.assertRaisesRegex(ValueError, 'coverage mismatch'):
                    package.validate_plan(self.root)

    def test_license_review_records_all_components_without_distribution_approval(self):
        report = license_review.review(self.root)
        self.assertEqual(report['registry_packages'], 205)
        self.assertEqual(sum(report['selected_expression_counts'].values()), 205)
        self.assertEqual(report['unique_retained_texts'], 334)
        self.assertEqual(report['embedded_header_references'], 277)
        self.assertFalse(report['distribution_approval'])
        self.assertFalse(report['publication_allowed'])
        self.assertTrue(all(p['candidate_texts'] and not p['text_applicability_approved']
                            for p in report['packages']))
        ring = next(p for p in report['packages'] if p['name'] == 'ring')
        self.assertEqual(ring['selected_expression'], 'Apache-2.0 AND ISC')
        self.assertIn('third_party/fiat/AUTHORS', [r['source_path'] for r in ring['candidate_texts']])

    def test_archive_review_rejects_uninventoried_notice_and_wrong_artifact(self):
        cache = Path(self.tmp.name) / 'cache'
        registry = cache / 'registry'
        registry.mkdir(parents=True)
        archive = registry / 'fixture-1.0.0.crate'
        data = b'original attribution'
        item = {'name': 'fixture', 'version': '1.0.0', 'files': [
            {'source_path': 'LICENSE', 'size_bytes': len(data), 'sha256': package.sha(data)}]}
        def write(files):
            if archive.exists():
                archive.unlink()
            package.write_archive(archive, 'fixture-1.0.0', files, 1234)
            item['artifact_sha256'] = package.sha(archive.read_bytes())
        write({'LICENSE': (data, 0o644)})
        report = license_review.archive_coverage([item], cache)
        self.assertEqual(report['registry_archives_verified'], 1)
        self.assertEqual(report['packages'][0]['notice_paths'], [])
        write({'LICENSE': (data, 0o644), 'NOTICE.txt': (b'extra attribution', 0o644)})
        with self.assertRaisesRegex(ValueError, 'missing from inventory'):
            license_review.archive_coverage([item], cache)
        write({'LICENSE': (b'modified attribution', 0o644)})
        with self.assertRaisesRegex(ValueError, 'differs from verified crate'):
            license_review.archive_coverage([item], cache)
        item['artifact_sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'hash-verified crate archive required'):
            license_review.archive_coverage([item], cache)

    def test_embedded_review_detects_license_after_blank_lines_and_directives(self):
        archive = Path(self.tmp.name) / 'source.tar.gz'
        data = b'// Copyright Example\n\n#include "base.h"\n// Licensed under Apache\n'
        package.write_archive(archive, 'ring-test', {'source.S': (data, 0o644)}, 1234)
        def ref(start, end):
            return {'source_path': 'source.S', 'source_file_sha256': package.sha(data),
                    'byte_start': start, 'byte_end': end, 'size_bytes': end-start,
                    'sha256': package.sha(data[start:end])}
        item = {'embedded_headers': [ref(0, data.index(b'\n')+1)]}
        with tarfile.open(archive, 'r:gz') as stream:
            members = {m.name: m for m in stream}
            with self.assertRaisesRegex(ValueError, 'unretained source attribution'):
                license_review.embedded_coverage(stream, members, 'ring-test/', item)
            item['embedded_headers'].append(ref(data.index(b'// Licensed'), len(data)))
            report = license_review.embedded_coverage(stream, members, 'ring-test/', item)
            self.assertEqual(report['source_signals_checked'], 2)
            self.assertEqual(report['embedded_references_verified'], 2)

    def test_embedded_review_rejects_wrong_source_hash_or_byte_range(self):
        archive = Path(self.tmp.name) / 'source.tar.gz'
        data = b'// Copyright Example\n'
        package.write_archive(archive, 'ring-test', {'source.S': (data, 0o644)}, 1234)
        original = {'source_path': 'source.S', 'source_file_sha256': package.sha(data),
                    'byte_start': 0, 'byte_end': len(data), 'size_bytes': len(data),
                    'sha256': package.sha(data)}
        with tarfile.open(archive, 'r:gz') as stream:
            members = {m.name: m for m in stream}
            for field, value in [('source_file_sha256', '0'*64), ('byte_start', True),
                                  ('byte_end', len(data)+1), ('sha256', '0'*64)]:
                with self.subTest(field=field):
                    with self.assertRaisesRegex(ValueError, 'notice bytes differ'):
                        license_review.embedded_coverage(stream, members, 'ring-test/',
                                                        {'embedded_headers': [{**original, field: value}]})

    def test_source_review_refuses_stale_revision_missing_original_and_byte_drift(self):
        archive = Path(self.tmp.name) / 'source.tar.gz'
        data = b'original source'
        vcs = package.json_bytes({'git': {'sha1': 'a'*40}, 'path_in_vcs': 'source'})
        package.write_archive(archive, 'fixture', {'src/lib.rs': (data, 0o644),
                              '.cargo_vcs_info.json': (vcs, 0o644)}, 1234)
        review = {'revision': 'a'*40, 'path_in_vcs': 'source', 'resolved_license': 'Apache-2.0',
                  'distribution_approval': False, 'excluded_generated_metadata': ['.cargo_vcs_info.json'],
                  'matched_files': [{'crate_path': 'src/lib.rs', 'sha256': package.sha(data)}]}
        with tarfile.open(archive, 'r:gz') as stream:
            members = {m.name: m for m in stream}
            item = {'declared_license': 'Apache-2.0', 'source_license_review': review}
            self.assertEqual(license_review.original_source_coverage(stream, members, 'fixture/', item)
                             ['original_files_verified'], 1)
            for field, value in [('revision', 'b'*40), ('matched_files', []),
                                  ('excluded_generated_metadata', ['src/lib.rs']),
                                  ('matched_files', [{'crate_path': 'src/lib.rs', 'sha256': '0'*64}])]:
                with self.subTest(field=field):
                    with self.assertRaises(ValueError):
                        license_review.original_source_coverage(stream, members, 'fixture/',
                            {**item, 'source_license_review': {**review, field: value}})

    def test_archive_deterministic_and_exact(self):
        files = {"bin/urdia": (b"fixture binary", 0o755), "LICENSE": (b"upstream bytes\r\n", 0o644)}
        first, second = Path(self.tmp.name) / "a.gz", Path(self.tmp.name) / "b.gz"
        package.write_archive(first, "urdia-test", files, 1234)
        package.write_archive(second, "urdia-test", dict(reversed(list(files.items()))), 1234)
        self.assertEqual(first.read_bytes(), second.read_bytes())
        package.verify_archive(first, "urdia-test", files, 1234)
        with self.assertRaises(FileExistsError):
            package.write_archive(first, "urdia-test", files, 1234)
        bad = Path(self.tmp.name) / "extra.gz"
        package.write_archive(bad, "urdia-test", {**files, "unexpected": (b"extra", 0o644)}, 1234)
        with self.assertRaisesRegex(ValueError, "unexpected archive"):
            package.verify_archive(bad, "urdia-test", files, 1234)

    def test_archive_symlink_and_changed_bytes_refused(self):
        files = {"LICENSE": (b"expected", 0o644)}
        bad = Path(self.tmp.name) / "bad.gz"
        with tarfile.open(bad, "w:gz") as archive:
            info = tarfile.TarInfo("urdia-test/LICENSE")
            info.type, info.linkname, info.mtime = tarfile.SYMTYPE, "../../outside", 1234
            archive.addfile(info)
        with self.assertRaises(ValueError):
            package.verify_archive(bad, "urdia-test", files, 1234)
        bad.unlink()
        package.write_archive(bad, "urdia-test", {"LICENSE": (b"modified", 0o644)}, 1234)
        with self.assertRaisesRegex(ValueError, "byte mismatch"):
            package.verify_archive(bad, "urdia-test", files, 1234)

    @staticmethod
    def map_fixture(work):
        def row(tail, size=4):
            return f"{0x1000:16x} {0x1000:16x} {size:8x} {4:5d} {tail}"
        return ("             VMA              LMA     Size Align Out     In      Symbol\n" +
                "\n".join([row(".text"),
                           row("        " + str(work / "target/libdemo.rlib") + "(native.o):(.text.secret)"),
                           row("                private_symbol"),
                           row("        <internal>:(.text)")]) + "\n").encode()

    def test_link_map_normalizes_private_paths_without_symbols(self):
        a, b = Path('/tmp/private-a'), Path('/tmp/private-b')
        first = package.link_map_report(self.map_fixture(a), b'binary', a, Path('/rustup'))
        second = package.link_map_report(self.map_fixture(b), b'binary', b, Path('/rustup'))
        self.assertEqual(first, second)
        self.assertEqual(first['inputs'], [{'origin': 'build/target/libdemo.rlib(native.o)',
                                          'output_section': '.text', 'input_section_count': 1,
                                          'reported_size_bytes': 4}])
        serialized = package.json_bytes(first)
        for secret in (b'private-a', b'private_symbol', b'secret'):
            self.assertNotIn(secret, serialized)
        self.assertFalse(first['source_license_attribution_complete'])

    def test_root_codegen_metadata_is_normalized_without_hiding_contributions(self):
        def fixture(work, metadata, unit='00', size=4, dependency='libdep-aaaa.rlib'):
            def row(tail, n=4):return f"{0x1000:16x} {0x1000:16x} {n:8x} {4:5d} {tail}"
            root=work/'target/x86_64-unknown-linux-gnu/release/deps'
            inputs=[row('.text'),row('        '+str(root/f'urdia-{metadata}.urdia.{metadata}-cgu.{unit}.rcgu.o')+':(.text)',size),
                    row('        '+str(root/f'urdia-{metadata}.{metadata}xyz.rcgu.o')+':(.text)'),
                    row('        '+str(root/dependency)+'(native.o):(.text)')]
            return ('             VMA              LMA     Size Align Out     In      Symbol\n'+'\n'.join(inputs)+'\n').encode()
        a,b=Path('/tmp/private-a'),Path('/tmp/private-b')
        first=package.link_map_report(fixture(a,'a'*16),b'binary',a,Path('/rustup'))
        second=package.link_map_report(fixture(b,'b'*16),b'binary',b,Path('/rustup'))
        self.assertEqual(first,second)
        self.assertEqual(first['schema'],'urdia.link-section-observations/v2')
        for kwargs in ({'size':8},{'unit':'01'},{'dependency':'libdep-bbbb.rlib'}):
            different=package.link_map_report(fixture(b,'b'*16,**kwargs),b'binary',b,Path('/rustup'))
            self.assertNotEqual(first,different)
        self.assertNotEqual(first,package.link_map_report(fixture(a,'a'*16),b'changed binary',a,Path('/rustup')))

    def test_link_map_rejects_unknown_format_origin_and_empty_inputs(self):
        work = Path('/tmp/private-a')
        data = self.map_fixture(work)
        for bad in (b'GNU map', data.replace(b'/tmp/private-a/target', b'/home/private/target'),
                    data.replace(b'(native.o)', b'(../native.o)'),
                    data.splitlines()[0] + b'\n'):
            with self.assertRaises(ValueError):
                package.link_map_report(bad, b'binary', work, Path('/rustup'))

    def test_link_sidecar_is_bound_to_binary_and_detached_signature(self):
        candidate, key, public = self.signing_fixture()
        data = package.json_bytes(package.link_map_report(self.map_fixture(Path('/tmp/private-a')),
                                  b'fixture binary', Path('/tmp/private-a'), Path('/rustup')))
        sidecar = candidate / 'link-observations.json'
        sidecar.write_bytes(data)
        evidence_path = candidate / 'package-evidence.json'
        evidence = json.loads(evidence_path.read_bytes())
        evidence['link_observations'] = {'path': sidecar.name, 'sha256': package.sha(data)}
        evidence_path.write_bytes(package.json_bytes(evidence))
        attest.sign(candidate, key)
        attest.verify(candidate, public)
        sidecar.write_bytes(data + b' ')
        with self.assertRaisesRegex(ValueError, 'link observation checksum'):
            attest.verify(candidate, public)
        changed = json.loads(data)
        changed['binary_sha256'] = '0' * 64
        sidecar.write_bytes(package.json_bytes(changed))
        evidence['link_observations']['sha256'] = package.sha(sidecar.read_bytes())
        evidence_path.write_bytes(package.json_bytes(evidence))
        with self.assertRaisesRegex(ValueError, 'invalid link observation'):
            rehearsal.inspect(candidate)
        sidecar.unlink()
        with self.assertRaises(OSError):
            rehearsal.inspect(candidate)

    def signing_fixture(self):
        candidate = Path(self.tmp.name) / 'signed-candidate'
        candidate.mkdir()
        files = {'bin/urdia': (b'fixture binary', 0o755)}
        name = 'urdia-test.tar.gz'
        package.write_archive(candidate / name, 'urdia-test', files, 1234)
        digest = package.sha((candidate / name).read_bytes())
        evidence = {'schema': 'urdia.private-package-evidence/v1', 'publication_allowed': False,
                    'commit_dirty': False, 'commit': 'a' * 40, 'checkout_input_sha256': 'b' * 64,
                    'archive': name, 'archive_sha256': digest, 'archive_epoch': 1234,
                    'files': [{'path': 'bin/urdia', 'mode': '0755', 'size_bytes': 14,
                               'sha256': package.sha(b'fixture binary')}]}
        (candidate / 'package-evidence.json').write_bytes(package.json_bytes(evidence))
        (candidate / 'SHA256SUMS').write_text(f'{digest}  {name}\n')
        key, public = Path(self.tmp.name) / 'key.pem', Path(self.tmp.name) / 'public.pem'
        attest.openssl(['genpkey', '-algorithm', 'ED25519', '-out', str(key)])
        key.chmod(0o600)
        public.write_bytes(attest.openssl(['pkey', '-in', str(key), '-pubout']))
        return candidate, key, public

    def test_detached_signature_and_independent_trust(self):
        candidate, key, public = self.signing_fixture()
        attest.sign(candidate, key)
        attest.verify(candidate, public)
        with self.assertRaisesRegex(ValueError, 'must not exist'):
            attest.sign(candidate, key)
        other = Path(self.tmp.name) / 'other.pem'
        attest.openssl(['genpkey', '-algorithm', 'ED25519', '-out', str(other)])
        public.write_bytes(attest.openssl(['pkey', '-in', str(other), '-pubout']))
        with self.assertRaisesRegex(ValueError, 'trusted key'):
            attest.verify(candidate, public)

    def test_signature_rejects_tampered_metadata_and_signature(self):
        candidate, key, public = self.signing_fixture()
        attest.sign(candidate, key)
        signature = candidate / attest.SIGNATURE
        signature.write_bytes(bytes(64))
        with self.assertRaisesRegex(ValueError, 'OpenSSL operation failed'):
            attest.verify(candidate, public)
        signature.unlink()
        (candidate / attest.ATTESTATION).unlink()
        attest.sign(candidate, key)
        evidence = candidate / 'package-evidence.json'
        value = json.loads(evidence.read_bytes())
        value['commit'] = 'c' * 40
        evidence.write_bytes(package.json_bytes(value))
        with self.assertRaisesRegex(ValueError, 'does not match'):
            attest.verify(candidate, public)

    def test_signing_refuses_dirty_candidate_and_exposed_key(self):
        candidate, key, _ = self.signing_fixture()
        key.chmod(0o644)
        with self.assertRaisesRegex(ValueError, 'owner-only'):
            attest.sign(candidate, key)
        key.chmod(0o600)
        evidence = candidate / 'package-evidence.json'
        value = json.loads(evidence.read_bytes())
        value['commit_dirty'] = True
        evidence.write_bytes(package.json_bytes(value))
        with self.assertRaisesRegex(ValueError, 'clean candidate'):
            attest.sign(candidate, key)
        self.assertFalse((candidate / attest.SIGNATURE).exists())

    def comparison_fixture(self):
        first, _, _ = self.signing_fixture()
        path = first / 'package-evidence.json'
        evidence = json.loads(path.read_bytes())
        evidence.update(target=package.TARGET, plan_sha256='c' * 64,
                        source_inventory_sha256='d' * 64, rustc='fixture compiler',
                        c_compiler='fixture C compiler', registry_packages=1,
                        elf_needed=['libsqlite3.so.0'], glibc_symbol_versions=['GLIBC_2.39'],
                        build_normalization='fixture flags')
        path.write_bytes(package.json_bytes(evidence))
        second = Path(self.tmp.name) / 'second-candidate'
        shutil.copytree(first, second)
        return first, second

    def test_comparison_validates_real_archives_and_saves_private_evidence(self):
        first, second = self.comparison_fixture()
        report = comparator.compare(first, second)
        self.assertEqual(report['result'], 'pass')
        self.assertFalse(report['publisher_identity_verified'])
        output = Path(self.tmp.name) / 'comparison.json'
        comparator.save(report, output)
        self.assertEqual(output.stat().st_mode & 0o777, 0o600)
        with self.assertRaises(FileExistsError):
            comparator.save(report, output)
        with self.assertRaisesRegex(ValueError, 'distinct'):
            comparator.compare(first, first)

    def test_comparison_refuses_dirty_and_changed_build_identities(self):
        first, second = self.comparison_fixture()
        path = second / 'package-evidence.json'
        original = path.read_bytes()
        for field, changed in (('commit_dirty', True), ('commit', 'e' * 40),
                               ('c_compiler', 'other compiler'), ('plan_sha256', 'f' * 64)):
            with self.subTest(field=field):
                evidence = json.loads(original)
                evidence[field] = changed
                path.write_bytes(package.json_bytes(evidence))
                with self.assertRaises(ValueError):
                    comparator.compare(first, second)
        path.write_bytes(original)

    def test_comparison_refuses_self_consistent_different_binary(self):
        first, second = self.comparison_fixture()
        path = second / 'package-evidence.json'
        evidence = json.loads(path.read_bytes())
        archive = second / evidence['archive']
        archive.unlink()
        data = b'changed binary'
        package.write_archive(archive, 'urdia-test', {'bin/urdia': (data, 0o755)}, 1234)
        digest = package.sha(archive.read_bytes())
        evidence['archive_sha256'] = digest
        evidence['files'][0].update(size_bytes=len(data), sha256=package.sha(data))
        path.write_bytes(package.json_bytes(evidence))
        (second / 'SHA256SUMS').write_text(f"{digest}  {evidence['archive']}\n")
        with self.assertRaisesRegex(ValueError, 'contents or manifests differ'):
            comparator.compare(first, second)

    def test_comparison_rejects_corruption_and_does_not_write_failed_report(self):
        first, second = self.comparison_fixture()
        name = json.loads((second / 'package-evidence.json').read_bytes())['archive']
        with (second / name).open('ab') as stream:
            stream.write(b'corrupt')
        with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
            comparator.compare(first, second)
        output = Path(self.tmp.name) / 'comparison.json'
        result = subprocess.run([sys.executable, '-I', '-B',
                                 str(ROOT / 'scripts/compare-core-packages.py'),
                                 '--first', str(first), '--second', str(second),
                                 '--evidence', str(output)], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 1)
        self.assertIn('checksum mismatch', result.stderr)
        self.assertFalse(output.exists())

    def test_rehearsal_refuses_changed_archive_and_duplicate_manifest(self):
        candidate = Path(self.tmp.name) / "candidate"
        candidate.mkdir()
        name = "urdia-test.tar.gz"
        files = {"bin/urdia": (b"fixture binary", 0o755)}
        package.write_archive(candidate / name, "urdia-test", files, 1234)
        digest = package.sha((candidate / name).read_bytes())
        evidence = {"schema": "urdia.private-package-evidence/v1", "publication_allowed": False,
                    "archive": name, "archive_sha256": digest, "archive_epoch": 1234,
                    "files": [{"path": "bin/urdia", "mode": "0755", "size_bytes": 14,
                               "sha256": package.sha(b"fixture binary")}]}
        (candidate / "package-evidence.json").write_bytes(package.json_bytes(evidence))
        (candidate / "SHA256SUMS").write_text(f"{digest}  {name}\n")
        _, loaded = rehearsal.inspect(candidate)
        self.assertEqual(loaded, files)
        for size in (-1, True, '14'):
            evidence['files'][0]['size_bytes'] = size
            (candidate / 'package-evidence.json').write_bytes(package.json_bytes(evidence))
            with self.assertRaisesRegex(ValueError, 'invalid file manifest'):
                rehearsal.inspect(candidate)
        evidence['files'][0]['size_bytes'] = 14
        (candidate / 'package-evidence.json').write_bytes(b'{"schema":1,"schema":2}')
        with self.assertRaisesRegex(ValueError, 'duplicate JSON'):
            rehearsal.inspect(candidate)
        evidence["files"].append(copy.deepcopy(evidence["files"][0]))
        (candidate / "package-evidence.json").write_bytes(package.json_bytes(evidence))
        with self.assertRaisesRegex(ValueError, "invalid file manifest"):
            rehearsal.inspect(candidate)
        evidence["files"].pop()
        (candidate / "package-evidence.json").write_bytes(package.json_bytes(evidence))
        with (candidate / name).open("ab") as stream:
            stream.write(b"corrupt")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            rehearsal.inspect(candidate)

    def host_fixture(self):
        candidate, key, public = self.signing_fixture()
        path = candidate / 'package-evidence.json'
        value = json.loads(path.read_bytes())
        value.update(target=package.TARGET, plan_sha256='c' * 64,
                     source_inventory_sha256='d' * 64, rustc='fixture compiler',
                     c_compiler='fixture compiler', registry_packages=1,
                     elf_needed=['libsqlite3.so.0'], glibc_symbol_versions=['GLIBC_2.39'],
                     build_normalization='fixture flags')
        path.write_bytes(package.json_bytes(value))
        required = {'elf_needed': value['elf_needed'],
                    'glibc_symbol_versions': value['glibc_symbol_versions'],
                    'interpreter': host_check.LOADER}
        observed = {'system': 'Linux', 'machine': 'x86_64', 'glibc': '2.39',
                    'interpreter_present': True, 'cached_x86_64_libraries': ['libsqlite3.so.0']}
        return candidate, key, public, required, observed

    def test_host_preflight_blocks_old_glibc_wrong_arch_and_missing_runtime(self):
        _, _, _, required, observed = self.host_fixture()
        required['glibc_symbol_versions'] = ['GLIBC_2.9', 'GLIBC_2.39']
        self.assertEqual(host_check.assess(required, observed), ('2.39', [], []))
        for field, value, reason in (
                ('glibc', '2.35', 'glibc_below_candidate_floor'),
                ('machine', 'aarch64', 'unsupported_host_platform'),
                ('glibc', None, 'glibc_not_observed'),
                ('interpreter_present', False, 'interpreter_unavailable'),
                ('cached_x86_64_libraries', [], 'required_libraries_not_in_verified_cache')):
            with self.subTest(field=field):
                floor, missing, reasons = host_check.assess(required, {**observed, field: value})
                self.assertEqual(floor, '2.39')
                self.assertIn(reason, reasons)
                if field == 'cached_x86_64_libraries':
                    self.assertEqual(missing, ['libsqlite3.so.0'])

    def test_host_preflight_refuses_metadata_drift_and_candidate_mutation(self):
        candidate, _, _, required, observed = self.host_fixture()
        with patch.object(host_check, 'requirements', return_value={**required, 'glibc_symbol_versions': ['GLIBC_2.9']}), \
                patch.object(host_check, 'host') as probe:
            with self.assertRaisesRegex(ValueError, 'ELF requirements differ'):
                host_check.check(candidate)
            probe.assert_not_called()
        def mutate():
            with (candidate / 'SHA256SUMS').open('ab') as stream:
                stream.write(b'changed')
            return observed
        with patch.object(host_check, 'requirements', return_value=required), \
                patch.object(host_check, 'host', side_effect=mutate):
            with self.assertRaisesRegex(ValueError, 'changed during host inspection'):
                host_check.check(candidate)

    def test_host_signature_is_optional_but_invalid_supplied_trust_is_fatal(self):
        candidate, key, public, required, observed = self.host_fixture()
        with patch.object(host_check, 'requirements', return_value=required), \
                patch.object(host_check, 'host', return_value=observed):
            report = host_check.check(candidate)
            self.assertFalse(report['trusted_signature_verified'])
            self.assertFalse(report['candidate_executed'])
            self.assertFalse(report['runtime_compatibility_verified'])
            attest.sign(candidate, key)
            self.assertTrue(host_check.check(candidate, public)['trusted_signature_verified'])
            (candidate / attest.SIGNATURE).write_bytes(bytes(64))
            with self.assertRaises(ValueError):
                host_check.check(candidate, public)

    def test_elf_probe_never_executes_candidate_and_rejects_unknown_interpreter(self):
        data = bytearray(64)
        data[:6] = b'\x7fELF\x02\x01'
        data[18] = 62
        dynamic = 'Shared library: [libsqlite3.so.0]'
        versions = 'Name: GLIBC_2.39'
        segments = f'[Requesting program interpreter: {host_check.LOADER}]'
        commands = []
        def readelf(args):
            commands.append(args)
            candidate = Path(args[-1])
            self.assertEqual(candidate.read_bytes(), data)
            self.assertEqual(candidate.stat().st_mode & 0o777, 0o600)
            return {'--dynamic': dynamic, '--version-info': versions, '--program-headers': segments}[args[2]]
        with patch.object(host_check, 'command', side_effect=readelf):
            self.assertEqual(host_check.requirements(data)['interpreter'], host_check.LOADER)
        self.assertEqual(len(commands), 3)
        self.assertTrue(all(args[0] == '/usr/bin/readelf' for args in commands))
        self.assertFalse(Path(commands[0][-1]).exists())
        with patch.object(host_check, 'command', side_effect=[dynamic, versions, '[Requesting program interpreter: /tmp/evil]']):
            with self.assertRaisesRegex(ValueError, 'unsupported dynamic'):
                host_check.requirements(data)
        with patch.object(host_check, 'command', side_effect=[dynamic + '\n(RUNPATH) [/tmp/evil]', versions, segments]):
            with self.assertRaisesRegex(ValueError, 'unsupported dynamic'):
                host_check.requirements(data)
        with patch.object(host_check, 'command') as command:
            with self.assertRaisesRegex(ValueError, 'ELF64'):
                host_check.requirements(b'executable shell script')
            command.assert_not_called()

    def test_host_cache_rejects_wrong_arch_and_never_updates_cache(self):
        cache = ('libsqlite3.so.0 (libc6,x86-64) => /system/sqlite\n'
                 'libfake.so (libc6) => /system/32bit\n'
                 'libbad.so (libc6,x86-64) => /system/not-elf\n')
        with patch.object(host_check.platform, 'system', return_value='Linux'), \
                patch.object(host_check.platform, 'machine', return_value='x86_64'), \
                patch.object(host_check, 'command', side_effect=['glibc 2.39', cache]) as command, \
                patch.object(host_check, 'system_elf', side_effect=lambda path: path in ('/system/sqlite', host_check.LOADER)):
            observed = host_check.host()
        self.assertEqual(observed['cached_x86_64_libraries'], ['libsqlite3.so.0'])
        self.assertEqual(command.call_args_list[-1].args[0], ['/sbin/ldconfig', '-p'])

    def test_host_cli_saves_blocked_report_privately_without_overwrite(self):
        candidate, _, _, required, observed = self.host_fixture()
        observed['glibc'] = '2.35'
        output = Path(self.tmp.name) / 'host.json'
        args = ['check-core-host.py', '--candidate-dir', str(candidate), '--evidence', str(output)]
        with patch.object(sys, 'argv', args), \
                patch.object(host_check, 'requirements', return_value=required), \
                patch.object(host_check, 'host', return_value=observed):
            self.assertEqual(host_check.main(), 2)
            original = output.read_bytes()
            self.assertEqual(json.loads(original)['result'], 'blocked')
            self.assertEqual(output.stat().st_mode & 0o777, 0o600)
            with self.assertRaises(FileExistsError):
                host_check.main()
            self.assertEqual(output.read_bytes(), original)


# Keep preparation regressions inside the existing Rust integration entrypoint.
prepare_tests_spec = importlib.util.spec_from_file_location('prepare_tests', ROOT / 'scripts/tests/test_prepare_core.py')
prepare_tests = importlib.util.module_from_spec(prepare_tests_spec)
prepare_tests_spec.loader.exec_module(prepare_tests)
PreparationTests = prepare_tests.PreparationTests


if __name__ == "__main__":
    unittest.main(verbosity=2)
