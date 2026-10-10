//! Core contracts shared by Urdia providers, stores, and clients.
//!
//! The core deliberately knows nothing about a model vendor or a transport. Agent
//! execution, permission decisions, semantic routing, and event persistence are
//! separate interfaces so an optional decision model can never grant an action.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

mod provider_failure;
pub use provider_failure::ProviderFailureKind;

/// The error type shared by Urdia boundaries.
#[derive(Debug, Error)]
pub enum UrdiaError {
    /// A workspace could not be canonicalized or was not absolute.
    #[error("invalid workspace {path}: {reason}")]
    InvalidWorkspace { path: PathBuf, reason: String },
    /// A provider failed while starting or using an agent session.
    #[error("provider error: {0}")]
    Provider(String),
    /// An allowlisted diagnosis with no retained provider message or payload.
    #[error("{0}")]
    ProviderFailure(ProviderFailureKind),
    /// Provider registration or selection failed.
    #[error(transparent)]
    ProviderRegistry(#[from] ProviderRegistryError),
    /// The requested operation was refused by the permission broker.
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    /// A store failed to persist or read an event.
    #[error("store error: {0}")]
    Store(String),
    /// A local user interface failed to bind or serve.
    #[error("UI error: {0}")]
    Ui(String),
    /// A protocol message or state transition was invalid.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// A generic invalid input error.
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

/// A result using the Urdia error type.
pub type Result<T> = std::result::Result<T, UrdiaError>;

/// A canonical absolute path used as an agent workspace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct WorkspacePath(PathBuf);

impl WorkspacePath {
    /// Canonicalize an existing directory and reject non-absolute paths.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let input = path.as_ref();
        let canonical =
            std::fs::canonicalize(input).map_err(|error| UrdiaError::InvalidWorkspace {
                path: input.to_path_buf(),
                reason: error.to_string(),
            })?;
        if !canonical.is_absolute() {
            return Err(UrdiaError::InvalidWorkspace {
                path: input.to_path_buf(),
                reason: "canonical path is not absolute".to_owned(),
            });
        }
        if !canonical.is_dir() {
            return Err(UrdiaError::InvalidWorkspace {
                path: canonical,
                reason: "workspace is not a directory".to_owned(),
            });
        }
        Ok(Self(canonical))
    }

    /// Return the canonical path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Borrow the path as a string when serializing command metadata.
    #[must_use]
    pub fn display(&self) -> String {
        self.0.display().to_string()
    }
}

impl AsRef<Path> for WorkspacePath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl<'de> Deserialize<'de> for WorkspacePath {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let path = PathBuf::deserialize(deserializer)?;
        Self::new(path).map_err(serde::de::Error::custom)
    }
}

/// A stable Urdia session identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(Uuid);

impl SessionId {
    /// Generate a new session identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for SessionId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for SessionId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

/// Explicit opaque model/effort selections, never permission or mode controls.
#[derive(Clone, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SessionControls {
    pub model: Option<String>,
    pub effort: Option<String>,
}

impl SessionControls {
    /// Validate bounded opaque identifiers without repeating source values.
    pub fn validate(&self) -> Result<()> {
        if self
            .model
            .iter()
            .chain(self.effort.iter())
            .any(|value| !valid_control_id(value))
        {
            return Err(UrdiaError::InvalidInput(
                "invalid session control identifier".into(),
            ));
        }
        Ok(())
    }
}

impl std::fmt::Debug for SessionControls {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionControls { values omitted }")
    }
}

/// Bound the opaque identifiers used by ACP selectors.
#[must_use]
pub fn valid_control_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && value.bytes().all(|byte| (32..=126).contains(&byte))
}

/// Agent-reported startup settings, not proof of remote inference configuration.
#[derive(Clone, Default, Serialize, Eq, PartialEq)]
pub struct SessionControlState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

/// Bounded opaque selectors reported by an agent, not account entitlements.
#[derive(Clone, Serialize, Eq, PartialEq)]
pub struct ControlOptions {
    pub models: Vec<String>,
    pub efforts: Vec<String>,
    pub current: SessionControlState,
}

/// Last bounded, session-scoped agent report. Does not attest remote inference.
#[derive(Clone, Serialize, Eq, PartialEq)]
pub struct LiveSessionControls {
    pub revision: u64,
    pub current: Option<SessionControlState>,
    pub changed_since_startup: bool,
    pub explicit_configuration_matches: bool,
}
impl std::fmt::Debug for LiveSessionControls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LiveSessionControls { values omitted }")
    }
}

impl std::fmt::Debug for SessionControlState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionControlState { values omitted }")
    }
}

/// Parameters and host-owned permission services for a new agent session.
#[derive(Clone)]
pub struct SessionRequest {
    /// Canonical workspace available to the agent.
    pub workspace: WorkspacePath,
    pub controls: SessionControls,
    permission_broker: Arc<dyn PermissionBroker>,
    permission_receipt_sink: Arc<dyn PermissionReceiptSink>,
}

impl SessionRequest {
    /// Create a request with deny-all permissions and a sink that refuses
    /// allow receipts until the host installs durable persistence.
    #[must_use]
    pub fn new(workspace: WorkspacePath) -> Self {
        Self {
            workspace,
            controls: SessionControls::default(),
            permission_broker: Arc::new(DenyAllPermissionBroker),
            permission_receipt_sink: Arc::new(DiscardPermissionReceiptSink),
        }
    }

