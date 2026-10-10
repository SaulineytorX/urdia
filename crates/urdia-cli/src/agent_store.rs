//! Opt-in launch registration; writes never spawn agents or mutate a running registry.
use crate::profile_store::{private_path, read_source, replace_source};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
use urdia_core::{ProviderRegistry, Result, UrdiaError, is_valid_provider_id};
use urdia_ui::{
    AgentCatalogSnapshot, AgentDraft, CatalogError, RegisterAgentRequest, RegisteredAgentView,
};

const MAX_BYTES: usize = 128 * 1024;
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default, rename = "provider")]
    agents: Vec<AgentDraft>,
}
struct State {
    file: File,
    source: Option<Vec<u8>>,
    revision: u64,
    changed: bool,
}
pub(crate) struct AgentStore {
    path: Option<PathBuf>,
    state: Mutex<State>,
    active: BTreeMap<String, String>,
}
fn invalid() -> UrdiaError {
    UrdiaError::InvalidInput("invalid agent store: require private launch-only TOML, unique bounded IDs, names, executable and argument array".into())
}
fn validate(agents: &[AgentDraft]) -> Result<()> {
    let mut ids = std::collections::BTreeSet::new();
    if agents.len() > 64 {
        return Err(invalid());
    }
    for agent in agents {
        let text = |s: &str, max: usize| {
            !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
        };
        if !is_valid_provider_id(&agent.id)
            || agent.id.len() > 128
            || !ids.insert(&agent.id)
            || !text(&agent.name, 128)
            || !text(&agent.command, 4096)
            || agent.args.len() > 64
            || agent
                .args
                .iter()
                .any(|arg| arg.len() > 4096 || arg.chars().any(char::is_control))
        {
            return Err(invalid());
        }
    }
    Ok(())
}
impl AgentStore {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let path = path.map(private_path).transpose()?;
        let source = path.as_ref().map(|p| read_source(p)).transpose()?.flatten();
        let file: File = source
            .as_ref()
            .map(|bytes| {
                toml::from_str(std::str::from_utf8(bytes).map_err(|_| invalid())?)
                    .map_err(|_| invalid())
            })
            .transpose()?
            .unwrap_or_default();
        validate(&file.agents)?;
        Ok(Self {
            path,
            active: BTreeMap::new(),
            state: Mutex::new(State {
                file,
                source,
                revision: 0,
                changed: false,
            }),
        })
    }
    pub fn definitions(&self) -> Vec<AgentDraft> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .file
            .agents
            .clone()
    }
    pub fn bind_registry(&mut self, registry: &ProviderRegistry) {
        self.active = registry
            .list()
            .into_iter()
            .map(|info| (info.id, info.name))
            .collect();
    }
    fn view(&self, state: &State) -> AgentCatalogSnapshot {
        let mut agents: BTreeMap<String, RegisteredAgentView> = self
            .active
            .iter()
            .map(|(id, name)| {
                (
                    id.clone(),
                    RegisteredAgentView {
                        id: id.clone(),
                        name: name.clone(),
                        active: true,
                    },
                )
            })
            .collect();
        for agent in &state.file.agents {
            agents
                .entry(agent.id.clone())
                .or_insert_with(|| RegisteredAgentView {
                    id: agent.id.clone(),
                    name: agent.name.clone(),
                    active: false,
                });
        }
        AgentCatalogSnapshot {
            editable: self.path.is_some(),
            revision: state.revision,
            restart_required: state.changed,
            agents: agents.into_values().collect(),
        }
    }
    pub fn snapshot(&self) -> AgentCatalogSnapshot {
        self.view(
            &self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
    pub fn register(
        &self,
        request: RegisterAgentRequest,
    ) -> std::result::Result<AgentCatalogSnapshot, CatalogError> {
        let path = self.path.as_ref().ok_or(CatalogError::Disabled)?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.revision != request.revision {
            return Err(CatalogError::Conflict);
        }
        if !request.confirm_launch || self.active.contains_key(&request.agent.id) {
            return Err(CatalogError::Invalid);
        }
        let mut next = state.file.agents.clone();
        next.push(request.agent);
        validate(&next).map_err(|_| CatalogError::Invalid)?;
        let next = File { agents: next };
        let source = toml::to_string(&next)
            .map_err(|_| CatalogError::Invalid)?
            .into_bytes();
        if source.len() > MAX_BYTES {
            return Err(CatalogError::Invalid);
        }
        replace_source(path, &state.source, &source)?;
        state.file = next;
        state.source = Some(source);
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
    fn request(id: &str) -> RegisterAgentRequest {
        RegisterAgentRequest {
            revision: 0,
            confirm_launch: true,
            agent: AgentDraft {
                id: id.into(),
                name: "Offline agent".into(),
                command: "missing-agent".into(),
                args: vec!["literal $(not-a-shell)".into()],
            },
        }
    }
    #[test]
    fn registration_is_private_redacted_and_applies_on_reload_only() {
        let dir = directory();
        let path = dir.path().join("agents.toml");
        let mut store = AgentStore::load(Some(&path)).unwrap();
        let mut registry = ProviderRegistry::new();
        registry
            .register_owned(urdia_provider_qwen::QwenProvider::new())
            .unwrap();
        store.bind_registry(&registry);
        assert!(!path.exists());
        let snapshot = store.register(request("my-agent")).unwrap();
        assert!(snapshot.restart_required);
        assert!(
            !snapshot
                .agents
                .iter()
                .find(|a| a.id == "my-agent")
                .unwrap()
                .active
        );
        assert!(registry.resolve("my-agent").is_err());
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("missing-agent"));
        assert!(!json.contains("not-a-shell"));
        let loaded = AgentStore::load(Some(&path)).unwrap();
        assert_eq!(loaded.definitions()[0].args, vec!["literal $(not-a-shell)"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(matches!(
            store.register(request("another")),
            Err(CatalogError::Conflict)
        ));
    }
    #[test]
    fn invalid_duplicate_and_unconfirmed_registrations_do_not_write() {
        let dir = directory();
        let path = dir.path().join("agents.toml");
        let mut store = AgentStore::load(Some(&path)).unwrap();
        let mut registry = ProviderRegistry::new();
        registry
            .register_owned(urdia_provider_qwen::QwenProvider::new())
            .unwrap();
        store.bind_registry(&registry);
        for id in ["qwen", "Bad ID"] {
            assert!(matches!(
                store.register(request(id)),
                Err(CatalogError::Invalid)
            ));
        }
        let mut unconfirmed = request("my-agent");
        unconfirmed.confirm_launch = false;
        assert!(store.register(unconfirmed).is_err());
        let mut control = request("my-agent");
        control.agent.command = "secret\ncommand".into();
        assert!(store.register(control).is_err());
        assert!(!path.exists());
        store.register(request("my-agent")).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mut duplicate = request("my-agent");
        duplicate.revision = 1;
        assert!(store.register(duplicate).is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
        assert!(matches!(
            AgentStore::load(None).unwrap().register(request("x")),
            Err(CatalogError::Disabled)
        ));
    }
    #[test]
    fn external_changes_and_private_schema_are_enforced() {
        let dir = directory();
        let path = dir.path().join("agents.toml");
        let store = AgentStore::load(Some(&path)).unwrap();
        store.register(request("my-agent")).unwrap();
        std::fs::write(&path, b"provider = []\n").unwrap();
        let mut second = request("second-agent");
        second.revision = 1;
        assert!(matches!(
            store.register(second),
            Err(CatalogError::Conflict)
        ));
        std::fs::write(&path, b"secret = 'DO_NOT_REPEAT'\n").unwrap();
        let error = AgentStore::load(Some(&path)).err().unwrap().to_string();
        assert!(!error.contains("DO_NOT_REPEAT"));
    }
}
