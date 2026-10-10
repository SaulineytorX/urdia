//! Explicit launch profiles. Labels describe operator configuration, not verified model capabilities.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use urdia_core::{ProviderRegistry, Result, UrdiaError, is_valid_provider_id};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionProfile {
    pub id: String,
    pub name: String,
    /// A dedicated registered ACP launch definition, including its exact argv.
    pub provider: String,
    pub agent: String,
    pub inference_provider: String,
    pub model: String,
    /// Omit when the integration has no supported effort control.
    pub effort: Option<String>,
    /// Optional exact ACP selector values. Labels above remain display-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_controls: Option<urdia_core::SessionControls>,
}

impl ExecutionProfile {
    pub fn view(&self, selection: &str) -> urdia_ui::ConversationProfile {
        urdia_ui::ConversationProfile {
            id: self.id.clone(),
            name: self.name.clone(),
            agent: self.agent.clone(),
            inference_provider: self.inference_provider.clone(),
            model: self.model.clone(),
            effort: self.effort.clone(),
            selection: selection.to_owned(),
            model_effort_verified: false,
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct ProfileCatalog {
    pub profiles: BTreeMap<String, ExecutionProfile>,
    pub default_profile: Option<String>,
}

fn invalid() -> UrdiaError {
    // Do not repeat untrusted config values, which may accidentally contain secrets.
    UrdiaError::InvalidInput("invalid execution profiles: require unique valid IDs, dedicated registered providers, bounded labels and a known default profile".to_owned())
}

impl ProfileCatalog {
    pub fn new(
        profiles: Vec<ExecutionProfile>,
        default_profile: Option<String>,
        registry: &ProviderRegistry,
    ) -> Result<Self> {
        let mut catalog = Self::default();
        let mut providers = BTreeSet::new();
        if profiles.len() > 64 {
            return Err(invalid());
        }
        for profile in profiles {
            if let Some(controls) = &profile.session_controls {
                controls.validate()?;
            }
            let labels = [
                &profile.name,
                &profile.agent,
                &profile.inference_provider,
                &profile.model,
            ];
            if !is_valid_provider_id(&profile.id)
                || registry.resolve(&profile.provider).is_err()
                || !providers.insert(profile.provider.clone())
                || labels
                    .into_iter()
                    .chain(profile.effort.iter())
                    .any(|label| {
                        label.trim().is_empty()
                            || label.len() > 128
                            || label.chars().any(char::is_control)
                    })
                || catalog.profiles.contains_key(&profile.id)
            {
                return Err(invalid());
            }
            catalog.profiles.insert(profile.id.clone(), profile);
        }
        if default_profile
            .as_ref()
            .is_some_and(|id| !catalog.profiles.contains_key(id))
        {
            return Err(invalid());
        }
        catalog.default_profile = default_profile;
        Ok(catalog)
    }

    pub fn controls(
        &self,
        provider: Option<&str>,
        profile: Option<&str>,
    ) -> urdia_core::SessionControls {
        if provider.is_some() {
            return urdia_core::SessionControls::default();
        }
        profile
            .or(self.default_profile.as_deref())
            .and_then(|id| self.profiles.get(id))
            .and_then(|profile| profile.session_controls.clone())
            .unwrap_or_default()
    }

    pub fn select(&self, provider: Option<&str>, profile: Option<&str>) -> Result<String> {
        if let Some(provider) = provider {
            if profile.is_some() {
                return Err(UrdiaError::InvalidInput(
                    "select either --provider or --profile".to_owned(),
                ));
            }
            return Ok(provider.to_owned());
        }
        let id = profile.or(self.default_profile.as_deref()).ok_or_else(|| {
            UrdiaError::InvalidInput(
                "select --provider, --profile or configure default_profile".to_owned(),
            )
        })?;
        self.profiles
            .get(id)
            .map(|profile| profile.provider.clone())
            .ok_or_else(|| UrdiaError::InvalidInput("unknown execution profile".to_owned()))
    }
}

impl std::fmt::Debug for ExecutionProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExecutionProfile { configuration omitted }")
    }
}
impl std::fmt::Debug for ProfileCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfileCatalog")
            .field("count", &self.profiles.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use urdia_provider_qwen::QwenProvider;

    fn registry() -> ProviderRegistry {
        let mut registry = ProviderRegistry::new();
        registry.register_owned(QwenProvider::new()).unwrap();
        registry
    }
    fn profile() -> ExecutionProfile {
        ExecutionProfile {
            id: "local-choice".into(),
            name: "My configured Qwen".into(),
            provider: "qwen".into(),
            agent: "Qwen Code".into(),
            inference_provider: "operator-configured".into(),
            model: "operator-configured".into(),
            effort: None,
            session_controls: None,
        }
    }
    #[test]
    fn explicit_selection_overrides_default_without_changing_launch_definition() {
        let catalog =
            ProfileCatalog::new(vec![profile()], Some("local-choice".into()), &registry()).unwrap();
        assert_eq!(catalog.select(None, None).unwrap(), "qwen");
        assert_eq!(catalog.select(None, Some("local-choice")).unwrap(), "qwen");
        assert_eq!(
            catalog.select(Some("other-provider"), None).unwrap(),
            "other-provider"
        );
        assert!(catalog.select(Some("qwen"), Some("local-choice")).is_err());
        assert!(catalog.select(None, Some("missing")).is_err());
        assert!(ProfileCatalog::default().select(None, None).is_err());
    }
    #[test]
    fn controls_are_explicit_bounded_and_bypassed_by_direct_provider_selection() {
        let mut configured = profile();
        configured.session_controls = Some(urdia_core::SessionControls {
            model: Some("model(openai)".into()),
            effort: Some("medium".into()),
        });
        let catalog = ProfileCatalog::new(
            vec![configured.clone()],
            Some("local-choice".into()),
            &registry(),
        )
        .unwrap();
        assert_eq!(
            catalog.controls(None, None).model.as_deref(),
            Some("model(openai)")
        );
        assert_eq!(
            catalog.controls(Some("qwen"), None),
            urdia_core::SessionControls::default()
        );
        assert_eq!(
            catalog.controls(None, Some("local-choice")),
            configured.session_controls.clone().unwrap()
        );
        configured.session_controls.as_mut().unwrap().effort = Some("PRIVATE_SENTINEL\n".into());
        let error = ProfileCatalog::new(vec![configured], None, &registry()).unwrap_err();
        assert!(!error.to_string().contains("PRIVATE_SENTINEL"));
    }
    #[test]
    fn invalid_catalogs_fail_closed_and_do_not_echo_source_values() {
        let registry = registry();
        let mut bad = profile();
        bad.provider = "secret-sentinel".into();
        let error = ProfileCatalog::new(vec![bad], None, &registry).unwrap_err();
        assert!(!error.to_string().contains("secret-sentinel"));
        assert!(ProfileCatalog::new(vec![profile(), profile()], None, &registry).is_err());
        let mut alias = profile();
        alias.id = "alias".into();
        assert!(ProfileCatalog::new(vec![profile(), alias], None, &registry).is_err());
        assert!(ProfileCatalog::new(vec![profile()], Some("missing".into()), &registry).is_err());
        for field in ["", "bad\nlabel"] {
            let mut bad = profile();
            bad.model = field.into();
            assert!(ProfileCatalog::new(vec![bad], None, &registry).is_err());
        }
        let mut bad = profile();
        bad.effort = Some("".into());
        assert!(ProfileCatalog::new(vec![bad], None, &registry).is_err());
        let mut bad = profile();
        bad.id = "Bad_ID".into();
        assert!(ProfileCatalog::new(vec![bad], None, &registry).is_err());
    }
}