    /// Request explicit startup configuration; ACP validates actual advertised values.
    #[must_use]
    pub fn controls(mut self, controls: SessionControls) -> Self {
        self.controls = controls;
        self
    }

    /// Configure the host-owned one-shot permission broker.
    #[must_use]
    pub fn permission_broker(mut self, broker: Arc<dyn PermissionBroker>) -> Self {
        self.permission_broker = broker;
        self
    }

    /// Configure the sink that must durably record permission receipts.
    #[must_use]
    pub fn permission_receipt_sink(mut self, sink: Arc<dyn PermissionReceiptSink>) -> Self {
        self.permission_receipt_sink = sink;
        self
    }

    /// Borrow the configured permission broker.
    #[must_use]
    pub fn broker(&self) -> &Arc<dyn PermissionBroker> {
        &self.permission_broker
    }

    /// Borrow the configured receipt sink.
    #[must_use]
    pub fn receipt_sink(&self) -> &Arc<dyn PermissionReceiptSink> {
        &self.permission_receipt_sink
    }
}

impl std::fmt::Debug for SessionRequest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionRequest")
            .field("workspace", &self.workspace)
            .field("permission_broker", &"<configured>")
            .field("permission_receipt_sink", &"<configured>")
            .finish()
    }
}

/// A user prompt sent to an agent session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Prompt {
    /// Text to send to the agent.
    pub text: String,
}

impl Prompt {
    /// Create a prompt from text.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// A completed prompt turn.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PromptResult {
    /// Text emitted by the agent during this turn.
    pub text: String,
    /// Provider-specific stop reason, when available.
    pub stop_reason: Option<String>,
}

/// Redacted, informational tool category reported by an agent.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    SwitchMode,
    Other,
}
/// Agent-reported status, without independent execution verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolStatus {
    Unknown,
    Pending,
    InProgress,
    Completed,
    Failed,
}
impl AgentToolStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
    pub fn rank(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Pending => 1,
            Self::InProgress => 2,
            Self::Completed | Self::Failed => 3,
        }
    }
}
/// Only an ordinal, category and status cross the ACP telemetry boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AgentToolActivity {
    pub ordinal: u16,
    pub kind: AgentToolKind,
    pub status: AgentToolStatus,
}
pub const MAX_TOOL_ACTIVITIES: usize = 128;

/// A provider-neutral update emitted while an agent is processing a prompt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentUpdate {
    /// A contiguous piece of text emitted by the agent.
    TextDelta {
        text: String,
    },
    ToolActivity(AgentToolActivity),
    ToolActivityLimitReached,
}

/// A synchronous, non-blocking destination for live agent updates.
pub trait AgentUpdateSink: Send + Sync {
    /// Publish one update from the active prompt turn.
    fn publish(&self, update: AgentUpdate);
}

/// Whether a provider executable or endpoint can currently be used.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAvailability {
    /// The provider's required executable or endpoint was found.
    Available,
    /// The provider's required executable or endpoint was not found.
    Unavailable,
}

impl ProviderAvailability {
    /// Return whether the provider is available for a new session.
    #[must_use]
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }
}

/// Authentication state reported by a provider adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationState {
    /// The adapter verified that credentials are configured.
    Configured,
    /// The adapter verified that credentials are missing.
    Missing,
    /// The adapter cannot determine authentication state safely.
    Unknown,
}

/// Where a provider performs model inference.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceLocation {
    /// Inference is performed by locally hosted model weights.
    Local,
    /// Inference is performed by a remote service.
    Remote,
    /// The provider can use both local and remote inference.
    Mixed,
    /// The adapter cannot determine the inference location safely.
    Unknown,
}

/// Provider capability metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Stable provider identifier, such as `qwen`.
    pub id: String,
    /// Human-readable provider name.
    pub name: String,
    /// Supported protocol or transport labels.
    pub protocols: Vec<String>,
    /// Host-visible capabilities supplied by this adapter.
    pub capabilities: Vec<String>,
    /// Whether the provider can currently be started.
    pub availability: ProviderAvailability,
    /// A safe diagnostic for the current availability state.
    pub availability_detail: Option<String>,
    /// Authentication state, when the adapter can establish it safely.
    pub authentication: AuthenticationState,
    /// Inference location, when the adapter can establish it safely.
    pub inference: InferenceLocation,
}

impl ProviderInfo {
    /// Build conservative provider metadata with unknown auth and inference.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, protocols: Vec<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            protocols,
            capabilities: Vec::new(),
            availability: ProviderAvailability::Unavailable,
            availability_detail: None,
            authentication: AuthenticationState::Unknown,
            inference: InferenceLocation::Unknown,
        }
    }
}

/// Errors returned while registering or resolving an agent provider.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum ProviderRegistryError {
    /// A provider with the same stable identifier is already registered.
    #[error("duplicate provider ID `{id}`")]
    DuplicateProvider { id: String },
    /// No provider is registered under the requested identifier.
    #[error("unknown provider `{id}`")]
    UnknownProvider { id: String },
    /// The provider is registered but its required executable or endpoint is unavailable.
    #[error("provider `{id}` is unavailable: {detail}")]
    UnavailableProvider { id: String, detail: String },
    /// A provider identifier is not lowercase ASCII kebab case.
    #[error("invalid provider ID `{id}`; expected lowercase ASCII kebab case")]
    InvalidProviderId { id: String },
}

