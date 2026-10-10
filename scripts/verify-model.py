#!/usr/bin/env python3
"""Verify a pinned model directory against a bounded integrity manifest.

The verifier intentionally uses only Python's standard library.  It never
imports a model package, starts a listener, or accesses the network.  On Linux
all model paths are opened relative to directory file descriptors with
symlink-following disabled.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import sys
from dataclasses import dataclass
from typing import Any, NoReturn


SCHEMA_VERSION = "urdia.model-integrity/v1"
ALLOWED_REPOSITORIES = frozenset(
    {
        "convaiinnovations/laya",
        "convaiinnovations/laya-typed-decisions",
    }
)
MAX_MANIFEST_BYTES = 256 * 1024
MAX_FILES = 64
MAX_PATH_CHARS = 200
MAX_FILE_SIZE = 2 * 1024 * 1024 * 1024
MAX_TOTAL_SIZE = 4 * 1024 * 1024 * 1024
READ_CHUNK_SIZE = 1024 * 1024
SHA256_RE = re.compile(r"[0-9a-fA-F]{64}\Z")
REVISION_RE = re.compile(r"[0-9a-fA-F]{40}\Z")
GENERIC_FAILURE = "model integrity verification failed\n"


class VerificationError(Exception):
    """An expected integrity or input validation failure."""


@dataclass(frozen=True)
class ManifestFile:
    path: str
    components: tuple[str, ...]
    size: int
    sha256: str


def _reject_json_constant(value: str) -> NoReturn:
    del value
    raise VerificationError


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise VerificationError
        result[key] = value
    return result


def _close_fd(fd: int) -> None:
    try:
        os.close(fd)
    except OSError:
        pass


def _open_regular_file(path: str) -> int:
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0)
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    if hasattr(os, "O_NONBLOCK"):
        flags |= os.O_NONBLOCK
    fd = os.open(path, flags)
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise VerificationError
        return fd
    except BaseException:
        _close_fd(fd)
        raise


def _read_manifest(path: str) -> dict[str, Any]:
    fd = _open_regular_file(path)
    data = bytearray()
    try:
        while len(data) < MAX_MANIFEST_BYTES + 1:
            chunk = os.read(
                fd,
                min(READ_CHUNK_SIZE, MAX_MANIFEST_BYTES + 1 - len(data)),
            )
            if not chunk:
                break
            data.extend(chunk)
    finally:
        _close_fd(fd)

    if len(data) > MAX_MANIFEST_BYTES:
        raise VerificationError
    try:
        text = bytes(data).decode("utf-8")
        value = json.loads(
            text,
            object_pairs_hook=_reject_duplicate_keys,
            parse_constant=_reject_json_constant,
        )
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError, ValueError):
        raise VerificationError from None
    if type(value) is not dict:
        raise VerificationError
    return value


def _validate_safe_path(value: Any) -> tuple[str, tuple[str, ...]]:
    if type(value) is not str or not value or len(value) > MAX_PATH_CHARS:
        raise VerificationError
    if value.startswith("/") or "\\" in value or "\x00" in value:
        raise VerificationError
    try:
        value.encode("utf-8")
    except UnicodeEncodeError:
        raise VerificationError from None
    components = tuple(value.split("/"))
    if any(not component or component in {".", ".."} for component in components):
        raise VerificationError
    return value, components


def _validate_identity(
    document: dict[str, Any], expected_repository: str, expected_revision: str
) -> None:
    if type(expected_repository) is not str or expected_repository not in ALLOWED_REPOSITORIES:
        raise VerificationError
    if type(expected_revision) is not str or not REVISION_RE.fullmatch(expected_revision):
        raise VerificationError
    if document.get("schema_version") != SCHEMA_VERSION:
        raise VerificationError
    repository = document.get("repository")
    revision = document.get("revision")
    if type(repository) is not str or repository not in ALLOWED_REPOSITORIES:
        raise VerificationError
    if type(revision) is not str or not REVISION_RE.fullmatch(revision):
        raise VerificationError
    if repository != expected_repository or revision != expected_revision:
        raise VerificationError


def _validate_manifest(
    document: dict[str, Any], expected_repository: str, expected_revision: str
) -> tuple[ManifestFile, ...]:
    required_keys = {"schema_version", "repository", "revision", "files"}
    if set(document) != required_keys:
        raise VerificationError
    _validate_identity(document, expected_repository, expected_revision)

    raw_files = document["files"]
    if type(raw_files) is not list or not 1 <= len(raw_files) <= MAX_FILES:
        raise VerificationError

    entries: list[ManifestFile] = []
    seen_paths: set[str] = set()
    total_size = 0
    for raw_file in raw_files:
        if type(raw_file) is not dict or set(raw_file) != {"path", "size", "sha256"}:
            raise VerificationError
        path, components = _validate_safe_path(raw_file["path"])
        if path in seen_paths:
            raise VerificationError
        seen_paths.add(path)

        size = raw_file["size"]
        if type(size) is not int or size < 0 or size > MAX_FILE_SIZE:
            raise VerificationError
        total_size += size
        if total_size > MAX_TOTAL_SIZE:
            raise VerificationError

        sha256 = raw_file["sha256"]
        if type(sha256) is not str or not SHA256_RE.fullmatch(sha256):
            raise VerificationError
        entries.append(ManifestFile(path, components, size, sha256.lower()))
    return tuple(entries)


def _require_linux_open_flags() -> None:
    for name in ("O_DIRECTORY", "O_NOFOLLOW", "O_NONBLOCK"):
        if not hasattr(os, name):
            raise VerificationError


def _directory_flags() -> int:
    return (
        os.O_RDONLY
        | os.O_DIRECTORY
        | os.O_NOFOLLOW
        | os.O_NONBLOCK
        | getattr(os, "O_CLOEXEC", 0)
    )


def _open_model_root(path: str) -> int:
    _require_linux_open_flags()
    fd = os.open(path, _directory_flags())
    try:
        if not stat.S_ISDIR(os.fstat(fd).st_mode):
            raise VerificationError
        return fd
    except BaseException:
        _close_fd(fd)
        raise


def _open_directory_at(parent_fd: int, component: str) -> int:
    fd = os.open(component, _directory_flags(), dir_fd=parent_fd)
    try:
        if not stat.S_ISDIR(os.fstat(fd).st_mode):
            raise VerificationError
        return fd
    except BaseException:
        _close_fd(fd)
        raise


def _open_leaf_at(parent_fd: int, component: str) -> int:
    flags = (
        os.O_RDONLY
        | os.O_NOFOLLOW
        | os.O_NONBLOCK
        | getattr(os, "O_CLOEXEC", 0)
    )
    fd = os.open(component, flags, dir_fd=parent_fd)
    try:
        if not stat.S_ISREG(os.fstat(fd).st_mode):
            raise VerificationError
        return fd
    except BaseException:
        _close_fd(fd)
        raise


def _hash_fd(fd: int, expected_size: int) -> str:
    digest = hashlib.sha256()
    remaining = expected_size
    while remaining:
        chunk = os.read(fd, min(READ_CHUNK_SIZE, remaining))
        if not chunk:
            raise VerificationError
        digest.update(chunk)
        remaining -= len(chunk)

    # Read one extra byte so a file that grew after the initial stat cannot
    # pass while only its manifest-sized prefix was hashed.
    if os.read(fd, 1):
        raise VerificationError
    return digest.hexdigest()


def _verify_file(root_fd: int, entry: ManifestFile) -> None:
    directory_fds: list[int] = []
    parent_fd = root_fd
    try:
        for component in entry.components[:-1]:
            parent_fd = _open_directory_at(parent_fd, component)
            directory_fds.append(parent_fd)
        leaf_fd = _open_leaf_at(parent_fd, entry.components[-1])
        try:
            before = os.fstat(leaf_fd)
            if before.st_size != entry.size:
                raise VerificationError
            if _hash_fd(leaf_fd, entry.size) != entry.sha256:
                raise VerificationError
            after = os.fstat(leaf_fd)
            if after.st_size != entry.size:
                raise VerificationError
        finally:
            _close_fd(leaf_fd)
    finally:
        for fd in reversed(directory_fds):
            _close_fd(fd)


def verify(manifest_path: str, model_dir: str, repository: str, revision: str) -> int:
    document = _read_manifest(manifest_path)
    entries = _validate_manifest(document, repository, revision)
    root_fd = _open_model_root(model_dir)
    try:
        for entry in entries:
            _verify_file(root_fd, entry)
    finally:
        _close_fd(root_fd)
    return len(entries)


def _parse_args(argv: list[str] | None) -> argparse.Namespace:
    class RedactedArgumentParser(argparse.ArgumentParser):
        def error(self, message: str) -> NoReturn:
            del message
            raise VerificationError

    parser = RedactedArgumentParser(prog="verify-model.py")
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--model-dir", required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--revision", required=True)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    try:
        args = _parse_args(argv)
        count = verify(args.manifest, args.model_dir, args.repository, args.revision)
    except Exception:
        sys.stderr.write(GENERIC_FAILURE)
        return 1
    print(count)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
