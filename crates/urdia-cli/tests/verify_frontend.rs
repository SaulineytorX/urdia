//! Snapshot safety and consent remain available without optional browser packages.
#![cfg(target_os = "linux")]
use std::path::Path;
use std::process::Command;
#[test]
fn frontend_snapshot_safety_and_consent_pass() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let output = Command::new("/usr/bin/python3")
        .args(["-I", "-B"])
        .arg(root.join("scripts/tests/test_verify_frontend.py"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
