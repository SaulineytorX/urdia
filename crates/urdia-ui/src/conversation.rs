//! In-memory, host-owned conversation coordination for the local UI.
//!
//! The browser can submit one client-generated turn at a time.  The host
//! consumes the resulting command and reports progress back through this
//! cloneable hub.  Conversation content is deliberately kept out of the
//! event store; callers only receive it through the authenticated live API.

use crate::plans::{PlanStore, PlanTask};
use crate::{PlanError, PlanLink, PlanRequest, PlanSnapshot};
use crate::{
    VerificationContract, VerificationError, VerificationJob, VerificationReport,
    VerificationRequest, VerificationSnapshot,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Maximum number of Unicode scalar values accepted in one prompt.
pub const MAX_PROMPT_CHARS: usize = 16 * 1024;
/// Maximum number of turns retained by the in-memory conversation history.
pub const MAX_HISTORY_TURNS: usize = 64;
/// Maximum number of accepted idempotency keys retained for one UI session.
pub const MAX_SESSION_TURNS: usize = 1024;
/// Maximum number of Unicode scalar values retained in one response.
pub const MAX_RESPONSE_CHARS: usize = 64 * 1024;
/// Maximum number of host commands that can wait behind a slow host loop.
pub const COMMAND_QUEUE_CAPACITY: usize = 4;

/// A command emitted by [`ConversationHub`] for the host to execute.
#[derive(Clone, Eq, PartialEq)]
pub enum ConversationCommand {
    /// Start one client-submitted turn.
    Prompt {
        turn_id: Uuid,
        text: String,
    },
    /// Host-approved one-off profile task; never populated by the prompt API.
    Recommended {
        turn_id: Uuid,
        text: String,
        profile_id: String,
    },
    OpenResult {
        request_id: Uuid,
    },
    Verify {
        contract: VerificationContract,
        request_id: Uuid,
    },
    CancelVerification {
        request_id: Uuid,
    },
    /// Explicitly replace the original failed session, without a prompt.
    Reconnect {
        revision: u64,
    },
    /// Ask the host to stop the active turn.
    Cancel {
        turn_id: Uuid,
    },
}

impl std::fmt::Debug for ConversationCommand {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Prompt { turn_id, text } | Self::Recommended { turn_id, text, .. } => formatter
                .debug_struct("Prompt")
                .field("turn_id", turn_id)
                .field("text_chars", &text.chars().count())
                .finish(),
            Self::OpenResult { request_id }
            | Self::Verify { request_id, .. }
            | Self::CancelVerification { request_id } => formatter
                .debug_struct("Verification")
                .field("request_id", request_id)
                .finish(),
            Self::Reconnect { revision } => formatter
                .debug_struct("Reconnect")
                .field("revision", revision)
                .finish(),
            Self::Cancel { turn_id } => formatter
                .debug_struct("Cancel")
                .field("turn_id", turn_id)
                .finish(),
        }
    }
}

/// Receiver owned by the host that runs the provider session.
pub type ConversationCommandReceiver = mpsc::Receiver<ConversationCommand>;

/// Public status for one conversation turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationTurnStatus {
    /// The host has not started processing the submitted prompt yet.
    Queued,
    /// The host is processing the prompt.
    Running,
    /// Cancellation was requested and the host is expected to stop the turn.
    Cancelling,
    /// The host completed the response.
    Completed,
    /// The host reported a failure.
    Failed,
    /// The host confirmed cancellation.
    Cancelled,
}

impl ConversationTurnStatus {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Host-observed durations and streaming counts; never token or cost estimates.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TurnMetrics {
    pub elapsed_ms: u64,
    pub execution_ms: Option<u64>,
    pub first_text_ms: Option<u64>,
    pub text_updates: u64,
    pub response_chars: usize,
}
struct TurnClock {
    accepted: Instant,
    started: Option<Instant>,
    finished: Option<Instant>,
    first_text: Option<Instant>,
    text_updates: u64,
    tools: Vec<urdia_core::AgentToolActivity>,
    tools_truncated: bool,
}
impl TurnClock {
    fn new() -> Self {
        Self {
            accepted: Instant::now(),
            started: None,
            finished: None,
            first_text: None,
            text_updates: 0,
            tools: Vec::new(),
            tools_truncated: false,
        }
    }
    fn snapshot(&self, now: Instant, response_chars: usize) -> TurnMetrics {
        let end = self.finished.unwrap_or(now);
        let millis = |a: Instant, b: Instant| {
            u64::try_from(b.saturating_duration_since(a).as_millis()).unwrap_or(u64::MAX)
        };
        TurnMetrics {
            elapsed_ms: millis(self.accepted, end),
            execution_ms: self.started.map(|start| millis(start, end)),
            first_text_ms: self
                .started
                .zip(self.first_text)
                .map(|(start, first)| millis(start, first)),
            text_updates: self.text_updates,
            response_chars,
        }
    }
}

/// One conversation turn exposed by an authenticated snapshot.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ConversationTurn {
    /// Original launch family, distinct from independent fresh-profile tasks.
    pub original_session: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ConversationContext>,
    /// Client-generated idempotency key for this turn.
    pub turn_id: Uuid,
    /// Prompt submitted by the local browser.
    pub prompt: String,
    /// Response text received from the host so far.
    pub response: String,
    /// Monotonic lifecycle status for this turn.
    pub status: ConversationTurnStatus,
    /// Host-provided failure message, when the turn failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl std::fmt::Debug for ConversationTurn {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConversationTurn")
            .field("turn_id", &self.turn_id)
            .field("prompt_chars", &self.prompt.chars().count())
            .field("response_chars", &self.response.chars().count())
            .field("status", &self.status)
            .field("error_present", &self.error.is_some())
            .finish()
    }
}

/// Host-provided identity for the current conversation, never a credential.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ConversationContext {
    /// Explicit provider ID selected by the host.
    pub provider_id: String,
    /// Human-readable provider name.
    pub provider_name: String,
    /// Project directory name, without its parent path.
    pub workspace_name: String,
    /// Present only for an explicitly selected or configured default profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<ConversationProfile>,
    /// Initial agent-reported selectors; not remote verification or a live guarantee.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_controls: Option<urdia_core::SessionControlState>,
}

/// Operator-declared launch identity. Model labels are not capability evidence.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ConversationProfile {
    pub id: String,
    pub name: String,
    pub agent: String,
    pub inference_provider: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    pub selection: String,
    pub model_effort_verified: bool,
}

/// Bounded last reported states for tools observed during a turn.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TurnToolActivity {
    pub calls: Vec<urdia_core::AgentToolActivity>,
    pub truncated: bool,
}

/// Immutable host choices and a memory-only default for future tasks.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ProfileSelectionSnapshot {
    pub revision: u64,
    pub selected_profile: Option<String>,
    pub choices: BTreeMap<String, ConversationContext>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileSelectionError {
    Unavailable,
    Busy,
    Stale,
    Unknown,
    Unconfirmed,
}

/// Host-owned original-session recovery; no native identifiers or credentials.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReconnectionSnapshot {
    pub revision: u64,
    pub status: &'static str,
    pub context_resets: u64,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconnectionError {
    Unavailable,
    Unconfirmed,
    Stale,
    Busy,
    NotFailed,
    Limit,
    Queue,
}

/// Complete, sequence-numbered conversation view.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ConversationSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnection: Option<ReconnectionSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_summary: Option<crate::ProjectSummary>,
    pub project_plan: PlanSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_preview: Option<crate::ResultPreviewSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_selection: Option<ProfileSelectionSnapshot>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub tool_activity: HashMap<Uuid, TurnToolActivity>,
    /// Metrics are memory-only and available through the authenticated conversation API.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub turn_metrics: HashMap<Uuid, TurnMetrics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_controls: Option<urdia_core::LiveSessionControls>,
    /// Monotonically increasing state revision.
    pub sequence: u64,
    /// At most one non-terminal turn can be active at a time.
    pub active_turn_id: Option<Uuid>,
    /// Oldest-to-newest bounded turn history.
    pub turns: Vec<ConversationTurn>,
    /// Optional current-session identity, available only through authenticated API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ConversationContext>,
}

impl std::fmt::Debug for ConversationSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConversationSnapshot")
            .field("sequence", &self.sequence)
            .field("active_turn_id", &self.active_turn_id)
            .field("turn_count", &self.turns.len())
            .finish()
    }
}

/// Failure returned when a host update cannot be applied monotonically.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConversationUpdateError {
    /// No turn exists under this id.
    Missing,
    /// The turn has already reached a terminal state.
    Terminal,
    /// The update is not valid for the turn's current state.
    InvalidState {
        turn_id: Uuid,
        status: ConversationTurnStatus,
    },
    /// The response would exceed [`MAX_RESPONSE_CHARS`].
    ResponseLimit,
    /// The hub's command receiver has been dropped.
    CommandReceiverClosed,
    /// The bounded host command queue is full.
    CommandQueueFull,
}

impl std::fmt::Display for ConversationUpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => formatter.write_str("conversation turn not found"),
            Self::Terminal => formatter.write_str("conversation turn is already terminal"),
            Self::InvalidState { status, .. } => {
                write!(formatter, "conversation turn is {status:?}")
            }
            Self::ResponseLimit => formatter.write_str("conversation response is too long"),
            Self::CommandReceiverClosed => {
                formatter.write_str("conversation host command receiver is closed")
            }
            Self::CommandQueueFull => {
                formatter.write_str("conversation host command queue is full")
            }
        }
    }
}

impl std::error::Error for ConversationUpdateError {}

/// Failure returned when a browser prompt or cancellation cannot be accepted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConversationSubmitError {
    /// The submitted prompt is empty.
    EmptyPrompt,
    /// The submitted prompt exceeds [`MAX_PROMPT_CHARS`].
    PromptLimit,
    /// Another non-terminal turn is already active.
    Busy {
        turn_id: Uuid,
    },
    /// The idempotency key was reused for different prompt text.
    Conflict {
        turn_id: Uuid,
    },
    /// The host is no longer listening for commands.
    CommandReceiverClosed,
    /// The bounded host command queue is full.
    CommandQueueFull,
    /// This UI session has reached its bounded turn count.
    SessionLimit,
    ProfileRevision,
    PrimaryDisconnected,
    PlanChanged,
}

impl std::fmt::Display for ConversationSubmitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPrompt => formatter.write_str("prompt must not be empty"),
            Self::PromptLimit => formatter.write_str("prompt exceeds the maximum length"),
            Self::Busy { .. } => formatter.write_str("another conversation turn is active"),
            Self::Conflict { .. } => {
                formatter.write_str("turn id already has different prompt text")
            }
            Self::CommandReceiverClosed => {
                formatter.write_str("conversation host command receiver is closed")
            }
            Self::CommandQueueFull => {
                formatter.write_str("conversation host command queue is full")
            }
            Self::SessionLimit => formatter.write_str("conversation session turn limit reached"),
            Self::PlanChanged => formatter.write_str("project plan changed or blocked"),
            Self::PrimaryDisconnected => formatter.write_str("original agent needs reconnection"),
            Self::ProfileRevision => formatter.write_str("profile selection revision changed"),
        }
    }
}

impl std::error::Error for ConversationSubmitError {}

#[derive(Clone)]
pub struct ConversationHub {
    state: Arc<Mutex<ConversationState>>,
    commands: mpsc::Sender<ConversationCommand>,
}

