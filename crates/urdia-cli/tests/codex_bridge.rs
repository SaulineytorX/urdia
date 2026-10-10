//! Offline regressions for the optional native Codex bridge; no account or network.
#![cfg(target_os = "linux")]
use std::path::Path;
use std::process::Command;

#[test]
fn offline_codex_bridge_regressions_pass() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let output = Command::new("/usr/bin/python3")
        .args(["-I", "-B"])
        .arg(root.join("scripts/tests/test_codex_acp.py"))
        .output()
        .expect("run offline Codex bridge regressions");
    assert!(
        output.status.success(),
        "Codex bridge regressions failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
