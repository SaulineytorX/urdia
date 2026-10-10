//! Explicitly selected, profile-only settings. Never changes active sessions.
use crate::profiles::{ExecutionProfile, ProfileCatalog};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use urdia_core::{ProviderRegistry, Result, UrdiaError};
use urdia_ui::{CatalogError, CatalogProvider, CatalogSnapshot, ProfileDraft, SaveProfileRequest};
const MAX_BYTES: u64 = 128 * 1024;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default, rename = "profile")]
    profiles: Vec<ExecutionProfile>,
    default_profile: Option<String>,
}
struct State {
    catalog: ProfileCatalog,
    source: Option<Vec<u8>>,
    revision: u64,
    changed: bool,
    pending: std::collections::BTreeSet<String>,
}
pub(crate) struct ProfileStore {
    registry: Mutex<ProviderRegistry>,
    path: Option<PathBuf>,
    state: Mutex<State>,
}
fn invalid() -> UrdiaError {
    UrdiaError::InvalidInput("profile store must be a bounded, private regular profile-only TOML file in an existing directory".into())
}
pub(crate) fn read_source(path: &Path) -> Result<Option<Vec<u8>>> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid()),
    };
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return Err(invalid());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if meta.permissions().mode() & 0o077 != 0 || meta.nlink() != 1 {
            return Err(invalid());
        }
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| invalid())?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    Ok(Some(bytes))
}
pub(crate) struct StoreLock(std::fs::File);
impl Drop for StoreLock {
    fn drop(&mut self) {
        // Explicit unlock releases the shared open-file-description lock even
        // when a concurrent process launch briefly inherits its descriptor.
        let _ = self.0.unlock();
    }
}
pub(crate) fn lock_store(path: &Path) -> std::result::Result<StoreLock, CatalogError> {
    let mut name = path
        .file_name()
        .ok_or(CatalogError::WriteFailed)?
        .to_os_string();
    name.push(".lock");
    let lock_path = path.with_file_name(name);
    let mut options = std::fs::File::options();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = match options.open(&lock_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            read_source(&lock_path).map_err(|_| CatalogError::Conflict)?;
            std::fs::File::options()
                .read(true)
                .write(true)
                .open(&lock_path)
                .map_err(|_| CatalogError::WriteFailed)?
        }
        Err(_) => return Err(CatalogError::WriteFailed),
    };
    file.try_lock().map_err(|_| CatalogError::Conflict)?;
    Ok(StoreLock(file))
}

pub(crate) fn private_path(path: &Path) -> Result<PathBuf> {
    let name = path.file_name().ok_or_else(invalid)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = std::fs::canonicalize(parent).map_err(|_| invalid())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(&parent)
            .map_err(|_| invalid())?
            .permissions()
            .mode()
            & 0o022
            != 0
        {
            return Err(invalid());
        }
    }
    Ok(parent.join(name))
}
pub(crate) fn replace_source(
    path: &Path,
    expected: &Option<Vec<u8>>,
    source: &[u8],
) -> std::result::Result<(), CatalogError> {
    // Stable adjacent inode serializes cooperating Urdia processes. Keep the
    // file in place; dropping this handle releases the OS lock after a crash too.
    let _lock = lock_store(path)?;
    // Refuse externally changed files and symlinks. This is change detection,
    // not filesystem CAS against non-cooperating same-user writers.
    if read_source(path).map_err(|_| CatalogError::Conflict)? != *expected {
        return Err(CatalogError::Conflict);
    }
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().ok_or(CatalogError::WriteFailed)?)
            .map_err(|_| CatalogError::WriteFailed)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| CatalogError::WriteFailed)?;
    }
    temporary
        .write_all(source)
        .map_err(|_| CatalogError::WriteFailed)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| CatalogError::WriteFailed)?;
    if read_source(path).map_err(|_| CatalogError::Conflict)? != *expected {
        return Err(CatalogError::Conflict);
    }
    if expected.is_none() {
        temporary
            .persist_noclobber(path)
            .map_err(|_| CatalogError::Conflict)?;
    } else {
        temporary
            .persist(path)
            .map_err(|_| CatalogError::WriteFailed)?;
    }
    Ok(())
}