impl std::fmt::Debug for ConversationHub {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConversationHub")
            .field("snapshot", &self.snapshot())
            .finish()
    }
}

struct AcceptedPrompt {
    plan_link: Option<PlanLink>,
    text: String,
    profile_id: Option<String>,
    revision: Option<u64>,
    manual: bool,
}

#[derive(Default)]
struct ConversationState {
    reconnection: Option<ReconnectionSnapshot>,
    reconnect_attempts: u64,
    primary_disconnected: bool,
    plans: PlanStore,
    result_preview: crate::result_preview::ResultPreviewStore,
    verification_enabled: bool,
    verification_jobs: BTreeMap<Uuid, VerificationJob>,
    verification_order: Vec<Uuid>,
    latest_verification: Option<Uuid>,
    clocks: HashMap<Uuid, TurnClock>,
    live_controls: Option<urdia_core::LiveSessionControls>,
    context: Option<ConversationContext>,
    sequence: u64,
    turns: VecDeque<ConversationTurn>,
    accepted_prompts: HashMap<Uuid, AcceptedPrompt>,
    profile_selection: Option<ProfileSelectionSnapshot>,
}

/// Cloneable in-memory conversation state with a host-owned command channel.
impl ConversationHub {
    /// Enable only for a non-trusted original session, before exposing the UI.
    pub fn enable_reconnection(&self) {
        self.lock_state().reconnection = Some(ReconnectionSnapshot {
            revision: 0,
            status: "ready",
            context_resets: 0,
            error: None,
        });
    }

    /// Admit one explicit, revision-bound, prompt-free replacement.
    pub fn request_reconnection(
        &self,
        revision: u64,
        confirm: bool,
    ) -> Result<ConversationSnapshot, ReconnectionError> {
        let mut state = self.lock_state();
        if !confirm {
            return Err(ReconnectionError::Unconfirmed);
        }
        let view = state
            .reconnection
            .as_ref()
            .ok_or(ReconnectionError::Unavailable)?;
        if view.revision != revision {
            return Err(ReconnectionError::Stale);
        }
        if view.status == "reconnecting" {
            return Ok(state.snapshot());
        }
        if state.active_turn().is_some()
            || (state.verification_active() || state.result_preview.preparing())
            || state.reconnecting()
        {
            return Err(ReconnectionError::Busy);
        }
        if !matches!(view.status, "needed" | "failed") {
            return Err(ReconnectionError::NotFailed);
        }
        if state.reconnect_attempts >= 16 {
            return Err(ReconnectionError::Limit);
        }
        self.commands
            .try_send(ConversationCommand::Reconnect { revision })
            .map_err(|_| ReconnectionError::Queue)?;
        state.reconnect_attempts += 1;
        state.primary_disconnected = true;
        let view = state.reconnection.as_mut().expect("checked above");
        view.status = "reconnecting";
        view.error = None;
        state.bump();
        Ok(state.snapshot())
    }

    /// Apply only the matching host result; failures retain no native error body.
    pub fn finish_reconnection(
        &self,
        revision: u64,
        result: Result<Option<urdia_core::SessionControlState>, urdia_core::ProviderFailureKind>,
    ) {
        let mut state = self.lock_state();
        let Some(view) = state.reconnection.as_mut() else {
            return;
        };
        if view.revision != revision || view.status != "reconnecting" {
            return;
        }
        view.revision += 1;
        match result {
            Ok(controls) => {
                view.status = "ready";
                view.error = None;
                view.context_resets += 1;
                state.primary_disconnected = false;
                if let Some(context) = &mut state.context {
                    context.startup_controls = controls;
                }
                state.live_controls = None;
            }
            Err(kind) => {
                view.status = "failed";
                view.error = Some(kind.message().into());
            }
        }
        state.bump();
    }

    pub fn enable_result_preview(&self) {
        self.lock_state().result_preview.enabled = true;
    }

    pub fn submit_result_preview(
        &self,
        request: crate::ResultPreviewRequest,
    ) -> Result<ConversationSnapshot, crate::ResultPreviewError> {
        use crate::{ResultPreviewError as Error, ResultPreviewJob};
        let mut state = self.lock_state();
        if !state.result_preview.enabled {
            return Err(Error::Unavailable);
        }
        if !request.confirm_execution {
            return Err(Error::Unconfirmed);
        }
        if let Some(job) = state.result_preview.jobs.get(&request.request_id) {
            return if job.turn_id != request.turn_id {
                Err(Error::Conflict)
            } else if state.result_preview.latest != Some(request.request_id) {
                Err(Error::Stale)
            } else {
                Ok(state.snapshot())
            };
        }
        if state.accepted_prompts.contains_key(&request.request_id)
            || state.verification_jobs.contains_key(&request.request_id)
        {
            return Err(Error::Conflict);
        }
        if state.active_turn().is_some()
            || state.verification_active()
            || state.result_preview.preparing()
            || state.reconnecting()
        {
            return Err(Error::Busy);
        }
        if !state.turns.back().is_some_and(|t| {
            t.turn_id == request.turn_id && t.status == ConversationTurnStatus::Completed
        }) {
            return Err(Error::Stale);
        }
        if state.result_preview.jobs.len() >= 16 {
            return Err(Error::Limit);
        }
        self.commands
            .try_send(ConversationCommand::OpenResult {
                request_id: request.request_id,
            })
            .map_err(|_| Error::HostUnavailable)?;
        if let Some(id) = state.result_preview.latest {
            state.result_preview.stop(id);
        }
        state.result_preview.jobs.insert(
            request.request_id,
            ResultPreviewJob {
                request_id: request.request_id,
                turn_id: request.turn_id,
                status: "queued",
                current_task: true,
                url: None,
                fingerprint: None,
                file_count: None,
                expires_in_seconds: 0,
            },
        );
        state.result_preview.latest = Some(request.request_id);
        state.bump();
        Ok(state.snapshot())
    }

    pub fn begin_result_preview(&self, id: Uuid) -> bool {
        let mut state = self.lock_state();
        let Some(job) = state.result_preview.jobs.get(&id) else {
            return false;
        };
        if job.status != "queued" {
            return false;
        }
        if state.result_preview.latest != Some(id)
            || !state.turns.back().is_some_and(|t| {
                t.turn_id == job.turn_id && t.status == ConversationTurnStatus::Completed
            })
        {
            state
                .result_preview
                .jobs
                .get_mut(&id)
                .expect("known request")
                .status = "unavailable";
            state.bump();
            return false;
        }
        state
            .result_preview
            .jobs
            .get_mut(&id)
            .expect("known request")
            .status = "preparing";
        state.bump();
        true
    }

    pub fn finish_result_preview(&self, id: Uuid, lease: Option<crate::ResultPreviewLease>) {
        let mut state = self.lock_state();
        let Some(job) = state
            .result_preview
            .jobs
            .get_mut(&id)
            .filter(|j| j.status == "preparing")
        else {
            return;
        };
        job.status = if lease.is_some() {
            "ready"
        } else {
            "unavailable"
        };
        if let Some(lease) = lease {
            state.result_preview.lease = Some((id, lease));
        }
        state.bump();
    }

    pub fn stop_result_preview(
        &self,
        id: Uuid,
    ) -> Result<ConversationSnapshot, crate::ResultPreviewError> {
        let mut state = self.lock_state();
        if !state.result_preview.enabled {
            return Err(crate::ResultPreviewError::Unavailable);
        }
        if !state.result_preview.jobs.contains_key(&id) {
            return Err(crate::ResultPreviewError::Stale);
        }
        state.result_preview.stop(id);
        state.bump();
        Ok(state.snapshot())
    }

    pub fn stop_result_previews(&self) {
        let mut state = self.lock_state();
        let ids: Vec<_> = state.result_preview.jobs.keys().copied().collect();
        for id in ids {
            state.result_preview.stop(id);
        }
        state.bump();
    }

    pub fn enable_verification(&self) {
        self.lock_state().verification_enabled = true;
    }

    pub fn submit_verification(
        &self,
        request: VerificationRequest,
    ) -> Result<ConversationSnapshot, VerificationError> {
        let mut state = self.lock_state();
        if !state.verification_enabled {
            return Err(VerificationError::Unavailable);
        }
        if !request.confirm_execution {
            return Err(VerificationError::Unconfirmed);
        }
        if let Some(job) = state.verification_jobs.get(&request.request_id) {
            return if job.turn_id == request.turn_id && job.contract == request.contract {
                Ok(state.snapshot())
            } else {
                Err(VerificationError::Conflict)
            };
        }
        if state.accepted_prompts.contains_key(&request.request_id)
            || state.result_preview.jobs.contains_key(&request.request_id)
        {
            return Err(VerificationError::Conflict);
        }
        if state.active_turn().is_some()
            || (state.verification_active() || state.result_preview.preparing())
            || state.reconnecting()
        {
            return Err(VerificationError::Busy);
        }
        if !state.turns.back().is_some_and(|turn| {
            turn.turn_id == request.turn_id && turn.status == ConversationTurnStatus::Completed
        }) {
            return Err(VerificationError::Stale);
        }
        if state.verification_jobs.len() >= 16 {
            return Err(VerificationError::Limit);
        }
        self.commands
            .try_send(ConversationCommand::Verify {
                contract: request.contract,
                request_id: request.request_id,
            })
            .map_err(|_| VerificationError::HostUnavailable)?;
        state.verification_jobs.insert(
            request.request_id,
            VerificationJob {
                contract: request.contract,
                request_id: request.request_id,
                turn_id: request.turn_id,
                status: "queued",
                report: None,
                stale: false,
            },
        );
        state.verification_order.push(request.request_id);
        state.latest_verification = Some(request.request_id);
        state.bump();
        Ok(state.snapshot())
    }
    pub fn cancel_verification(
        &self,
        request_id: Uuid,
    ) -> Result<ConversationSnapshot, VerificationError> {
        let mut state = self.lock_state();
        if !state.verification_enabled {
            return Err(VerificationError::Unavailable);
        }
        let Some(job) = state.verification_jobs.get_mut(&request_id) else {
            return Err(VerificationError::Stale);
        };
        if job.status == "cancelling" {
            return Ok(state.snapshot());
        }
        if !matches!(job.status, "queued" | "running") {
            return Err(VerificationError::Stale);
        }
        self.commands
            .try_send(ConversationCommand::CancelVerification { request_id })
            .map_err(|_| VerificationError::HostUnavailable)?;
        job.status = "cancelling";
        state.bump();
        Ok(state.snapshot())
    }
    pub fn begin_verification(&self, request_id: Uuid) -> bool {
        let mut state = self.lock_state();
        let Some(job) = state.verification_jobs.get_mut(&request_id) else {
            return false;
        };
        if job.status == "cancelling" {
            job.status = "cancelled";
            state.bump();
            return false;
        }
        if job.status != "queued" {
            return false;
        }
        job.status = "running";
        state.bump();
        true
    }
    pub fn finish_verification(&self, request_id: Uuid, report: Option<VerificationReport>) {
        let mut state = self.lock_state();
        let Some(job) = state.verification_jobs.get_mut(&request_id) else {
            return;
        };
        if !matches!(job.status, "running" | "cancelling") {
            return;
        }
        let report = report.filter(|r| r.valid() && r.contract == job.contract.id());
        job.status = if job.status == "cancelling" {
            "cancelled"
        } else {
            report.as_ref().map_or("unavailable", |r| {
                if r.status == "passed" {
                    "passed"
                } else {
                    "failed"
                }
            })
        };
        job.report = if job.status == "cancelled" {
            None
        } else {
            report
        };
        state.bump();
    }

