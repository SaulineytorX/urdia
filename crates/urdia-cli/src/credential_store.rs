//! OS vault only; no project files, shell expansion, plaintext fallback or key argv.
use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[async_trait]
pub trait CredentialStore: Send + Sync {
    fn kind(&self) -> Option<&'static str>;
    fn max_key_bytes(&self) -> usize {
        4096
    }
    async fn read(&self) -> Result<Option<String>, ()>;
    async fn write(&self, key: &str) -> Result<(), ()>;
    async fn delete(&self) -> Result<(), ()>;
}
pub struct OsCredentialStore {
    program: Option<(PathBuf, bool)>,
}
impl OsCredentialStore {
    pub fn detect() -> Self {
        let windows =
            PathBuf::from("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe");
        let linux = PathBuf::from("/usr/bin/secret-tool");
        let program = if windows.is_file() && std::env::var_os("WSL_INTEROP").is_some() {
            Some((windows, true))
        } else if linux.is_file()
            && std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok_and(|v| v.starts_with("unix:"))
        {
            Some((linux, false))
        } else {
            None
        };
        Self { program }
    }
    async fn invoke(&self, action: &str, key: Option<&str>) -> Result<Option<String>, ()> {
        let (program, windows) = self.program.as_ref().ok_or(())?;
        let mut command = tokio::process::Command::new(program);
        command.env_clear().env("PATH", "/usr/bin:/bin");
        for name in [
            "HOME",
            "DBUS_SESSION_BUS_ADDRESS",
            "XDG_RUNTIME_DIR",
            "DISPLAY",
            "WSL_INTEROP",
            "WSL_DISTRO_NAME",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let input = if *windows {
            command.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                include_str!("credential-store.ps1"),
            ]);
            serde_json::to_vec(&serde_json::json!({"action":action,"key":key})).map_err(|_| ())?
        } else {
            match action {
                "read" => {
                    command.arg("lookup");
                }
                "write" => {
                    command.args(["store", "--label=Urdia Jev"]);
                }
                "delete" => {
                    command.arg("clear");
                }
                _ => return Err(()),
            }
            command.args(["service", "urdia", "provider", "jev", "schema", "1"]);
            key.map(|value| format!("{value}\n").into_bytes())
                .unwrap_or_default()
        };
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let operation = async {
            let mut child = command.spawn().map_err(|_| ())?;
            let mut stdin = child.stdin.take().ok_or(())?;
            stdin.write_all(&input).await.map_err(|_| ())?;
            drop(stdin);
            let stdout = child.stdout.take().ok_or(())?;
            let stderr = child.stderr.take().ok_or(())?;
            let (bytes, errors) = tokio::try_join!(
                async {
                    let mut bytes = Vec::new();
                    stdout
                        .take(8193)
                        .read_to_end(&mut bytes)
                        .await
                        .map_err(|_| ())?;
                    if bytes.len() > 8192 {
                        return Err(());
                    }
                    Ok(bytes)
                },
                async {
                    let mut errors = Vec::new();
                    stderr
                        .take(4097)
                        .read_to_end(&mut errors)
                        .await
                        .map_err(|_| ())?;
                    if errors.len() > 4096 {
                        return Err(());
                    }
                    Ok(errors)
                }
            )?;
            let status = child.wait().await.map_err(|_| ())?;
            // secret-tool uses 1 for an absent item as well as actual failures.
            // Only a quiet miss is absence/idempotent deletion; never echo stderr.
            if errors.is_empty()
                && ((action == "read" && status.code() == Some(if *windows { 2 } else { 1 }))
                    || (!windows && action == "delete" && status.code() == Some(1)))
            {
                return Ok(None);
            }
            if !status.success() {
                return Err(());
            }
            if action != "read" {
                return Ok(None);
            }
            let value = String::from_utf8(bytes).map_err(|_| ())?;
            let value = if *windows {
                value.trim_start_matches('\u{feff}').to_owned()
            } else {
                value.trim_end_matches(['\r', '\n']).to_owned()
            };
            if value.is_empty()
                || value.len() > 4096
                || value.bytes().any(|v| !(33..=126).contains(&v))
            {
                return Err(());
            }
            Ok(Some(value))
        };
        tokio::time::timeout(Duration::from_secs(5), operation)
            .await
            .map_err(|_| ())?
    }
}
#[async_trait]
impl CredentialStore for OsCredentialStore {
    fn kind(&self) -> Option<&'static str> {
        self.program.as_ref().map(|(_, win)| {
            if *win {
                "windows_credentials"
            } else {
                "secret_service"
            }
        })
    }
    fn max_key_bytes(&self) -> usize {
        if self.program.as_ref().is_some_and(|(_, win)| *win) {
            2560
        } else {
            4096
        }
    }
    async fn read(&self) -> Result<Option<String>, ()> {
        self.invoke("read", None).await
    }
    async fn write(&self, key: &str) -> Result<(), ()> {
        self.invoke("write", Some(key)).await.map(|_| ())
    }
    async fn delete(&self) -> Result<(), ()> {
        self.invoke("delete", None).await.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    /// Run only inside a disposable HOME + private D-Bus + native keyring session.
    #[tokio::test]
    #[ignore = "requires explicitly isolated native Secret Service tools"]
    async fn native_secret_service_acceptance() {
        let program =
            std::env::var_os("URDIA_NATIVE_SECRET_TOOL").expect("isolated helper path required");
        let phase = std::env::var("URDIA_NATIVE_VAULT_PHASE").expect("isolated phase required");
        let root = PathBuf::from(
            std::env::var_os("URDIA_NATIVE_VAULT_ROOT").expect("isolated root required"),
        )
        .canonicalize()
        .unwrap();
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("urdia-native-vault-")
        );
        assert_eq!(
            PathBuf::from(std::env::var_os("HOME").unwrap())
                .canonicalize()
                .unwrap(),
            root.join("vault-home")
        );
        assert_eq!(
            std::env::var("DBUS_SESSION_BUS_ADDRESS")
                .unwrap()
                .split(',')
                .next()
                .unwrap(),
            format!("unix:path={}", root.join("runtime/bus").display())
        );

        let store = OsCredentialStore {
            program: Some((PathBuf::from(program), false)),
        };
        const KEY: &str = "URDIA_SYNTHETIC_NATIVE_VAULT_KEY";
        match phase.as_str() {
            "write" => {
                assert!(store.read().await.unwrap().is_none());
                store.write(KEY).await.expect("native save failed");
                assert_eq!(store.read().await.unwrap().as_deref(), Some(KEY));
            }
            "read" => assert_eq!(store.read().await.unwrap().as_deref(), Some(KEY)),
            "locked" => assert_ne!(store.read().await.ok().flatten().as_deref(), Some(KEY)),
            "delete" => {
                store.delete().await.expect("native delete failed");
                assert!(store.read().await.unwrap().is_none());
                store.delete().await.expect("repeated native delete failed");
            }
            _ => panic!("unknown isolated phase"),
        }
    }

    #[tokio::test]
    async fn helper_uses_stdin_and_round_trips_without_key_argv_or_environment() {
        let root = tempfile::tempdir().unwrap();
        let program = root.path().join("tool");
        let data = root.path().join("fake-vault");
        let path = data.to_string_lossy();
        std::fs::write(&program, format!("#!/bin/sh\ncase \"$*\" in *TEST_KEY_SENTINEL*) exit 90;; esac\n[ -z \"$TYPESAFE_API_KEY\" ] || exit 91\ncase \"$1\" in\nstore) cat > '{path}';;\nlookup) [ -f '{path}' ] || exit 1; cat '{path}';;\nclear) rm -f '{path}';;\n*) exit 92;;\nesac\n")).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let store = OsCredentialStore {
            program: Some((program, false)),
        };
        assert!(store.read().await.unwrap().is_none());
        store.write("TEST_KEY_SENTINEL").await.unwrap();
        assert_eq!(
            store.read().await.unwrap().as_deref(),
            Some("TEST_KEY_SENTINEL")
        );
        store.delete().await.unwrap();
        assert!(store.read().await.unwrap().is_none());
    }
    #[tokio::test]
    async fn quiet_absence_is_not_confused_with_backend_errors() {
        let root = tempfile::tempdir().unwrap();
        // Use an immutable installed executable for the quiet exit, avoiding
        // freshly written executable files during parallel process creation.
        let store = OsCredentialStore {
            program: Some((PathBuf::from("/usr/bin/false"), false)),
        };
        assert!(store.read().await.unwrap().is_none());
        assert!(store.delete().await.is_ok());
        assert!(store.write("TEST_KEY_SENTINEL").await.is_err());
        let program = root.path().join("diagnostic-error");
        std::fs::write(
            &program,
            "#!/bin/sh\nprintf PRIVATE_ERROR_SENTINEL >&2\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let store = OsCredentialStore {
            program: Some((program, false)),
        };
        assert!(store.read().await.is_err());
        assert!(store.delete().await.is_err());
    }

    #[tokio::test]
    async fn missing_vault_refuses_storage_and_oversized_output_is_bounded() {
        let absent = OsCredentialStore { program: None };
        assert!(absent.read().await.is_err());
        assert!(absent.write("TEST_KEY_SENTINEL").await.is_err());
        assert!(absent.delete().await.is_err());
        let root = tempfile::tempdir().unwrap();
        let program = root.path().join("oversized");
        std::fs::write(
            &program,
            "#!/bin/sh\nexec /usr/bin/python3 -c 'import os; os.write(1,b\"x\"*9000)'\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let store = OsCredentialStore {
            program: Some((program, false)),
        };
        assert!(store.read().await.is_err());
    }
}
