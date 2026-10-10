//! Offline safety regressions for the optional real-Qwen control probe.
#![cfg(target_os = "linux")]
use std::path::Path;
use std::process::Command;

#[test]
fn offline_qwen_control_probe_regressions_pass() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let output = Command::new("/usr/bin/python3")
        .args(["-I", "-B"])
        .arg(root.join("scripts/tests/test_qwen_controls.py"))
        .output()
        .expect("run offline control probe regressions");
    assert!(
        output.status.success(),
        "control probe regressions failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
