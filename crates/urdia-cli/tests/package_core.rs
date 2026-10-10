//! Offline archive and input-validation regressions; no build or network.
#![cfg(target_os = "linux")]

use std::path::Path;
use std::process::Command;

#[test]
fn core_package_regression_suite_passes_under_isolated_python() {
    let python = ["/usr/bin/python3", "/usr/local/bin/python3"]
        .into_iter()
        .find(|candidate| Path::new(candidate).is_file())
        .expect("Python 3 is required for package regressions");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI is inside the repository");
    let output = Command::new(python)
        .args(["-I", "-B"])
        .arg(root.join("scripts/tests/test_package_core.py"))
        .output()
        .expect("run package regression suite");
    assert!(
        output.status.success(),
        "package regressions failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