impl ProfileStore {
    pub fn new(
        registry: &ProviderRegistry,
        base: &ProfileCatalog,
        path: Option<&Path>,
    ) -> Result<Self> {
        let path = path.map(private_path).transpose()?;
        let source = path.as_ref().map(|p| read_source(p)).transpose()?.flatten();
        let catalog = if let Some(bytes) = &source {
            let file: File = toml::from_str(std::str::from_utf8(bytes).map_err(|_| invalid())?)
                .map_err(|_| invalid())?;
            ProfileCatalog::new(file.profiles, file.default_profile, registry)?
        } else {
            base.clone()
        };
        let mut shadow = ProviderRegistry::new();
        for info in registry.list() {
            shadow.register(registry.resolve(&info.id)?)?;
        }
        Ok(Self {
            registry: Mutex::new(shadow),
            path,
            state: Mutex::new(State {
                catalog,
                source,
                revision: 0,
                changed: false,
                pending: Default::default(),
            }),
        })
    }
    /// Editor-only launch addition; execution/advice registries remain immutable.
    pub fn stage_provider(
        &self,
        provider: std::sync::Arc<dyn urdia_core::AgentProvider>,
    ) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let id = provider.info().id;
        self.registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .register(provider)?;
        state.pending.insert(id);
        state.revision += 1;
        Ok(())
    }
    pub fn active_catalog(&self) -> ProfileCatalog {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .catalog
            .clone()
    }
    fn view(&self, state: &State) -> CatalogSnapshot {
        CatalogSnapshot {
            editable: self.path.is_some(),
            restart_required: state.changed || !state.pending.is_empty(),
            revision: state.revision,
            default_profile: state.catalog.default_profile.clone(),
            providers: self
                .registry
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .list()
                .iter()
                .map(|info| CatalogProvider {
                    id: info.id.clone(),
                    name: info.name.clone(),
                    executable_available: info.availability.is_available(),
                    pending_restart: state.pending.contains(&info.id),
                })
                .collect(),
            profiles: state
                .catalog
                .profiles
                .values()
                .map(|p| ProfileDraft {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    provider: p.provider.clone(),
                    agent: p.agent.clone(),
                    inference_provider: p.inference_provider.clone(),
                    model: p.model.clone(),
                    effort: p.effort.clone(),
                    model_selector: p.session_controls.as_ref().and_then(|c| c.model.clone()),
                    effort_selector: p.session_controls.as_ref().and_then(|c| c.effort.clone()),
                })
                .collect(),
        }
    }
    pub fn snapshot(&self) -> CatalogSnapshot {
        self.view(
            &self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
    pub fn save(
        &self,
        request: SaveProfileRequest,
    ) -> std::result::Result<CatalogSnapshot, CatalogError> {
        let path = self.path.as_ref().ok_or(CatalogError::Disabled)?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if request.revision != state.revision {
            return Err(CatalogError::Conflict);
        }
        let draft = request.profile;
        if draft.id.len() > 128 || draft.provider.len() > 128 {
            return Err(CatalogError::Invalid);
        }
        let controls = urdia_core::SessionControls {
            model: draft.model_selector,
            effort: draft.effort_selector,
        };
        let profile = ExecutionProfile {
            id: draft.id,
            name: draft.name,
            provider: draft.provider,
            agent: draft.agent,
            inference_provider: draft.inference_provider,
            model: draft.model,
            effort: draft.effort,
            session_controls: if controls.model.is_some() || controls.effort.is_some() {
                Some(controls)
            } else {
                None
            },
        };
        let mut next = state.catalog.profiles.clone();
        next.insert(profile.id.clone(), profile.clone());
        let default = if request.make_default {
            Some(profile.id)
        } else {
            state.catalog.default_profile.clone()
        };
        let catalog = ProfileCatalog::new(
            next.into_values().collect(),
            default,
            &self
                .registry
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
        .map_err(|_| CatalogError::Invalid)?;
        let source = toml::to_string(&File {
            profiles: catalog.profiles.values().cloned().collect(),
            default_profile: catalog.default_profile.clone(),
        })
        .map_err(|_| CatalogError::Invalid)?
        .into_bytes();
        if source.len() as u64 > MAX_BYTES {
            return Err(CatalogError::Invalid);
        }
        replace_source(path, &state.source, &source)?;
        state.source = Some(source);
        state.catalog = catalog;
        state.revision += 1;
        state.changed = true;
        Ok(self.view(&state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        dir
    }
    fn registry() -> ProviderRegistry {
        let mut registry = ProviderRegistry::new();
        registry
            .register_owned(urdia_acp::AcpProvider::with_metadata(
                "offline",
                "Reference",
                urdia_acp::AcpCommand::new("/usr/bin/true").args(["SECRET_ARG_SENTINEL"]),
            ))
            .unwrap();
        registry
    }
    fn request(revision: u64) -> SaveProfileRequest {
        SaveProfileRequest {
            revision,
            make_default: true,
            profile: ProfileDraft {
                id: "backend".into(),
                name: "Backend".into(),
                provider: "offline".into(),
                agent: "Reference".into(),
                inference_provider: "Unknown".into(),
                model: "Model name".into(),
                effort: Some("Medium".into()),
                model_selector: Some("exact-model".into()),
                effort_selector: Some("medium".into()),
            },
        }
    }
    #[test]
    fn pending_launch_can_be_saved_without_mutating_execution_registry() {
        let dir = directory();
        let path = dir.path().join("profiles.toml");
        let active = registry();
        let store = ProfileStore::new(&active, &ProfileCatalog::default(), Some(&path)).unwrap();
        store
            .stage_provider(std::sync::Arc::new(urdia_acp::AcpProvider::with_metadata(
                "pending",
                "Pending",
                urdia_acp::AcpCommand::new("/usr/bin/true"),
            )))
            .unwrap();
        let snapshot = store.snapshot();
        assert_eq!(snapshot.revision, 1);
        assert!(snapshot.restart_required);
        assert!(
            snapshot
                .providers
                .iter()
                .find(|p| p.id == "pending")
                .unwrap()
                .pending_restart
        );
        assert!(active.resolve("pending").is_err());
        let mut save = request(1);
        save.profile.provider = "pending".into();
        store.save(save).unwrap();
        assert!(ProfileStore::new(&active, &ProfileCatalog::default(), Some(&path)).is_err());
        let mut next = registry();
        next.register_owned(urdia_acp::AcpProvider::with_metadata(
            "pending",
            "Pending",
            urdia_acp::AcpCommand::new("/usr/bin/true"),
        ))
        .unwrap();
        let reloaded = ProfileStore::new(&next, &ProfileCatalog::default(), Some(&path)).unwrap();
        assert!(!reloaded.snapshot().restart_required);
        assert_eq!(
            reloaded.active_catalog().profiles["backend"].provider,
            "pending"
        );
    }
    #[test]
    fn saving_is_private_profile_only_restart_scoped_and_conflict_checked() {
        let dir = directory();
        let path = dir.path().join("profiles.toml");
        let registry = registry();
        let store = ProfileStore::new(&registry, &ProfileCatalog::default(), Some(&path)).unwrap();
        assert!(!path.exists());
        let active = store.active_catalog();
        let saved = store.save(request(0)).unwrap();
        assert!(saved.restart_required);
        assert_eq!(saved.revision, 1);
        assert!(active.profiles.is_empty());
        assert!(matches!(
            store.save(request(0)),
            Err(CatalogError::Conflict)
        ));
        let bytes = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("SECRET_ARG_SENTINEL"));
        assert!(
            !serde_json::to_string(&saved)
                .unwrap()
                .contains("SECRET_ARG_SENTINEL")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let resumed =
            ProfileStore::new(&registry, &ProfileCatalog::default(), Some(&path)).unwrap();
        assert!(!resumed.snapshot().restart_required);
        assert_eq!(
            resumed.active_catalog().default_profile.as_deref(),
            Some("backend")
        );
        assert_eq!(
            resumed.active_catalog().profiles["backend"]
                .session_controls
                .as_ref()
                .unwrap()
                .model
                .as_deref(),
            Some("exact-model")
        );
        let mut external = bytes;
        external.extend_from_slice(b"\n# external edit\n");
        std::fs::write(&path, &external).unwrap();
        assert!(matches!(
            store.save(request(1)),
            Err(CatalogError::Conflict)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), external);
    }
    #[test]
    fn disabled_unknown_launch_invalid_pins_and_duplicate_launch_never_write() {
        let dir = directory();
        let path = dir.path().join("profiles.toml");
        let registry = registry();
        let disabled = ProfileStore::new(&registry, &ProfileCatalog::default(), None).unwrap();
        assert!(matches!(
            disabled.save(request(0)),
            Err(CatalogError::Disabled)
        ));
        let store = ProfileStore::new(&registry, &ProfileCatalog::default(), Some(&path)).unwrap();
        let mut invalid = request(0);
        invalid.profile.provider = "unregistered".into();
        assert!(matches!(store.save(invalid), Err(CatalogError::Invalid)));
        let mut invalid = request(0);
        invalid.profile.model_selector = Some("SECRET\nVALUE".into());
        assert!(matches!(store.save(invalid), Err(CatalogError::Invalid)));
        assert!(!path.exists());
        store.save(request(0)).unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut duplicate = request(1);
        duplicate.profile.id = "other-profile".into();
        assert!(matches!(store.save(duplicate), Err(CatalogError::Invalid)));
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    #[test]
    fn cooperating_writers_are_locked_and_stale_instances_cannot_replace_new_settings() {
        let dir = directory();
        let path = dir.path().join("profiles.toml");
        let registry = registry();
        let first = ProfileStore::new(&registry, &ProfileCatalog::default(), Some(&path)).unwrap();
        let second = ProfileStore::new(&registry, &ProfileCatalog::default(), Some(&path)).unwrap();
        let held = lock_store(&path).unwrap();
        assert!(matches!(
            first.save(request(0)),
            Err(CatalogError::Conflict)
        ));
        assert!(!path.exists());
        drop(held);
        first.save(request(0)).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert!(matches!(
            second.save(request(0)),
            Err(CatalogError::Conflict)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), saved);
    }

    #[test]
    fn lock_release_does_not_wait_for_a_duplicate_descriptor_to_close() {
        let dir = directory();
        let path = dir.path().join("profiles.toml");
        let held = lock_store(&path).unwrap();
        let duplicate = held.0.try_clone().unwrap();
        assert!(matches!(lock_store(&path), Err(CatalogError::Conflict)));
        drop(held);
        let reacquired = lock_store(&path).expect("explicit release must unlock shared descriptor");
        drop(reacquired);
        drop(duplicate);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_hardlinks_public_modes_and_non_profile_config_are_refused() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = directory();
        let path = dir.path().join("profiles.toml");
        let target = dir.path().join("target");
        std::fs::write(&target, "SECRET_VALUE").unwrap();
        symlink(&target, &path).unwrap();
        let registry = registry();
        assert!(ProfileStore::new(&registry, &ProfileCatalog::default(), Some(&path)).is_err());
        std::fs::remove_file(&path).unwrap();
        let store = ProfileStore::new(&registry, &ProfileCatalog::default(), Some(&path)).unwrap();
        symlink(&target, &path).unwrap();
        assert!(matches!(
            store.save(request(0)),
            Err(CatalogError::Conflict)
        ));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "SECRET_VALUE");
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, "[[provider]]\ncommand = 'SECRET_VALUE'\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let error = ProfileStore::new(&registry, &ProfileCatalog::default(), Some(&path))
            .err()
            .unwrap()
            .to_string();
        assert!(!error.contains("SECRET_VALUE"));
        std::fs::write(&path, "").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(ProfileStore::new(&registry, &ProfileCatalog::default(), Some(&path)).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::hard_link(&path, dir.path().join("linked")).unwrap();
        assert!(ProfileStore::new(&registry, &ProfileCatalog::default(), Some(&path)).is_err());
    }
}