/// Backwards-friendly short name for provider registry errors.
pub type ProviderError = ProviderRegistryError;

/// A host-owned deterministic registry of agent providers.
#[derive(Clone, Default)]
pub struct ProviderRegistry {
    providers: BTreeMap<String, Arc<dyn AgentProvider>>,
}

impl std::fmt::Debug for ProviderRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderRegistry")
            .field("provider_ids", &self.providers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ProviderRegistry {
    /// Create an empty provider registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one provider under the stable ID returned by `info`.
    ///
    /// Registration validates IDs before mutating the registry and never
    /// replaces an existing provider.
    pub fn register(
        &mut self,
        provider: Arc<dyn AgentProvider>,
    ) -> std::result::Result<(), ProviderRegistryError> {
        let id = provider.info().id;
        if !is_valid_provider_id(&id) {
            return Err(ProviderRegistryError::InvalidProviderId { id });
        }
        if self.providers.contains_key(&id) {
            return Err(ProviderRegistryError::DuplicateProvider { id });
        }
        self.providers.insert(id, provider);
        Ok(())
    }

    /// Register an owned concrete provider by wrapping it in an `Arc`.
    pub fn register_owned<P>(
        &mut self,
        provider: P,
    ) -> std::result::Result<(), ProviderRegistryError>
    where
        P: AgentProvider + 'static,
    {
        self.register(Arc::new(provider))
    }

    /// Return registered provider metadata in stable ID order.
    #[must_use]
    pub fn list(&self) -> Vec<ProviderInfo> {
        self.providers
            .values()
            .map(|provider| provider.info())
            .collect()
    }

    /// Resolve a provider without requiring it to be available.
    pub fn resolve(
        &self,
        id: &str,
    ) -> std::result::Result<Arc<dyn AgentProvider>, ProviderRegistryError> {
        self.providers
            .get(id)
            .cloned()
            .ok_or_else(|| ProviderRegistryError::UnknownProvider { id: id.to_owned() })
    }

    /// Resolve a provider only when its metadata reports it as available.
    pub fn resolve_available(
        &self,
        id: &str,
    ) -> std::result::Result<Arc<dyn AgentProvider>, ProviderRegistryError> {
        let provider = self.resolve(id)?;
        let info = provider.info();
        if info.availability.is_available() {
            return Ok(provider);
        }
        Err(ProviderRegistryError::UnavailableProvider {
            id: info.id,
            detail: info
                .availability_detail
                .unwrap_or_else(|| "availability was reported as unavailable".to_owned()),
        })
    }

    /// Return whether an ID has a registered provider.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.providers.contains_key(id)
    }

    /// Return the number of registered providers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Return whether no providers are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

/// Validate a provider ID as lowercase ASCII kebab case.
#[must_use]
pub fn is_valid_provider_id(id: &str) -> bool {
    if id.is_empty() || !id.is_ascii() || id.starts_with('-') || id.ends_with('-') {
        return false;
    }
    let mut previous_was_separator = false;
    for character in id.bytes() {
        if character == b'-' {
            if previous_was_separator {
                return false;
            }
            previous_was_separator = true;
        } else if character.is_ascii_lowercase() || character.is_ascii_digit() {
            previous_was_separator = false;
        } else {
            return false;
        }
    }
    true
}

/// A provider that can create independent agent sessions.
#[async_trait]
pub trait AgentProvider: Send + Sync {
    /// Return provider metadata without starting a child process.
    fn info(&self) -> ProviderInfo;

    /// Start a session in the supplied workspace.
    async fn start_session(&self, request: SessionRequest) -> Result<Box<dyn AgentSession>>;
}

/// A running conversation with one agent provider.
#[async_trait]
pub trait AgentSession: Send + Sync {
    /// Return the Urdia session identifier.
    fn id(&self) -> &SessionId;

    /// Return settings reported at startup, if supported. Not a live guarantee.
    fn startup_controls(&self) -> Option<SessionControlState> {
        None
    }

    /// Latest agent-reported selectors, without remote attestation.
    fn live_controls(&self) -> Option<LiveSessionControls> {
        None
    }

    /// Last validated model/effort options; excludes modes and private metadata.
    fn control_options(&self) -> Option<ControlOptions> {
        None
    }

    /// Submit a prompt and await its completed turn.
    async fn prompt(&self, prompt: Prompt) -> Result<PromptResult>;

    /// Submit a prompt and publish provider updates while it is running.
    ///
    /// Implementations that do not expose streaming updates retain the
    /// established prompt behavior through this default method.
    async fn prompt_with_updates(
        &self,
        prompt: Prompt,
        _sink: Arc<dyn AgentUpdateSink>,
    ) -> Result<PromptResult> {
        self.prompt(prompt).await
    }

    /// Ask the provider to cancel active work.
    async fn cancel(&self) -> Result<()>;

    /// Remove and return redacted permission receipts accumulated by this session.
    fn take_permission_receipts(&self) -> Vec<PermissionReceipt> {
        Vec::new()
    }
}

/// A value that authorization code may inspect but must not serialize or log.
#[derive(Clone, PartialEq)]
pub struct SensitiveValue(Value);

impl SensitiveValue {
    #[must_use]
    pub const fn new(value: Value) -> Self {
        Self(value)
    }

    /// Explicitly expose the value to an in-memory permission broker.
    #[must_use]
    pub const fn expose(&self) -> &Value {
        &self.0
    }
}

impl std::fmt::Debug for SensitiveValue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SensitiveValue(<redacted>)")
    }
}