    /// Create an empty hub and the receiver that the host owns.
    #[must_use]
    pub fn new() -> (Self, ConversationCommandReceiver) {
        let (commands, receiver) = mpsc::channel(COMMAND_QUEUE_CAPACITY);
        (
            Self {
                state: Arc::new(Mutex::new(ConversationState::default())),
                commands,
            },
            receiver,
        )
    }

    /// Create a hub with immutable current-session identity supplied by the host.
    #[must_use]
    pub fn with_context(context: ConversationContext) -> (Self, ConversationCommandReceiver) {
        let (hub, receiver) = Self::new();
        hub.lock_state().context = Some(context);
        (hub, receiver)
    }

    /// Configure immutable eligible choices once, before exposing the server.
    pub fn configure_profile_selection(&self, choices: BTreeMap<String, ConversationContext>) {
        let mut state = self.lock_state();
        if state.profile_selection.is_none()
            && !choices.is_empty()
            && state.accepted_prompts.is_empty()
        {
            state.profile_selection = Some(ProfileSelectionSnapshot {
                revision: 0,
                selected_profile: None,
                choices,
            });
            state.bump();
        }
    }

    /// Change future-task selection only; this starts no agent and grants no access.
    pub fn select_profile(
        &self,
        revision: u64,
        profile_id: Option<String>,
        confirm: bool,
    ) -> Result<ConversationSnapshot, ProfileSelectionError> {
        let mut state = self.lock_state();
        if !confirm {
            return Err(ProfileSelectionError::Unconfirmed);
        }
        if state.active_turn().is_some()
            || (state.verification_active() || state.result_preview.preparing())
            || state.reconnecting()
        {
            return Err(ProfileSelectionError::Busy);
        }
        let selection = state
            .profile_selection
            .as_mut()
            .ok_or(ProfileSelectionError::Unavailable)?;
        if selection.revision != revision {
            return Err(ProfileSelectionError::Stale);
        }
        if profile_id
            .as_ref()
            .is_some_and(|id| !selection.choices.contains_key(id))
        {
            return Err(ProfileSelectionError::Unknown);
        }
        selection.selected_profile = profile_id;
        selection.revision = selection.revision.saturating_add(1);
        state.bump();
        Ok(state.snapshot())
    }

    pub fn submit_prompt_with_revision(
        &self,
        turn_id: Uuid,
        text: String,
        revision: Option<u64>,
    ) -> Result<ConversationSnapshot, ConversationSubmitError> {
        self.submit(turn_id, text, None, revision, true, None)
    }

    pub fn submit_planned_prompt_with_revision(
        &self,
        turn_id: Uuid,
        text: String,
        revision: Option<u64>,
        plan_link: Option<PlanLink>,
    ) -> Result<ConversationSnapshot, ConversationSubmitError> {
        self.submit(turn_id, text, None, revision, true, plan_link)
    }
    pub fn export_plan(&self) -> Result<crate::PlanArchive, PlanError> {
        let state = self.lock_state();
        state
            .plans
            .export(state.context.as_ref().map(|c| c.workspace_name.clone()))
    }
    pub fn mutate_plan(&self, request: PlanRequest) -> Result<ConversationSnapshot, PlanError> {
        let mut state = self.lock_state();
        let busy = state.active_turn().is_some()
            || (state.verification_active() || state.result_preview.preparing())
            || state.reconnecting();
        state.plans.mutate(request, busy)?;
        state.bump();
        Ok(state.snapshot())
    }

    /// Update memory-only observed controls from the host.
    pub fn set_live_controls(&self, controls: Option<urdia_core::LiveSessionControls>) {
        let mut state = self.lock_state();
        if state.live_controls != controls {
            state.live_controls = controls;
            state.bump();
        }
    }

    /// Return the current sequence-numbered snapshot.
    #[must_use]
    pub fn snapshot(&self) -> ConversationSnapshot {
        let state = self.lock_state();
        state.snapshot()
    }

    /// Accept one browser prompt and emit one host command.
    ///
    /// Repeating the same `(turn_id, text)` is idempotent and never emits a
    /// second command.  Reusing a turn id for different text is a conflict.
    pub fn submit_prompt(
        &self,
        turn_id: Uuid,
        text: String,
    ) -> std::result::Result<ConversationSnapshot, ConversationSubmitError> {
        self.submit(turn_id, text, None, None, true, None)
    }

    /// Read-only freshness check. Linked admission repeats it atomically before enqueue.
    pub fn validate_plan_step(&self, link: &PlanLink, text: &str) -> Result<(), PlanError> {
        let state = self.lock_state();
        state.plans.validate_link(link, text)?;
        if state.active_turn().is_some()
            || (state.verification_active() || state.result_preview.preparing())
            || state.reconnecting()
        {
            return Err(PlanError::Busy);
        }
        Ok(())
    }

    pub fn submit_recommended_with_plan(
        &self,
        turn_id: Uuid,
        text: String,
        profile_id: String,
        plan_link: Option<PlanLink>,
    ) -> Result<ConversationSnapshot, ConversationSubmitError> {
        self.submit(turn_id, text, Some(profile_id), None, false, plan_link)
    }

    /// Called only after host-bound recommendation confirmation.
    pub fn submit_recommended(
        &self,
        turn_id: Uuid,
        text: String,
        profile_id: String,
    ) -> std::result::Result<ConversationSnapshot, ConversationSubmitError> {
        self.submit(turn_id, text, Some(profile_id), None, false, None)
    }

    pub fn set_execution(&self, turn_id: Uuid, context: ConversationContext) {
        let mut state = self.lock_state();
        if let Some(turn) = state.turn_mut(turn_id) {
            turn.execution = Some(context);
            state.bump();
        }
    }

    fn submit(
        &self,
        turn_id: Uuid,
        text: String,
        mut profile_id: Option<String>,
        revision: Option<u64>,
        manual: bool,
        plan_link: Option<PlanLink>,
    ) -> std::result::Result<ConversationSnapshot, ConversationSubmitError> {
        validate_prompt(&text)?;
        let mut state = self.lock_state();

        if state.verification_jobs.contains_key(&turn_id)
            || state.result_preview.jobs.contains_key(&turn_id)
        {
            return Err(ConversationSubmitError::Conflict { turn_id });
        }
        if let Some(existing) = state.accepted_prompts.get(&turn_id) {
            if existing.text == text
                && existing.plan_link == plan_link
                && existing.manual == manual
                && if manual {
                    existing.revision == revision
                } else {
                    existing.profile_id == profile_id
                }
            {
                return Ok(state.snapshot());
            }
            return Err(ConversationSubmitError::Conflict { turn_id });
        }

        if let Some(link) = &plan_link {
            state
                .plans
                .validate_link(link, &text)
                .map_err(|_| ConversationSubmitError::PlanChanged)?;
        }
        if manual && let Some(selection) = &state.profile_selection {
            if revision != Some(selection.revision) {
                return Err(ConversationSubmitError::ProfileRevision);
            }
            profile_id = selection.selected_profile.clone();
        }

        if state.accepted_prompts.len() >= MAX_SESSION_TURNS {
            return Err(ConversationSubmitError::SessionLimit);
        }

        if state.primary_disconnected && profile_id.is_none() && !state.reconnecting() {
            return Err(ConversationSubmitError::PrimaryDisconnected);
        }
        if (state.verification_active() || state.result_preview.preparing()) || state.reconnecting()
        {
            return Err(ConversationSubmitError::Busy {
                turn_id: state
                    .latest_verification
                    .and_then(|id| state.verification_jobs.get(&id))
                    .map_or(turn_id, |job| job.turn_id),
            });
        }
        if let Some(active) = state.active_turn() {
            return Err(ConversationSubmitError::Busy {
                turn_id: active.turn_id,
            });
        }

        let mut evicted = VecDeque::new();
        while state.turns.len() >= MAX_HISTORY_TURNS {
            if let Some(turn) = state.turns.pop_front() {
                evicted.push_back(turn);
            }
        }
        state.clocks.insert(turn_id, TurnClock::new());
        let execution = profile_id
            .as_ref()
            .filter(|_| manual)
            .and_then(|id| state.profile_selection.as_ref()?.choices.get(id).cloned())
            .or_else(|| {
                profile_id
                    .is_none()
                    .then(|| state.context.clone())
                    .flatten()
            });
        state.turns.push_back(ConversationTurn {
            original_session: profile_id.is_none(),
            execution,
            turn_id,
            prompt: text.clone(),
            response: String::new(),
            status: ConversationTurnStatus::Queued,
            error: None,
        });
        state.accepted_prompts.insert(
            turn_id,
            AcceptedPrompt {
                plan_link: plan_link.clone(),
                text: text.clone(),
                profile_id: profile_id.clone(),
                revision,
                manual,
            },
        );
        state.bump();
        let command = match profile_id {
            Some(profile_id) => ConversationCommand::Recommended {
                turn_id,
                text,
                profile_id,
            },
            None => ConversationCommand::Prompt { turn_id, text },
        };
        match self.commands.try_send(command) {
            Ok(()) => {
                if let Some(link) = &plan_link {
                    state.plans.bind(link, turn_id);
                    state.bump();
                }
                let retained: std::collections::HashSet<_> =
                    state.turns.iter().map(|turn| turn.turn_id).collect();
                state.clocks.retain(|id, _| retained.contains(id));
                Ok(state.snapshot())
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                state.clocks.remove(&turn_id);
                state.turns.pop_back();
                state.accepted_prompts.remove(&turn_id);
                while let Some(turn) = evicted.pop_back() {
                    state.turns.push_front(turn);
                }
                state.bump();
                Err(ConversationSubmitError::CommandQueueFull)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                state.clocks.remove(&turn_id);
                state.turns.pop_back();
                state.accepted_prompts.remove(&turn_id);
                while let Some(turn) = evicted.pop_back() {
                    state.turns.push_front(turn);
                }
                state.bump();
                Err(ConversationSubmitError::CommandReceiverClosed)
            }
        }
    }

    /// Mark a submitted turn as running.
    pub fn mark_running(&self, turn_id: Uuid) -> Result<(), ConversationUpdateError> {
        self.transition(turn_id, |turn| match turn.status {
            ConversationTurnStatus::Queued => {
                turn.status = ConversationTurnStatus::Running;
                turn.error = None;
                Ok(true)
            }
            ConversationTurnStatus::Running => Ok(false),
            status => Err(ConversationUpdateError::InvalidState { turn_id, status }),
        })
    }

    /// Append one host-provided response fragment to a running turn.
    pub fn publish_delta(&self, turn_id: Uuid, delta: &str) -> Result<(), ConversationUpdateError> {
        let mut state = self.lock_state();
        let turn = state
            .turn_mut(turn_id)
            .ok_or(ConversationUpdateError::Missing)?;
        if turn.status != ConversationTurnStatus::Running {
            return Err(ConversationUpdateError::InvalidState {
                turn_id,
                status: turn.status,
            });
        }
        let current = turn.response.chars().count();
        let incoming = delta.chars().count();
        if current.saturating_add(incoming) > MAX_RESPONSE_CHARS {
            return Err(ConversationUpdateError::ResponseLimit);
        }
        turn.response.push_str(delta);
        if !delta.is_empty()
            && let Some(clock) = state.clocks.get_mut(&turn_id)
        {
            clock.first_text.get_or_insert_with(Instant::now);
            clock.text_updates = clock.text_updates.saturating_add(1);
        }
        state.bump();
        Ok(())
    }

