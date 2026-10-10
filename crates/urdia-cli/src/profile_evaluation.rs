//! Fixture-only profile eligibility. This is not the live dispatch policy.
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use urdia_core::{Result, SessionControls, UrdiaError};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    schema: String,
    synthetic_fixture: bool,
    policy: Policy,
    profiles: BTreeMap<String, Profile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    revision_floor: u64,
    pinned_profile: Option<String>,
    default_profile: Option<String>,
    automatic_execution: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    declared_model: String,
    executable_available: bool,
    authentication_confirmed: bool,
    launch_digest: String,
    evidence_launch_digest: String,
    evidence_revision: u64,
    requested_controls: SessionControls,
    evidence_controls: SessionControls,
    advertised_models: Vec<String>,
    advertised_efforts: Vec<String>,
    task_evidence: Vec<String>,
    measured_latency_ms: Option<u64>,
    measured_cost_microunits: Option<u64>,
}
fn invalid() -> UrdiaError {
    UrdiaError::InvalidInput("profile evaluation state violates the fixture contract".into())
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
/// Host eligibility excludes availability/auth/control/freshness failures and obeys pins.
/// Task fitness and preference among eligible profiles remain semantic judgments.
pub(super) fn eligible(state: &Value) -> Result<BTreeSet<String>> {
    let state: State = serde_json::from_value(state.clone()).map_err(|_| invalid())?;
    if state.schema != "urdia-profile-evaluation-state-v1"
        || !state.synthetic_fixture
        || state.policy.automatic_execution
        || state.policy.revision_floor == 0
        || state.profiles.is_empty()
        || state.profiles.len() > 8
        || state
            .policy
            .pinned_profile
            .iter()
            .chain(state.policy.default_profile.iter())
            .any(|id| !state.profiles.contains_key(id))
    {
        return Err(invalid());
    }
    let mut eligible = BTreeSet::new();
    for (id, profile) in state.profiles {
        if profile.declared_model.is_empty()
            || profile.declared_model.len() > 128
            || !digest(&profile.launch_digest)
            || !digest(&profile.evidence_launch_digest)
            || profile.advertised_models.len() > 128
            || profile.advertised_efforts.len() > 128
            || profile.task_evidence.len() > 16
            || profile
                .task_evidence
                .iter()
                .any(|value| value.is_empty() || value.len() > 256)
            || profile
                .advertised_models
                .iter()
                .chain(&profile.advertised_efforts)
                .any(|value| !urdia_core::valid_control_id(value))
        {
            return Err(invalid());
        }
        profile
            .requested_controls
            .validate()
            .map_err(|_| invalid())?;
        let confirmed = profile
            .requested_controls
            .model
            .as_ref()
            .is_some_and(|model| profile.advertised_models.contains(model))
            && profile
                .requested_controls
                .effort
                .as_ref()
                .is_none_or(|effort| profile.advertised_efforts.contains(effort));
        // Measurements do not confer eligibility or authorization, even if present.
        let _measurements = (
            profile.measured_latency_ms,
            profile.measured_cost_microunits,
        );
        if profile.executable_available
            && profile.authentication_confirmed
            && confirmed
            && profile.evidence_revision >= state.policy.revision_floor
            && profile.launch_digest == profile.evidence_launch_digest
            && profile.requested_controls == profile.evidence_controls
            && !profile.task_evidence.is_empty()
            && state
                .policy
                .pinned_profile
                .as_ref()
                .is_none_or(|pinned| pinned == &id)
        {
            eligible.insert(id);
        }
    }
    Ok(eligible)
}
