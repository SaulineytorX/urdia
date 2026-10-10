#!/usr/bin/env python3
"""Build a private core candidate. This does not publish or approve a release."""
from __future__ import annotations

import argparse
import copy
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import shlex
import signal
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-unknown-linux-gnu"
PLAN = "deploy/release/core-linux-x86_64.json"
FIXED = {"README.md": "docs/package-readme.md", "LICENSE": "LICENSE",
         "NOTICE": "NOTICE", "THIRD_PARTY_NOTICES.md": "docs/core-third-party-notices.md",
         "third-party/core-license-choices.json": "third-party/core-license-choices.json"}
BUILD = ["cargo", "build", "--release", "--locked", "--target", TARGET,
         "-p", "urdia", "--bin", "urdia"]
ACTIVE = None
INTERRUPTED = False
# Bounded candidate policy for the declarations actually reviewed in this graph.
# This is not a general SPDX parser or distribution approval. New declarations
# and changed selections require an explicit review of this table and ledger.
LICENSE_SELECTIONS = {
    "MIT": "MIT", "Apache-2.0": "Apache-2.0", "ISC": "ISC", "Zlib": "Zlib",
    "BSD-3-Clause": "BSD-3-Clause", "Unicode-3.0": "Unicode-3.0",
    "CDLA-Permissive-2.0": "CDLA-Permissive-2.0",
    "MIT OR Apache-2.0": "MIT", "Apache-2.0 OR MIT": "MIT",
    "MIT/Apache-2.0": "MIT",  # Explicit legacy publisher spelling, not SPDX syntax.
    "Apache-2.0 OR ISC OR MIT": "MIT", "Unlicense OR MIT": "MIT",
    "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT": "MIT",
    "Apache-2.0 OR BSL-1.0": "Apache-2.0",
    "MIT AND BSD-3-Clause": "MIT AND BSD-3-Clause",
    "Apache-2.0 AND ISC": "Apache-2.0 AND ISC",
    "(MIT OR Apache-2.0) AND Unicode-3.0": "MIT AND Unicode-3.0",
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def pairs_unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def relative(path):
    if not isinstance(path, str) or not path or "\\" in path:
        raise ValueError("invalid relative path")
    if PurePosixPath(path).is_absolute() or any(x in ("", ".", "..") for x in path.split("/")):
        raise ValueError("invalid relative path")
    return Path(path)


def regular(root, path, limit=16 * 1024 * 1024):
    part = root
    for segment in relative(path).parts:
        part = part / segment
        if part.is_symlink():
            raise ValueError("symlink input refused")
    info = part.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
        raise ValueError("invalid input file")
    data = part.read_bytes()
    if len(data) > limit:
        raise ValueError("oversized input")
    return data


def json_bytes(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def validate_plan(root):
    plan = json.loads(regular(root, PLAN), object_pairs_hook=pairs_unique)
    if (plan["schema"] != "urdia.release-content-plan/v1" or plan["target"] != TARGET
            or plan["publication_allowed"] is not False or plan["format"] != "tar.gz"
            or plan["status"] != "candidate_plan_not_release_approval"):
        raise ValueError("unsupported candidate plan")
    version = plan["package_version"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise ValueError("invalid package version")
    if plan["root_directory"] != f"urdia-{version}-{TARGET}":
        raise ValueError("invalid archive root")
    if set(plan["input_sha256"]) != {"Cargo.lock", "third-party/inventory.json",
                                    "third-party/core-license-choices.json"}:
        raise ValueError("invalid input set")
    for path, digest in plan["input_sha256"].items():
        if sha(regular(root, path)) != digest:
            raise ValueError("stale plan input hash")
    inventory = json.loads(regular(root, "third-party/inventory.json"), object_pairs_hook=pairs_unique)
    selected = {(p["name"], p["version"]): p["artifact_sha256"] for p in plan["registry_packages"]}
    if len(selected) != len(plan["registry_packages"]):
        raise ValueError("duplicate package identity")
    packages = [p for p in inventory["rust"] if (p["name"], p["version"]) in selected]
    if len(packages) != len(selected):
        raise ValueError("missing or duplicate inventory package")
    if {(p["name"], p["version"]): p["artifact_sha256"] for p in packages} != selected:
        raise ValueError("package identity mismatch")
    choices = json.loads(regular(root, "third-party/core-license-choices.json"), object_pairs_hook=pairs_unique)
    if (choices["schema"] != "urdia.core-license-choices/v1" or choices["target"] != TARGET
            or choices["distribution_approval"] is not False
            or {(p["name"], p["version"]) for p in choices["registry_packages"]} != set(selected)
            or len(choices["registry_packages"]) != len(selected)):
        raise ValueError("license choice scope mismatch")
    declared = {(p["name"], p["version"]): p["declared_license"] for p in packages}
    for choice in choices["registry_packages"]:
        if (choice["declared_expression"] != declared[(choice["name"], choice["version"])]
                or not isinstance(choice["selected_expression"], str)
                or not choice["selected_expression"].strip()):
            raise ValueError("license choice declaration mismatch")
        if (choice["declared_expression"] not in LICENSE_SELECTIONS
                or choice["selected_expression"] != LICENSE_SELECTIONS[choice["declared_expression"]]):
            raise ValueError("license selection requires policy review")
    texts = {}
    refs = []
    for package in packages:
        package_refs = package["files"] + package.get("supplemental_upstream", {}).get("files", [])
        if not package_refs:
            raise ValueError("missing package text")
        refs.extend(package_refs + package.get("embedded_headers", []))
    toolchains = inventory.get("toolchains", [])
    if len(toolchains) != 1 or toolchains[0]["version"] != "1.98.1":
        raise ValueError("missing pinned toolchain notices")
    refs.extend(toolchains[0]["files"])
    for ref in refs:
        path = ref["retained_path"]
        if path != f"third-party/texts/{ref['sha256']}.txt":
            raise ValueError("invalid retained text path")
        data = regular(root, path)
        if len(data) != ref["size_bytes"] or sha(data) != ref["sha256"]:
            raise ValueError("retained text mismatch")
        texts[path] = ref["sha256"]
    validate_notice_index(root, packages, texts, toolchains)
    expected = set(FIXED) | set(texts) | {"bin/urdia", "third-party/inventory.json"}
    destinations = set()
    for entry in plan["contents"]:
        destination = entry["destination"]
        relative(destination)
        if destination in destinations:
            raise ValueError("duplicate destination")
        destinations.add(destination)
        if entry["mode"] != ("0755" if destination == "bin/urdia" else "0644"):
            raise ValueError("unexpected file mode")
        if destination in FIXED:
            if entry.get("source") != FIXED[destination]:
                raise ValueError("unexpected source")
            data = regular(root, entry["source"])
            if "sha256" in entry and sha(data) != entry["sha256"]:
                raise ValueError("fixed source hash mismatch")
        elif destination in texts:
            if entry.get("source") != destination or entry.get("sha256") != texts[destination]:
                raise ValueError("unexpected text mapping")
        elif destination == "bin/urdia":
            if entry.get("generated_by") != BUILD:
                raise ValueError("unexpected build command")
        elif destination == "third-party/inventory.json":
            if "generation" not in entry or "source" in entry:
                raise ValueError("unexpected inventory mapping")
        else:
            raise ValueError("unexpected destination")
        if "transform" in entry and destination != "THIRD_PARTY_NOTICES.md":
            raise ValueError("unexpected transform")
    if destinations != expected:
        raise ValueError("missing planned content")
    return plan, inventory, packages


def validate_notice_index(root, packages, texts, toolchains):
    index = regular(root, FIXED["THIRD_PARTY_NOTICES.md"]).decode("utf-8")
    def links(section):
        return set(re.findall(r"\]\(\.\./(third-party/texts/[0-9a-f]{64}\.txt)\)", section))
    if links(index) != set(texts):
        raise ValueError("notice index text coverage mismatch")
    sections = {}
    for match in re.finditer(r"^### ([^\n]+)\n(.*?)(?=^#{1,3} |\Z)", index, re.M | re.S):
        heading, body = match.groups()
        if heading in sections:
            raise ValueError("duplicate notice index section")
        sections[heading] = body
    for item in packages:
        heading = item["name"] + " " + item["version"]
        refs = item["files"] + item.get("supplemental_upstream", {}).get("files", [])
        expected = {ref["retained_path"] for ref in refs}
        available = links(sections.get(heading, ""))
        if item["name"] == "ring":
            native = links(sections.get("ring source headers and fiat AUTHORS", ""))
            if not {ref["retained_path"] for ref in item.get("embedded_headers", [])}.issubset(native):
                raise ValueError("notice index native header coverage mismatch")
            available |= native
        if heading not in sections or not expected.issubset(available):
            raise ValueError("notice index package coverage mismatch")
    for toolchain in toolchains:
        available = links(sections.get("Rust " + toolchain["version"] + " standard-library notice", ""))
        if not {ref["retained_path"] for ref in toolchain["files"]}.issubset(available):
            raise ValueError("notice index toolchain coverage mismatch")


def assemble_files(root, plan, inventory, packages, binary):
    filtered = copy.deepcopy(inventory)
    filtered.update(rust=packages, python=[], model_cards=[],
                    source_inventory_sha256=plan["input_sha256"]["third-party/inventory.json"])
    filtered["scope"] = {"rust": plan["build_scope"], "python": "excluded",
                         "text_review_complete": False, "distribution_approval": False}
    filtered["remaining"] = plan["remaining_gates"]
    filtered["counts"] = {"rust_packages": len(packages), "python_wheels": 0,
                          "model_cards_retained": 0,
                          "rust_candidate_files": sum(len(p["files"]) for p in packages),
                          "embedded_header_references": sum(len(p.get("embedded_headers", [])) for p in packages),
                          "supplemental_upstream_file_references": sum(len(p.get("supplemental_upstream", {}).get("files", [])) for p in packages),
                          "toolchain_notice_references": sum(len(t["files"]) for t in inventory["toolchains"]),
                          "unique_retained_texts": sum(e["destination"].startswith("third-party/texts/")
                                                       for e in plan["contents"])}
    files = {}
    for entry in plan["contents"]:
        path = entry["destination"]
        if path == "bin/urdia":
            data = binary
        elif path == "third-party/inventory.json":
            data = json_bytes(filtered)
        else:
            data = regular(root, entry["source"])
            if path == "THIRD_PARTY_NOTICES.md":
                data = data.replace(b"](../third-party/", b"](third-party/")
        files[path] = (data, int(entry["mode"], 8))
    return files


def directories(root_name, files):
    result = {root_name}
    for path in files:
        parts = (root_name + "/" + path).split("/")[:-1]
        for n in range(1, len(parts) + 1):
            result.add("/".join(parts[:n]))
    return result


def write_archive(path, root_name, files, epoch):
    with path.open("xb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
            for name in sorted(directories(root_name, files)):
                info = tarfile.TarInfo(name)
                info.type, info.mode, info.mtime = tarfile.DIRTYPE, 0o755, epoch
                archive.addfile(info)
            for name, (data, mode) in sorted(files.items()):
                info = tarfile.TarInfo(root_name + "/" + name)
                info.size, info.mode, info.mtime = len(data), mode, epoch
                archive.addfile(info, io.BytesIO(data))


def verify_archive(path, root_name, files, epoch):
    expected_files = {root_name + "/" + name: value for name, value in files.items()}
    expected_dirs = directories(root_name, files)
    seen = set()
    with tarfile.open(path, "r:gz") as archive:
        for member in archive:
            relative(member.name)
            if member.name in seen or member.uid != 0 or member.gid != 0 or member.mtime != epoch:
                raise ValueError("invalid archive metadata")
            seen.add(member.name)
            if member.name in expected_dirs:
                if not member.isdir() or member.mode != 0o755:
                    raise ValueError("invalid archive directory")
            elif member.name in expected_files:
                data, mode = expected_files[member.name]
                if not member.isfile() or member.mode != mode or member.size != len(data):
                    raise ValueError("invalid archive file")
                if sha(archive.extractfile(member).read(len(data) + 1)) != sha(data):
                    raise ValueError("archive byte mismatch")
            else:
                raise ValueError("unexpected archive member")
    if seen != set(expected_files) | expected_dirs:
        raise ValueError("missing archive member")


def interrupted(signum, _frame):
    global INTERRUPTED
    INTERRUPTED = True
    if ACTIVE is not None and ACTIVE.poll() is None:
        try:
            ACTIVE.send_signal(signum)
        except ProcessLookupError:
            pass


def bounded(command, env, work, timeout=60):
    global ACTIVE
    if INTERRUPTED:
        raise ValueError("interrupted")
    out, err = work / "command.stdout", work / "command.stderr"
    args = [sys.executable, "-I", "-B", str(ROOT / "scripts/smoke-supervisor.py"),
            "--timeout", str(timeout), "--kill-after", "3", "--stdout", str(out),
            "--stderr", str(err), "--", *command]
    ACTIVE = subprocess.Popen(args, cwd=ROOT, env=env)
    if INTERRUPTED and ACTIVE.poll() is None:
        try:
            ACTIVE.send_signal(signal.SIGTERM)
        except ProcessLookupError:
            pass
    status = ACTIVE.wait()
    ACTIVE = None
    if status or INTERRUPTED:
        raise ValueError("bounded command failed; candidate not produced")
    return out.read_text()


def checkout_fingerprint(env, work):
    names = bounded(["git", "ls-files", "--cached", "--others",
                     "--exclude-standard", "-z"], env, work).split("\0")
    hashes = {name: sha(regular(ROOT, name)) for name in names if name}
    return sha(json_bytes(hashes))


def reproducible_flags(source, work, rustup):
    mappings = [(str(source), "/urdia/source"), (str(work), "/urdia/build"),
                (str(rustup), "/urdia/toolchain")]
    return {
        "CARGO_ENCODED_RUSTFLAGS": "\x1f".join("--remap-path-prefix=" + old + "=" + new
                                               for old, new in mappings),
        "CFLAGS": shlex.join("-ffile-prefix-map=" + old + "=" + new for old, new in mappings[:2]),
        "CC_SHELL_ESCAPED_FLAGS": "1",
        "CARGO_PROFILE_RELEASE_STRIP": "symbols",
    }



def link_map_report(data, binary, work, rustup):
    """Bounded lld section observations, not a source-level license attribution."""
    if len(data) > 64 * 1024 * 1024:
        raise ValueError("oversized linker map")
    lines = data.decode("utf-8").splitlines()
    if not lines or lines[0].split() != ["VMA", "LMA", "Size", "Align", "Out", "In", "Symbol"]:
        raise ValueError("unsupported linker map format")
    groups = {}
    output_section = None
    for line in lines[1:]:
        if not line.strip():
            continue
        # LLVM lld ELF MapFile.cpp writes four fixed-width numeric columns.
        if (len(line) < 50 or not re.fullmatch(r"[ 0-9a-f]+", line[:49])
                or len(line[:49].split()) != 4):
            raise ValueError("invalid linker map row")
        fields, tail = line[:49].split(), line[49:]
        size = int(fields[2], 16)
        if tail.startswith("                "):  # Symbol, not an input section.
            continue
        if not tail.startswith("        "):
            if not re.fullmatch(r"\.[A-Za-z0-9_.-]+", tail):
                raise ValueError("invalid output section")
            output_section = tail
            continue
        match = re.fullmatch(r"        (.+):\((.+)\)", tail)
        if not match or output_section is None:
            raise ValueError("invalid input section")
        origin = match[1]
        if origin == "<internal>" or size == 0:
            continue
        archive = re.fullmatch(r"(.+)\(([^/()]+)\)", origin)
        source = archive[1] if archive else origin
        member = archive[2] if archive else None
        path = Path(source)
        if not path.is_absolute():
            raise ValueError("unknown linker input origin")
        # Lexical normalization handles GCC's ../ startup-object paths. No input
        # is opened here: rustc removes some transient objects before returning.
        path = Path(os.path.normpath(path))
        normalized = None
        for prefix, label in ((work / "target", "build/target"),
                              (rustup, "toolchain"), (Path("/usr/lib"), "host/usr/lib"),
                              (Path("/lib"), "host/lib")):
            if path.is_relative_to(prefix):
                normalized = label + "/" + path.relative_to(prefix).as_posix()
                break
        if normalized is None and path.is_relative_to(work):
            parts = path.relative_to(work).parts
            if len(parts) == 2 and re.fullmatch(r"rustc[A-Za-z0-9]+", parts[0]) and parts[1] == "symbols.o":
                normalized = "build/rustc/symbols.o"
        if normalized is None or not re.fullmatch(r"[A-Za-z0-9_./+-]+", normalized):
            raise ValueError("unknown linker input origin")
        # Cargo's relink-map flag contains the isolated build path, which changes
        # only root-crate object disambiguators even when stripped bytes match.
        # Keep the codegen unit number and every section count/size; do not
        # normalize dependency/archive identities or substantive binary bytes.
        if member is None and normalized.startswith("build/target/" + TARGET + "/release/deps/"):
            match_root = re.fullmatch(r"urdia-[0-9a-f]{16}\.(?:urdia\.[0-9a-f]{16}-cgu\.([0-9]{2})|([a-z0-9]{1,64}))\.rcgu\.o", path.name)
            if match_root:
                name = ("urdia.cgu." + match_root[1] + ".rcgu.o" if match_root[1]
                        else "urdia.generated.rcgu.o")
                normalized = normalized.rsplit("/", 1)[0] + "/" + name
        if member:
            if not re.fullmatch(r"[A-Za-z0-9_.+-]+", member):
                raise ValueError("invalid archive member identity")
            normalized += "(" + member + ")"
        key = (normalized, output_section)
        count, total = groups.get(key, (0, 0))
        groups[key] = (count + 1, total + size)
    if not groups:
        raise ValueError("empty linker contribution evidence")
    return {"schema": "urdia.link-section-observations/v2",
            "origin_normalization": "root-codegen-unit-metadata-v1", "publication_allowed": False,
            "binary_sha256": sha(binary), "format": "LLVM lld ELF map",
            "scope": "positive-size input sections grouped by origin and output section; synthetic inputs excluded",
            "source_license_attribution_complete": False,
            "inputs": [{"origin": origin, "output_section": section,
                        "input_section_count": count, "reported_size_bytes": total}
                       for (origin, section), (count, total) in sorted(groups.items())]}


def validate_link_observations(candidate, evidence, binary):
    # Legacy candidates predate map capture. Their evidence remains readable;
    # absence must never be presented as linked-component coverage.
    if "link_observations" not in evidence:
        return
    binding = evidence["link_observations"]
    if set(binding) != {"path", "sha256"} or binding["path"] != "link-observations.json":
        raise ValueError("invalid link observation binding")
    data = regular(candidate, binding["path"])
    if sha(data) != binding["sha256"]:
        raise ValueError("link observation checksum mismatch")
    report = json.loads(data, object_pairs_hook=pairs_unique)
    if (report.get("schema") not in ("urdia.link-section-observations/v1", "urdia.link-section-observations/v2")
            or (report.get("schema") == "urdia.link-section-observations/v2" and report.get("origin_normalization") != "root-codegen-unit-metadata-v1")
            or report.get("publication_allowed") is not False
            or report.get("source_license_attribution_complete") is not False
            or report.get("binary_sha256") != sha(binary)
            or not isinstance(report.get("inputs"), list) or not report["inputs"]):
        raise ValueError("invalid link observation evidence")

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, required=True,
                        help="new private output directory outside the checkout")
    parser.add_argument("--allow-dirty", action="store_true",
                        help="development only; evidence records dirty inputs")
    args = parser.parse_args()
    output = args.output_dir.absolute()
    if output.exists() or output.is_symlink() or ROOT in output.resolve().parents or output.resolve() == ROOT:
        raise ValueError("output must be a new directory outside the checkout")
    if not output.parent.is_dir():
        raise ValueError("output parent must exist")
    plan, inventory, packages = validate_plan(ROOT)
    if sys.platform != "linux":
        raise ValueError("Linux process supervision required")
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, interrupted)
    with tempfile.TemporaryDirectory(prefix="urdia-package-") as raw:
        work = Path(raw)
        home = work / "home"
        home.mkdir()
        cargo = shutil.which("cargo")
        if not cargo:
            raise ValueError("Cargo unavailable")
        env = {"PATH": str(Path(cargo).parent) + ":/usr/local/bin:/usr/bin:/bin",
               "HOME": str(home), "LC_ALL": "C", "TMPDIR": str(work),
               "RUSTUP_HOME": os.environ.get("RUSTUP_HOME", str(Path.home() / ".rustup")),
               "CARGO_HOME": str(work / "cargo"), "CARGO_TARGET_DIR": str(work / "target"),
               "CARGO_BUILD_JOBS": "2"}
        env.update(reproducible_flags(ROOT, work, Path(env["RUSTUP_HOME"])))
        dirty = bool(bounded(["git", "status", "--porcelain", "--untracked-files=all"], env, work).strip())
        if dirty and not args.allow_dirty:
            raise ValueError("dirty checkout; use --allow-dirty only for development")
        commit = bounded(["git", "rev-parse", "HEAD"], env, work).strip()
        epoch = int(bounded(["git", "show", "-s", "--format=%ct", "HEAD"], env, work).strip())
        env["SOURCE_DATE_EPOCH"] = str(epoch)
        fingerprint = checkout_fingerprint(env, work)
        rustc = bounded(["rustc", "--version"], env, work).strip()
        if rustc != inventory["toolchains"][0]["observed_rustc"]:
            raise ValueError("toolchain notice identity differs from compiler")
        cc = bounded(["cc", "--version"], env, work).splitlines()[0]
        print("Building private core candidate (locked sources, isolated Cargo home)", flush=True)
        bounded(BUILD, env, work, timeout=1200)
        tree = bounded(["cargo", "tree", "--locked", "--offline", "-p", "urdia", "--target", TARGET,
                        "--edges", "normal,build", "--prefix", "none", "--format", "{p}"], env, work)
        keys = set(re.findall(r"^([^\s]+) v([^\s]+)", tree, re.M))
        actual = {(p["name"], p["version"]) for p in inventory["rust"] if (p["name"], p["version"]) in keys}
        expected = {(p["name"], p["version"]) for p in packages}
        metadata = json.loads(bounded(["cargo", "metadata", "--locked", "--offline", "--no-deps",
                                       "--format-version", "1"], env, work))
        internal = {(p["name"], p["version"]) for p in metadata["packages"]
                    if p["id"] in metadata["workspace_members"]}
        unknown = keys - actual - internal
        if actual != expected or unknown:
            raise ValueError("Linux dependency closure differs from plan")
        binary_path = work / "target" / TARGET / "release/urdia"
        binary = regular(binary_path.parent, "urdia", 128 * 1024 * 1024)
        map_path = work / "final-link.map"
        relink = ["cargo", "rustc", *BUILD[2:], "--", "-C", "link-arg=-Wl,-Map=" + str(map_path)]
        bounded(relink, env, work, timeout=1200)
        observed_binary = regular(binary_path.parent, "urdia", 128 * 1024 * 1024)
        if observed_binary != binary:
            raise ValueError("map capture changed candidate binary bytes")
        link_data = json_bytes(link_map_report(regular(work, "final-link.map", 64 * 1024 * 1024),
                                              binary, work, Path(env["RUSTUP_HOME"])))
        if len(binary) < 64 or binary[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", binary, 18)[0] != 62:
            raise ValueError("unexpected ELF target")
        dynamic = bounded(["readelf", "--dynamic", str(binary_path)], env, work)
        needed = sorted(set(re.findall(r"Shared library: \[([^\]]+)\]", dynamic)))
        if "libsqlite3.so.0" not in needed:
            raise ValueError("expected external SQLite library not found")
        versions = bounded(["readelf", "--version-info", str(binary_path)], env, work)
        glibc = sorted(set(re.findall(r"Name: (GLIBC_[0-9.]+)", versions)))
        # Recheck sources after building; refuse concurrent changes rather than
        # combining stale inventory/plan inputs with a newly read file set.
        current_plan, current_inventory, _ = validate_plan(ROOT)
        if current_plan != plan or current_inventory != inventory:
            raise ValueError("inputs changed during build")
        if checkout_fingerprint(env, work) != fingerprint:
            raise ValueError("checkout changed during build")
        if bounded(["git", "rev-parse", "HEAD"], env, work).strip() != commit:
            raise ValueError("HEAD changed during build")
        files = assemble_files(ROOT, plan, inventory, packages, binary)
        if checkout_fingerprint(env, work) != fingerprint:
            raise ValueError("checkout changed during assembly")
        archive = work / (plan["root_directory"] + ".tar.gz")
        write_archive(archive, plan["root_directory"], files, epoch)
        verify_archive(archive, plan["root_directory"], files, epoch)
        archive_hash = sha(archive.read_bytes())
        evidence = {"schema": "urdia.private-package-evidence/v1", "commit": commit,
                    "commit_dirty": dirty, "target": TARGET, "rustc": rustc,
                    "c_compiler": cc,
                    "link_observations": {"path": "link-observations.json", "sha256": sha(link_data)},
                    "build_normalization": "Rust and C source/build paths remapped; release symbols stripped",
                    "checkout_input_sha256": fingerprint,
                    "plan_sha256": sha(regular(ROOT, PLAN)),
                    "source_inventory_sha256": plan["input_sha256"]["third-party/inventory.json"],
                    "archive": archive.name, "archive_sha256": archive_hash,
                    "archive_epoch": epoch, "registry_packages": len(packages),
                    "elf_needed": needed, "glibc_symbol_versions": glibc,
                    "files": [{"path": p, "sha256": sha(data), "size_bytes": len(data),
                               "mode": format(mode, "04o")} for p, (data, mode) in sorted(files.items())],
                    "publication_allowed": False, "remaining_gates": plan["remaining_gates"],
                    "reproducibility": "archive metadata deterministic; independent rebuild not verified"}
        if INTERRUPTED:
            raise ValueError("interrupted")
        output.mkdir(mode=0o700)  # Exclusive: never replace an existing output.
        try:
            shutil.copyfile(archive, output / archive.name)
            (output / "SHA256SUMS").write_text(f"{archive_hash}  {archive.name}\n")
            (output / "package-evidence.json").write_bytes(json_bytes(evidence))
            (output / "link-observations.json").write_bytes(link_data)
            for path in output.iterdir():
                path.chmod(0o600)
            if INTERRUPTED:
                raise ValueError("interrupted before candidate completion")
        except BaseException:
            shutil.rmtree(output)
            raise
        print("Private candidate and checksums created; publication remains disabled")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"package: {error}", file=sys.stderr)
        raise SystemExit(1)