    /// Accept bounded redacted tool telemetry only while the host turn is running.
    pub fn publish_tool_activity(&self, turn_id: Uuid, update: urdia_core::AgentUpdate) {
        let mut state = self.lock_state();
        if !state
            .turns
            .iter()
            .any(|turn| turn.turn_id == turn_id && turn.status == ConversationTurnStatus::Running)
        {
            return;
        }
        let Some(clock) = state.clocks.get_mut(&turn_id) else {
            return;
        };
        match update {
            urdia_core::AgentUpdate::ToolActivity(activity) => {
                if activity.ordinal == 0
                    || usize::from(activity.ordinal) > urdia_core::MAX_TOOL_ACTIVITIES
                {
                    return;
                }
                if let Some(previous) = clock
                    .tools
                    .iter_mut()
                    .find(|call| call.ordinal == activity.ordinal)
                {
                    if previous.status.is_terminal()
                        || activity.status.rank() < previous.status.rank()
                        || *previous == activity
                    {
                        return;
                    }
                    *previous = activity;
                } else if clock.tools.len() < urdia_core::MAX_TOOL_ACTIVITIES {
                    clock.tools.push(activity);
                } else {
                    return;
                }
            }
            urdia_core::AgentUpdate::ToolActivityLimitReached => {
                if clock.tools_truncated {
                    return;
                }
                clock.tools_truncated = true;
            }
            _ => return,
        }
        state.bump();
    }

    /// Complete a running turn.
    pub fn complete(&self, turn_id: Uuid) -> Result<(), ConversationUpdateError> {
        self.transition(turn_id, |turn| match turn.status {
            ConversationTurnStatus::Queued | ConversationTurnStatus::Running => {
                turn.status = ConversationTurnStatus::Completed;
                Ok(true)
            }
            ConversationTurnStatus::Completed => Ok(false),
            status => Err(ConversationUpdateError::InvalidState { turn_id, status }),
        })
    }

    /// Record a host-reported failure for a non-terminal turn.
    pub fn fail(&self, turn_id: Uuid, error: String) -> Result<(), ConversationUpdateError> {
        if error.chars().count() > MAX_RESPONSE_CHARS {
            return Err(ConversationUpdateError::ResponseLimit);
        }
        self.transition(turn_id, |turn| match turn.status {
            ConversationTurnStatus::Queued
            | ConversationTurnStatus::Running
            | ConversationTurnStatus::Cancelling => {
                turn.status = ConversationTurnStatus::Failed;
                turn.error = Some(error);
                Ok(true)
            }
            ConversationTurnStatus::Failed => Ok(false),
            status => Err(ConversationUpdateError::InvalidState { turn_id, status }),
        })
    }

    /// Request cancellation from the host-facing API and emit one command.
    ///
    /// Repeating the request after it has already been accepted is idempotent.
    pub fn request_cancel(
        &self,
        turn_id: Uuid,
    ) -> Result<ConversationSnapshot, ConversationUpdateError> {
        let mut state = self.lock_state();
        let previous_status = state
            .turn_mut(turn_id)
            .ok_or(ConversationUpdateError::Missing)?
            .status;
        match previous_status {
            ConversationTurnStatus::Queued | ConversationTurnStatus::Running => {
                state
                    .turn_mut(turn_id)
                    .expect("turn was found above")
                    .status = ConversationTurnStatus::Cancelling;
                state.bump();
                match self
                    .commands
                    .try_send(ConversationCommand::Cancel { turn_id })
                {
                    Ok(()) => Ok(state.snapshot()),
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        state
                            .turn_mut(turn_id)
                            .expect("turn was found above")
                            .status = previous_status;
                        state.bump();
                        Err(ConversationUpdateError::CommandQueueFull)
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        state
                            .turn_mut(turn_id)
                            .expect("turn was found above")
                            .status = previous_status;
                        state.bump();
                        Err(ConversationUpdateError::CommandReceiverClosed)
                    }
                }
            }
            ConversationTurnStatus::Cancelling => Ok(state.snapshot()),
            ConversationTurnStatus::Cancelled => Ok(state.snapshot()),
            status => Err(ConversationUpdateError::InvalidState { turn_id, status }),
        }
    }

    /// Confirm that the host cancelled a turn.
    pub fn cancel(&self, turn_id: Uuid) -> Result<(), ConversationUpdateError> {
        self.transition(turn_id, |turn| match turn.status {
            ConversationTurnStatus::Queued
            | ConversationTurnStatus::Running
            | ConversationTurnStatus::Cancelling => {
                turn.status = ConversationTurnStatus::Cancelled;
                Ok(true)
            }
            ConversationTurnStatus::Cancelled => Ok(false),
            status => Err(ConversationUpdateError::InvalidState { turn_id, status }),
        })
    }

    fn transition<F>(&self, turn_id: Uuid, transition: F) -> Result<(), ConversationUpdateError>
    where
        F: FnOnce(&mut ConversationTurn) -> Result<bool, ConversationUpdateError>,
    {
        let mut state = self.lock_state();
        let changed = {
            let turn = state
                .turn_mut(turn_id)
                .ok_or(ConversationUpdateError::Missing)?;
            transition(turn)?
        };
        if changed {
            let status = state.turn_mut(turn_id).expect("existing turn").status;
            if let Some(clock) = state.clocks.get_mut(&turn_id) {
                if status == ConversationTurnStatus::Running {
                    clock.started.get_or_insert_with(Instant::now);
                }
                if status.is_terminal() {
                    clock.finished.get_or_insert_with(Instant::now);
                }
            }
            if state
                .accepted_prompts
                .get(&turn_id)
                .is_some_and(|p| p.profile_id.is_none())
                && let Some(view) = &mut state.reconnection
            {
                if status == ConversationTurnStatus::Failed {
                    view.status = "needed";
                    view.error = None;
                    view.revision += 1;
                } else if status == ConversationTurnStatus::Completed {
                    view.status = "ready";
                    view.error = None;
                    view.revision += 1;
                }
            }
            state.bump();
        }
        Ok(())
    }

    fn lock_state(&self) -> MutexGuard<'_, ConversationState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ConversationState {
    fn reconnecting(&self) -> bool {
        self.reconnection
            .as_ref()
            .is_some_and(|v| v.status == "reconnecting")
    }
    fn verification_active(&self) -> bool {
        self.latest_verification
            .and_then(|id| self.verification_jobs.get(&id))
            .is_some_and(|job| matches!(job.status, "queued" | "running" | "cancelling"))
    }

    fn active_turn(&self) -> Option<&ConversationTurn> {
        self.turns.iter().find(|turn| !turn.status.is_terminal())
    }

    fn turn_mut(&mut self, turn_id: Uuid) -> Option<&mut ConversationTurn> {
        self.turns.iter_mut().find(|turn| turn.turn_id == turn_id)
    }

    fn bump(&mut self) {
        let now = Instant::now();
        for turn in &self.turns {
            let context = turn.execution.as_ref().or_else(|| {
                self.accepted_prompts
                    .get(&turn.turn_id)
                    .filter(|p| p.manual)
                    .and(self.context.as_ref())
            });
            self.plans.observe(PlanTask {
                profile: context.and_then(|c| c.profile.clone()),
                turn_id: turn.turn_id,
                status: turn.status,
                agent: context.map(|c| c.provider_name.clone()),
                elapsed_ms: self
                    .clocks
                    .get(&turn.turn_id)
                    .map(|c| c.snapshot(now, turn.response.chars().count()).elapsed_ms),
            });
        }
        self.sequence = self.sequence.saturating_add(1);
    }

    fn snapshot(&self) -> ConversationSnapshot {
        let now = Instant::now();
        ConversationSnapshot {
            reconnection: self.reconnection.clone(),
            project_summary: self.plans.snapshot().plan.as_ref().map(|plan| {
                crate::project_summary::summarize(
                    plan,
                    &self.verification_jobs,
                    &self.verification_order,
                    self.turns.back().map(|t| t.turn_id),
                    &self
                        .clocks
                        .iter()
                        .map(|(id, c)| (*id, c.snapshot(now, 0).elapsed_ms))
                        .collect(),
                )
            }),
            project_plan: self.plans.snapshot(),
            result_preview: self.result_preview.enabled.then(|| {
                self.result_preview
                    .snapshot(self.turns.back().map(|t| t.turn_id))
            }),
            verification: self.verification_enabled.then(|| VerificationSnapshot {
                active: self.verification_active(),
                latest: self
                    .latest_verification
                    .and_then(|id| self.verification_jobs.get(&id))
                    .map(|job| {
                        let mut view = job.clone();
                        view.stale =
                            self.turns.back().map(|turn| turn.turn_id) != Some(view.turn_id);
                        view
                    }),
            }),
            profile_selection: self.profile_selection.clone(),
            tool_activity: self
                .turns
                .iter()
                .filter_map(|turn| {
                    self.clocks
                        .get(&turn.turn_id)
                        .filter(|clock| !clock.tools.is_empty() || clock.tools_truncated)
                        .map(|clock| {
                            (
                                turn.turn_id,
                                TurnToolActivity {
                                    calls: clock.tools.clone(),
                                    truncated: clock.tools_truncated,
                                },
                            )
                        })
                })
                .collect(),
            turn_metrics: self
                .turns
                .iter()
                .filter_map(|turn| {
                    self.clocks.get(&turn.turn_id).map(|clock| {
                        (
                            turn.turn_id,
                            clock.snapshot(now, turn.response.chars().count()),
                        )
                    })
                })
                .collect(),
            live_controls: self.live_controls.clone(),
            sequence: self.sequence,
            active_turn_id: self.active_turn().map(|turn| turn.turn_id),
            turns: self.turns.iter().cloned().collect(),
            context: self.context.clone(),
        }
    }
}