/// Transport-neutral category for an agent tool operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOperationKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    SwitchMode,
    Other,
}

/// A location supplied by an agent as part of a permission request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionLocation {
    pub path: PathBuf,
    pub line: Option<u32>,
}

/// The exact structured operation an agent asks the host to authorize.
#[derive(Clone, Debug, PartialEq)]
pub struct PermissionOperation {
    pub tool_call_id: String,
    pub title: Option<String>,
    pub name: Option<String>,
    pub kind: PermissionOperationKind,
    pub arguments: Option<SensitiveValue>,
    pub locations: Vec<PermissionLocation>,
}

/// A request routed to the permission broker.
///
/// This type deliberately does not implement `Serialize`: it contains the raw
/// arguments needed to make a narrow decision. Persist a `PermissionReceipt`
/// instead.
#[derive(Clone, Debug, PartialEq)]
pub struct PermissionRequest {
    pub id: Uuid,
    pub session_id: SessionId,
    pub provider_session_id: String,
    pub workspace: WorkspacePath,
    pub revision: u64,
    pub operation: PermissionOperation,
}

/// Opaque host-owned provenance for an allow granted by a trusted-session
/// policy.
///
/// The policy identifier is intentionally not serializable or included in the
/// debug representation.  ACP uses it only to bind a decision to the broker
/// that issued it and stores a one-way fingerprint in the receipt.
#[derive(Clone, Eq, PartialEq)]
pub struct TrustedPermissionGrant {
    policy_id: Uuid,
    policy_revision: u64,
}

impl TrustedPermissionGrant {
    /// Construct a trusted-session grant owned by the host permission broker.
    #[must_use]
    pub const fn new(policy_id: Uuid, policy_revision: u64) -> Self {
        Self {
            policy_id,
            policy_revision,
        }
    }

    /// Return the opaque policy identifier for in-memory validation.
    #[must_use]
    pub const fn policy_id(&self) -> Uuid {
        self.policy_id
    }

    /// Return the monotonically increasing policy generation.
    #[must_use]
    pub const fn policy_revision(&self) -> u64 {
        self.policy_revision
    }
}

impl std::fmt::Debug for TrustedPermissionGrant {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TrustedPermissionGrant")
            .field("policy_id", &"<opaque>")
            .field("policy_revision", &self.policy_revision)
            .finish()
    }
}

/// The host's answer to a permission request.
#[derive(Clone, Eq, PartialEq)]
pub enum PermissionDecision {
    /// Allow exactly this request revision once.
    AllowOnce,
    /// Refuse exactly this request revision once.
    RejectOnce,
    /// Allow exactly this request once under an already activated,
    /// host-validated trusted-session policy.
    AllowTrusted {
        /// Opaque identifier of the activated policy.
        policy_id: Uuid,
        /// Monotonically increasing policy generation.
        policy_revision: u64,
    },
}

impl std::fmt::Debug for PermissionDecision {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AllowOnce => formatter.write_str("AllowOnce"),
            Self::RejectOnce => formatter.write_str("RejectOnce"),
            Self::AllowTrusted {
                policy_revision, ..
            } => formatter
                .debug_struct("AllowTrusted")
                .field("policy_id", &"<opaque>")
                .field("policy_revision", policy_revision)
                .finish(),
        }
    }
}

impl PermissionDecision {
    /// Construct a trusted-session allow from a host-owned policy generation.
    #[must_use]
    pub const fn allow_trusted(policy_id: Uuid, policy_revision: u64) -> Self {
        Self::AllowTrusted {
            policy_id,
            policy_revision,
        }
    }

    /// Return the trusted authorization metadata, if this is a trusted allow.
    #[must_use]
    pub fn trusted_grant(&self) -> Option<TrustedPermissionGrant> {
        match self {
            Self::AllowTrusted {
                policy_id,
                policy_revision,
            } => Some(TrustedPermissionGrant::new(*policy_id, *policy_revision)),
            Self::AllowOnce | Self::RejectOnce => None,
        }
    }
}

/// Browser-facing one-shot decisions remain the only serializable decision
/// values.  Trusted provenance must be created by a host broker in memory;
/// accepting it from JSON would let a browser forge an authorization.
impl Serialize for PermissionDecision {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::AllowOnce => serializer.serialize_str("allow_once"),
            Self::RejectOnce => serializer.serialize_str("reject_once"),
            Self::AllowTrusted { .. } => Err(serde::ser::Error::custom(
                "trusted permission decisions are host-only",
            )),
        }
    }
}

impl<'de> Deserialize<'de> for PermissionDecision {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "allow_once" => Ok(Self::AllowOnce),
            "reject_once" => Ok(Self::RejectOnce),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["allow_once", "reject_once"],
            )),
        }
    }
}

/// Outcome persisted for an individual permission request.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionReceiptOutcome {
    AllowedOnce,
    RejectedOnce,
    Cancelled,
}

/// Safe reason for a cancelled permission exchange.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionCancellationReason {
    SessionMismatch,
    NoActivePrompt,
    StaleRevision,
    BrokerTimeout,
    BrokerError,
    TrustedAuthorizationInvalid,
    CompatibleOptionMissing,
}

