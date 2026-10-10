//! Exercise the project launcher against a real local authenticated conversation.
#![cfg(target_os = "linux")]

use std::path::Path;
use std::process::Command;

#[test]
fn project_preview_launches_an_offline_authenticated_conversation() {
    let binary = Path::new(env!("CARGO_BIN_EXE_urdia"));
    let fake = binary.parent().unwrap().join("urdia-fake-agent");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let output = Command::new("/usr/bin/python3")
        .args(["-I", "-B"])
        .arg(root.join("scripts/tests/test_start_project.py"))
        .env("URDIA_TEST_BINARY", binary)
        .env("URDIA_TEST_FAKE", fake)
        .output()
        .expect("run project preview regressions");
    assert!(
        output.status.success(),
        "project preview regressions failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