fn validate_prompt(text: &str) -> Result<(), ConversationSubmitError> {
    if text.trim().is_empty() {
        return Err(ConversationSubmitError::EmptyPrompt);
    }
    if text.chars().count() > MAX_PROMPT_CHARS {
        return Err(ConversationSubmitError::PromptLimit);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnection_is_explicit_revision_bound_and_excludes_other_work() {
        let (hub, mut commands) = ConversationHub::with_context(ConversationContext {
            provider_id: "original".into(),
            provider_name: "Original agent".into(),
            workspace_name: "Project".into(),
            profile: None,
            startup_controls: Some(urdia_core::SessionControlState {
                model: Some("model-a".into()),
                effort: Some("low".into()),
            }),
        });
        assert_eq!(
            hub.request_reconnection(0, true),
            Err(ReconnectionError::Unavailable)
        );
        hub.enable_reconnection();
        assert_eq!(
            hub.request_reconnection(0, false),
            Err(ReconnectionError::Unconfirmed)
        );
        assert_eq!(
            hub.request_reconnection(0, true),
            Err(ReconnectionError::NotFailed)
        );
        let turn = Uuid::new_v4();
        hub.submit_prompt(turn, "first task".into()).unwrap();
        commands.try_recv().unwrap();
        assert_eq!(
            hub.request_reconnection(0, true),
            Err(ReconnectionError::Busy)
        );
        hub.fail(turn, "fixed failure".into()).unwrap();
        let revision = hub.snapshot().reconnection.unwrap().revision;
        assert_eq!(
            hub.request_reconnection(0, true),
            Err(ReconnectionError::Stale)
        );
        hub.enable_verification();
        hub.request_reconnection(revision, true).unwrap();
        hub.request_reconnection(revision, true).unwrap();
        assert_eq!(
            commands.try_recv().unwrap(),
            ConversationCommand::Reconnect { revision }
        );
        assert!(commands.try_recv().is_err());
        assert!(matches!(
            hub.submit_prompt(Uuid::new_v4(), "no race".into()),
            Err(ConversationSubmitError::Busy { .. })
        ));
        assert_eq!(
            hub.select_profile(0, None, true),
            Err(ProfileSelectionError::Busy)
        );
        assert_eq!(
            hub.submit_verification(VerificationRequest {
                contract: VerificationContract::Add,
                request_id: Uuid::new_v4(),
                turn_id: turn,
                confirm_execution: true
            }),
            Err(VerificationError::Busy)
        );
        hub.finish_reconnection(revision + 1, Ok(None));
        assert_eq!(hub.snapshot().reconnection.unwrap().status, "reconnecting");
        hub.finish_reconnection(
            revision,
            Err(urdia_core::ProviderFailureKind::Authentication),
        );
        let failed = hub.snapshot();
        assert_eq!(failed.reconnection.as_ref().unwrap().status, "failed");
        assert_eq!(
            hub.submit_prompt(Uuid::new_v4(), "dead connection".into()),
            Err(ConversationSubmitError::PrimaryDisconnected)
        );
        let retry = failed.reconnection.unwrap().revision;
        hub.request_reconnection(retry, true).unwrap();
        hub.finish_reconnection(retry, Ok(None));
        let ready = hub.snapshot();
        assert_eq!(ready.reconnection.unwrap().context_resets, 1);
        assert_eq!(ready.turns, failed.turns);
        assert!(ready.context.unwrap().startup_controls.is_none());
        assert!(ready.turns[0].original_session);
        assert_eq!(
            ready.turns[0]
                .execution
                .as_ref()
                .unwrap()
                .startup_controls
                .as_ref()
                .unwrap()
                .model
                .as_deref(),
            Some("model-a")
        );
        assert!(ready.project_plan == failed.project_plan);
        hub.submit_prompt(Uuid::new_v4(), "explicit new task".into())
            .unwrap();
    }

    #[test]
    fn fresh_profile_failure_never_requests_original_reconnection_and_queue_failure_is_atomic() {
        let (hub, mut commands) = ConversationHub::new();
        hub.enable_reconnection();
        let turn = Uuid::new_v4();
        hub.submit_recommended(turn, "fresh".into(), "profile-a".into())
            .unwrap();
        commands.try_recv().unwrap();
        hub.fail(turn, "fixed".into()).unwrap();
        assert_eq!(
            hub.request_reconnection(0, true),
            Err(ReconnectionError::NotFailed)
        );
        let original = Uuid::new_v4();
        hub.submit_prompt(original, "original".into()).unwrap();
        commands.try_recv().unwrap();
        hub.fail(original, "fixed".into()).unwrap();
        let before = hub.snapshot();
        drop(commands);
        assert_eq!(
            hub.request_reconnection(before.reconnection.as_ref().unwrap().revision, true),
            Err(ReconnectionError::Queue)
        );
        assert_eq!(before, hub.snapshot());
    }

    fn plan_fixture() -> (
        ConversationHub,
        ConversationCommandReceiver,
        PlanLink,
        String,
    ) {
        let (hub, receiver) = ConversationHub::new();
        hub.mutate_plan(PlanRequest {
            request_id: Uuid::new_v4(),
            revision: 0,
            operation: crate::PlanOperation::Create {
                objective: "PLAN_SECRET".into(),
                steps: vec![crate::PlanStepDefinition {
                    title: "one".into(),
                    scope: "two files".into(),
                    acceptance: "checks".into(),
                }],
            },
        })
        .unwrap();
        let state = hub.lock_state();
        let link = PlanLink {
            plan_id: state.plans.snapshot().plan.unwrap().id,
            revision: 1,
            step_index: 0,
        };
        let prompt = state.plans.prompt(0).unwrap();
        drop(state);
        (hub, receiver, link, prompt)
    }
    #[test]
    fn planned_admission_replays_original_link_and_retains_terminal_evidence() {
        let (hub, mut receiver, link, prompt) = plan_fixture();
        let id = Uuid::new_v4();
        hub.submit_planned_prompt_with_revision(id, prompt.clone(), None, Some(link.clone()))
            .unwrap();
        receiver.try_recv().unwrap();
        hub.submit_planned_prompt_with_revision(id, prompt.clone(), None, Some(link.clone()))
            .unwrap();
        assert!(receiver.try_recv().is_err());
        assert_eq!(
            hub.submit_planned_prompt_with_revision(id, prompt, None, None)
                .unwrap_err(),
            ConversationSubmitError::Conflict { turn_id: id }
        );
        hub.complete(id).unwrap();
        for _ in 0..MAX_HISTORY_TURNS + 2 {
            let next = Uuid::new_v4();
            hub.submit_prompt(next, "other".into()).unwrap();
            receiver.try_recv().unwrap();
            hub.complete(next).unwrap();
        }
        let snapshot = hub.snapshot();
        assert!(!snapshot.turns.iter().any(|t| t.turn_id == id));
        assert!(!format!("{snapshot:?}").contains("PLAN_SECRET"));
        let task = snapshot.project_plan.plan.unwrap().steps[0]
            .task
            .clone()
            .unwrap();
        assert_eq!(task.turn_id, id);
        assert_eq!(task.status, ConversationTurnStatus::Completed);
        hub.mutate_plan(PlanRequest {
            request_id: Uuid::new_v4(),
            revision: 2,
            operation: crate::PlanOperation::Review {
                plan_id: link.plan_id,
                step_index: 0,
                turn_id: id,
                confirm_review: true,
            },
        })
        .unwrap();
    }
    #[test]
    fn rejected_dispatch_never_binds_or_advances_a_plan() {
        let (hub, receiver, link, prompt) = plan_fixture();
        drop(receiver);
        let before = hub.snapshot().project_plan;
        assert_eq!(
            hub.submit_planned_prompt_with_revision(Uuid::new_v4(), prompt, None, Some(link))
                .unwrap_err(),
            ConversationSubmitError::CommandReceiverClosed
        );
        assert!(hub.snapshot().project_plan == before);
        let (hub, _receiver, link, prompt) = plan_fixture();
        for _ in 0..COMMAND_QUEUE_CAPACITY {
            let id = Uuid::new_v4();
            hub.submit_prompt(id, "queued".into()).unwrap();
            hub.complete(id).unwrap();
        }
        let before = hub.snapshot().project_plan;
        assert_eq!(
            hub.submit_planned_prompt_with_revision(Uuid::new_v4(), prompt, None, Some(link))
                .unwrap_err(),
            ConversationSubmitError::CommandQueueFull
        );
        assert!(hub.snapshot().project_plan == before);
    }
    #[test]
    fn two_tabs_cannot_admit_two_tasks_for_the_same_plan_step() {
        for _ in 0..16 {
            let (hub, mut receiver, link, prompt) = plan_fixture();
            let barrier = Arc::new(std::sync::Barrier::new(3));
            let workers: Vec<_> = (0..2)
                .map(|_| {
                    let h = hub.clone();
                    let b = barrier.clone();
                    let l = link.clone();
                    let text = prompt.clone();
                    std::thread::spawn(move || {
                        b.wait();
                        h.submit_planned_prompt_with_revision(Uuid::new_v4(), text, None, Some(l))
                            .is_ok()
                    })
                })
                .collect();
            barrier.wait();
            assert_eq!(
                workers
                    .into_iter()
                    .filter_map(|w| w.join().ok())
                    .filter(|ok| *ok)
                    .count(),
                1
            );
            receiver.try_recv().unwrap();
            assert!(receiver.try_recv().is_err());
            assert_eq!(
                hub.snapshot().project_plan.plan.unwrap().steps[0].attempts,
                1
            );
        }
    }

    #[test]
    fn plan_clear_and_linked_admission_race_has_exactly_one_winner() {
        for _ in 0..16 {
            let (hub, mut receiver, link, prompt) = plan_fixture();
            let barrier = Arc::new(std::sync::Barrier::new(3));
            let h = hub.clone();
            let b = barrier.clone();
            let l = link.clone();
            let clear = std::thread::spawn(move || {
                b.wait();
                h.mutate_plan(PlanRequest {
                    request_id: Uuid::new_v4(),
                    revision: l.revision,
                    operation: crate::PlanOperation::Clear { plan_id: l.plan_id },
                })
                .is_ok()
            });
            let h = hub.clone();
            let b = barrier.clone();
            let send = std::thread::spawn(move || {
                b.wait();
                h.submit_planned_prompt_with_revision(Uuid::new_v4(), prompt, None, Some(link))
                    .is_ok()
            });
            barrier.wait();
            let sent = send.join().unwrap();
            assert_ne!(clear.join().unwrap(), sent);
            assert_eq!(receiver.try_recv().is_ok(), sent);
            assert!(receiver.try_recv().is_err());
        }
    }

    #[tokio::test]
    async fn result_preview_is_confirmed_idempotent_and_never_restarted_by_replay() {
        use crate::{
            ResultPreviewBundle, ResultPreviewError as Error, ResultPreviewLease,
            ResultPreviewRequest,
        };
        let (hub, mut receiver) = ConversationHub::new();
        let turn = Uuid::new_v4();
        let id = Uuid::new_v4();
        let request = ResultPreviewRequest {
            request_id: id,
            turn_id: turn,
            confirm_execution: true,
        };
        assert_eq!(
            hub.submit_result_preview(request.clone()).unwrap_err(),
            Error::Unavailable
        );
        hub.enable_result_preview();
        assert_eq!(
            hub.submit_result_preview(ResultPreviewRequest {
                confirm_execution: false,
                ..request.clone()
            })
            .unwrap_err(),
            Error::Unconfirmed
        );
        assert_eq!(
            hub.submit_result_preview(request.clone()).unwrap_err(),
            Error::Stale
        );
        hub.submit_prompt(turn, "existing web project".into())
            .unwrap();
        receiver.try_recv().unwrap();
        assert_eq!(
            hub.submit_result_preview(request.clone()).unwrap_err(),
            Error::Busy
        );
        hub.complete(turn).unwrap();
        hub.submit_result_preview(request.clone()).unwrap();
        hub.submit_result_preview(request.clone()).unwrap();
        assert_eq!(
            receiver.try_recv().unwrap(),
            ConversationCommand::OpenResult { request_id: id }
        );
        assert!(receiver.try_recv().is_err());
        assert_eq!(
            hub.submit_result_preview(ResultPreviewRequest {
                turn_id: Uuid::new_v4(),
                ..request.clone()
            })
            .unwrap_err(),
            Error::Conflict
        );
        assert!(
            hub.submit_prompt(Uuid::new_v4(), "racing task".into())
                .is_err()
        );
        hub.enable_verification();
        assert_eq!(
            hub.submit_verification(VerificationRequest {
                request_id: Uuid::new_v4(),
                turn_id: turn,
                contract: VerificationContract::FrontendStatic,
                confirm_execution: true
            })
            .unwrap_err(),
            VerificationError::Busy
        );
        assert!(hub.begin_result_preview(id));
        let bundle = ResultPreviewBundle::new(BTreeMap::from([(
            "index.html".into(),
            b"<h1>Snapshot</h1>".to_vec(),
        )]))
        .unwrap();
        hub.finish_result_preview(id, Some(ResultPreviewLease::start(bundle).await.unwrap()));
        let first = hub.snapshot().result_preview.unwrap().latest.unwrap();
        assert_eq!(first.status, "ready");
        assert!(first.url.is_some());
        hub.submit_result_preview(request.clone()).unwrap();
        assert!(receiver.try_recv().is_err());
        let next = Uuid::new_v4();
        hub.submit_prompt(next, "another task".into()).unwrap();
        receiver.try_recv().unwrap();
        assert!(
            !hub.snapshot()
                .result_preview
                .unwrap()
                .latest
                .unwrap()
                .current_task
        );
        hub.stop_result_preview(id).unwrap();
        let stopped = hub
            .submit_result_preview(request)
            .unwrap()
            .result_preview
            .unwrap()
            .latest
            .unwrap();
        assert_eq!(stopped.status, "stopped");
        assert!(stopped.url.is_none());
        assert!(receiver.try_recv().is_err());
        assert!(hub.submit_prompt(id, "reuse preview ID".into()).is_err());
        hub.complete(next).unwrap();
        hub.submit_result_preview(ResultPreviewRequest {
            request_id: Uuid::new_v4(),
            turn_id: next,
            confirm_execution: true,
        })
        .unwrap();
        assert_eq!(
            hub.submit_result_preview(ResultPreviewRequest {
                request_id: id,
                turn_id: turn,
                confirm_execution: true
            })
            .unwrap_err(),
            Error::Stale
        );
    }

    #[test]
    fn stale_host_start_cannot_leave_preview_permanently_queued() {
        let (hub, mut receiver) = ConversationHub::new();
        hub.enable_result_preview();
        let turn = Uuid::new_v4();
        hub.submit_prompt(turn, "seed".into()).unwrap();
        receiver.try_recv().unwrap();
        hub.complete(turn).unwrap();
        let id = Uuid::new_v4();
        hub.submit_result_preview(crate::ResultPreviewRequest {
            request_id: id,
            turn_id: turn,
            confirm_execution: true,
        })
        .unwrap();
        receiver.try_recv().unwrap();
        // Defensive host corruption; ordinary admissions serialize this out.
        hub.lock_state().turns.back_mut().unwrap().status = ConversationTurnStatus::Failed;
        assert!(!hub.begin_result_preview(id));
        let view = hub.snapshot().result_preview.unwrap();
        assert!(!view.preparing);
        assert_eq!(view.latest.unwrap().status, "unavailable");
    }

    #[test]
    fn result_preview_and_new_task_admission_race_has_one_winner() {
        for _ in 0..16 {
            let (hub, mut receiver) = ConversationHub::new();
            hub.enable_result_preview();
            let turn = Uuid::new_v4();
            hub.submit_prompt(turn, "seed".into()).unwrap();
            receiver.try_recv().unwrap();
            hub.complete(turn).unwrap();
            let barrier = Arc::new(std::sync::Barrier::new(3));
            let h = hub.clone();
            let b = barrier.clone();
            let open = std::thread::spawn(move || {
                b.wait();
                h.submit_result_preview(crate::ResultPreviewRequest {
                    request_id: Uuid::new_v4(),
                    turn_id: turn,
                    confirm_execution: true,
                })
                .is_ok()
            });
            let h = hub.clone();
            let b = barrier.clone();
            let task = std::thread::spawn(move || {
                b.wait();
                h.submit_prompt(Uuid::new_v4(), "new task".into()).is_ok()
            });
            barrier.wait();
            assert_ne!(open.join().unwrap(), task.join().unwrap());
            receiver.try_recv().unwrap();
            assert!(receiver.try_recv().is_err());
        }
    }

    #[test]
    fn queued_preview_stop_and_failed_capture_are_terminal_and_bounded() {
        let (hub, mut receiver) = ConversationHub::new();
        hub.enable_result_preview();
        let turn = Uuid::new_v4();
        hub.submit_prompt(turn, "seed".into()).unwrap();
        receiver.try_recv().unwrap();
        hub.complete(turn).unwrap();
        let request = crate::ResultPreviewRequest {
            request_id: Uuid::new_v4(),
            turn_id: turn,
            confirm_execution: true,
        };
        hub.submit_result_preview(request.clone()).unwrap();
        receiver.try_recv().unwrap();
        hub.stop_result_preview(request.request_id).unwrap();
        assert!(!hub.begin_result_preview(request.request_id));
        hub.submit_result_preview(request).unwrap();
        assert!(receiver.try_recv().is_err());
        for _ in 1..16 {
            let id = Uuid::new_v4();
            hub.submit_result_preview(crate::ResultPreviewRequest {
                request_id: id,
                turn_id: turn,
                confirm_execution: true,
            })
            .unwrap();
            receiver.try_recv().unwrap();
            assert!(hub.begin_result_preview(id));
            hub.finish_result_preview(id, None);
        }
        assert_eq!(
            hub.submit_result_preview(crate::ResultPreviewRequest {
                request_id: Uuid::new_v4(),
                turn_id: turn,
                confirm_execution: true
            })
            .unwrap_err(),
            crate::ResultPreviewError::Limit
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn verification_is_confirmed_bound_bounded_and_serialized_with_tasks() {
        let (hub, mut receiver) = ConversationHub::new();
        let turn_id = Uuid::new_v4();
        let request_id = Uuid::new_v4();
        let request = || VerificationRequest {
            contract: VerificationContract::Add,
            request_id,
            turn_id,
            confirm_execution: true,
        };
        assert_eq!(
            hub.submit_verification(request()).unwrap_err(),
            VerificationError::Unavailable
        );
        hub.enable_verification();
        hub.submit_prompt(turn_id, "calculator".into()).unwrap();
        receiver.try_recv().unwrap();
        assert_eq!(
            hub.submit_verification(request()).unwrap_err(),
            VerificationError::Busy
        );
        hub.complete(turn_id).unwrap();
        assert_eq!(
            hub.submit_verification(VerificationRequest {
                contract: VerificationContract::Add,
                request_id: turn_id,
                ..request()
            })
            .unwrap_err(),
            VerificationError::Conflict
        );
        assert_eq!(
            hub.submit_verification(VerificationRequest {
                contract: VerificationContract::Add,
                confirm_execution: false,
                ..request()
            })
            .unwrap_err(),
            VerificationError::Unconfirmed
        );
        assert_eq!(
            hub.submit_verification(VerificationRequest {
                contract: VerificationContract::Add,
                turn_id: Uuid::new_v4(),
                ..request()
            })
            .unwrap_err(),
            VerificationError::Stale
        );
        hub.submit_verification(request()).unwrap();
        hub.submit_verification(request()).unwrap();
        assert_eq!(
            hub.submit_verification(VerificationRequest {
                contract: VerificationContract::Five,
                ..request()
            })
            .unwrap_err(),
            VerificationError::Conflict
        );
        assert!(
            matches!(receiver.try_recv(), Ok(ConversationCommand::Verify { request_id: id, .. }) if id == request_id)
        );
        assert!(receiver.try_recv().is_err());
        assert!(matches!(
            hub.submit_prompt(Uuid::new_v4(), "next".into()),
            Err(ConversationSubmitError::Busy { .. })
        ));
        assert_eq!(
            hub.submit_verification(VerificationRequest {
                contract: VerificationContract::Add,
                turn_id: Uuid::new_v4(),
                ..request()
            })
            .unwrap_err(),
            VerificationError::Conflict
        );
        assert!(hub.begin_verification(request_id));
        hub.cancel_verification(request_id).unwrap();
        hub.finish_verification(request_id, None);
        assert_eq!(
            hub.snapshot().verification.unwrap().latest.unwrap().status,
            "cancelled"
        );
        assert!(matches!(
            hub.submit_prompt(request_id, "collision".into()),
            Err(ConversationSubmitError::Conflict { .. })
        ));
        let next = Uuid::new_v4();
        hub.submit_prompt(next, "next".into()).unwrap();
        assert!(hub.snapshot().verification.unwrap().latest.unwrap().stale);
        // Replaying the old request never rechecks a newer task.
        hub.submit_verification(request()).unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Ok(ConversationCommand::CancelVerification { .. })
        ));
        assert!(matches!(
            receiver.try_recv(),
            Ok(ConversationCommand::Prompt { .. })
        ));
        assert!(receiver.try_recv().is_err());
    }
    #[test]
    fn verification_rejects_valid_evidence_for_a_different_contract() {
        let (hub, mut receiver) = ConversationHub::new();
        hub.enable_verification();
        let turn_id = Uuid::new_v4();
        hub.submit_prompt(turn_id, "five numbers".into()).unwrap();
        receiver.try_recv().unwrap();
        hub.complete(turn_id).unwrap();
        let request_id = Uuid::new_v4();
        hub.submit_verification(VerificationRequest {
            contract: VerificationContract::Five,
            request_id,
            turn_id,
            confirm_execution: true,
        })
        .unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Ok(ConversationCommand::Verify {
                contract: VerificationContract::Five,
                ..
            })
        ));
        assert!(hub.begin_verification(request_id));
        let report = serde_json::from_value(serde_json::json!({
            "schema_version":1,"contract":"calculator-add-v1","status":"passed",
            "files_sha256_before":{"calculator.py":"0".repeat(64),"test_calculator.py":"1".repeat(64)},
            "acceptance_files_unchanged":true,
            "checks":{"host_contract":{"status":"passed","exit_code":0,"elapsed_ms":100},"project_tests":{"status":"passed","exit_code":0,"elapsed_ms":100}}
        })).unwrap();
        hub.finish_verification(request_id, Some(report));
        let job = hub.snapshot().verification.unwrap().latest.unwrap();
        assert_eq!(job.status, "unavailable");
        assert!(job.report.is_none());
        let invalid = serde_json::json!({"request_id": request_id, "turn_id": turn_id,
            "confirm_execution":true, "contract":"arbitrary-command"});
        assert!(serde_json::from_value::<VerificationRequest>(invalid).is_err());
    }

    #[test]
    fn verification_queued_cancel_and_closed_host_fail_without_execution() {
        let (hub, mut receiver) = ConversationHub::new();
        hub.enable_verification();
        let turn_id = Uuid::new_v4();
        hub.submit_prompt(turn_id, "task".into()).unwrap();
        receiver.try_recv().unwrap();
        hub.complete(turn_id).unwrap();
        let id = Uuid::new_v4();
        hub.submit_verification(VerificationRequest {
            contract: VerificationContract::Add,
            request_id: id,
            turn_id,
            confirm_execution: true,
        })
        .unwrap();
        hub.cancel_verification(id).unwrap();
        assert!(!hub.begin_verification(id));
        assert_eq!(
            hub.snapshot().verification.unwrap().latest.unwrap().status,
            "cancelled"
        );
        drop(receiver);
        assert_eq!(
            hub.submit_verification(VerificationRequest {
                contract: VerificationContract::Add,
                request_id: Uuid::new_v4(),
                turn_id,
                confirm_execution: true
            })
            .unwrap_err(),
            VerificationError::HostUnavailable
        );
        assert!(!hub.snapshot().verification.unwrap().active);
    }

    #[test]
    fn verification_and_task_admission_race_has_one_winner_and_jobs_are_bounded() {
        for _ in 0..16 {
            let (hub, mut receiver) = ConversationHub::new();
            hub.enable_verification();
            let turn_id = Uuid::new_v4();
            hub.submit_prompt(turn_id, "task".into()).unwrap();
            receiver.try_recv().unwrap();
            hub.complete(turn_id).unwrap();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
            let h = hub.clone();
            let b = barrier.clone();
            let verify = std::thread::spawn(move || {
                b.wait();
                h.submit_verification(VerificationRequest {
                    contract: VerificationContract::Add,
                    request_id: Uuid::new_v4(),
                    turn_id,
                    confirm_execution: true,
                })
                .is_ok()
            });
            let h = hub.clone();
            let b = barrier.clone();
            let prompt = std::thread::spawn(move || {
                b.wait();
                h.submit_prompt(Uuid::new_v4(), "next".into()).is_ok()
            });
            barrier.wait();
            assert_ne!(verify.join().unwrap(), prompt.join().unwrap());
            receiver.try_recv().unwrap();
            assert!(receiver.try_recv().is_err());
        }
        let (hub, mut receiver) = ConversationHub::new();
        hub.enable_verification();
        let turn_id = Uuid::new_v4();
        hub.submit_prompt(turn_id, "task".into()).unwrap();
        receiver.try_recv().unwrap();
        hub.complete(turn_id).unwrap();
        for _ in 0..16 {
            let id = Uuid::new_v4();
            hub.submit_verification(VerificationRequest {
                contract: VerificationContract::Add,
                request_id: id,
                turn_id,
                confirm_execution: true,
            })
            .unwrap();
            receiver.try_recv().unwrap();
            assert_eq!(
                hub.select_profile(0, None, true).unwrap_err(),
                ProfileSelectionError::Busy
            );
            assert!(hub.begin_verification(id));
            hub.finish_verification(id, None);
        }
        assert_eq!(
            hub.submit_verification(VerificationRequest {
                contract: VerificationContract::Add,
                request_id: Uuid::new_v4(),
                turn_id,
                confirm_execution: true
            })
            .unwrap_err(),
            VerificationError::Limit
        );
        assert_eq!(hub.lock_state().verification_jobs.len(), 16);
    }

    fn selection_fixture() -> (ConversationHub, ConversationCommandReceiver) {
        let (hub, receiver) = ConversationHub::new();
        let choice = ConversationContext {
            provider_id: "agent-b".into(),
            provider_name: "Agent B".into(),
            workspace_name: "Project".into(),
            startup_controls: None,
            profile: Some(ConversationProfile {
                id: "backend".into(),
                name: "Backend".into(),
                agent: "Agent B".into(),
                inference_provider: "Unverified".into(),
                model: "model-b".into(),
                effort: Some("none".into()),
                selection: "manual-next-task".into(),
                model_effort_verified: false,
            }),
        };
        hub.configure_profile_selection(BTreeMap::from([("backend".into(), choice)]));
        (hub, receiver)
    }

    #[test]
    fn profile_switch_and_prompt_admission_cannot_both_win_a_race() {
        for _ in 0..24 {
            let (hub, _commands) = selection_fixture();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
            let first = hub.clone();
            let a = barrier.clone();
            let switch = std::thread::spawn(move || {
                a.wait();
                first.select_profile(0, Some("backend".into()), true)
            });
            let second = hub.clone();
            let b = barrier.clone();
            let prompt = std::thread::spawn(move || {
                b.wait();
                second.submit_prompt_with_revision(Uuid::new_v4(), "task".into(), Some(0))
            });
            barrier.wait();
            let switched = switch.join().unwrap();
            let submitted = prompt.join().unwrap();
            assert!(matches!(
                (switched, submitted),
                (Ok(_), Err(ConversationSubmitError::ProfileRevision))
                    | (Err(ProfileSelectionError::Busy), Ok(_))
            ));
        }
    }

    #[test]
    fn selecting_profile_is_confirmed_revision_bound_and_inert() {
        let (hub, mut commands) = selection_fixture();
        assert_eq!(
            hub.select_profile(0, Some("backend".into()), false)
                .unwrap_err(),
            ProfileSelectionError::Unconfirmed
        );
        assert_eq!(
            hub.select_profile(0, Some("missing".into()), true)
                .unwrap_err(),
            ProfileSelectionError::Unknown
        );
        let applied = hub.select_profile(0, Some("backend".into()), true).unwrap();
        assert_eq!(applied.profile_selection.unwrap().revision, 1);
        assert!(commands.try_recv().is_err());
        assert!(hub.snapshot().turns.is_empty());
        assert_eq!(
            hub.select_profile(0, None, true).unwrap_err(),
            ProfileSelectionError::Stale
        );
        assert!(matches!(
            hub.submit_prompt_with_revision(Uuid::new_v4(), "x".into(), Some(0)),
            Err(ConversationSubmitError::ProfileRevision)
        ));
        assert!(matches!(
            hub.submit_prompt(Uuid::new_v4(), "x".into()),
            Err(ConversationSubmitError::ProfileRevision)
        ));
    }

    #[tokio::test]
    async fn selection_binds_queued_execution_and_replays_after_switch_without_dispatch() {
        let (hub, mut commands) = selection_fixture();
        hub.select_profile(0, Some("backend".into()), true).unwrap();
        let id = Uuid::new_v4();
        let queued = hub
            .submit_prompt_with_revision(id, "task".into(), Some(1))
            .unwrap();
        assert_eq!(
            queued.turns[0].execution.as_ref().unwrap().provider_id,
            "agent-b"
        );
        assert_eq!(
            hub.select_profile(1, None, true).unwrap_err(),
            ProfileSelectionError::Busy
        );
        assert_eq!(
            commands.recv().await,
            Some(ConversationCommand::Recommended {
                turn_id: id,
                text: "task".into(),
                profile_id: "backend".into()
            })
        );
        hub.mark_running(id).unwrap();
        hub.complete(id).unwrap();
        hub.select_profile(1, None, true).unwrap();
        hub.submit_prompt_with_revision(id, "task".into(), Some(1))
            .unwrap();
        assert!(commands.try_recv().is_err());
        assert!(matches!(
            hub.submit_prompt_with_revision(id, "task".into(), Some(2)),
            Err(ConversationSubmitError::Conflict { .. })
        ));
        let original = Uuid::new_v4();
        hub.submit_prompt_with_revision(original, "original".into(), Some(2))
            .unwrap();
        assert_eq!(
            commands.recv().await,
            Some(ConversationCommand::Prompt {
                turn_id: original,
                text: "original".into()
            })
        );
    }

    #[tokio::test]
    async fn explicit_advisory_override_does_not_change_manual_selection() {
        let (hub, mut commands) = selection_fixture();
        hub.select_profile(0, Some("backend".into()), true).unwrap();
        let id = Uuid::new_v4();
        hub.submit_recommended(id, "override".into(), "reviewed-other".into())
            .unwrap();
        assert_eq!(
            commands.recv().await,
            Some(ConversationCommand::Recommended {
                turn_id: id,
                text: "override".into(),
                profile_id: "reviewed-other".into()
            })
        );
        assert_eq!(
            hub.snapshot()
                .profile_selection
                .unwrap()
                .selected_profile
                .as_deref(),
            Some("backend")
        );
        assert!(hub.snapshot().turns[0].execution.is_none());
    }

    #[tokio::test]
    async fn prompt_is_idempotent_and_host_owned() {
        let (hub, mut commands) = ConversationHub::new();
        let turn_id = Uuid::new_v4();
        let first = hub
            .submit_prompt(turn_id, "hello".to_owned())
            .expect("prompt should be accepted");
        assert_eq!(
            commands.recv().await,
            Some(ConversationCommand::Prompt {
                turn_id,
                text: "hello".to_owned()
            })
        );
        let duplicate = hub
            .submit_prompt(turn_id, "hello".to_owned())
            .expect("same prompt should be idempotent");
        assert_eq!(duplicate.sequence, first.sequence);
        assert_eq!(duplicate.turns, first.turns);
        assert!(
            duplicate.turn_metrics[&turn_id].elapsed_ms >= first.turn_metrics[&turn_id].elapsed_ms
        );
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn host_context_is_optional_in_json_and_omitted_from_debug() {
        let (plain, _) = ConversationHub::new();
        assert!(
            serde_json::to_value(plain.snapshot())
                .unwrap()
                .get("context")
                .is_none()
        );
        let context = ConversationContext {
            provider_id: "selected-agent".to_owned(),
            provider_name: "Selected agent".to_owned(),
            workspace_name: "private-project-name".to_owned(),
            profile: None,
            startup_controls: None,
        };
        let (hub, _) = ConversationHub::with_context(context.clone());
        assert!(hub.snapshot().context == Some(context.clone()));
        assert_eq!(
            serde_json::to_value(hub.snapshot()).unwrap()["context"]["workspace_name"],
            context.workspace_name
        );
        assert!(!format!("{:?}", hub.snapshot()).contains("private-project-name"));
        assert!(!format!("{:?}", hub).contains("Selected agent"));
    }

    #[test]
    fn declared_profile_is_authenticated_context_not_debug_or_conversation_content() {
        let (hub, _) = ConversationHub::with_context(ConversationContext {
            provider_id: "profile-launch".into(),
            provider_name: "Launch".into(),
            workspace_name: "Project".into(),
            startup_controls: Some(urdia_core::SessionControlState {
                model: Some("REPORTED_SENTINEL".into()),
                effort: Some("medium".into()),
            }),
            profile: Some(ConversationProfile {
                id: "backend".into(),
                name: "Configured backend".into(),
                agent: "ACP agent".into(),
                inference_provider: "Configured service".into(),
                model: "MODEL_SENTINEL".into(),
                effort: None,
                selection: "default_profile".into(),
                model_effort_verified: false,
            }),
        });
        let snapshot = hub.snapshot();
        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(json["context"]["profile"]["model"], "MODEL_SENTINEL");
        assert_eq!(json["context"]["profile"]["selection"], "default_profile");
        assert_eq!(json["context"]["profile"]["model_effort_verified"], false);
        assert!(json["context"]["profile"].get("effort").is_none());
        assert_eq!(
            json["context"]["startup_controls"]["model"],
            "REPORTED_SENTINEL"
        );
        assert!(!format!("{snapshot:?}").contains("REPORTED_SENTINEL"));
        assert!(snapshot.turns.is_empty());
        assert!(!format!("{snapshot:?}").contains("MODEL_SENTINEL"));
        assert!(!format!("{hub:?}").contains("MODEL_SENTINEL"));
    }

    #[test]
    fn live_controls_are_memory_only_redacted_and_change_snapshot_sequence() {
        let (hub, _) = ConversationHub::new();
        let controls = urdia_core::LiveSessionControls {
            revision: 1,
            current: Some(urdia_core::SessionControlState {
                model: Some("LIVE_SENTINEL".into()),
                effort: None,
            }),
            changed_since_startup: true,
            explicit_configuration_matches: false,
        };
        hub.set_live_controls(Some(controls.clone()));
        assert_eq!(hub.snapshot().sequence, 1);
        hub.set_live_controls(Some(controls));
        assert_eq!(hub.snapshot().sequence, 1);
        assert_eq!(
            serde_json::to_value(hub.snapshot()).unwrap()["live_controls"]["current"]["model"],
            "LIVE_SENTINEL"
        );
        assert!(!format!("{hub:?}").contains("LIVE_SENTINEL"));
        assert!(hub.snapshot().turns.is_empty());
    }

    #[test]
    fn tool_activity_is_deduplicated_bounded_and_ignored_after_turn_end() {
        use urdia_core::{AgentToolActivity, AgentToolKind, AgentToolStatus, AgentUpdate};
        let (hub, _commands) = ConversationHub::new();
        let id = Uuid::new_v4();
        hub.submit_prompt(id, "private".into()).unwrap();
        let event = AgentToolActivity {
            ordinal: 1,
            kind: AgentToolKind::Read,
            status: AgentToolStatus::Pending,
        };
        hub.publish_tool_activity(id, AgentUpdate::ToolActivity(event));
        assert!(hub.snapshot().tool_activity.is_empty());
        hub.mark_running(id).unwrap();
        hub.publish_tool_activity(id, AgentUpdate::ToolActivity(event));
        let sequence = hub.snapshot().sequence;
        hub.publish_tool_activity(id, AgentUpdate::ToolActivity(event));
        assert_eq!(hub.snapshot().sequence, sequence);
        for ordinal in 2..=urdia_core::MAX_TOOL_ACTIVITIES as u16 {
            hub.publish_tool_activity(
                id,
                AgentUpdate::ToolActivity(AgentToolActivity { ordinal, ..event }),
            );
        }
        for ordinal in [0, 129] {
            hub.publish_tool_activity(
                id,
                AgentUpdate::ToolActivity(AgentToolActivity { ordinal, ..event }),
            );
        }
        hub.publish_tool_activity(id, AgentUpdate::ToolActivityLimitReached);
        assert_eq!(hub.snapshot().tool_activity[&id].calls.len(), 128);
        assert!(hub.snapshot().tool_activity[&id].truncated);
        let done = AgentToolActivity {
            status: AgentToolStatus::Completed,
            ..event
        };
        hub.publish_tool_activity(id, AgentUpdate::ToolActivity(done));
        hub.publish_tool_activity(id, AgentUpdate::ToolActivity(event));
        assert_eq!(hub.snapshot().tool_activity[&id].calls[0], done);
        hub.cancel(id).unwrap();
        let before = hub.snapshot();
        hub.publish_tool_activity(
            id,
            AgentUpdate::ToolActivity(AgentToolActivity {
                status: AgentToolStatus::Failed,
                ..event
            }),
        );
        assert_eq!(hub.snapshot(), before);
        let serialized = serde_json::to_value(before).unwrap();
        assert!(
            serialized["tool_activity"][id.to_string()]["calls"][0]
                .get("raw_input")
                .is_none()
        );
    }

    #[test]
    fn metrics_use_monotonic_host_times_and_freeze_at_terminal_state() {
        use std::time::Duration;
        let base = Instant::now();
        let clock = TurnClock {
            accepted: base,
            started: Some(base + Duration::from_millis(100)),
            first_text: Some(base + Duration::from_millis(350)),
            finished: Some(base + Duration::from_millis(900)),
            text_updates: 2,
            tools: Vec::new(),
            tools_truncated: false,
        };
        let metrics = clock.snapshot(base + Duration::from_secs(10), 3);
        assert_eq!(metrics.elapsed_ms, 900);
        assert_eq!(metrics.execution_ms, Some(800));
        assert_eq!(metrics.first_text_ms, Some(250));
        assert_eq!(metrics.text_updates, 2);
        assert_eq!(clock.snapshot(base + Duration::from_secs(20), 3), metrics);
        let queued = TurnClock {
            accepted: base,
            started: None,
            first_text: None,
            finished: None,
            text_updates: 0,
            tools: Vec::new(),
            tools_truncated: false,
        };
        assert_eq!(
            queued
                .snapshot(base + Duration::from_millis(50), 0)
                .elapsed_ms,
            50
        );
        assert!(queued.snapshot(base, 0).execution_ms.is_none());
    }

    #[test]
    fn metrics_ignore_replays_empty_and_late_deltas_and_stop_for_all_terminal_outcomes() {
        for outcome in ["completed", "failed", "cancelled"] {
            let (hub, mut commands) = ConversationHub::new();
            let id = Uuid::new_v4();
            hub.submit_prompt(id, "PRIVATE_PROMPT".into()).unwrap();
            commands.try_recv().unwrap();
            hub.submit_prompt(id, "PRIVATE_PROMPT".into()).unwrap();
            assert!(commands.try_recv().is_err());
            hub.mark_running(id).unwrap();
            hub.mark_running(id).unwrap();
            hub.publish_delta(id, "").unwrap();
            assert!(hub.snapshot().turn_metrics[&id].first_text_ms.is_none());
            hub.publish_delta(id, "ñ🙂").unwrap();
            hub.publish_delta(id, "x").unwrap();
            match outcome {
                "completed" => hub.complete(id).unwrap(),
                "failed" => hub.fail(id, "failed".into()).unwrap(),
                _ => hub.cancel(id).unwrap(),
            }
            let metrics = hub.snapshot().turn_metrics[&id].clone();
            assert_eq!(metrics.text_updates, 2);
            assert_eq!(metrics.response_chars, 3);
            assert!(metrics.first_text_ms.is_some());
            assert!(hub.publish_delta(id, "late").is_err());
            assert_eq!(hub.snapshot().turn_metrics[&id], metrics);
            assert!(!format!("{:?}", hub.snapshot()).contains("PRIVATE_PROMPT"));
        }
        let (hub, _commands) = ConversationHub::new();
        let id = Uuid::new_v4();
        hub.submit_prompt(id, "queued".into()).unwrap();
        hub.cancel(id).unwrap();
        assert!(hub.snapshot().turn_metrics[&id].execution_ms.is_none());
    }

    #[test]
    fn metrics_are_bounded_with_history_and_rolled_back_with_rejected_submission() {
        let (hub, mut commands) = ConversationHub::new();
        for _ in 0..MAX_HISTORY_TURNS + 2 {
            let id = Uuid::new_v4();
            hub.submit_prompt(id, "bounded".into()).unwrap();
            commands.try_recv().unwrap();
            hub.complete(id).unwrap();
        }
        assert_eq!(hub.lock_state().clocks.len(), MAX_HISTORY_TURNS);
        assert_eq!(hub.snapshot().turn_metrics.len(), MAX_HISTORY_TURNS);
        drop(commands);
        let id = Uuid::new_v4();
        assert!(hub.submit_prompt(id, "rejected".into()).is_err());
        assert!(!hub.lock_state().clocks.contains_key(&id));
        assert_eq!(hub.lock_state().clocks.len(), MAX_HISTORY_TURNS);
    }

    #[test]
    fn state_sequence_and_status_only_move_forward() {
        let (hub, _commands) = ConversationHub::new();
        let turn_id = Uuid::new_v4();
        assert_eq!(hub.snapshot().sequence, 0);
        hub.submit_prompt(turn_id, "hello".to_owned())
            .expect("prompt should be accepted");
        assert_eq!(
            hub.snapshot().turns[0].status,
            ConversationTurnStatus::Queued
        );
        hub.mark_running(turn_id).expect("turn should run");
        hub.publish_delta(turn_id, "world")
            .expect("delta should append");
        hub.complete(turn_id).expect("turn should complete");
        assert_eq!(
            hub.snapshot().turns[0],
            ConversationTurn {
                original_session: true,
                execution: None,
                turn_id,
                prompt: "hello".to_owned(),
                response: "world".to_owned(),
                status: ConversationTurnStatus::Completed,
                error: None,
            }
        );
        assert!(matches!(
            hub.publish_delta(turn_id, "late"),
            Err(ConversationUpdateError::InvalidState {
                status: ConversationTurnStatus::Completed,
                ..
            })
        ));
    }

    #[test]
    fn active_turn_and_length_limits_are_enforced() {
        let (hub, _commands) = ConversationHub::new();
        let first = Uuid::new_v4();
        hub.submit_prompt(first, "first".to_owned())
            .expect("first prompt should be accepted");
        let second = Uuid::new_v4();
        assert!(matches!(
            hub.submit_prompt(second, "second".to_owned()),
            Err(ConversationSubmitError::Busy { turn_id }) if turn_id == first
        ));
        assert!(matches!(
            hub.submit_prompt(Uuid::new_v4(), "x".repeat(MAX_PROMPT_CHARS + 1)),
            Err(ConversationSubmitError::PromptLimit)
        ));
        hub.mark_running(first).expect("turn should run");
        assert!(matches!(
            hub.publish_delta(first, &"x".repeat(MAX_RESPONSE_CHARS + 1)),
            Err(ConversationUpdateError::ResponseLimit)
        ));
    }

    #[test]
    fn replay_protection_survives_visible_history_eviction() {
        let (hub, mut commands) = ConversationHub::new();
        let first = Uuid::new_v4();
        for index in 0..=MAX_HISTORY_TURNS {
            let turn_id = if index == 0 { first } else { Uuid::new_v4() };
            let text = format!("turn-{index}");
            hub.submit_prompt(turn_id, text)
                .expect("bounded sequential prompt should be accepted");
            assert!(matches!(
                commands.try_recv(),
                Ok(ConversationCommand::Prompt { .. })
            ));
            hub.mark_running(turn_id).expect("turn should run");
            hub.complete(turn_id).expect("turn should complete");
        }
        assert_eq!(hub.snapshot().turns.len(), MAX_HISTORY_TURNS);
        assert!(
            !hub.snapshot()
                .turns
                .iter()
                .any(|turn| turn.turn_id == first)
        );

        hub.submit_prompt(first, "turn-0".to_owned())
            .expect("replay should return the current snapshot");
        assert!(commands.try_recv().is_err());
        assert!(matches!(
            hub.submit_prompt(first, "changed".to_owned()),
            Err(ConversationSubmitError::Conflict { turn_id }) if turn_id == first
        ));
    }

    #[test]
    fn bounded_command_queue_rejects_without_losing_history() {
        let (hub, _commands) = ConversationHub::new();
        for _ in 0..COMMAND_QUEUE_CAPACITY {
            let turn_id = Uuid::new_v4();
            hub.submit_prompt(turn_id, "queued".to_owned())
                .expect("queue should accept its bounded capacity");
            hub.complete(turn_id).expect("queued turn can complete");
        }
        let before = hub.snapshot();
        let rejected = hub.submit_prompt(Uuid::new_v4(), "overflow".to_owned());
        assert_eq!(rejected, Err(ConversationSubmitError::CommandQueueFull));
        assert_eq!(hub.snapshot().turns, before.turns);
        assert!(hub.snapshot().sequence > before.sequence);
    }

    #[tokio::test]
    async fn cancellation_is_idempotent_and_emits_once() {
        let (hub, mut commands) = ConversationHub::new();
        let turn_id = Uuid::new_v4();
        hub.submit_prompt(turn_id, "hello".to_owned())
            .expect("prompt should be accepted");
        let _ = commands.recv().await;
        hub.request_cancel(turn_id)
            .expect("cancel should be accepted");
        assert_eq!(
            commands.recv().await,
            Some(ConversationCommand::Cancel { turn_id })
        );
        hub.request_cancel(turn_id)
            .expect("duplicate cancel should be idempotent");
        assert!(commands.try_recv().is_err());
        hub.cancel(turn_id)
            .expect("host should confirm cancellation");
        assert_eq!(
            hub.cancel(turn_id),
            Ok(()),
            "host cancellation confirmation should also be idempotent"
        );
    }
}
