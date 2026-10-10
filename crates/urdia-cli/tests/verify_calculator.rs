//! Offline acceptance verification must not trust agent prose or an empty suite.
#![cfg(target_os = "linux")]
use std::path::Path;
use std::process::Command;

#[test]
fn calculator_verification_regressions_pass_under_isolated_python() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let output = Command::new("/usr/bin/python3")
        .args(["-I", "-B"])
        .arg(root.join("scripts/tests/test_verify_calculator.py"))
        .output()
        .expect("run offline calculator verification regressions");
    assert!(
        output.status.success(),
        "calculator verification failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