/// Provenance of a permission authorization recorded in the audit receipt.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAuthorizationSource {
    #[default]
    OneShot,
    TrustedSession,
}

/// Redacted, durable record of one permission decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PermissionReceipt {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub session_id: SessionId,
    pub revision: u64,
    pub provider_session_fingerprint: String,
    pub workspace_fingerprint: String,
    pub operation_fingerprint: String,
    pub tool_call_fingerprint: String,
    pub kind: PermissionOperationKind,
    pub arguments_present: bool,
    pub location_count: usize,
    #[serde(default)]
    pub authorization: PermissionAuthorizationSource,
    pub policy_id_fingerprint: Option<String>,
    pub policy_revision: Option<u64>,
    pub outcome: PermissionReceiptOutcome,
    pub cancellation_reason: Option<PermissionCancellationReason>,
}

impl PermissionReceipt {
    /// Create a redacted receipt bound to the complete in-memory request.
    #[must_use]
    pub fn from_request(
        request: &PermissionRequest,
        outcome: PermissionReceiptOutcome,
        cancellation_reason: Option<PermissionCancellationReason>,
    ) -> Self {
        Self::from_request_with_provenance(
            request,
            outcome,
            cancellation_reason,
            PermissionAuthorizationSource::OneShot,
            None,
            None,
        )
    }

    /// Create a redacted receipt for an operation authorized by a trusted
    /// session.  Only a one-way policy fingerprint leaves the host process.
    #[must_use]
    pub fn from_trusted_request(
        request: &PermissionRequest,
        outcome: PermissionReceiptOutcome,
        cancellation_reason: Option<PermissionCancellationReason>,
        grant: &TrustedPermissionGrant,
    ) -> Self {
        Self::from_request_with_provenance(
            request,
            outcome,
            cancellation_reason,
            PermissionAuthorizationSource::TrustedSession,
            Some(fingerprint(grant.policy_id().as_bytes())),
            Some(grant.policy_revision()),
        )
    }

    fn from_request_with_provenance(
        request: &PermissionRequest,
        outcome: PermissionReceiptOutcome,
        cancellation_reason: Option<PermissionCancellationReason>,
        authorization: PermissionAuthorizationSource,
        policy_id_fingerprint: Option<String>,
        policy_revision: Option<u64>,
    ) -> Self {
        Self {
            schema_version: 2,
            request_id: request.id,
            session_id: request.session_id.clone(),
            revision: request.revision,
            provider_session_fingerprint: fingerprint(request.provider_session_id.as_bytes()),
            workspace_fingerprint: fingerprint(
                request.workspace.as_path().as_os_str().as_encoded_bytes(),
            ),
            operation_fingerprint: permission_operation_fingerprint(&request.operation),
            tool_call_fingerprint: fingerprint(request.operation.tool_call_id.as_bytes()),
            kind: request.operation.kind,
            arguments_present: request.operation.arguments.is_some(),
            location_count: request.operation.locations.len(),
            authorization,
            policy_id_fingerprint,
            policy_revision,
            outcome,
            cancellation_reason,
        }
    }
}

fn fingerprint(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn permission_operation_fingerprint(operation: &PermissionOperation) -> String {
    let mut hasher = Sha256::new();
    hash_part(&mut hasher, operation.tool_call_id.as_bytes());
    hash_part(
        &mut hasher,
        operation.title.as_deref().unwrap_or_default().as_bytes(),
    );
    hash_part(
        &mut hasher,
        operation.name.as_deref().unwrap_or_default().as_bytes(),
    );
    hash_part(&mut hasher, format!("{:?}", operation.kind).as_bytes());
    if let Some(arguments) = &operation.arguments {
        hash_part(
            &mut hasher,
            &serde_json::to_vec(arguments.expose()).unwrap_or_default(),
        );
    }
    for location in &operation.locations {
        hash_part(&mut hasher, location.path.as_os_str().as_encoded_bytes());
        hash_part(
            &mut hasher,
            &location
                .line
                .map_or_else(Vec::new, |line| line.to_le_bytes().to_vec()),
        );
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hash_part(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(bytes.len().to_le_bytes());
    hasher.update(bytes);
}

/// Policy boundary for all agent actions.
#[async_trait]
pub trait PermissionBroker: Send + Sync {
    /// Decide whether the host permits the operation.
    async fn authorize(&self, request: PermissionRequest) -> Result<PermissionDecision>;

    /// Revalidate a trusted-session allow immediately before ACP releases the
    /// response.  A broker can use this hook for expiry, revocation, scope,
    /// and generation checks.  One-shot decisions never call this method.
    async fn revalidate_trusted(
        &self,
        _request: &PermissionRequest,
        _grant: &TrustedPermissionGrant,
    ) -> Result<()> {
        Ok(())
    }
}

/// Durable boundary for redacted permission receipts.
#[async_trait]
pub trait PermissionReceiptSink: Send + Sync {
    /// Persist one receipt before an allow-once response is released.
    async fn record(&self, receipt: PermissionReceipt) -> Result<()>;
}

/// Default sink that permits discarded denials but refuses discarded allows.
#[derive(Clone, Debug, Default)]
pub struct DiscardPermissionReceiptSink;

#[async_trait]
impl PermissionReceiptSink for DiscardPermissionReceiptSink {
    async fn record(&self, receipt: PermissionReceipt) -> Result<()> {
        if receipt.outcome == PermissionReceiptOutcome::AllowedOnce {
            return Err(UrdiaError::Store(
                "durable permission receipt sink is not configured".to_owned(),
            ));
        }
        Ok(())
    }
}

/// A safe default policy that refuses every action.
#[derive(Clone, Debug, Default)]
pub struct DenyAllPermissionBroker;

#[async_trait]
impl PermissionBroker for DenyAllPermissionBroker {
    async fn authorize(&self, _request: PermissionRequest) -> Result<PermissionDecision> {
        Ok(PermissionDecision::RejectOnce)
    }
}

/// The reserved escalation ID is intentionally kept in this crate so every
/// decision provider uses the same sentinel when a host-prepared candidate
/// set is insufficient.
pub const RESERVED_ESCALATION_ID: &str = "__urdia_escalate__";

/// Backwards-friendly name for [`RESERVED_ESCALATION_ID`].
pub const ESCALATION_CANDIDATE_ID: &str = RESERVED_ESCALATION_ID;

/// Input for a semantic routing or ranking decision.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionInput {
    /// User intent or task description.
    pub objective: String,
    /// Candidate provider or handler IDs.
    pub candidates: Vec<String>,
    /// Structured context supplied to the decision provider.
    pub state: Value,
}

/// A typed, non-authorizing semantic decision.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Decision {
    /// Selected candidate, or `None` when no candidate fits.
    pub selected: Option<String>,
    /// Provider-reported probabilities keyed by candidate ID.
    pub probabilities: BTreeMap<String, f32>,
    /// Provider-reported confidence when that provider defines one.
    pub confidence: Option<f32>,
    /// Meaning of the probability values. Consumers must not compare unlike
    /// semantics with one shared threshold.
    pub probability_semantics: ProbabilitySemantics,
}

/// Meaning attached to probabilities returned by a decision provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProbabilitySemantics {
    /// The provider documents a calibration scope for these probabilities.
    Calibrated,
    /// Probabilities are normalized over constrained candidate tokens.
    ConstrainedTokenDistribution,
    /// Values come from raw or transformed logits without calibration claims.
    Logits,
    /// The provider does not define the statistical meaning.
    Unknown,
}

