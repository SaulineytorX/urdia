//! Stable ACP v1 over stdio for Urdia.
//!
//! This crate owns process transport and protocol handling. It deliberately
//! accepts only an executable plus explicit arguments; it never reads a shell
//! profile or provider config. The child currently inherits the host process
//! environment, as documented in Urdia's security policy.

mod controls;
mod failure;
mod tool_activity;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, InitializeRequest, NewSessionRequest, PermissionOptionKind,
    PromptRequest, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate, TextContent, ToolKind,
};
use agent_client_protocol::{AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo};
use async_trait::async_trait;
use serde_json::Value;
use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard, mpsc, oneshot, watch};
use urdia_core::{
    AgentProvider, AgentSession, AgentUpdate, AgentUpdateSink, AuthenticationState,
    InferenceLocation, PermissionBroker, PermissionCancellationReason, PermissionDecision,
    PermissionLocation, PermissionOperation, PermissionOperationKind, PermissionReceipt,
    PermissionReceiptOutcome, PermissionReceiptSink, PermissionRequest, Prompt, PromptResult,
    ProviderAvailability, ProviderInfo, Result, SensitiveValue, SessionId, SessionRequest,
    TrustedPermissionGrant, UrdiaError,
};

const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(300);
const PERMISSION_DECISION_TIMEOUT: Duration = Duration::from_secs(60);
const COMMAND_QUEUE_SIZE: usize = 8;

/// An executable and explicit arguments for an ACP agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcpCommand {
    command: PathBuf,
    args: Vec<String>,
    startup_timeout: Duration,
    turn_timeout: Duration,
}

impl AcpCommand {
    /// Build a command with bounded default startup and turn timeouts.
    #[must_use]
    pub fn new(command: impl Into<PathBuf>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
            turn_timeout: DEFAULT_TURN_TIMEOUT,
        }
    }

    /// Append one explicit argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Append explicit arguments.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set the maximum time allowed for ACP initialization and session setup.
    #[must_use]
    pub fn startup_timeout(mut self, timeout: Duration) -> Self {
        self.startup_timeout = timeout;
        self
    }

    /// Set the maximum time allowed for one prompt turn.
    #[must_use]
    pub fn turn_timeout(mut self, timeout: Duration) -> Self {
        self.turn_timeout = timeout;
        self
    }

    /// Return the configured executable.
    #[must_use]
    pub fn command(&self) -> &PathBuf {
        &self.command
    }

    /// Return the configured arguments.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.args
    }
}

/// A provider backed by any ACP v1 executable speaking newline-delimited JSON on stdio.
#[derive(Clone)]
pub struct AcpProvider {
    id: String,
    name: String,
    command: AcpCommand,
}

impl std::fmt::Debug for AcpProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AcpProvider")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("command", &self.command)
            .finish_non_exhaustive()
    }
}

impl AcpProvider {
    /// Construct the default generic ACP provider.
    #[must_use]
    pub fn new(command: AcpCommand) -> Self {
        Self::with_metadata("acp", "Agent Client Protocol v1", command)
    }

    /// Construct an ACP provider with an external stable ID and display name.
    ///
    /// The ID is retained as metadata and is validated by
    /// [`urdia_core::ProviderRegistry`] at registration time. The command is
    /// still represented by an executable path and argument vector; no shell
    /// command string is constructed.
    #[must_use]
    pub fn with_metadata(
        id: impl Into<String>,
        name: impl Into<String>,
        command: AcpCommand,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            command,
        }
    }

    /// Return the command configuration.
    #[must_use]
    pub fn command(&self) -> &AcpCommand {
        &self.command
    }

    /// Return the stable provider identifier configured for this adapter.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Return the display name configured for this adapter.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[async_trait]
impl AgentProvider for AcpProvider {
    fn info(&self) -> ProviderInfo {
        let (availability, availability_detail) = executable_metadata(&self.command.command);
        ProviderInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            protocols: vec!["acp-v1".to_owned(), "stdio".to_owned()],
            capabilities: vec![
                "sessions".to_owned(),
                "prompt".to_owned(),
                "cancel".to_owned(),
            ],
            availability,
            availability_detail,
            authentication: AuthenticationState::Unknown,
            inference: InferenceLocation::Unknown,
        }
    }

    async fn start_session(&self, request: SessionRequest) -> Result<Box<dyn AgentSession>> {
        request.controls.validate()?;
        let requested_controls = request.controls.clone();
        let observation = Arc::new(Mutex::new(controls::Observation::default()));
        let id = SessionId::new();
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (ready_tx, ready_rx) = oneshot::channel();
        let worker_command = self.command.clone();
        let active_turn = Arc::new(Mutex::new(None));
        let permission_broker = Arc::clone(request.broker());
        let permission_receipt_sink = Arc::clone(request.receipt_sink());
        let workspace = request.workspace.as_path().to_path_buf();
        let permission_runtime = PermissionRuntime {
            urdia_session_id: id.clone(),
            workspace: request.workspace,
            expected_acp_session: Arc::new(Mutex::new(None)),
            prompt_revision: Arc::new(AtomicU64::new(0)),
            prompt_active: Arc::new(AtomicBool::new(false)),
            response_gate: Arc::new(AsyncMutex::new(())),
            broker: permission_broker,
            receipt_sink: permission_receipt_sink,
            decision_timeout: self.command.turn_timeout.min(PERMISSION_DECISION_TIMEOUT),
        };

        tokio::spawn(run_connection(
            worker_command,
            workspace,
            active_turn,
            permission_runtime,
            command_rx,
            ready_tx,
            requested_controls,
            Arc::clone(&observation),
            shutdown_rx,
        ));

        match tokio::time::timeout(self.command.startup_timeout, ready_rx).await {
            Ok(Ok(Ok(control_state))) => Ok(Box::new(AcpSession {
                control_state,
                observation,
                id,
                command_tx,
                shutdown_tx,
                turn_timeout: self.command.turn_timeout,
            })),
            Ok(Ok(Err(error))) => Err(error),
            Ok(Err(_)) => Err(UrdiaError::Provider(
                "ACP worker stopped before session setup completed".to_owned(),
            )),
            Err(_) => {
                let _ = shutdown_tx.send(true);
                Err(UrdiaError::ProviderFailure(
                    urdia_core::ProviderFailureKind::Timeout,
                ))
            }
        }
    }
}

