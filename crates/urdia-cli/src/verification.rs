//! Bundled host verification helpers with an outer bounded Linux supervisor.
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use urdia_ui::{VerificationContract, VerificationReport};

const CHECKER: &[u8] = include_bytes!("../../../scripts/verify-calculator.py");
const PYTHON_CHECKER: &[u8] = include_bytes!("../../../scripts/verify-python-project.py");
const FRONTEND_CHECKER: &[u8] = include_bytes!("../../../scripts/verify-frontend.py");
const SUPERVISOR: &[u8] = include_bytes!("../../../scripts/smoke-supervisor.py");

pub fn available() -> bool {
    cfg!(target_os = "linux")
        && Path::new("/usr/bin/python3").is_file()
        && Path::new("/bin/kill").is_file()
}

pub fn run(
    workspace: &Path,
    contract: VerificationContract,
    mut cancel: mpsc::Receiver<()>,
) -> Option<VerificationReport> {
    if !available() || cancel.try_recv().is_ok() {
        return None;
    }
    let directory = tempfile::tempdir().ok()?;
    let scripts = directory.path().join("scripts");
    std::fs::create_dir(&scripts).ok()?;
    let checker = scripts.join("verify-calculator.py");
    let supervisor = scripts.join("smoke-supervisor.py");
    let python_checker = scripts.join("verify-python-project.py");
    std::fs::write(&python_checker, PYTHON_CHECKER).ok()?;
    std::fs::write(&checker, CHECKER).ok()?;
    std::fs::write(&supervisor, SUPERVISOR).ok()?;
    let frontend = matches!(
        contract,
        VerificationContract::FrontendStatic | VerificationContract::FrontendCounter
    );
    let frontend_checker = scripts.join("verify-frontend.py");
    std::fs::write(&frontend_checker, FRONTEND_CHECKER).ok()?;
    let python = if frontend {
        std::env::var_os("URDIA_FRONTEND_PYTHON")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| {
                    std::path::PathBuf::from(h)
                        .join(".local/share/urdia/frontend-checks/bin/python")
                })
            })?
    } else {
        std::path::PathBuf::from("/usr/bin/python3")
    };
    if !python.is_absolute() || !python.is_file() {
        return None;
    }
    let browser = std::env::var_os("URDIA_FRONTEND_BROWSER")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/opt/google/chrome/chrome"));
    if frontend && (!browser.is_absolute() || !browser.is_file()) {
        return None;
    }
    let output = directory.path().join("report.json");
    let mut command = Command::new("/usr/bin/python3");
    command
        .args(["-I", "-B"])
        .arg(&supervisor)
        .args(["--timeout", "35", "--kill-after", "1", "--stdout"])
        .arg(&output)
        .args(["--stderr", "/dev/null", "--"])
        .arg(&python)
        .args(["-I", "-B"])
        .arg(if frontend {
            &frontend_checker
        } else if contract == VerificationContract::PythonUnittest {
            &python_checker
        } else {
            &checker
        })
        .arg("--workspace")
        .arg(workspace)
        .args(["--contract", contract.id()])
        .arg("--confirm-execution")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", directory.path())
        .env("LC_ALL", "C")
        .current_dir(directory.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if frontend {
        command.arg("--browser").arg(browser);
    }
    let mut child = command.spawn().ok()?;
    let started = Instant::now();
    let mut interrupted = false;
    let status = loop {
        // Keep cancellation latched. The outer supervisor owns descendant cleanup.
        if !interrupted
            && (cancel.try_recv().is_ok() || started.elapsed() > Duration::from_secs(40))
        {
            interrupted = true;
            let _ = Command::new("/bin/kill")
                .args(["-TERM", &child.id().to_string()])
                .env_clear()
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => {
                let _ = child.wait();
                return None;
            }
        }
    };
    if interrupted || !matches!(status.code(), Some(0 | 1)) {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(output)
        .ok()?
        .take(8193)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 8192 {
        return None;
    }
    let report: VerificationReport = serde_json::from_slice(&bytes).ok()?;
    (report.valid()
        && report.schema_version
            == if frontend {
                4
            } else if contract == VerificationContract::PythonUnittest {
                3
            } else {
                2
            }
        && report.contract == contract.id()
        && (status.code() == Some(0)) == (report.status == "passed"))
        .then_some(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_frontend_counter_has_observed_pass_and_failure() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        if !std::path::PathBuf::from(home)
            .join(".local/share/urdia/frontend-checks/bin/python")
            .is_file()
            || !Path::new("/opt/google/chrome/chrome").is_file()
        {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<!doctype html><meta name='viewport' content='width=device-width,initial-scale=1'><p id='value'>0</p><button id='increment'>+</button><button id='decrement'>-</button><button id='reset'>Reset</button><script src='counter.js'></script>").unwrap();
        let good = "let n=0;const v=document.getElementById('value');document.getElementById('increment').onclick=()=>v.textContent=++n;document.getElementById('decrement').onclick=()=>v.textContent=n=Math.max(0,n-1);document.getElementById('reset').onclick=()=>v.textContent=n=0;";
        std::fs::write(dir.path().join("counter.js"), good).unwrap();
        let (_sender, receiver) = mpsc::channel(1);
        let report = run(dir.path(), VerificationContract::FrontendCounter, receiver)
            .expect("browser runtime must produce evidence");
        assert_eq!(report.status, "passed");
        assert_eq!(report.frontend.unwrap().checks_run, 14);
        std::fs::write(
            dir.path().join("counter.js"),
            good.replace("Math.max(0,n-1)", "n-1"),
        )
        .unwrap();
        let (_sender, receiver) = mpsc::channel(1);
        let report = run(dir.path(), VerificationContract::FrontendCounter, receiver).unwrap();
        assert_eq!(report.status, "failed");
        std::fs::write(
            dir.path().join("counter.js"),
            "throw new Error('private failure detail');",
        )
        .unwrap();
        let (_sender, receiver) = mpsc::channel(1);
        let report = run(dir.path(), VerificationContract::FrontendStatic, receiver).unwrap();
        assert_eq!(report.status, "failed");
        assert_eq!(
            report.checks["desktop"].failure_reason.as_deref(),
            Some("runtime")
        );
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("private failure detail")
        );
        std::fs::write(
            dir.path().join("index.html"),
            "<!doctype html><p>Missing mobile viewport</p>",
        )
        .unwrap();
        let (_sender, receiver) = mpsc::channel(1);
        let report = run(dir.path(), VerificationContract::FrontendStatic, receiver).unwrap();
        assert_eq!(report.checks["desktop"].status, "passed");
        assert_eq!(
            report.checks["mobile"].failure_reason.as_deref(),
            Some("layout")
        );
        assert_eq!(report.status, "failed");
    }

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("calculator.py"),
            "def add(a, b): return a + b\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("test_calculator.py"), "import unittest\nfrom calculator import add\nclass T(unittest.TestCase):\n def test_x(self): self.assertEqual(add(2, 3), 5)\n").unwrap();
        dir
    }
    #[test]
    fn bundled_python_project_checker_observes_generic_sources() {
        if !available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("tax.py"),
            "def tax(amount): return amount * 0.16\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("test_tax.py"), "import unittest\nfrom tax import tax\nclass T(unittest.TestCase):\n def test_x(self): self.assertAlmostEqual(tax(100),16)\n").unwrap();
        let (_sender, receiver) = mpsc::channel(1);
        let report = run(dir.path(), VerificationContract::PythonUnittest, receiver).unwrap();
        assert_eq!(report.status, "passed");
        assert_eq!(report.source_file_count, Some(2));
        assert_eq!(report.schema_version, 3);
        assert_eq!(report.checks.len(), 1);
        assert!(report.valid());
    }

    #[test]
    fn bundled_checks_observe_real_pass_and_independent_failure() {
        if !available() {
            return;
        }
        let dir = fixture();
        let (_sender, receiver) = mpsc::channel(1);
        assert_eq!(
            run(dir.path(), VerificationContract::Add, receiver)
                .unwrap()
                .status,
            "passed"
        );
        std::fs::write(
            dir.path().join("calculator.py"),
            "def add(a, b): return 5\n",
        )
        .unwrap();
        let (_sender, receiver) = mpsc::channel(1);
        let report = run(dir.path(), VerificationContract::Add, receiver).unwrap();
        assert_eq!(report.status, "failed");
        assert_eq!(report.checks["project_tests"].status, "passed");
        assert_eq!(report.checks["host_contract"].status, "failed");
    }
    #[test]
    fn cancellation_cleans_worker_and_refused_files_never_pass() {
        if !available() {
            return;
        }
        let dir = fixture();
        std::fs::write(
            dir.path().join("test_calculator.py"),
            "import os, time\nopen('worker.pid', 'w').write(str(os.getpid()))\ntime.sleep(60)\n",
        )
        .unwrap();
        let (sender, receiver) = mpsc::channel(1);
        let project = dir.path().to_path_buf();
        let worker = std::thread::spawn(move || run(&project, VerificationContract::Add, receiver));
        let deadline = Instant::now() + Duration::from_secs(5);
        let marker = dir.path().join("worker.pid");
        while !marker.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(marker.exists());
        let pid = std::fs::read_to_string(marker).unwrap();
        sender.blocking_send(()).unwrap();
        assert!(worker.join().unwrap().is_none());
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
        std::fs::remove_file(dir.path().join("calculator.py")).unwrap();
        let (_sender, receiver) = mpsc::channel(1);
        assert!(run(dir.path(), VerificationContract::Add, receiver).is_none());
    }
}