/// Optional semantic intelligence used for routing, ranking, or verification.
///
/// Implementations may use Jev, Laya, TypeLLM, a local model, or deterministic
/// rules. The result is data only; permission authorization remains a separate
/// `PermissionBroker` call.
#[async_trait]
pub trait DecisionProvider: Send + Sync {
    /// Stable decision provider identifier.
    fn id(&self) -> &str;

    /// Evaluate a bounded typed decision.
    async fn decide(&self, input: DecisionInput) -> Result<Decision>;
}

/// The kind of event persisted by an event store.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EventKind {
    /// A session was opened.
    SessionStarted,
    /// A prompt was submitted.
    PromptSubmitted,
    /// Agent text was received.
    AgentText,
    /// A permission request was denied or allowed.
    Permission,
    /// A session was closed.
    SessionFinished,
    /// An error occurred during a session.
    Error,
}

/// An append-only event emitted by the host.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    /// Event schema version for durable replay and future migrations.
    pub schema_version: u16,
    /// Unique event identifier.
    pub id: Uuid,
    /// Session that produced the event.
    pub session_id: SessionId,
    /// Event classification.
    pub kind: EventKind,
    /// Milliseconds since the Unix epoch assigned by the host.
    pub occurred_at_ms: i64,
    /// Structured event payload. Callers must not put credentials here.
    pub data: Value,
}

impl Event {
    /// Build an event with a fresh identifier.
    #[must_use]
    pub fn new(session_id: SessionId, kind: EventKind, data: Value) -> Self {
        let occurred_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
            });
        Self {
            schema_version: 1,
            id: Uuid::new_v4(),
            session_id,
            kind,
            occurred_at_ms,
            data,
        }
    }
}

/// An event with a store-assigned sequence number.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredEvent {
    /// Monotonic sequence assigned by the store.
    pub sequence: i64,
    /// Original event payload.
    pub event: Event,
}

/// Summary of one durable session for timeline and history views.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredSession {
    /// Session identifier shared by every event in the session.
    pub session_id: SessionId,
    /// First store sequence assigned to the session.
    pub first_sequence: i64,
    /// Most recent store sequence assigned to the session.
    pub last_sequence: i64,
    /// Number of events currently stored for the session.
    pub event_count: u64,
    /// Timestamp of the first stored event.
    pub started_at_ms: i64,
    /// Timestamp of the most recent stored event.
    pub updated_at_ms: i64,
    /// Stored metadata from `SessionStarted`, or `null` when absent. Consumers
    /// must project an explicit allowlist before presenting it.
    pub session_data: Value,
}

/// Durable append-only event storage.
#[async_trait]
pub trait EventStore: Send + Sync {
    /// Append one event.
    async fn append(&self, event: Event) -> Result<()>;

    /// Read events for one session in insertion order.
    async fn list(&self, session_id: &SessionId) -> Result<Vec<StoredEvent>>;

    /// Read recent sessions in descending update order.
    async fn list_sessions(&self, limit: usize) -> Result<Vec<StoredSession>>;
}

/// A deterministic decision provider useful for contract tests and offline demos.
#[derive(Clone, Debug)]
pub struct FirstCandidateDecisionProvider {
    id: String,
}

