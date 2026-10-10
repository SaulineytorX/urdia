//! Session-scoped, bounded model/effort selectors. Mode and permission controls are excluded.
use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOptions, SetSessionConfigOptionRequest,
};
use agent_client_protocol::{Agent, ConnectionTo};
use std::{collections::BTreeSet, time::Duration};
use urdia_core::{Result, SessionControlState, SessionControls, UrdiaError, valid_control_id};

struct Selector {
    id: String,
    current: String,
    values: BTreeSet<String>,
}
#[derive(Default)]
struct Selectors {
    model: Option<Selector>,
    effort: Option<Selector>,
}
pub(super) fn invalid() -> UrdiaError {
    UrdiaError::Protocol(
        "ACP model/effort controls are unavailable, malformed or unconfirmed".into(),
    )
}
fn discover(options: &[SessionConfigOption]) -> Result<Selectors> {
    if options.len() > 64 {
        return Err(invalid());
    }
    let mut result = Selectors::default();
    let mut ids = BTreeSet::new();
    for option in options {
        let id = option.id.to_string();
        if !valid_control_id(&id) || !ids.insert(id.clone()) {
            return Err(invalid());
        }
        let slot = match option.category {
            Some(SessionConfigOptionCategory::Model) => &mut result.model,
            Some(SessionConfigOptionCategory::ThoughtLevel) => &mut result.effort,
            _ => continue,
        };
        if slot.is_some() {
            return Err(invalid());
        }
        let SessionConfigKind::Select(select) = &option.kind else {
            return Err(invalid());
        };
        let entries: Vec<_> = match &select.options {
            SessionConfigSelectOptions::Ungrouped(options) => options.iter().collect(),
            SessionConfigSelectOptions::Grouped(groups) => {
                if groups.len() > 128
                    || groups
                        .iter()
                        .map(|group| group.options.len())
                        .sum::<usize>()
                        > 128
                {
                    return Err(invalid());
                }
                groups.iter().flat_map(|group| &group.options).collect()
            }
            _ => return Err(invalid()),
        };
        if entries.is_empty() || entries.len() > 128 {
            return Err(invalid());
        }
        let mut values = BTreeSet::new();
        for entry in entries {
            let value = entry.value.to_string();
            if !valid_control_id(&value) || !values.insert(value) {
                return Err(invalid());
            }
        }
        let current = select.current_value.to_string();
        if !values.contains(&current) {
            return Err(invalid());
        }
        *slot = Some(Selector {
            id,
            current,
            values,
        });
    }
    Ok(result)
}