fn executable_metadata(command: &std::path::Path) -> (ProviderAvailability, Option<String>) {
    match resolve_executable(command) {
        Some(path) => (
            ProviderAvailability::Available,
            Some(format!("executable found at {}", path.display())),
        ),
        None => (
            ProviderAvailability::Unavailable,
            Some(format!("executable not found: {}", command.display())),
        ),
    }
}

fn resolve_executable(command: &std::path::Path) -> Option<PathBuf> {
    if command.components().count() > 1 {
        return is_executable_file(command).then(|| command.to_path_buf());
    }

    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|directory| directory.join(command))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &std::path::Path) -> bool {
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

struct AcpSession {
    observation: Arc<Mutex<controls::Observation>>,
    control_state: Option<urdia_core::SessionControlState>,
    id: SessionId,
    command_tx: mpsc::Sender<SessionCommand>,
    shutdown_tx: watch::Sender<bool>,
    turn_timeout: Duration,
}

#[async_trait]
impl AgentSession for AcpSession {
    fn startup_controls(&self) -> Option<urdia_core::SessionControlState> {
        self.control_state.clone()
    }
    fn live_controls(&self) -> Option<urdia_core::LiveSessionControls> {
        self.observation.lock().ok().map(|state| state.snapshot())
    }
    fn control_options(&self) -> Option<urdia_core::ControlOptions> {
        self.observation
            .lock()
            .ok()
            .and_then(|state| state.options())
    }
    fn id(&self) -> &SessionId {
        &self.id
    }

    async fn prompt(&self, prompt: Prompt) -> Result<PromptResult> {
        self.prompt_inner(prompt, None).await
    }

    async fn prompt_with_updates(
        &self,
        prompt: Prompt,
        sink: Arc<dyn AgentUpdateSink>,
    ) -> Result<PromptResult> {
        self.prompt_inner(prompt, Some(sink)).await
    }

    async fn cancel(&self) -> Result<()> {
        let (response_tx, response_rx) = oneshot::channel();
        let command = SessionCommand::Cancel {
            response: response_tx,
        };
        match tokio::time::timeout(self.turn_timeout, self.command_tx.send(command)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                return Err(UrdiaError::Provider(
                    "ACP worker is no longer running".to_owned(),
                ));
            }
            Err(_) => {
                let _ = self.shutdown_tx.send(true);
                return Err(UrdiaError::ProviderFailure(
                    urdia_core::ProviderFailureKind::Timeout,
                ));
            }
        }
        match tokio::time::timeout(self.turn_timeout, response_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(UrdiaError::Provider(
                "ACP worker dropped cancellation response".to_owned(),
            )),
            Err(_) => {
                let _ = self.shutdown_tx.send(true);
                Err(UrdiaError::ProviderFailure(
                    urdia_core::ProviderFailureKind::Timeout,
                ))
            }
        }
    }
}

impl AcpSession {
    async fn prompt_inner(
        &self,
        prompt: Prompt,
        sink: Option<Arc<dyn AgentUpdateSink>>,
    ) -> Result<PromptResult> {
        let (response_tx, response_rx) = oneshot::channel();
        let command = SessionCommand::Prompt {
            text: prompt.text,
            response: response_tx,
            sink,
        };
        match tokio::time::timeout(self.turn_timeout, self.command_tx.send(command)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                return Err(UrdiaError::Provider(
                    "ACP worker is no longer running".to_owned(),
                ));
            }
            Err(_) => {
                let _ = self.shutdown_tx.send(true);
                return Err(UrdiaError::ProviderFailure(
                    urdia_core::ProviderFailureKind::Timeout,
                ));
            }
        }
        match tokio::time::timeout(self.turn_timeout, response_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(UrdiaError::Provider(
                "ACP worker dropped prompt response".to_owned(),
            )),
            Err(_) => {
                let _ = self.shutdown_tx.send(true);
                Err(UrdiaError::ProviderFailure(
                    urdia_core::ProviderFailureKind::Timeout,
                ))
            }
        }
    }
}

enum SessionCommand {
    Prompt {
        text: String,
        response: oneshot::Sender<Result<PromptResult>>,
        sink: Option<Arc<dyn AgentUpdateSink>>,
    },
    Cancel {
        response: oneshot::Sender<Result<()>>,
    },
}

struct ActivePrompt {
    output: String,
    tools: tool_activity::ToolActivityTracker,
    sink: Option<Arc<dyn AgentUpdateSink>>,
}

type ActivePromptState = Arc<Mutex<Option<ActivePrompt>>>;

#[derive(Clone)]
struct PermissionRuntime {
    urdia_session_id: SessionId,
    workspace: urdia_core::WorkspacePath,
    expected_acp_session: Arc<Mutex<Option<String>>>,
    prompt_revision: Arc<AtomicU64>,
    prompt_active: Arc<AtomicBool>,
    response_gate: Arc<AsyncMutex<()>>,
    broker: Arc<dyn PermissionBroker>,
    receipt_sink: Arc<dyn PermissionReceiptSink>,
    decision_timeout: Duration,
}

struct PermissionResolution {
    outcome: RequestPermissionOutcome,
    _response_guard: Option<OwnedMutexGuard<()>>,
}