impl FirstCandidateDecisionProvider {
    /// Create a deterministic provider with a stable ID.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

#[async_trait]
impl DecisionProvider for FirstCandidateDecisionProvider {
    fn id(&self) -> &str {
        &self.id
    }

    async fn decide(&self, input: DecisionInput) -> Result<Decision> {
        let selected = input.candidates.first().cloned();
        let mut probabilities = BTreeMap::new();
        for candidate in &input.candidates {
            probabilities.insert(
                candidate.clone(),
                if selected.as_deref() == Some(candidate.as_str()) {
                    1.0
                } else {
                    0.0
                },
            );
        }
        Ok(Decision {
            selected,
            probabilities,
            confidence: Some(1.0),
            probability_semantics: ProbabilitySemantics::Unknown,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct StubProvider {
        info: ProviderInfo,
    }

    #[async_trait]
    impl AgentProvider for StubProvider {
        fn info(&self) -> ProviderInfo {
            self.info.clone()
        }

        async fn start_session(&self, _request: SessionRequest) -> Result<Box<dyn AgentSession>> {
            Err(UrdiaError::Provider("stub provider".to_owned()))
        }
    }

    fn stub_provider(id: &str, availability: ProviderAvailability) -> Arc<dyn AgentProvider> {
        let mut info = ProviderInfo::new(id, id, vec!["test".to_owned()]);
        info.availability = availability;
        info.availability_detail = Some(format!("{id} detail"));
        Arc::new(StubProvider { info })
    }

    #[test]
    fn deserialized_workspace_is_validated() {
        let missing = std::env::temp_dir().join(format!("urdia-missing-{}", Uuid::new_v4()));
        let encoded = serde_json::to_string(&missing).expect("path should serialize");
        let error = serde_json::from_str::<WorkspacePath>(&encoded)
            .expect_err("missing workspace must be rejected");
        assert!(error.to_string().contains("invalid workspace"));
    }

    #[tokio::test]
    async fn default_permission_policy_denies() {
        let broker = DenyAllPermissionBroker;
        let workspace = WorkspacePath::new(std::env::current_dir().expect("current directory"))
            .expect("workspace should be valid");
        let decision = broker
            .authorize(PermissionRequest {
                id: Uuid::new_v4(),
                session_id: SessionId::new(),
                provider_session_id: "provider-session".to_owned(),
                workspace,
                revision: 1,
                operation: PermissionOperation {
                    tool_call_id: "tool-1".to_owned(),
                    title: Some("Git status".to_owned()),
                    name: Some("shell".to_owned()),
                    kind: PermissionOperationKind::Execute,
                    arguments: Some(SensitiveValue::new(serde_json::json!({
                        "command": "git status"
                    }))),
                    locations: Vec::new(),
                },
            })
            .await
            .expect("default policy should return a decision");
        assert_eq!(decision, PermissionDecision::RejectOnce);
    }

    #[test]
    fn permission_receipt_hashes_sensitive_context() {
        let workspace = WorkspacePath::new(std::env::current_dir().expect("current directory"))
            .expect("workspace should be valid");
        let request = PermissionRequest {
            id: Uuid::new_v4(),
            session_id: SessionId::new(),
            provider_session_id: "private-provider-session".to_owned(),
            workspace,
            revision: 7,
            operation: PermissionOperation {
                tool_call_id: "tool-7".to_owned(),
                title: Some("Run deployment".to_owned()),
                name: Some("shell".to_owned()),
                kind: PermissionOperationKind::Execute,
                arguments: Some(SensitiveValue::new(serde_json::json!({
                    "token": "TOP_SECRET_SENTINEL"
                }))),
                locations: vec![PermissionLocation {
                    path: PathBuf::from("/private/project/file.rs"),
                    line: Some(12),
                }],
            },
        };
        assert!(!format!("{request:?}").contains("TOP_SECRET_SENTINEL"));
        let receipt =
            PermissionReceipt::from_request(&request, PermissionReceiptOutcome::RejectedOnce, None);
        let serialized = serde_json::to_string(&receipt).expect("receipt should serialize");
        assert!(!serialized.contains("TOP_SECRET_SENTINEL"));
        assert!(!serialized.contains("private-provider-session"));
        assert!(!serialized.contains("/private/project"));
        assert!(receipt.arguments_present);
        assert_eq!(receipt.location_count, 1);
    }

    #[test]
    fn trusted_receipt_keeps_only_policy_fingerprint_and_generation() {
        let workspace = WorkspacePath::new(std::env::current_dir().expect("current directory"))
            .expect("workspace should be valid");
        let request = PermissionRequest {
            id: Uuid::new_v4(),
            session_id: SessionId::new(),
            provider_session_id: "trusted-provider-session".to_owned(),
            workspace,
            revision: 11,
            operation: PermissionOperation {
                tool_call_id: "tool-trusted".to_owned(),
                title: Some("Run private command".to_owned()),
                name: Some("shell".to_owned()),
                kind: PermissionOperationKind::Execute,
                arguments: Some(SensitiveValue::new(serde_json::json!({
                    "command": "echo TRUSTED_SECRET_SENTINEL"
                }))),
                locations: vec![PermissionLocation {
                    path: PathBuf::from("/private/trusted/path"),
                    line: Some(3),
                }],
            },
        };
        let policy_id = Uuid::new_v4();
        let grant = TrustedPermissionGrant::new(policy_id, 19);
        let receipt = PermissionReceipt::from_trusted_request(
            &request,
            PermissionReceiptOutcome::AllowedOnce,
            None,
            &grant,
        );
        assert_eq!(receipt.schema_version, 2);
        assert_eq!(
            receipt.authorization,
            PermissionAuthorizationSource::TrustedSession
        );
        assert_eq!(receipt.policy_revision, Some(19));
        assert_eq!(
            receipt.policy_id_fingerprint,
            Some(fingerprint(policy_id.as_bytes()))
        );
        let serialized = serde_json::to_string(&receipt).expect("receipt should serialize");
        assert!(!serialized.contains(&policy_id.to_string()));
        assert!(!serialized.contains("trusted-provider-session"));
        assert!(!serialized.contains("TRUSTED_SECRET_SENTINEL"));
        assert!(!format!("{grant:?}").contains(&policy_id.to_string()));
    }

    #[test]
    fn historical_v1_permission_receipt_defaults_to_one_shot_provenance() {
        let receipt: PermissionReceipt = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "request_id": Uuid::new_v4(),
            "session_id": SessionId::new(),
            "revision": 2,
            "provider_session_fingerprint": "provider",
            "workspace_fingerprint": "workspace",
            "operation_fingerprint": "operation",
            "tool_call_fingerprint": "tool",
            "kind": "read",
            "arguments_present": false,
            "location_count": 0,
            "outcome": "allowed_once",
            "cancellation_reason": null
        }))
        .expect("schema-v1 receipts should remain readable");
        assert_eq!(
            receipt.authorization,
            PermissionAuthorizationSource::OneShot
        );
        assert_eq!(receipt.policy_id_fingerprint, None);
        assert_eq!(receipt.policy_revision, None);
    }

