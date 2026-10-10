//! Linux-gated regression coverage for the offline model integrity verifier.
//!
//! The Python suite owns the filesystem fixtures.  Running it through an
//! isolated interpreter keeps the deployment check independent of optional
//! model packages and service configuration.
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
fn model_integrity_regression_suite_passes_under_isolated_python() {
    let Some(python) = ["/usr/bin/python3", "/usr/local/bin/python3"]
        .into_iter()
        .find(|candidate| Path::new(candidate).is_file())
    else {
        eprintln!("skipping: no Python 3 interpreter on this host");
        return;
    };
    let root = repository_root();
    let suite = root.join("scripts/tests/test_verify_model.py");
    assert!(suite.is_file(), "missing model integrity regression suite");

    let output = Command::new(python)
        .args(["-I", "-B"])
        .arg(&suite)
        .current_dir(&root)
        .output()
        .expect("run the isolated model integrity regression suite");
    assert!(
        output.status.success(),
        "model integrity regression suite failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
