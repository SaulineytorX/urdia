//! Contract tests for the release smoke test and its evidence file.
//!
//! These tests exercise `scripts/smoke-test.sh` in modes that require no Rust
//! toolchain, no agent, and no network: the sanitizer self-test, the guard that
//! keeps evidence outside the repository, the fail-closed path that still writes
//! evidence when a required check aborts the run, the refusal to publish evidence
//! to a destination that cannot hold it, and prompt interruption of a
//! long-running check together with the removal of its descendants.
//!
//! The script uses the Linux process-supervisor contract, so the module is
//! Linux-gated: non-Linux hosts do not claim support for this contract.
#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

const EVIDENCE_SCHEMA: &str = "urdia.smoke-evidence/v1";

const SELF_TEST_CHECKS: [&str; 11] = [
    "self-test-sanitizer-redacts-private-values",
    "self-test-diagnostics-are-bounded-and-escaped",
    "self-test-failure-log-is-sanitized",
    "self-test-first-failure-wins",
    "self-test-installed-binary-is-selected",
    "self-test-pinned-toolchain-is-selected",
    "self-test-bounded-command-cleans-after-successful-parent-exit",
    "self-test-bounded-command-kills-term-ignoring-descendants",
    "self-test-evidence-validator-rejects-bad-documents",
    "self-test-evidence-fails-closed",
    "self-test-evidence-contract",
];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the CLI crate should sit inside the repository")
        .to_path_buf()
}

fn smoke_script() -> PathBuf {
    let script = repository_root().join("scripts/smoke-test.sh");
    assert!(
        script.is_file(),
        "missing smoke script at {}",
        script.display()
    );
    script
}

fn bash() -> Option<&'static str> {
    ["/bin/bash", "/usr/bin/bash"]
        .into_iter()
        .find(|candidate| Path::new(candidate).is_file())
}

fn read_evidence(directory: &Path) -> (String, Value) {
    let path = directory.join("smoke-evidence.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let parsed: Value = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
    (text, parsed)
}

fn assert_no_private_paths(text: &str) {
    let root = repository_root();
    assert!(
        !text.contains(&root.display().to_string()),
        "evidence leaked the repository path"
    );
    let home = std::env::var("HOME").unwrap_or_default();
    if home.len() > 1 {
        assert!(!text.contains(&home), "evidence leaked the home directory");
    }
}

fn check_names(evidence: &Value) -> Vec<String> {
    evidence["checks"]
        .as_array()
        .expect("checks should be an array")
        .iter()
        .map(|check| {
            check["name"]
                .as_str()
                .expect("every check should carry a name")
                .to_owned()
        })
        .collect()
}

#[test]
fn self_test_emits_sanitized_passing_evidence() {
    let Some(bash) = bash() else {
        eprintln!("skipping: no bash interpreter on this host");
        return;
    };
    let evidence_dir = TempDir::new().expect("temporary evidence directory");
    let output = Command::new(bash)
        .arg(smoke_script())
        .args(["--self-test", "--evidence-dir"])
        .arg(evidence_dir.path())
        .output()
        .expect("run the smoke self-test");
    assert!(
        output.status.success(),
        "smoke self-test failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let (text, evidence) = read_evidence(evidence_dir.path());
    assert_eq!(evidence["schema_version"], EVIDENCE_SCHEMA);
    assert_eq!(evidence["mode"], "self-test");
    assert_eq!(evidence["result"], "pass");
    assert_eq!(evidence["failed"], 0);

    let checks = evidence["checks"].as_array().expect("checks array");
    assert_eq!(evidence["passed"], checks.len() as u64);
    assert_eq!(check_names(&evidence), SELF_TEST_CHECKS);
    for check in checks {
        assert_eq!(check["status"], "pass");
        assert!(
            check["duration_ms"].as_u64().is_some(),
            "duration_ms should be a non-negative integer"
        );
    }

    let commit = evidence["commit"].as_str().expect("commit string");
    assert!(
        commit == "unknown" || commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()),
        "commit should be a full hash or unknown, got {commit}"
    );
    assert!(
        evidence["commit_dirty"].is_boolean() || evidence["commit_dirty"].is_null(),
        "commit_dirty should be a boolean, or null when git could not report it"
    );
    assert!(evidence["toolchain"]["rustc"].is_string());
    assert!(evidence["toolchain"]["cargo"].is_string());
    assert!(evidence["platform"]["os"].is_string());
    assert!(evidence["platform"]["arch"].is_string());
    assert!(evidence["duration_ms"].as_u64().is_some());

    assert_no_private_paths(&text);
    assert!(
        !text.contains("sentinel"),
        "evidence should not discuss sentinel values"
    );
}

#[test]
fn evidence_directory_must_stay_outside_the_repository() {
    let Some(bash) = bash() else {
        eprintln!("skipping: no bash interpreter on this host");
        return;
    };
    let output = Command::new(bash)
        .arg(smoke_script())
        .args(["--self-test", "--evidence-dir"])
        .arg(repository_root())
        .output()
        .expect("run the smoke self-test");
    assert!(
        !output.status.success(),
        "an in-repository evidence directory must be rejected"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("outside the repository working tree"),
        "unexpected rejection message: {stderr}"
    );
    assert!(
        !repository_root().join("smoke-evidence.json").exists(),
        "a rejected run must not write evidence into the repository"
    );
}

#[test]
fn an_aborted_run_still_records_failing_evidence() {
    let Some(bash) = bash() else {
        eprintln!("skipping: no bash interpreter on this host");
        return;
    };
    let evidence_dir = TempDir::new().expect("temporary evidence directory");
    let empty_target = TempDir::new().expect("temporary target directory");
    let output = Command::new(bash)
        .arg(smoke_script())
        .args(["--skip-build", "--target-dir"])
        .arg(empty_target.path())
        .arg("--evidence-dir")
        .arg(evidence_dir.path())
        .output()
        .expect("run the smoke test without binaries");
    assert!(
        !output.status.success(),
        "missing release binaries must fail the run"
    );

    let (text, evidence) = read_evidence(evidence_dir.path());
    assert_eq!(evidence["schema_version"], EVIDENCE_SCHEMA);
    assert_eq!(evidence["mode"], "full");
    assert_eq!(evidence["result"], "fail");
    assert_eq!(evidence["failed"], 1);
    assert_eq!(
        check_names(&evidence),
        ["prebuilt-binaries-present"],
        "an aborted run should stop after the failed required check"
    );
    assert_no_private_paths(&text);

    let failures = std::fs::read_to_string(evidence_dir.path().join("failures.log"))
        .expect("an aborted run should leave a failure log");
    assert!(failures.contains("prebuilt-binaries-present"));
    assert_no_private_paths(&failures);
}

fn effective_uid_is_zero() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|text| text.trim() == "0")
        .unwrap_or(false)
}

