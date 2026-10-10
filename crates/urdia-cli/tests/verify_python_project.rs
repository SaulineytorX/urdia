//! General Python checks remain explicit, bounded and independent of agent prose.
#![cfg(target_os = "linux")]
use std::{path::Path, process::Command};
#[test]
fn python_project_verification_regressions_pass_under_isolated_python() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let output = Command::new("/usr/bin/python3")
        .args(["-I", "-B"])
        .arg(root.join("scripts/tests/test_verify_python_project.py"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "project check regression failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
