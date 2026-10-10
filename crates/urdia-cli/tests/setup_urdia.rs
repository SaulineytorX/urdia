//! Guided first run must work without Git and keep the selected project unchanged.
#![cfg(target_os = "linux")]
use std::path::Path;
use std::process::Command;

#[test]
fn guided_setup_regressions_and_authenticated_demo_pass() {
    let binary = Path::new(env!("CARGO_BIN_EXE_urdia"));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap();
    let output = Command::new("/usr/bin/python3")
        .args(["-I", "-B"])
        .arg(root.join("scripts/tests/test_setup_urdia.py"))
        .env("URDIA_TEST_BINARY", binary)
        .env(
            "URDIA_TEST_FAKE",
            binary.parent().unwrap().join("urdia-fake-agent"),
        )
        .output()
        .expect("run offline guided setup regression tests");
    assert!(
        output.status.success(),
        "guided setup regression failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