/// Evidence is mandatory. A destination that cannot even be created must fail
/// the run instead of letting it report success without publishing anything.
#[test]
fn an_uncreatable_evidence_destination_fails_the_run() {
    let Some(bash) = bash() else {
        eprintln!("skipping: no bash interpreter on this host");
        return;
    };
    let parent = TempDir::new().expect("temporary directory");
    // A regular file where a directory is required: this cannot succeed for any
    // user, so the assertion holds even when the suite runs as root.
    let blocker = parent.path().join("not-a-directory");
    std::fs::write(&blocker, b"").expect("write the blocker file");
    let output = Command::new(bash)
        .arg(smoke_script())
        .args(["--self-test", "--evidence-dir"])
        .arg(blocker.join("evidence"))
        .output()
        .expect("run the smoke self-test");
    assert!(
        !output.status.success(),
        "an uncreatable evidence directory must fail the run"
    );
    assert!(
        !blocker.join("evidence").exists(),
        "no evidence directory should have been created"
    );
}

/// A destination that exists but cannot be written must also fail the run.
#[test]
fn an_unwritable_evidence_destination_fails_the_run() {
    let Some(bash) = bash() else {
        eprintln!("skipping: no bash interpreter on this host");
        return;
    };
    if effective_uid_is_zero() {
        eprintln!("skipping: file permissions do not restrict the superuser");
        return;
    }
    let parent = TempDir::new().expect("temporary directory");
    let read_only = parent.path().join("evidence");
    std::fs::create_dir(&read_only).expect("create the evidence directory");
    // 0o500 keeps the directory traversable, so creating it succeeds and only
    // the write inside it fails.
    std::fs::set_permissions(&read_only, std::fs::Permissions::from_mode(0o500))
        .expect("make the evidence directory read-only");

    let output = Command::new(bash)
        .arg(smoke_script())
        .args(["--self-test", "--evidence-dir"])
        .arg(&read_only)
        .output()
        .expect("run the smoke self-test");

    // Restore write permission so the temporary directory can be removed.
    let restored = std::fs::set_permissions(&read_only, std::fs::Permissions::from_mode(0o700));
    assert!(
        restored.is_ok(),
        "the evidence directory should be restorable"
    );

    assert!(
        !output.status.success(),
        "an unwritable evidence directory must fail the run"
    );
    assert!(
        !read_only.join("smoke-evidence.json").exists(),
        "no evidence file should have been published"
    );
    assert!(
        read_only
            .read_dir()
            .expect("read the directory")
            .next()
            .is_none(),
        "a failed run must not leave a temporary evidence file behind"
    );
}

#[test]
fn supervisor_regression_suite_passes_under_isolated_python() {
    let python = ["/usr/bin/python3", "/usr/local/bin/python3"]
        .into_iter()
        .find(|candidate| Path::new(candidate).is_file())
        .expect("Python 3 is required for the Linux smoke supervisor");
    let suite = repository_root().join("scripts/tests/test_smoke_supervisor.py");
    assert!(suite.is_file(), "missing supervisor regression suite");
    let output = Command::new(python)
        .arg("-I")
        .arg(suite)
        .output()
        .expect("run the isolated supervisor regression suite");
    assert!(
        output.status.success(),
        "supervisor regression suite failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
