//! Offline regression tests for the disposable Laya Python installer inputs.

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Command;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the CLI crate should sit inside the repository")
        .to_path_buf()
}

#[test]
fn install_laya_python_regression_suite_passes_under_isolated_python() {
    let python = ["/usr/bin/python3", "/usr/local/bin/python3"]
        .into_iter()
        .find(|candidate| Path::new(candidate).is_file())
        .expect("Python 3 is required for the Laya installer regression suite");
    let suite = repository_root().join("scripts/tests/test_install_laya_python.py");
    assert!(suite.is_file(), "missing installer regression suite");
    let output = Command::new(python)
        .args(["-I", "-B"])
        .arg(suite)
        .output()
        .expect("run the isolated installer regression suite");
    assert!(
        output.status.success(),
        "installer regression suite failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
