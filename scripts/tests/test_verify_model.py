#!/usr/bin/env python3
"""Regression fixtures for the offline model integrity verifier."""

from __future__ import annotations

import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "verify-model.py"
PYTHON = sys.executable
REPOSITORY = "convaiinnovations/laya"
REVISION = "0123456789abcdef0123456789abcdef01234567"
GENERIC_FAILURE = "model integrity verification failed\n"
MAX_MANIFEST_BYTES = 256 * 1024
MAX_FILE_SIZE = 2 * 1024 * 1024 * 1024
MAX_TOTAL_SIZE = 4 * 1024 * 1024 * 1024

BASE_CONTENT = {
    "config/config.json": b'{"version":1}\n',
    "metadata/revision.txt": REVISION.encode("ascii"),
    "nested/weights.bin": b"weights\x00\x01\x02",
    "nested/vocab.txt": b"hello\nworld\n",
    "tokenizer.json": b'{"tokenizer":"fixture"}\n',
}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def entry(path: str, data: bytes) -> dict[str, Any]:
    return {"path": path, "size": len(data), "sha256": digest(data)}


def valid_document(
    entries: list[dict[str, Any]] | None = None,
    *,
    repository: str = REPOSITORY,
    revision: str = REVISION,
) -> dict[str, Any]:
    return {
        "schema_version": "urdia.model-integrity/v1",
        "repository": repository,
        "revision": revision,
        "files": entries if entries is not None else [entry(path, data) for path, data in BASE_CONTENT.items()],
    }


class VerifyModelTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp_dir.name)
        self.model = self.root / "model"
        self.model.mkdir()
        for relative, data in BASE_CONTENT.items():
            path = self.model / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        self.manifest = self.root / "manifest.json"
        self.write_document(valid_document())

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def write_document(self, document: dict[str, Any]) -> None:
        self.manifest.write_text(json.dumps(document), encoding="utf-8")

    def run_verifier(
        self,
        *,
        manifest: pathlib.Path | None = None,
        model: pathlib.Path | None = None,
        repository: str = REPOSITORY,
        revision: str = REVISION,
        timeout: float = 3.0,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                PYTHON,
                "-I",
                "-B",
                str(SCRIPT),
                "--manifest",
                str(manifest or self.manifest),
                "--model-dir",
                str(model or self.model),
                "--repository",
                repository,
                "--revision",
                revision,
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
        )

    def assert_success(self, output: subprocess.CompletedProcess[str], count: int) -> None:
        self.assertEqual(output.returncode, 0, output.stderr)
        self.assertEqual(output.stdout, f"{count}\n")
        self.assertEqual(output.stderr, "")

    def assert_failure(self, output: subprocess.CompletedProcess[str]) -> None:
        self.assertNotEqual(output.returncode, 0)
        self.assertEqual(output.stdout, "")
        self.assertEqual(output.stderr, GENERIC_FAILURE)
        self.assertNotIn(str(self.root), output.stderr)
        self.assertNotIn(str(self.model), output.stderr)
        self.assertNotIn(str(self.manifest), output.stderr)

    def test_valid_manifest_with_nested_files_outputs_only_count(self) -> None:
        self.assert_success(self.run_verifier(), len(BASE_CONTENT))

    def test_tampered_missing_and_wrong_size_files_fail(self) -> None:
        tampered = self.model / "nested/weights.bin"
        tampered.write_bytes(b"x" * len(BASE_CONTENT["nested/weights.bin"]))
        self.assert_failure(self.run_verifier())

        tampered.write_bytes(BASE_CONTENT["nested/weights.bin"])
        wrong_digest = valid_document()
        wrong_digest["files"][0]["sha256"] = "0" * 64
        self.write_document(wrong_digest)
        self.assert_failure(self.run_verifier())
        self.write_document(valid_document())
        tampered.unlink()
        self.assert_failure(self.run_verifier())

        tampered.write_bytes(BASE_CONTENT["nested/weights.bin"])
        document = valid_document()
        document["files"][0]["size"] += 1
        self.write_document(document)
        self.assert_failure(self.run_verifier())

    def test_leaf_and_ancestor_symlinks_fail(self) -> None:
        outside = self.root / "outside.bin"
        outside.write_bytes(b"outside")
        leaf = self.model / "leaf-link"
        leaf.symlink_to(outside)
        self.write_document(valid_document([entry("leaf-link", b"outside")]))
        self.assert_failure(self.run_verifier())

        leaf.unlink()
        ancestor = self.root / "outside-dir"
        ancestor.mkdir()
        (ancestor / "file.bin").write_bytes(b"outside")
        alias = self.model / "ancestor-link"
        alias.symlink_to(ancestor, target_is_directory=True)
        self.write_document(valid_document([entry("ancestor-link/file.bin", b"outside")]))
        self.assert_failure(self.run_verifier())

        root_link = self.root / "model-link"
        root_link.symlink_to(self.model, target_is_directory=True)
        self.write_document(valid_document())
        self.assert_failure(self.run_verifier(model=root_link))

    def test_malformed_paths_are_rejected(self) -> None:
        for path in (
            "/absolute",
            "../escape",
            "nested/../weights.bin",
            "./weights.bin",
            "nested/./weights.bin",
            "nested\\weights.bin",
            "nested//weights.bin",
            "",
            "x" * 201,
        ):
            with self.subTest(path=path):
                self.write_document(valid_document([entry(path, b"x")]))
                self.assert_failure(self.run_verifier())

    def test_schema_identity_and_duplicate_path_failures(self) -> None:
        documents: list[dict[str, Any]] = []
        missing = valid_document()
        del missing["files"]
        documents.append(missing)

        extra = valid_document()
        extra["unexpected"] = True
        documents.append(extra)

        wrong_files = valid_document()
        wrong_files["files"] = {"path": "x"}
        documents.append(wrong_files)

        wrong_entry = valid_document([entry("config/config.json", BASE_CONTENT["config/config.json"])])
        wrong_entry["files"][0]["extra"] = False
        documents.append(wrong_entry)

        malformed_json = self.root / "malformed.json"
        malformed_json.write_text('{"schema_version":', encoding="utf-8")
        self.assert_failure(self.run_verifier(manifest=malformed_json))

        duplicate_json = self.root / "duplicate-keys.json"
        duplicate_json.write_text(
            '{"schema_version":"urdia.model-integrity/v1",'
            '"schema_version":"urdia.model-integrity/v1",'
            f'"repository":"{REPOSITORY}","revision":"{REVISION}","files":[]' "}",
            encoding="utf-8",
        )
        self.assert_failure(self.run_verifier(manifest=duplicate_json))

        for document in documents:
            with self.subTest(document=document):
                self.write_document(document)
                self.assert_failure(self.run_verifier())

        duplicate = valid_document()
        duplicate["files"] = [duplicate["files"][0], duplicate["files"][0]]
        self.write_document(duplicate)
        self.assert_failure(self.run_verifier())

        identity_cases = (
            (valid_document(repository="other/repository"), REPOSITORY, REVISION),
            (valid_document(), "convaiinnovations/laya-typed-decisions", REVISION),
            (valid_document(revision="f" * 40), REPOSITORY, REVISION),
            (valid_document(), REPOSITORY, "f" * 40),
            (valid_document(revision="not-a-revision"), REPOSITORY, REVISION),
        )
        for document, repository, revision in identity_cases:
            with self.subTest(repository=repository, revision=revision):
                self.write_document(document)
                self.assert_failure(self.run_verifier(repository=repository, revision=revision))

    def test_fifo_and_nonregular_leaf_fail_without_blocking(self) -> None:
        fifo = self.model / "pipe"
        os.mkfifo(fifo)
        self.write_document(valid_document([entry("pipe", b"")]))
        self.assert_failure(self.run_verifier(timeout=1.0))

        directory = self.model / "directory"
        directory.mkdir()
        self.write_document(valid_document([entry("directory", b"")]))
        self.assert_failure(self.run_verifier())

    def test_manifest_and_graph_bounds_fail_before_file_access(self) -> None:
        oversized = self.root / "oversized.json"
        oversized.write_bytes(b" " * (MAX_MANIFEST_BYTES + 1))
        self.assert_failure(self.run_verifier(manifest=oversized))

        for entries in (
            [],
            [entry(f"file-{index}", b"") for index in range(65)],
            [{"path": "too-large", "size": MAX_FILE_SIZE + 1, "sha256": "0" * 64}],
            [
                {"path": "first", "size": MAX_FILE_SIZE, "sha256": "0" * 64},
                {"path": "second", "size": MAX_FILE_SIZE, "sha256": "0" * 64},
                {"path": "third", "size": 1, "sha256": "0" * 64},
            ],
        ):
            with self.subTest(entry_count=len(entries)):
                self.write_document(valid_document(entries))
                self.assert_failure(self.run_verifier())

    def test_io_failures_are_redacted(self) -> None:
        missing_manifest = self.root / "missing.json"
        self.assert_failure(self.run_verifier(manifest=missing_manifest))

        missing_model = self.root / "missing-model"
        self.assert_failure(self.run_verifier(model=missing_model))

        model_file = self.root / "model-file"
        model_file.write_bytes(b"not a directory")
        self.assert_failure(self.run_verifier(model=model_file))

        manifest_directory = self.root / "manifest-directory"
        manifest_directory.mkdir()
        self.assert_failure(self.run_verifier(manifest=manifest_directory))


if __name__ == "__main__":
    unittest.main()
