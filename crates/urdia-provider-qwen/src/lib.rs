//! Qwen Code provider for Urdia.
//!
//! Qwen is launched only as `qwen --acp`; no Qwen config, credentials, or
//! environment values are read by this crate. The optional live integration is
//! therefore safe to discover in `urdia doctor` without starting the process.

use async_trait::async_trait;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;
use urdia_acp::{AcpCommand, AcpProvider};
use urdia_core::{AgentProvider, AgentSession, ProviderInfo, Result, SessionRequest};

/// The default Qwen Code executable name.
pub const QWEN_COMMAND: &str = "qwen";

/// A Qwen Code ACP provider.
#[derive(Clone, Debug)]
pub struct QwenProvider {
    inner: AcpProvider,
    executable: PathBuf,
}

impl QwenProvider {
    /// Build a provider using `qwen` resolved by the child process PATH.
    #[must_use]
    pub fn new() -> Self {
        Self::from_executable(PathBuf::from(QWEN_COMMAND))
    }

    /// Build a provider from an explicit executable path, useful for tests.
    #[must_use]
    pub fn from_executable(executable: PathBuf) -> Self {
        let command = AcpCommand::new(executable.clone())
            .arg("--acp")
            .startup_timeout(Duration::from_secs(30))
            .turn_timeout(Duration::from_secs(300));
        Self {
            inner: AcpProvider::with_metadata("qwen", "Qwen Code", command),
            executable,
        }
    }

    /// Return the executable configured for this provider.
    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Resolve `qwen` from PATH without executing it or reading its config.
    #[must_use]
    pub fn discover() -> Option<PathBuf> {
        resolve_on_path(QWEN_COMMAND)
    }

    /// Return whether a Qwen executable is present on PATH.
    #[must_use]
    pub fn is_available() -> bool {
        Self::discover().is_some()
    }
}

impl Default for QwenProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentProvider for QwenProvider {
    fn info(&self) -> ProviderInfo {
        self.inner.info()
    }

    async fn start_session(&self, request: SessionRequest) -> Result<Box<dyn AgentSession>> {
        self.inner.start_session(request).await
    }
}

fn resolve_on_path(command: &str) -> Option<PathBuf> {
    let path = Path::new(command);
    if path.components().count() > 1 {
        return is_executable_file(path).then(|| path.to_path_buf());
    }
    let paths = env::var_os("PATH")?;
    for directory in env::split_paths(&paths) {
        let candidate = directory.join(command);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use urdia_core::{AuthenticationState, InferenceLocation, ProviderAvailability};

    #[test]
    fn discovery_does_not_execute_qwen() {
        let provider = QwenProvider::from_executable(PathBuf::from("/definitely/not/qwen"));
        assert_eq!(provider.executable(), Path::new("/definitely/not/qwen"));

        let info = provider.info();
        assert_eq!(info.id, "qwen");
        assert_eq!(info.name, "Qwen Code");
        assert_eq!(info.availability, ProviderAvailability::Unavailable);
        assert_eq!(info.authentication, AuthenticationState::Unknown);
        assert_eq!(info.inference, InferenceLocation::Unknown);
        assert_eq!(
            info.availability_detail.as_deref(),
            Some("executable not found: /definitely/not/qwen")
        );
    }
}
