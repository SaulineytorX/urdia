#!/usr/bin/env python3
"""Offline safety regressions for ``install-python.sh``.

The hash-failure case copies the helper into a disposable project and gives it
only local ``file://`` wheel URLs.  It therefore exercises the real pip
commands without downloading or mutating the repository's install inputs.
"""

from __future__ import annotations

import hashlib
import json
import re
import pathlib
import platform
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile


ROOT = pathlib.Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts" / "install-python.sh"
LIBC, LIBC_VERSION = platform.libc_ver()
SUPPORTED_TARGET = (
    sys.implementation.name == "cpython"
    and sys.platform == "linux"
    and sys.version_info[:2] == (3, 12)
    and platform.machine() == "x86_64"
    and LIBC == "glibc"
    and tuple(int(part) for part in LIBC_VERSION.split(".")[:2]) >= (2, 28)
)


def make_wheel(
    path: pathlib.Path,
    distribution: str,
    version: str,
    files: dict[str, str],
    requires: tuple[str, ...] = (),
) -> None:
    """Write the smallest valid pure-Python wheel needed by pip."""

    normalized = distribution.replace("-", "_")
    dist_info = f"{normalized}-{version}.dist-info"
    metadata = (
        "Metadata-Version: 2.1\n"
        f"Name: {distribution}\n"
        f"Version: {version}\n"
        + "".join(f"Requires-Dist: {item}\n" for item in requires)
        + "\n"
    )
    wheel_metadata = (
        "Wheel-Version: 1.0\n"
        "Generator: urdia-install-regression\n"
        "Root-Is-Purelib: true\n"
        "Tag: py3-none-any\n"
    )
    entries = dict(files)
    entries[f"{dist_info}/METADATA"] = metadata
    entries[f"{dist_info}/WHEEL"] = wheel_metadata
    entries[f"{dist_info}/RECORD"] = ""
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, content in entries.items():
            archive.writestr(name, content)


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class InstallLayaPythonTests(unittest.TestCase):
    def test_artifact_lock_matches_version_intent_and_evidence(self) -> None:
        evidence = json.loads(
            (ROOT / "deploy/laya/artifact-evidence.json").read_text()
        )
        normalize = lambda value: re.sub(r"[-_.]+", "-", value).lower()
        intended = {}
        for line in (ROOT / "deploy/laya/requirements.lock").read_text().splitlines():
            name, version = line.split("==")
            self.assertNotIn(normalize(name), intended)
            intended[normalize(name)] = version
        artifacts = {normalize(a["name"]): a for a in evidence["artifacts"]}
        self.assertEqual(len(artifacts), 44)
        self.assertEqual(set(artifacts), set(intended) | {"pip"})
        for name, version in intended.items():
            self.assertEqual(artifacts[name]["version"], version)
        self.assertEqual(artifacts["pip"]["version"], "26.2.1")
        source = evidence["scope"]["source_input"]
        self.assertEqual(sha256(ROOT / source["relative_path"]), source["sha256"])
        locked_names = set()
        for relative_path, record in evidence["lockfiles"].items():
            path = ROOT / relative_path
            self.assertEqual(sha256(path), record["sha256"])
            lines = path.read_text().splitlines()
            self.assertEqual(len(lines), record["entry_count"])
            for line in lines:
                match = re.fullmatch(r"([^ ]+) @ (https://[^ ]+) --hash=sha256:([0-9a-f]{64})", line)
                self.assertIsNotNone(match, line)
                name, url, digest = match.groups()
                name = normalize(name)
                self.assertNotIn(name, locked_names)
                locked_names.add(name)
                self.assertEqual(url, artifacts[name]["url"])
                self.assertEqual(digest, artifacts[name]["sha256"])
                self.assertTrue(url.startswith(
                    "https://download.pytorch.org/" if name == "torch"
                    else "https://files.pythonhosted.org/"
                ))
        self.assertEqual(locked_names, set(artifacts))

    def test_typellm_artifact_lock_matches_intent_and_evidence(self) -> None:
        evidence = json.loads(
            (ROOT / "deploy/typellm/artifact-evidence.json").read_text()
        )
        normalize = lambda value: re.sub(r"[-_.]+", "-", value).lower()
        intended = dict(
            (normalize(name), version) for name, version in
            (line.split("==") for line in (ROOT / "deploy/typellm/requirements.lock").read_text().splitlines())
        )
        self.assertEqual(len(intended), 29)
        intended["typellm"] = "0.2.6"
        artifacts = {normalize(a["name"]): a for a in evidence["artifacts"]}
        self.assertEqual(len(artifacts), len(evidence["artifacts"]))
        self.assertEqual({name: a["version"] for name, a in artifacts.items()}, intended)
        source = evidence["scope"]["source_input"]
        self.assertEqual(sha256(ROOT / source["relative_path"]), source["sha256"])
        locked = set()
        for relative_path, record in evidence["lockfiles"].items():
            path = ROOT / relative_path
            self.assertEqual(sha256(path), record["sha256"])
            lines = path.read_text().splitlines()
            self.assertEqual(len(lines), record["entry_count"])
            for line in lines:
                match = re.fullmatch(r"([^ ]+) @ (https://[^ ]+) --hash=sha256:([0-9a-f]{64})", line)
                self.assertIsNotNone(match, line)
                name, url, digest = match.groups()
                name = normalize(name)
                self.assertNotIn(name, locked)
                locked.add(name)
                self.assertEqual(url, artifacts[name]["url"])
                self.assertEqual(digest, artifacts[name]["sha256"])
                self.assertTrue(url.startswith("https://files.pythonhosted.org/"))
        self.assertEqual(locked, set(intended))

    def test_laya_compatibility_entrypoint_preserves_profile(self) -> None:
        # /usr/bin/python exists on some runners. Test argv delegation directly
        # rather than depending on the host's installed Python commands.
        with tempfile.TemporaryDirectory(prefix="urdia-wrapper-") as raw:
            scripts = pathlib.Path(raw) / "scripts with spaces"
            scripts.mkdir()
            wrapper = scripts / "install-laya-python.sh"
            shutil.copy2(ROOT / "scripts/install-laya-python.sh", wrapper)
            helper = scripts / "install-python.sh"
            helper.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\nexit 23\n')
            helper.chmod(0o700)
            venv = str(pathlib.Path(raw) / "venv with spaces")
            result = subprocess.run(
                ["/bin/sh", str(wrapper), venv],
                cwd="/", capture_output=True, text=True, timeout=10,
            )
            self.assertEqual(result.returncode, 23)
            self.assertEqual(result.stdout.splitlines(), [venv, "laya"])
            self.assertEqual(result.stderr, "")

    def test_unknown_profile_is_rejected_before_accessing_venv(self) -> None:
        result = subprocess.run(
            ["/bin/sh", str(HELPER), "/does-not-exist", "../unreviewed"],
            capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, "")
        self.assertIn("unknown Python installation profile", result.stderr)

    def test_nonvenv_is_rejected_before_pip(self) -> None:
        self.assertTrue(HELPER.is_file(), f"missing helper: {HELPER}")
        with tempfile.TemporaryDirectory(prefix="urdia-laya-nonvenv-") as raw:
            root = pathlib.Path(raw)
            nonvenv = root / "not-a-venv"
            (nonvenv / "bin").mkdir(parents=True)
            (nonvenv / "bin" / "python").symlink_to(sys.executable)
            hostile_home = root / "hostile-home"
            hostile_home.mkdir()
            output = subprocess.run(
                ["/bin/sh", str(HELPER), str(nonvenv), "laya"],
                cwd=ROOT,
                env={
                    "PATH": "/usr/bin:/bin",
                    "HOME": str(hostile_home),
                    "PIP_INDEX_URL": "http://127.0.0.1:9/should-not-be-used",
                    "PIP_EXTRA_INDEX_URL": "http://127.0.0.1:9/should-not-be-used",
                    "PIP_TARGET": str(root / "pip-target"),
                    "PYTHONPATH": str(root / "poison-pythonpath"),
                },
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=30,
            )
            self.assertEqual(output.returncode, 1, output.stderr)
            self.assertEqual(output.stdout, "")
            self.assertIn("requires a CPython 3.12", output.stderr)
            self.assertFalse((root / "pip-target").exists())
            self.assertEqual(list(hostile_home.iterdir()), [])

    @unittest.skipUnless(SUPPORTED_TARGET, "supported Python/platform required")
    def test_typellm_missing_dependency_fails_without_index_fallback(self) -> None:
        with tempfile.TemporaryDirectory(prefix="urdia-typellm-closure-") as raw:
            root = pathlib.Path(raw)
            project = root / "project"
            (project / "scripts").mkdir(parents=True)
            (project / "deploy/python").mkdir(parents=True)
            (project / "deploy/typellm").mkdir(parents=True)
            helper = project / "scripts" / HELPER.name
            shutil.copy2(HELPER, helper)
            bootstrap = root / "bootstrap_fixture-0.0.1-py3-none-any.whl"
            main = root / "closure_fixture-0.0.1-py3-none-any.whl"
            make_wheel(bootstrap, "bootstrap-fixture", "0.0.1", {})
            make_wheel(main, "closure-fixture", "0.0.1", {}, ("unlisted-fixture>=1",))
            (project / "deploy/python/bootstrap.lock").write_text(
                f"bootstrap-fixture @ {bootstrap.as_uri()} --hash=sha256:{sha256(bootstrap)}\n"
            )
            (project / "deploy/typellm/requirements-linux-x86_64-py312.lock").write_text(
                f"closure-fixture @ {main.as_uri()} --hash=sha256:{sha256(main)}\n"
            )
            venv = root / "venv"
            subprocess.run([sys.executable, "-I", "-m", "venv", str(venv)],
                           check=True, capture_output=True, timeout=60)
            result = subprocess.run(["/bin/sh", str(helper), str(venv), "typellm"],
                                    capture_output=True, text=True, timeout=90)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unlisted-fixture", result.stdout + result.stderr)
            site = venv / "lib/python3.12/site-packages"
            self.assertTrue(list(site.glob("bootstrap_fixture-*.dist-info")))
            self.assertFalse(list(site.glob("closure_fixture-*.dist-info")))

    @unittest.skipUnless(
        SUPPORTED_TARGET,
        "the helper's supported CPython 3.12 Linux gate is required",
    )
    def test_wrong_bootstrap_hash_fails_before_main_lock_or_side_effects(self) -> None:
        with tempfile.TemporaryDirectory(prefix="urdia-laya-hash-") as raw:
            root = pathlib.Path(raw)
            project = root / "project"
            (project / "scripts").mkdir(parents=True)
            (project / "deploy" / "python").mkdir(parents=True)
            (project / "deploy" / "laya").mkdir(parents=True)
            copied_helper = project / "scripts" / HELPER.name
            shutil.copy2(HELPER, copied_helper)

            wheel_dir = root / "wheels"
            wheel_dir.mkdir()
            bootstrap_wheel = wheel_dir / "bootstrap_fixture-0.0.1-py3-none-any.whl"
            make_wheel(
                bootstrap_wheel,
                "bootstrap-fixture",
                "0.0.1",
                {"bootstrap_fixture.py": "VALUE = 'bootstrap'\n"},
            )
            sentinel_wheel = wheel_dir / "sentinel_fixture-0.0.1-py3-none-any.whl"
            make_wheel(
                sentinel_wheel,
                "sentinel-fixture",
                "0.0.1",
                {"sentinel_fixture.py": "VALUE = 'must-not-install'\n"},
            )

            wrong_hash = "0" * 64
            (project / "deploy" / "python" / "bootstrap.lock").write_text(
                f"bootstrap-fixture @ {bootstrap_wheel.as_uri()} "
                f"--hash=sha256:{wrong_hash}\n",
                encoding="utf-8",
            )
            (project / "deploy" / "laya" / "requirements-linux-x86_64-py312.lock").write_text(
                f"sentinel-fixture @ {sentinel_wheel.as_uri()} "
                f"--hash=sha256:{sha256(sentinel_wheel)}\n",
                encoding="utf-8",
            )

            venv = root / "venv"
            subprocess.run(
                [sys.executable, "-m", "venv", str(venv)],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=60,
            )
            venv_python = venv / "bin" / "python"
            before_pip = subprocess.run(
                [str(venv_python), "-I", "-m", "pip", "--version"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=30,
            ).stdout

            hostile_home = root / "hostile-home"
            pip_config = hostile_home / ".config" / "pip" / "pip.conf"
            pip_config.parent.mkdir(parents=True)
            pip_config.write_text(
                "[global]\nindex-url = http://127.0.0.1:9/should-not-be-used\n",
                encoding="utf-8",
            )
            pip_target = root / "pip-target"
            output = subprocess.run(
                ["/bin/sh", str(copied_helper), str(venv), "laya"],
                cwd=project,
                env={
                    "PATH": "/usr/bin:/bin",
                    "HOME": str(hostile_home),
                    "PIP_INDEX_URL": "http://127.0.0.1:9/should-not-be-used",
                    "PIP_EXTRA_INDEX_URL": "http://127.0.0.1:9/should-not-be-used",
                    "PIP_TARGET": str(pip_target),
                    "PYTHONPATH": str(root / "poison-pythonpath"),
                },
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=90,
            )
            self.assertNotEqual(output.returncode, 0, output.stdout)
            diagnostic = f"{output.stdout}\n{output.stderr}".lower()
            self.assertIn("do not match the hashes", diagnostic)
            self.assertFalse(
                (venv / "lib" / "python3.12" / "site-packages" / "sentinel_fixture.py").exists()
            )
            self.assertFalse(
                any(
                    path.name.startswith("sentinel_fixture-")
                    for path in (venv / "lib" / "python3.12" / "site-packages").glob(
                        "sentinel_fixture-*"
                    )
                )
            )
            self.assertFalse(
                (venv / "lib" / "python3.12" / "site-packages" / "bootstrap_fixture.py").exists()
            )
            self.assertFalse(pip_target.exists())
            self.assertEqual(pip_config.read_text(encoding="utf-8"), "[global]\nindex-url = http://127.0.0.1:9/should-not-be-used\n")

            after_pip = subprocess.run(
                [str(venv_python), "-I", "-m", "pip", "--version"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=30,
            ).stdout
            self.assertEqual(after_pip, before_pip)


if __name__ == "__main__":
    unittest.main(verbosity=2)