impl PermissionResolution {
    fn cancelled() -> Self {
        Self {
            outcome: RequestPermissionOutcome::Cancelled,
            _response_guard: None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_connection(
    command: AcpCommand,
    workspace: PathBuf,
    active_prompt: ActivePromptState,
    permission_runtime: PermissionRuntime,
    mut commands: mpsc::Receiver<SessionCommand>,
    ready: oneshot::Sender<Result<Option<urdia_core::SessionControlState>>>,
    requested_controls: urdia_core::SessionControls,
    observation: Arc<Mutex<controls::Observation>>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let startup_timeout = command.startup_timeout;
    let turn_timeout = command.turn_timeout;
    let agent = AcpAgent::new(AcpAgentConfig::new(command.command).args(command.args));
    let active_prompt_for_updates = Arc::clone(&active_prompt);
    let permission_runtime_for_requests = permission_runtime.clone();
    let observation_for_updates = Arc::clone(&observation);
    let expected_session_for_updates = Arc::clone(&permission_runtime.expected_acp_session);
    let result = Client
        .builder()
        .on_receive_notification(
            async move |notification: SessionNotification, _connection| {
                if let SessionUpdate::ConfigOptionUpdate(update) = &notification.update
                    && let Ok(mut state) = observation_for_updates.lock() {
                        state.update(&notification.session_id.to_string(), &update.config_options);
                }
                let matches_session = expected_session_for_updates.lock().ok()
                    .is_some_and(|expected| expected.as_deref() == Some(notification.session_id.to_string().as_str()));
                if !matches_session { return Ok(()); }
                publish_tool_update(&active_prompt_for_updates, &notification.update);
                if let Some(text) = text_from_update(&notification.update) {
                    publish_text_update(&active_prompt_for_updates, text);
                }
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, connection| {
                let permission_runtime = permission_runtime_for_requests.clone();
                connection.spawn(async move {
                    let resolution =
                        handle_permission_request(request, &permission_runtime).await;
                    responder.respond(RequestPermissionResponse::new(resolution.outcome))?;
                    drop(resolution._response_guard);
                    Ok(())
                })?;
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
            let initialize = match tokio::time::timeout(
                startup_timeout,
                connection
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task(),
            )
            .await
            {
                Ok(Ok(response)) => response,
                Ok(Err(error)) => {
                    let error = failure::sanitize(&error);
                    let message = error.to_string();
                    let _ = ready.send(Err(error));
                    return Err(agent_client_protocol::Error::internal_error().data(message));
                }
                Err(_) => {
                    let message = "ACP initialize timed out".to_owned();
                    let _ = ready.send(Err(UrdiaError::ProviderFailure(urdia_core::ProviderFailureKind::Timeout)));
                    return Err(agent_client_protocol::Error::internal_error().data(message));
                }
            };
            if initialize.protocol_version != ProtocolVersion::V1 {
                let message = "ACP agent did not negotiate protocol v1".to_owned();
                let _ = ready.send(Err(UrdiaError::Protocol(message.clone())));
                return Err(agent_client_protocol::Error::internal_error().data(message));
            }

            let new_session = match tokio::time::timeout(
                startup_timeout,
                connection
                    .send_request(NewSessionRequest::new(workspace))
                    .block_task(),
            )
            .await
            {
                Ok(Ok(response)) => response,
                Ok(Err(error)) => {
                    let error = failure::sanitize(&error);
                    let message = error.to_string();
                    let _ = ready.send(Err(error));
                    return Err(agent_client_protocol::Error::internal_error().data(message));
                }
                Err(_) => {
                    let message = "ACP session creation timed out".to_owned();
                    let _ = ready.send(Err(UrdiaError::ProviderFailure(urdia_core::ProviderFailureKind::Timeout)));
                    return Err(agent_client_protocol::Error::internal_error().data(message));
                }
            };
            if let Ok(mut state) = observation.lock() {
                state.begin(new_session.session_id.to_string(), requested_controls.clone());
            }
            let configuration = tokio::select! {
                result = controls::configure(
                    &connection, &new_session.session_id, new_session.config_options,
                    &requested_controls, startup_timeout,
                ) => result,
                _ = shutdown_rx.changed() => Err(UrdiaError::Provider("ACP configuration cancelled".into())),
            };
            let (control_state, control_options) = match configuration {
                Ok(state) => state,
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return Err(agent_client_protocol::Error::internal_error());
                }
            };
            let confirmation = observation.lock().map_err(|_| controls::invalid()).and_then(|mut state| { state.set_options(control_options); state.finish(control_state.clone()) });
            if let Err(error) = confirmation {
                let _ = ready.send(Err(error));
                return Err(agent_client_protocol::Error::internal_error());
            }
            let _ = ready.send(Ok(control_state));
            let acp_session_id = new_session.session_id;
            if let Ok(mut expected) = permission_runtime.expected_acp_session.lock() {
                *expected = Some(acp_session_id.to_string());
            }

            loop {
                tokio::select! {
                    command = commands.recv() => match command {
                        Some(SessionCommand::Prompt { text, response, sink }) => {
                            activate_prompt(&permission_runtime).await;
                            let guard = observation.lock().map_err(|_| controls::invalid()).and_then(|state| state.check());
                            if let Err(error) = guard {
                                deactivate_prompt(&permission_runtime).await;
                                let _ = response.send(Err(error));
                                continue;
                            }
                            activate_active_prompt(&active_prompt, sink);
                            let mut prompt_response = Some(response);
                            let prompt = connection
                                .send_request(PromptRequest::new(
                                    acp_session_id.clone(),
                                    vec![ContentBlock::Text(TextContent::new(text))],
                                ))
                                .block_task();
                            tokio::pin!(prompt);
                            let turn_deadline = tokio::time::Instant::now() + turn_timeout;
                            let turn_timeout_sleep = tokio::time::sleep_until(turn_deadline);
                            tokio::pin!(turn_timeout_sleep);
                            let prompt_result = loop {
                                tokio::select! {
                                    response = &mut prompt => {
                                        let text = finish_active_prompt(&active_prompt);
                                        break match response {
                                            Ok(response) => {
                                                Some(Ok(PromptResult {
                                                    text,
                                                    stop_reason: Some(format!("{:?}", response.stop_reason)),
                                                }))
                                            }
                                            Err(error) => Some(Err(failure::sanitize(&error))),
                                        };
                                    }
                                    command = commands.recv() => match command {
                                        Some(SessionCommand::Cancel { response }) => {
                                            let cancellation = connection
                                                .send_notification(CancelNotification::new(acp_session_id.clone()))
                                                .map_err(|error| {
                                                    failure::sanitize(&error)
                                                });
                                            let cancellation_failed = cancellation.is_err();
                                            let _ = response.send(cancellation);
                                            let _ = finish_active_prompt(&active_prompt);
                                            deactivate_prompt(&permission_runtime).await;
                                            if let Some(prompt_response) = prompt_response.take() {
                                                let _ = prompt_response.send(Err(UrdiaError::Provider(
                                                    "ACP prompt cancelled".to_owned(),
                                                )));
                                            }
                                            if cancellation_failed {
                                                return Err(agent_client_protocol::Error::internal_error()
                                                    .data("ACP cancellation failed"));
                                            }
                                            match tokio::time::timeout_at(turn_deadline, &mut prompt).await {
                                                Ok(_) => break None,
                                                Err(_) => return Err(agent_client_protocol::Error::internal_error()
                                                    .data("ACP prompt timed out after cancellation")),
                                            }
                                        }
                                        Some(SessionCommand::Prompt { response, .. }) => {
                                            let _ = response.send(Err(UrdiaError::Provider(
                                                "ACP prompt already active".to_owned(),
                                            )));
                                        }
                                        None => {
                                            let _ = finish_active_prompt(&active_prompt);
                                            deactivate_prompt(&permission_runtime).await;
                                            if let Some(prompt_response) = prompt_response.take() {
                                                let _ = prompt_response.send(Err(UrdiaError::Provider(
                                                    "ACP command channel closed".to_owned(),
                                                )));
                                            }
                                            return Ok(());
                                        }
                                    },
                                    changed = shutdown_rx.changed() => {
                                        if changed.is_err() || *shutdown_rx.borrow() {
                                            let _ = finish_active_prompt(&active_prompt);
                                            deactivate_prompt(&permission_runtime).await;
                                            if let Some(prompt_response) = prompt_response.take() {
                                                let _ = prompt_response.send(Err(UrdiaError::Provider(
                                                    "ACP prompt interrupted".to_owned(),
                                                )));
                                            }
                                            return Err(agent_client_protocol::Error::internal_error()
                                                .data("ACP prompt interrupted"));
                                        }
                                    }
                                    _ = &mut turn_timeout_sleep => {
                                        let _ = finish_active_prompt(&active_prompt);
                                        deactivate_prompt(&permission_runtime).await;
                                        if let Some(prompt_response) = prompt_response.take() {
                                                let _ = prompt_response.send(Err(UrdiaError::ProviderFailure(
                                                    urdia_core::ProviderFailureKind::Timeout,
                                                )));
                                        }
                                        return Err(agent_client_protocol::Error::internal_error()
                                            .data("ACP prompt timed out"));
                                    }
                                }
                            };
                            if let Some(prompt_result) = prompt_result {
                                deactivate_prompt(&permission_runtime).await;
                                if let Some(prompt_response) = prompt_response {
                                    let _ = prompt_response.send(prompt_result);
                                }
                            }
                        }
                        Some(SessionCommand::Cancel { response }) => {
                            // Cancellation is idempotent once no prompt is
                            // active. This also closes the race where a turn
                            // finishes between a browser cancellation request
                            // and the host processing its command.
                            let _ = response.send(Ok(()));
                        }
                        None => return Ok(()),
                    },
                    changed = shutdown_rx.changed() => {
                        if changed.is_err() || *shutdown_rx.borrow() {
                            return Err(agent_client_protocol::Error::internal_error()
                                .data("ACP worker shutdown requested"));
                        }
                    }
                }
            }
        })
        .await;

    if let Err(error) = result {
        // A setup error is sent through `ready`; once a session is running the
        // command response receives the provider error instead. No child output
        // or environment values are included in this diagnostic.
        let _ = error;
    }
}

fn activate_active_prompt(state: &ActivePromptState, sink: Option<Arc<dyn AgentUpdateSink>>) {
    if let Ok(mut active_prompt) = state.lock() {
        *active_prompt = Some(ActivePrompt {
            output: String::new(),
            tools: tool_activity::ToolActivityTracker::default(),
            sink,
        });
    }
}

fn publish_tool_update(state: &ActivePromptState, update: &SessionUpdate) {
    if let Ok(mut active) = state.lock()
        && let Some(active) = active.as_mut()
        && let Some(update) = active.tools.observe(update)
        && let Some(sink) = active.sink.as_ref()
    {
        sink.publish(update);
    }
}

fn publish_text_update(state: &ActivePromptState, text: String) {
    if let Ok(mut active_prompt) = state.lock()
        && let Some(active_prompt) = active_prompt.as_mut()
    {
        active_prompt.output.push_str(&text);
        if let Some(sink) = active_prompt.sink.as_ref() {
            sink.publish(AgentUpdate::TextDelta { text });
        }
    }
}

fn finish_active_prompt(state: &ActivePromptState) -> String {
    state
        .lock()
        .ok()
        .and_then(|mut active_prompt| active_prompt.take())
        .map(|active_prompt| active_prompt.output)
        .unwrap_or_default()
}

async fn activate_prompt(runtime: &PermissionRuntime) {
    let _gate = runtime.response_gate.lock().await;
    runtime.prompt_revision.fetch_add(1, Ordering::AcqRel);
    runtime.prompt_active.store(true, Ordering::Release);
}

async fn deactivate_prompt(runtime: &PermissionRuntime) {
    let _gate = runtime.response_gate.lock().await;
    runtime.prompt_active.store(false, Ordering::Release);
}

async fn handle_permission_request(
    request: RequestPermissionRequest,
    runtime: &PermissionRuntime,
) -> PermissionResolution {
    let request_session_id = request.session_id.to_string();
    let revision = runtime.prompt_revision.load(Ordering::Acquire);
    let permission_request = PermissionRequest {
        id: uuid::Uuid::new_v4(),
        session_id: runtime.urdia_session_id.clone(),
        provider_session_id: request_session_id.clone(),
        workspace: runtime.workspace.clone(),
        revision,
        operation: PermissionOperation {
            tool_call_id: request.tool_call.tool_call_id.to_string(),
            title: request.tool_call.fields.title,
            name: request.tool_call.fields.name,
            kind: permission_kind(request.tool_call.fields.kind),
            arguments: request.tool_call.fields.raw_input.map(SensitiveValue::new),
            locations: request
                .tool_call
                .fields
                .locations
                .unwrap_or_default()
                .into_iter()
                .map(|location| PermissionLocation {
                    path: location.path,
                    line: location.line,
                })
                .collect(),
        },
    };

    let expected = runtime
        .expected_acp_session
        .lock()
        .ok()
        .and_then(|value| value.clone());
    if expected.as_deref() != Some(request_session_id.as_str()) {
        let _ = persist_receipt(
            runtime,
            PermissionReceipt::from_request(
                &permission_request,
                PermissionReceiptOutcome::Cancelled,
                Some(PermissionCancellationReason::SessionMismatch),
            ),
        )
        .await;
        return PermissionResolution::cancelled();
    }
    if revision == 0 || !runtime.prompt_active.load(Ordering::Acquire) {
        let _ = persist_receipt(
            runtime,
            PermissionReceipt::from_request(
                &permission_request,
                PermissionReceiptOutcome::Cancelled,
                Some(PermissionCancellationReason::NoActivePrompt),
            ),
        )
        .await;
        return PermissionResolution::cancelled();
    }

    let decision = match tokio::time::timeout(
        runtime.decision_timeout,
        runtime.broker.authorize(permission_request.clone()),
    )
    .await
    {
        Ok(Ok(decision)) => decision,
        Ok(Err(_)) => {
            let _ = persist_receipt(
                runtime,
                PermissionReceipt::from_request(
                    &permission_request,
                    PermissionReceiptOutcome::Cancelled,
                    Some(PermissionCancellationReason::BrokerError),
                ),
            )
            .await;
            return PermissionResolution::cancelled();
        }
        Err(_) => {
            let _ = persist_receipt(
                runtime,
                PermissionReceipt::from_request(
                    &permission_request,
                    PermissionReceiptOutcome::Cancelled,
                    Some(PermissionCancellationReason::BrokerTimeout),
                ),
            )
            .await;
            return PermissionResolution::cancelled();
        }
    };
    if runtime.prompt_revision.load(Ordering::Acquire) != revision
        || !runtime.prompt_active.load(Ordering::Acquire)
    {
        let _ = persist_receipt(
            runtime,
            PermissionReceipt::from_request(
                &permission_request,
                PermissionReceiptOutcome::Cancelled,
                Some(PermissionCancellationReason::StaleRevision),
            ),
        )
        .await;
        return PermissionResolution::cancelled();
    }
    let (desired_kind, receipt_outcome, trusted_grant) = match decision {
        PermissionDecision::AllowOnce => (
            PermissionOptionKind::AllowOnce,
            PermissionReceiptOutcome::AllowedOnce,
            None,
        ),
        PermissionDecision::RejectOnce => (
            PermissionOptionKind::RejectOnce,
            PermissionReceiptOutcome::RejectedOnce,
            None,
        ),
        PermissionDecision::AllowTrusted {
            policy_id,
            policy_revision,
        } => (
            // A trusted-session decision still selects ACP's one-shot option.
            // The policy provenance never crosses the protocol boundary.
            PermissionOptionKind::AllowOnce,
            PermissionReceiptOutcome::AllowedOnce,
            Some(TrustedPermissionGrant::new(policy_id, policy_revision)),
        ),
    };
    if let Some(option) = request
        .options
        .into_iter()
        .find(|option| option.kind == desired_kind)
    {
        let receipt = trusted_grant.as_ref().map_or_else(
            || PermissionReceipt::from_request(&permission_request, receipt_outcome, None),
            |grant| {
                PermissionReceipt::from_trusted_request(
                    &permission_request,
                    receipt_outcome,
                    None,
                    grant,
                )
            },
        );
        if !persist_receipt(runtime, receipt).await {
            return PermissionResolution::cancelled();
        }
        let response_guard = Arc::clone(&runtime.response_gate).lock_owned().await;
        if runtime.prompt_revision.load(Ordering::Acquire) != revision
            || !runtime.prompt_active.load(Ordering::Acquire)
        {
            drop(response_guard);
            let _ = persist_receipt(
                runtime,
                PermissionReceipt::from_request(
                    &permission_request,
                    PermissionReceiptOutcome::Cancelled,
                    Some(PermissionCancellationReason::StaleRevision),
                ),
            )
            .await;
            return PermissionResolution::cancelled();
        }
        if let Some(grant) = trusted_grant.as_ref() {
            let revalidated = tokio::time::timeout(
                runtime.decision_timeout,
                runtime
                    .broker
                    .revalidate_trusted(&permission_request, grant),
            )
            .await;
            if !matches!(revalidated, Ok(Ok(()))) {
                drop(response_guard);
                let _ = persist_receipt(
                    runtime,
                    PermissionReceipt::from_trusted_request(
                        &permission_request,
                        PermissionReceiptOutcome::Cancelled,
                        Some(PermissionCancellationReason::TrustedAuthorizationInvalid),
                        grant,
                    ),
                )
                .await;
                return PermissionResolution::cancelled();
            }
        }
        return PermissionResolution {
            outcome: RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                option.option_id,
            )),
            _response_guard: Some(response_guard),
        };
    }
    let _ = persist_receipt(
        runtime,
        PermissionReceipt::from_request(
            &permission_request,
            PermissionReceiptOutcome::Cancelled,
            Some(PermissionCancellationReason::CompatibleOptionMissing),
        ),
    )
    .await;
    PermissionResolution::cancelled()
}

async fn persist_receipt(runtime: &PermissionRuntime, receipt: PermissionReceipt) -> bool {
    matches!(
        tokio::time::timeout(
            runtime.decision_timeout,
            runtime.receipt_sink.record(receipt),
        )
        .await,
        Ok(Ok(()))
    )
}

fn permission_kind(kind: Option<ToolKind>) -> PermissionOperationKind {
    match kind.unwrap_or(ToolKind::Other) {
        ToolKind::Read => PermissionOperationKind::Read,
        ToolKind::Edit => PermissionOperationKind::Edit,
        ToolKind::Delete => PermissionOperationKind::Delete,
        ToolKind::Move => PermissionOperationKind::Move,
        ToolKind::Search => PermissionOperationKind::Search,
        ToolKind::Execute => PermissionOperationKind::Execute,
        ToolKind::Think => PermissionOperationKind::Think,
        ToolKind::Fetch => PermissionOperationKind::Fetch,
        ToolKind::SwitchMode => PermissionOperationKind::SwitchMode,
        _ => PermissionOperationKind::Other,
    }
}

fn text_from_update(update: &SessionUpdate) -> Option<String> {
    let value = serde_json::to_value(update).ok()?;
    if value.get("sessionUpdate")?.as_str()? != "agent_message_chunk" {
        return None;
    }
    value
        .get("content")
        .and_then(|content| content.get("text"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        PermissionOption, ToolCallLocation, ToolCallUpdate, ToolCallUpdateFields,
    };
    use async_trait::async_trait;
    use urdia_core::{
        DenyAllPermissionBroker, DiscardPermissionReceiptSink, PermissionAuthorizationSource,
        PermissionBroker, PermissionDecision, PermissionReceiptSink, WorkspacePath,
    };

    struct FixedBroker {
        decision: PermissionDecision,
        seen: Mutex<Vec<PermissionRequest>>,
    }

    #[derive(Default)]
    struct UpdateSink(Mutex<Vec<AgentUpdate>>);

    impl AgentUpdateSink for UpdateSink {
        fn publish(&self, update: AgentUpdate) {
            self.0.lock().expect("update lock").push(update);
        }
    }

    #[async_trait]
    impl PermissionBroker for FixedBroker {
        async fn authorize(&self, request: PermissionRequest) -> Result<PermissionDecision> {
            self.seen.lock().expect("broker lock").push(request);
            Ok(self.decision.clone())
        }
    }

    struct RevocableTrustedBroker {
        policy_id: uuid::Uuid,
        policy_revision: u64,
        revoked: AtomicBool,
    }

    #[async_trait]
    impl PermissionBroker for RevocableTrustedBroker {
        async fn authorize(&self, _request: PermissionRequest) -> Result<PermissionDecision> {
            Ok(PermissionDecision::allow_trusted(
                self.policy_id,
                self.policy_revision,
            ))
        }

        async fn revalidate_trusted(
            &self,
            _request: &PermissionRequest,
            grant: &urdia_core::TrustedPermissionGrant,
        ) -> Result<()> {
            assert_eq!(grant.policy_id(), self.policy_id);
            assert_eq!(grant.policy_revision(), self.policy_revision);
            if self.revoked.load(Ordering::Acquire) {
                return Err(UrdiaError::PermissionDenied(
                    "trusted policy revoked during authorization".to_owned(),
                ));
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingSink(Mutex<Vec<PermissionReceipt>>);

    #[async_trait]
    impl PermissionReceiptSink for RecordingSink {
        async fn record(&self, receipt: PermissionReceipt) -> Result<()> {
            self.0.lock().expect("receipt lock").push(receipt);
            Ok(())
        }
    }

    struct FailingSink;

    #[async_trait]
    impl PermissionReceiptSink for FailingSink {
        async fn record(&self, _receipt: PermissionReceipt) -> Result<()> {
            Err(UrdiaError::Store("test receipt failure".to_owned()))
        }
    }

    struct BlockingSink {
        block_first: AtomicBool,
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
        receipts: Mutex<Vec<PermissionReceipt>>,
    }

    impl Default for BlockingSink {
        fn default() -> Self {
            Self {
                block_first: AtomicBool::new(true),
                entered: tokio::sync::Notify::new(),
                release: tokio::sync::Notify::new(),
                receipts: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl PermissionReceiptSink for BlockingSink {
        async fn record(&self, receipt: PermissionReceipt) -> Result<()> {
            if self.block_first.swap(false, Ordering::AcqRel) {
                self.entered.notify_one();
                self.release.notified().await;
            }
            self.receipts.lock().expect("receipt lock").push(receipt);
            Ok(())
        }
    }

    fn permission_request(options: Vec<PermissionOption>) -> RequestPermissionRequest {
        RequestPermissionRequest::new(
            "acp-session",
            ToolCallUpdate::new(
                "tool-1",
                ToolCallUpdateFields::new()
                    .title("Run command")
                    .name("shell")
                    .kind(ToolKind::Execute)
                    .raw_input(serde_json::json!({"token": "SECRET_SENTINEL"}))
                    .locations(vec![ToolCallLocation::new("/tmp/example").line(9)]),
            ),
            options,
        )
    }

    fn permission_runtime(
        broker: Arc<dyn PermissionBroker>,
        sink: Arc<dyn PermissionReceiptSink>,
    ) -> PermissionRuntime {
        PermissionRuntime {
            urdia_session_id: SessionId::new(),
            workspace: WorkspacePath::new(std::env::current_dir().expect("current directory"))
                .expect("workspace"),
            expected_acp_session: Arc::new(Mutex::new(Some("acp-session".to_owned()))),
            prompt_revision: Arc::new(AtomicU64::new(4)),
            prompt_active: Arc::new(AtomicBool::new(true)),
            response_gate: Arc::new(AsyncMutex::new(())),
            broker,
            receipt_sink: sink,
            decision_timeout: Duration::from_secs(1),
        }
    }

    #[test]
    fn extracts_only_agent_text_chunks() {
        let update =
            SessionUpdate::AgentMessageChunk(agent_client_protocol::schema::v1::ContentChunk::new(
                ContentBlock::Text(TextContent::new("hello")),
            ));
        assert_eq!(text_from_update(&update).as_deref(), Some("hello"));
    }

    #[test]
    fn active_prompt_drops_late_chunks_and_switches_sinks_per_turn() {
        let state = Arc::new(Mutex::new(None));
        let first_sink = Arc::new(UpdateSink::default());
        activate_active_prompt(&state, Some(first_sink.clone()));
        publish_text_update(&state, "first".to_owned());
        assert_eq!(finish_active_prompt(&state), "first");

        publish_text_update(&state, "late".to_owned());
        assert_eq!(first_sink.0.lock().expect("update lock").len(), 1);

        let second_sink = Arc::new(UpdateSink::default());
        activate_active_prompt(&state, Some(second_sink.clone()));
        publish_text_update(&state, "second".to_owned());
        assert_eq!(finish_active_prompt(&state), "second");
        assert_eq!(first_sink.0.lock().expect("update lock").len(), 1);
        assert_eq!(second_sink.0.lock().expect("update lock").len(), 1);
    }

    #[test]
    fn command_stores_no_explicit_environment_overrides() {
        let command = AcpCommand::new("qwen").arg("--acp");
        assert_eq!(command.command(), &PathBuf::from("qwen"));
        assert_eq!(command.arguments(), &["--acp".to_owned()]);
    }

    #[test]
    fn external_metadata_is_conservative_and_does_not_spawn() {
        let provider = AcpProvider::with_metadata(
            "external-agent",
            "External Agent",
            AcpCommand::new("/definitely/not/an-agent").arg("--acp"),
        );
        let info = provider.info();
        assert_eq!(info.id, "external-agent");
        assert_eq!(info.name, "External Agent");
        assert_eq!(
            info.availability,
            ProviderAvailability::Unavailable,
            "metadata should only inspect the executable path"
        );
        assert_eq!(info.authentication, AuthenticationState::Unknown);
        assert_eq!(info.inference, InferenceLocation::Unknown);
        assert_eq!(
            info.availability_detail.as_deref(),
            Some("executable not found: /definitely/not/an-agent")
        );
    }

    #[tokio::test]
    async fn default_broker_is_deny_all() {
        let sink = Arc::new(RecordingSink::default());
        let runtime = permission_runtime(Arc::new(DenyAllPermissionBroker), sink.clone());
        let outcome = handle_permission_request(
            permission_request(vec![PermissionOption::new(
                "reject-once",
                "Reject once",
                PermissionOptionKind::RejectOnce,
            )]),
            &runtime,
        )
        .await;
        let RequestPermissionOutcome::Selected(selected) = outcome.outcome else {
            panic!("deny-all should select reject-once when available");
        };
        assert_eq!(selected.option_id.to_string(), "reject-once");
        assert_eq!(
            sink.0.lock().expect("receipt lock")[0].outcome,
            PermissionReceiptOutcome::RejectedOnce
        );
    }

    #[tokio::test]
    async fn allow_once_selects_only_matching_option_and_redacts_receipt() {
        let broker = Arc::new(FixedBroker {
            decision: PermissionDecision::AllowOnce,
            seen: Mutex::new(Vec::new()),
        });
        let sink = Arc::new(RecordingSink::default());
        let runtime = permission_runtime(broker.clone(), sink.clone());
        let outcome = handle_permission_request(
            permission_request(vec![
                PermissionOption::new(
                    "allow-always",
                    "Allow always",
                    PermissionOptionKind::AllowAlways,
                ),
                PermissionOption::new("allow-once", "Allow once", PermissionOptionKind::AllowOnce),
            ]),
            &runtime,
        )
        .await;
        let RequestPermissionOutcome::Selected(selected) = outcome.outcome else {
            panic!("allow-once should select an ACP option");
        };
        assert_eq!(selected.option_id.to_string(), "allow-once");
        let seen = broker.seen.lock().expect("broker lock");
        assert_eq!(seen[0].session_id, runtime.urdia_session_id);
        assert_eq!(seen[0].workspace, runtime.workspace);
        assert_eq!(seen[0].revision, 4);
        assert_eq!(
            seen[0]
                .operation
                .arguments
                .as_ref()
                .expect("raw arguments")
                .expose()["token"],
            "SECRET_SENTINEL"
        );
        drop(seen);
        let receipts = sink.0.lock().expect("receipt lock");
        assert_eq!(receipts[0].outcome, PermissionReceiptOutcome::AllowedOnce);
        assert!(
            !serde_json::to_string(&receipts[0])
                .expect("serialize receipt")
                .contains("SECRET_SENTINEL")
        );
    }

    #[tokio::test]
    async fn trusted_allow_selects_only_allow_once_and_records_provenance() {
        let broker = Arc::new(FixedBroker {
            decision: PermissionDecision::allow_trusted(uuid::Uuid::new_v4(), 9),
            seen: Mutex::new(Vec::new()),
        });
        let sink = Arc::new(RecordingSink::default());
        let runtime = permission_runtime(broker, sink.clone());
        let outcome = handle_permission_request(
            permission_request(vec![
                PermissionOption::new(
                    "allow-always",
                    "Allow always",
                    PermissionOptionKind::AllowAlways,
                ),
                PermissionOption::new("allow-once", "Allow once", PermissionOptionKind::AllowOnce),
            ]),
            &runtime,
        )
        .await;
        let RequestPermissionOutcome::Selected(selected) = outcome.outcome else {
            panic!("trusted allow should select a one-shot ACP option");
        };
        assert_eq!(selected.option_id.to_string(), "allow-once");
        let receipts = sink.0.lock().expect("receipt lock");
        assert_eq!(receipts.len(), 1);
        assert_eq!(
            receipts[0].authorization,
            PermissionAuthorizationSource::TrustedSession
        );
        assert!(receipts[0].policy_id_fingerprint.is_some());
        assert_eq!(receipts[0].policy_revision, Some(9));
    }

    #[tokio::test]
    async fn persistent_options_and_receipt_failures_never_allow() {
        let broker = Arc::new(FixedBroker {
            decision: PermissionDecision::AllowOnce,
            seen: Mutex::new(Vec::new()),
        });
        let sink = Arc::new(RecordingSink::default());
        let runtime = permission_runtime(broker.clone(), sink.clone());
        let persistent_only = handle_permission_request(
            permission_request(vec![PermissionOption::new(
                "allow-always",
                "Allow always",
                PermissionOptionKind::AllowAlways,
            )]),
            &runtime,
        )
        .await;
        assert_eq!(persistent_only.outcome, RequestPermissionOutcome::Cancelled);
        assert_eq!(
            sink.0.lock().expect("receipt lock")[0].cancellation_reason,
            Some(PermissionCancellationReason::CompatibleOptionMissing)
        );

        let failing_runtime = permission_runtime(broker.clone(), Arc::new(FailingSink));
        let persistence_failure = handle_permission_request(
            permission_request(vec![PermissionOption::new(
                "allow-once",
                "Allow once",
                PermissionOptionKind::AllowOnce,
            )]),
            &failing_runtime,
        )
        .await;
        assert_eq!(
            persistence_failure.outcome,
            RequestPermissionOutcome::Cancelled
        );

        let default_sink_runtime =
            permission_runtime(broker, Arc::new(DiscardPermissionReceiptSink));
        let missing_durable_sink = handle_permission_request(
            permission_request(vec![PermissionOption::new(
                "allow-once",
                "Allow once",
                PermissionOptionKind::AllowOnce,
            )]),
            &default_sink_runtime,
        )
        .await;
        assert_eq!(
            missing_durable_sink.outcome,
            RequestPermissionOutcome::Cancelled
        );
    }

    #[tokio::test]
    async fn prompt_end_while_receipt_is_pending_cancels_allow_once() {
        let broker = Arc::new(FixedBroker {
            decision: PermissionDecision::AllowOnce,
            seen: Mutex::new(Vec::new()),
        });
        let sink = Arc::new(BlockingSink::default());
        let runtime = permission_runtime(broker, sink.clone());
        let worker_runtime = runtime.clone();
        let pending = tokio::spawn(async move {
            handle_permission_request(
                permission_request(vec![PermissionOption::new(
                    "allow-once",
                    "Allow once",
                    PermissionOptionKind::AllowOnce,
                )]),
                &worker_runtime,
            )
            .await
        });

        sink.entered.notified().await;
        deactivate_prompt(&runtime).await;
        sink.release.notify_one();
        let resolution = pending.await.expect("permission task should join");
        assert_eq!(resolution.outcome, RequestPermissionOutcome::Cancelled);
        let receipts = sink.receipts.lock().expect("receipt lock");
        assert_eq!(receipts.len(), 2);
        assert_eq!(
            receipts[1].cancellation_reason,
            Some(PermissionCancellationReason::StaleRevision)
        );
    }

    #[tokio::test]
    async fn trusted_revocation_during_blocking_receipt_revalidation_fails_closed() {
        let broker = Arc::new(RevocableTrustedBroker {
            policy_id: uuid::Uuid::new_v4(),
            policy_revision: 12,
            revoked: AtomicBool::new(false),
        });
        let sink = Arc::new(BlockingSink::default());
        let runtime = permission_runtime(broker.clone(), sink.clone());
        let worker_runtime = runtime.clone();
        let pending = tokio::spawn(async move {
            handle_permission_request(
                permission_request(vec![PermissionOption::new(
                    "allow-once",
                    "Allow once",
                    PermissionOptionKind::AllowOnce,
                )]),
                &worker_runtime,
            )
            .await
        });

        sink.entered.notified().await;
        broker.revoked.store(true, Ordering::Release);
        sink.release.notify_one();

        let resolution = pending.await.expect("permission task should join");
        assert_eq!(resolution.outcome, RequestPermissionOutcome::Cancelled);
        let receipts = sink.receipts.lock().expect("receipt lock");
        assert_eq!(receipts.len(), 2);
        assert_eq!(
            receipts[0].authorization,
            PermissionAuthorizationSource::TrustedSession
        );
        assert_eq!(receipts[0].policy_revision, Some(12));
        assert_eq!(
            receipts[1].cancellation_reason,
            Some(PermissionCancellationReason::TrustedAuthorizationInvalid)
        );
        assert_eq!(
            receipts[1].authorization,
            PermissionAuthorizationSource::TrustedSession
        );
    }

    #[tokio::test]
    async fn mismatched_provider_session_never_reaches_the_broker() {
        let broker = Arc::new(FixedBroker {
            decision: PermissionDecision::AllowOnce,
            seen: Mutex::new(Vec::new()),
        });
        let sink = Arc::new(RecordingSink::default());
        let runtime = permission_runtime(broker.clone(), sink.clone());
        *runtime.expected_acp_session.lock().expect("session lock") =
            Some("different-session".to_owned());
        let outcome = handle_permission_request(
            permission_request(vec![PermissionOption::new(
                "allow-once",
                "Allow once",
                PermissionOptionKind::AllowOnce,
            )]),
            &runtime,
        )
        .await;

        assert_eq!(outcome.outcome, RequestPermissionOutcome::Cancelled);
        assert!(broker.seen.lock().expect("broker lock").is_empty());
        assert_eq!(
            sink.0.lock().expect("receipt lock")[0].cancellation_reason,
            Some(PermissionCancellationReason::SessionMismatch)
        );
    }
}