    #[test]
    fn trusted_decisions_cannot_be_forged_or_serialized_through_json() {
        let trusted = PermissionDecision::allow_trusted(Uuid::new_v4(), 4);
        assert!(serde_json::to_string(&trusted).is_err());
        let forged = serde_json::from_str::<PermissionDecision>(
            r#"{"allow_trusted":{"policy_id":"00000000-0000-0000-0000-000000000001","policy_revision":4}}"#,
        );
        assert!(forged.is_err());
        assert_eq!(
            serde_json::from_str::<PermissionDecision>(r#""allow_once""#)
                .expect("one-shot decisions remain browser-compatible"),
            PermissionDecision::AllowOnce
        );
    }

    #[tokio::test]
    async fn deterministic_decision_does_not_authorize() {
        let provider = FirstCandidateDecisionProvider::new("test-router");
        let decision = provider
            .decide(DecisionInput {
                objective: "route this task".to_owned(),
                candidates: vec!["qwen".to_owned(), "codex".to_owned()],
                state: Value::Null,
            })
            .await
            .expect("deterministic decision should succeed");
        assert_eq!(decision.selected.as_deref(), Some("qwen"));
        assert_eq!(decision.probabilities.get("qwen"), Some(&1.0));
        assert_eq!(decision.probabilities.get("codex"), Some(&0.0));
    }

    #[test]
    fn provider_registry_rejects_duplicates_and_invalid_ids() {
        let mut registry = ProviderRegistry::new();
        registry
            .register(stub_provider("first", ProviderAvailability::Available))
            .expect("first provider should register");

        let duplicate =
            match registry.register(stub_provider("first", ProviderAvailability::Available)) {
                Ok(()) => panic!("duplicate provider IDs must be rejected"),
                Err(error) => error,
            };
        assert!(matches!(
            duplicate,
            ProviderRegistryError::DuplicateProvider { id } if id == "first"
        ));

        let invalid =
            match registry.register(stub_provider("not_valid", ProviderAvailability::Available)) {
                Ok(()) => panic!("provider IDs must use lowercase ASCII kebab case"),
                Err(error) => error,
            };
        assert!(matches!(
            invalid,
            ProviderRegistryError::InvalidProviderId { id } if id == "not_valid"
        ));
    }

    #[test]
    fn provider_registry_lists_ids_in_stable_order() {
        let mut registry = ProviderRegistry::new();
        registry
            .register(stub_provider("zeta", ProviderAvailability::Available))
            .expect("zeta should register");
        registry
            .register(stub_provider("alpha", ProviderAvailability::Available))
            .expect("alpha should register");
        registry
            .register(stub_provider("middle", ProviderAvailability::Available))
            .expect("middle should register");

        let ids = registry
            .list()
            .into_iter()
            .map(|info| info.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, ["alpha", "middle", "zeta"]);
    }

    #[test]
    fn provider_registry_reports_unknown_and_unavailable() {
        let mut registry = ProviderRegistry::new();
        registry
            .register(stub_provider("offline", ProviderAvailability::Unavailable))
            .expect("offline provider should register");

        let unknown = match registry.resolve("missing") {
            Ok(_) => panic!("unknown provider IDs must fail"),
            Err(error) => error,
        };
        assert!(matches!(
            unknown,
            ProviderRegistryError::UnknownProvider { id } if id == "missing"
        ));

        let unavailable = match registry.resolve_available("offline") {
            Ok(_) => panic!("unavailable providers must not resolve for a session"),
            Err(error) => error,
        };
        assert!(matches!(
            unavailable,
            ProviderRegistryError::UnavailableProvider { id, detail }
                if id == "offline" && detail == "offline detail"
        ));
        assert!(registry.resolve("offline").is_ok());
    }
}
