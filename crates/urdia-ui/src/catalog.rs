//! Safe catalog views and explicit opt-in configuration requests.
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize)]
pub struct CatalogProvider {
    pub id: String,
    pub name: String,
    pub executable_available: bool,
    pub pending_restart: bool,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileDraft {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub agent: String,
    pub inference_provider: String,
    pub model: String,
    pub effort: Option<String>,
    pub model_selector: Option<String>,
    pub effort_selector: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveProfileRequest {
    pub revision: u64,
    pub profile: ProfileDraft,
    #[serde(default)]
    pub make_default: bool,
}
#[derive(Serialize)]
pub struct CatalogSnapshot {
    pub editable: bool,
    pub restart_required: bool,
    pub revision: u64,
    pub default_profile: Option<String>,
    pub providers: Vec<CatalogProvider>,
    pub profiles: Vec<ProfileDraft>,
}
#[derive(Clone, Copy, Debug)]
pub enum CatalogError {
    Disabled,
    Invalid,
    Conflict,
    WriteFailed,
}

/// Explicit launch registration, without environment or credential fields.
/// Do not derive Debug: operators must keep secrets out of command arguments.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDraft {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterAgentRequest {
    pub revision: u64,
    pub agent: AgentDraft,
    pub confirm_launch: bool,
}
#[derive(Serialize)]
pub struct AgentCatalogSnapshot {
    pub editable: bool,
    pub revision: u64,
    pub restart_required: bool,
    pub agents: Vec<RegisteredAgentView>,
}
#[derive(Serialize)]
pub struct RegisteredAgentView {
    pub id: String,
    pub name: String,
    pub active: bool,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlProbeRequest {
    pub request_id: uuid::Uuid,
    pub provider: String,
    pub model: Option<String>,
    pub confirm_start: bool,
}
#[derive(Clone, Serialize)]
pub struct ControlProbeResult {
    pub request_id: uuid::Uuid,
    pub provider: String,
    pub requested_model: Option<String>,
    pub options: urdia_core::ControlOptions,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ControlProbeError {
    Invalid,
    Unavailable,
    Busy,
    Conflict,
    Limit,
    Failed,
    Timeout,
}