fn option_view(selectors: &Selectors) -> urdia_core::ControlOptions {
    urdia_core::ControlOptions {
        models: selectors
            .model
            .as_ref()
            .map(|s| s.values.iter().cloned().collect())
            .unwrap_or_default(),
        efforts: selectors
            .effort
            .as_ref()
            .map(|s| s.values.iter().cloned().collect())
            .unwrap_or_default(),
        current: SessionControlState {
            model: selectors.model.as_ref().map(|s| s.current.clone()),
            effort: selectors.effort.as_ref().map(|s| s.current.clone()),
        },
    }
}
#[derive(Default)]
pub(super) struct Observation {
    session: Option<String>,
    startup: Option<SessionControlState>,
    current: Option<SessionControlState>,
    revision: u64,
    request: SessionControls,
    options: Option<urdia_core::ControlOptions>,
}
impl Observation {
    pub fn begin(&mut self, session: String, request: SessionControls) {
        self.session = Some(session);
        self.request = request;
    }
    pub fn update(&mut self, session: &str, options: &[SessionConfigOption]) {
        if self.session.as_deref() != Some(session) {
            return;
        }
        self.revision = self.revision.saturating_add(1);
        self.options = discover(options)
            .ok()
            .map(|selectors| option_view(&selectors));
        self.current = self.options.as_ref().map(|options| options.current.clone());
    }
    pub fn set_options(&mut self, options: Option<urdia_core::ControlOptions>) {
        if self.revision == 0 {
            self.options = options;
        }
    }
    pub fn options(&self) -> Option<urdia_core::ControlOptions> {
        self.options.clone()
    }
    pub fn finish(&mut self, startup: Option<SessionControlState>) -> Result<()> {
        if self.revision == 0 {
            self.current = startup.clone();
        }
        self.startup = startup;
        self.check()
    }
    pub fn check(&self) -> Result<()> {
        let matches = |desired: &Option<String>, actual: Option<&String>| {
            desired.as_ref().is_none_or(|value| Some(value) == actual)
        };
        if matches(
            &self.request.model,
            self.current.as_ref().and_then(|s| s.model.as_ref()),
        ) && matches(
            &self.request.effort,
            self.current.as_ref().and_then(|s| s.effort.as_ref()),
        ) {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    pub fn snapshot(&self) -> urdia_core::LiveSessionControls {
        urdia_core::LiveSessionControls {
            revision: self.revision,
            current: self.current.clone(),
            changed_since_startup: self.current != self.startup,
            explicit_configuration_matches: self.check().is_ok(),
        }
    }
}

pub(super) async fn configure(
    connection: &ConnectionTo<Agent>,
    session: &agent_client_protocol::schema::v1::SessionId,
    options: Option<Vec<SessionConfigOption>>,
    request: &SessionControls,
    timeout: Duration,
) -> Result<(
    Option<SessionControlState>,
    Option<urdia_core::ControlOptions>,
)> {
    request.validate()?;
    let Some(options) = options else {
        return if request.model.is_none() && request.effort.is_none() {
            Ok((None, None))
        } else {
            Err(invalid())
        };
    };
    let mut selectors = discover(&options)?;
    // The model may change the effort choices. Always validate the refreshed list.
    for (is_model, desired) in [(true, &request.model), (false, &request.effort)] {
        let Some(desired) = desired else {
            continue;
        };
        let selector = if is_model {
            &selectors.model
        } else {
            &selectors.effort
        };
        let selector = selector.as_ref().ok_or_else(invalid)?;
        if !selector.values.contains(desired) {
            return Err(invalid());
        }
        if selector.current != *desired {
            let response = tokio::time::timeout(
                timeout,
                connection
                    .send_request(SetSessionConfigOptionRequest::new(
                        session.clone(),
                        selector.id.clone(),
                        desired.as_str(),
                    ))
                    .block_task(),
            )
            .await
            .map_err(|_| invalid())?
            .map_err(|_| invalid())?;
            selectors = discover(&response.config_options)?;
        }
        let confirmed = if is_model {
            &selectors.model
        } else {
            &selectors.effort
        };
        if confirmed.as_ref().map(|value| &value.current) != Some(desired) {
            return Err(invalid());
        }
    }
    // Setting effort must not silently change a previously requested model.
    if request.model.as_ref().is_some_and(|desired| {
        selectors.model.as_ref().map(|value| &value.current) != Some(desired)
    }) {
        return Err(invalid());
    }
    let options = option_view(&selectors);
    Ok((Some(options.current.clone()), Some(options)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn options() -> serde_json::Value {
        json!([{"id":"model", "name":"PRIVATE_LABEL", "category":"model", "type":"select", "currentValue":"m(a)",
            "options":[{"value":"m(a)","name":"A"},{"value":"m(b)","name":"B"}], "_meta":{"secret":"PRIVATE_META"}}])
    }
    #[test]
    fn option_views_exclude_names_metadata_and_clear_malformed_updates() {
        let options: Vec<SessionConfigOption> = serde_json::from_value(options()).unwrap();
        let mut observation = Observation::default();
        observation.begin("session".into(), SessionControls::default());
        observation.update("session", &options);
        let view = observation.options().unwrap();
        assert_eq!(view.models, vec!["m(a)", "m(b)"]);
        assert!(view.efforts.is_empty());
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("PRIVATE"));
        let mut bad = options;
        bad.push(bad[0].clone());
        observation.update("session", &bad);
        assert!(observation.options().is_none());
    }
    #[test]
    fn discovery_rejects_duplicates_invalid_defaults_ambiguity_and_budgets() {
        let original = options();
        let mut cases = Vec::new();
        let mut bad = original.clone();
        bad[0]["currentValue"] = json!("unknown");
        cases.push(bad);
        let mut bad = original.clone();
        bad[0]["options"][1]["value"] = json!("m(a)");
        cases.push(bad);
        let mut bad = original.clone();
        bad[0]["id"] = json!("bad\nsecret");
        cases.push(bad);
        let mut bad = original.clone();
        let mut alias = bad[0].clone();
        alias["id"] = json!("alias");
        bad.as_array_mut().unwrap().push(alias);
        cases.push(bad);
        let mut bad = original.clone();
        bad[0]["options"] = json!(
            (0..129)
                .map(|i| json!({"value":format!("m{i}"),"name":"M"}))
                .collect::<Vec<_>>()
        );
        cases.push(bad);
        for case in cases {
            let options: Vec<SessionConfigOption> = serde_json::from_value(case).unwrap();
            let error = discover(&options)
                .err()
                .expect("must reject malformed controls");
            assert!(!error.to_string().contains("PRIVATE"));
        }
        let options: Vec<SessionConfigOption> = serde_json::from_value(original).unwrap();
        assert_eq!(discover(&options).unwrap().model.unwrap().current, "m(a)");
    }
    #[test]
    fn live_updates_replace_values_redact_invalid_reports_and_allow_exact_restoration() {
        let options: Vec<SessionConfigOption> = serde_json::from_value(options()).unwrap();
        let startup = SessionControlState {
            model: Some("m(a)".into()),
            effort: None,
        };
        let mut observation = Observation::default();
        observation.begin(
            "session".into(),
            SessionControls {
                model: startup.model.clone(),
                effort: None,
            },
        );
        observation.finish(Some(startup.clone())).unwrap();
        let mut malformed = options.clone();
        malformed.push(options[0].clone());
        observation.update("wrong", &malformed);
        assert_eq!(observation.snapshot().revision, 0);
        observation.update("session", &malformed);
        assert!(observation.snapshot().current.is_none());
        assert!(observation.check().is_err());
        assert!(
            !observation
                .check()
                .unwrap_err()
                .to_string()
                .contains("PRIVATE")
        );
        observation.update("session", &options);
        assert!(observation.check().is_ok());
        assert_eq!(observation.snapshot().current, Some(startup));
        assert_eq!(observation.snapshot().revision, 2);
        assert!(!observation.snapshot().changed_since_startup);
    }
    #[test]
    fn startup_does_not_overwrite_a_newer_notification() {
        let mut observation = Observation::default();
        observation.begin(
            "s".into(),
            SessionControls {
                model: Some("m(a)".into()),
                effort: None,
            },
        );
        observation.update("s", &[]);
        assert!(
            observation
                .finish(Some(SessionControlState {
                    model: Some("m(a)".into()),
                    effort: None
                }))
                .is_err()
        );
        assert!(observation.snapshot().current.unwrap().model.is_none());
    }

    #[test]
    fn permission_mode_and_uncategorized_controls_are_not_model_selectors() {
        for category in [
            json!("mode"),
            json!("model_config"),
            json!("_custom"),
            json!(null),
        ] {
            let mut options = options();
            options[0]["category"] = category;
            let options: Vec<SessionConfigOption> = serde_json::from_value(options).unwrap();
            let selectors = discover(&options).unwrap();
            assert!(selectors.model.is_none() && selectors.effort.is_none());
        }
    }
}
