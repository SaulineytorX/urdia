//! Offline limits and privacy checks for the opt-in planning evaluation runner.
#![cfg(target_os = "linux")]
use std::path::Path;
use std::process::Command;

#[test]
fn planning_runner_regressions_pass_without_account_use() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let output = Command::new("/usr/bin/python3")
        .args(["-I", "-B"])
        .arg(root.join("scripts/tests/test_planning_codex.py"))
        .output()
        .expect("offline planning runner checks");
    assert!(
        output.status.success(),
        "Planning runner regressions failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
