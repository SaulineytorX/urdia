//! Static and live read-only, privacy-filtered timeline surfaces for Urdia.

mod advisory;
mod catalog;
pub use catalog::{
    AgentCatalogSnapshot, AgentDraft, CatalogError, CatalogProvider, CatalogSnapshot,
    ControlProbeError, ControlProbeRequest, ControlProbeResult, ProfileDraft, RegisterAgentRequest,
    RegisteredAgentView, SaveProfileRequest,
};
mod verification;
pub use verification::{
    FrontendCounts, UnittestCounts, VerificationCheck, VerificationContract, VerificationError,
    VerificationJob, VerificationReport, VerificationRequest, VerificationSnapshot,
};
mod plans;
pub use plans::{
    ArchivedStep, ArchivedTask, MAX_PLAN_ARCHIVE_BYTES, PlanArchive, PlanDraft, PlanError,
    PlanLink, PlanOperation, PlanRecovery, PlanRequest, PlanSnapshot, PlanStep, PlanStepDefinition,
    PlanTask, ProjectPlan,
};
mod project_summary;
pub use project_summary::{ProjectSummary, StepSummary, StepVerification};
mod conversation;
pub use advisory::{
    AdvisoryEngine, AdvisoryHub, AdvisoryProvider, AdvisoryRequest, AdvisoryResult,
    AdvisorySubmitError, ConfirmExecutionRequest, ExecutionError, ExecutionPreview,
    ExecutionTarget, JevConnectionError, JevConnectionRequest, JevConnectionView,
    LayaConnectionError, LayaConnectionRequest, LayaConnectionView, PrepareExecutionRequest,
};
mod permissions;

pub use conversation::{
    COMMAND_QUEUE_CAPACITY, ConversationCommand, ConversationCommandReceiver, ConversationContext,
    ConversationHub, ConversationProfile, ConversationSnapshot, ConversationSubmitError,
    ConversationTurn, ConversationTurnStatus, ConversationUpdateError, MAX_HISTORY_TURNS,
    MAX_PROMPT_CHARS, MAX_RESPONSE_CHARS, MAX_SESSION_TURNS, ProfileSelectionError,
    ProfileSelectionSnapshot, ReconnectionError, ReconnectionSnapshot, TurnMetrics,
    TurnToolActivity,
};
pub use permissions::{
    InteractivePermissionBroker, PendingPermissionView, PermissionResolveError,
    TRUSTED_SESSION_CAPABILITIES, TRUSTED_SESSION_TTL_SECONDS, TrustedSessionChallenge,
    TrustedSessionError, TrustedSessionSnapshot, TrustedSessionStatus,
};

use axum::Json;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{DefaultBodyLimit, Path as AxumPath, State};
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Write;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::net::TcpListener;
use urdia_core::{
    EventKind, EventStore, PermissionDecision, StoredEvent, StoredSession, UrdiaError,
};
use urdia_store_sqlite::SqliteEventStore;
use uuid::Uuid;

const LIVE_HTML: &str = include_str!("../assets/live.html");
const LIVE_CSS: &str = include_str!("../assets/live.css");
const LIVE_JS: &str = include_str!("../assets/live.js");
const BRAND_SVG: &str = include_str!("../assets/brand.svg");

/// One stored session and its ordered events, ready for projection.
#[derive(Clone, Debug)]
pub struct TimelineSession {
    /// Summary returned by the event store.
    pub summary: StoredSession,
    /// Events ordered by store sequence.
    pub events: Vec<StoredEvent>,
}

impl TimelineSession {
    /// Combine a session summary with its events.
    #[must_use]
    pub const fn new(summary: StoredSession, events: Vec<StoredEvent>) -> Self {
        Self { summary, events }
    }
}

/// Load recent sessions and their ordered events from an event store.
pub async fn load_timeline(
    store: &SqliteEventStore,
    limit: usize,
) -> urdia_core::Result<Vec<TimelineSession>> {
    let summaries = store.list_sessions(limit).await?;
    let mut sessions = Vec::with_capacity(summaries.len());
    for summary in summaries {
        let events = store.list(&summary.session_id).await?;
        sessions.push(TimelineSession::new(summary, events));
    }
    Ok(sessions)
}

#[derive(Clone)]
struct LiveState {
    store: SqliteEventStore,
    token: Arc<str>,
    limit: usize,
    permissions: Option<InteractivePermissionBroker>,
    conversation: Option<ConversationHub>,
    advisory: Option<AdvisoryHub>,
}

/// Bound loopback-only server for the live read-only timeline.
pub struct LiveUiServer {
    listener: TcpListener,
    app: Router,
    address: SocketAddr,
    access_url: String,
}

impl std::fmt::Debug for LiveUiServer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LiveUiServer")
            .field("address", &self.address)
            .field("access_url", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl LiveUiServer {
    /// Bind an authenticated read-only UI to a loopback socket.
    ///
    /// Port zero lets the operating system choose a free ephemeral port.
    pub async fn bind(
        store: SqliteEventStore,
        address: SocketAddr,
        limit: usize,
    ) -> urdia_core::Result<Self> {
        Self::bind_inner(store, address, limit, None, None, None).await
    }

    /// Bind a loopback UI that can resolve requests from the supplied broker.
    pub async fn bind_interactive(
        store: SqliteEventStore,
        address: SocketAddr,
        limit: usize,
        permissions: InteractivePermissionBroker,
    ) -> urdia_core::Result<Self> {
        Self::bind_inner(store, address, limit, Some(permissions), None, None).await
    }

    /// Bind a loopback UI with an authenticated host-owned conversation.
    pub async fn bind_conversation(
        store: SqliteEventStore,
        address: SocketAddr,
        limit: usize,
        conversation: ConversationHub,
    ) -> urdia_core::Result<Self> {
        Self::bind_inner(store, address, limit, None, Some(conversation), None).await
    }

    /// Bind a loopback UI with conversation and one-shot permission controls.
    pub async fn bind_conversation_interactive(
        store: SqliteEventStore,
        address: SocketAddr,
        limit: usize,
        conversation: ConversationHub,
        permissions: InteractivePermissionBroker,
    ) -> urdia_core::Result<Self> {
        Self::bind_inner(
            store,
            address,
            limit,
            Some(permissions),
            Some(conversation),
            None,
        )
        .await
    }

    /// Alias for [`Self::bind_conversation`] with an explicit modifier name.
    pub async fn bind_with_conversation(
        store: SqliteEventStore,
        address: SocketAddr,
        limit: usize,
        conversation: ConversationHub,
    ) -> urdia_core::Result<Self> {
        Self::bind_conversation(store, address, limit, conversation).await
    }

    /// Alias for [`Self::bind_conversation_interactive`].
    pub async fn bind_interactive_with_conversation(
        store: SqliteEventStore,
        address: SocketAddr,
        limit: usize,
        permissions: InteractivePermissionBroker,
        conversation: ConversationHub,
    ) -> urdia_core::Result<Self> {
        Self::bind_conversation_interactive(store, address, limit, conversation, permissions).await
    }

    /// Bind explicitly enabled advisory recommendations alongside a conversation.
    pub async fn bind_workspace(
        store: SqliteEventStore,
        address: SocketAddr,
        conversation: ConversationHub,
        permissions: Option<InteractivePermissionBroker>,
        advisory: AdvisoryHub,
    ) -> urdia_core::Result<Self> {
        Self::bind_inner(
            store,
            address,
            50,
            permissions,
            Some(conversation),
            Some(advisory),
        )
        .await
    }

    async fn bind_inner(
        store: SqliteEventStore,
        address: SocketAddr,
        limit: usize,
        permissions: Option<InteractivePermissionBroker>,
        conversation: Option<ConversationHub>,
        advisory: Option<AdvisoryHub>,
    ) -> urdia_core::Result<Self> {
        if !address.ip().is_loopback() {
            return Err(UrdiaError::InvalidInput(format!(
                "UI bind address must be loopback, received {}",
                address.ip()
            )));
        }
        if !(1..=500).contains(&limit) {
            return Err(UrdiaError::InvalidInput(
                "UI session limit must be between 1 and 500".to_owned(),
            ));
        }
        let listener = TcpListener::bind(address)
            .await
            .map_err(|error| UrdiaError::Ui(format!("bind loopback listener: {error}")))?;
        let address = listener
            .local_addr()
            .map_err(|error| UrdiaError::Ui(format!("read loopback listener address: {error}")))?;
        let token = generate_token();
        let access_url = format!("http://{address}/#token={token}");
        let app =
            live_router_with_options(store, token, limit, permissions, conversation, advisory);
        Ok(Self {
            listener,
            app,
            address,
            access_url,
        })
    }

    /// Return the actual loopback address, including its assigned port.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    /// Return the launch URL carrying the token in its non-network fragment.
    #[must_use]
    pub fn access_url(&self) -> &str {
        &self.access_url
    }

    /// Serve until Ctrl-C or a listener error.
    pub async fn serve(self) -> urdia_core::Result<()> {
        self.serve_until(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
    }

    /// Serve until a host-owned shutdown future completes.
    pub async fn serve_until<F>(self, shutdown: F) -> urdia_core::Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        axum::serve(self.listener, self.app)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|error| UrdiaError::Ui(format!("serve loopback UI: {error}")))
    }
}

fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[cfg(test)]
fn live_router(store: SqliteEventStore, token: String, limit: usize) -> Router {
    live_router_with_options(store, token, limit, None, None, None)
}

#[cfg(test)]
fn live_router_with_permissions(
    store: SqliteEventStore,
    token: String,
    limit: usize,
    permissions: Option<InteractivePermissionBroker>,
) -> Router {
    live_router_with_options(store, token, limit, permissions, None, None)
}

#[cfg(test)]
fn live_router_with_conversation(
    store: SqliteEventStore,
    token: String,
    limit: usize,
    conversation: ConversationHub,
) -> Router {
    live_router_with_options(store, token, limit, None, Some(conversation), None)
}

fn live_router_with_options(
    store: SqliteEventStore,
    token: String,
    limit: usize,
    permissions: Option<InteractivePermissionBroker>,
    conversation: Option<ConversationHub>,
    advisory: Option<AdvisoryHub>,
) -> Router {
    let state = LiveState {
        store,
        token: Arc::from(token),
        limit,
        permissions,
        conversation,
        advisory,
    };
    let protected = Router::new()
        .route("/api/advisory", get(advisory_state).post(submit_advisory))
        .route("/api/advisory/laya", post(configure_laya))
        .route("/api/advisory/jev", post(configure_jev))
        .route("/api/advisory/prepare", post(prepare_execution))
        .route("/api/profiles", get(catalog_state).post(save_profile))
        .route("/api/agents", get(agent_catalog_state).post(register_agent))
        .route("/api/profiles/inspect", post(inspect_controls))
        .route("/api/advisory/execute", post(confirm_execution))
        .route("/api/timeline", get(live_timeline))
        .route("/api/permissions", get(live_permissions))
        .route("/api/permissions/{id}/decision", post(resolve_permission))
        .route(
            "/api/trusted-session/prepare",
            post(prepare_trusted_session),
        )
        .route(
            "/api/trusted-session/confirm",
            post(confirm_trusted_session),
        )
        .route("/api/trusted-session/revoke", post(revoke_trusted_session))
        .route("/api/trusted-session", get(trusted_session_state))
        .route("/api/conversation", get(live_conversation))
        .route("/api/conversation/prompt", post(submit_conversation_prompt))
        .route(
            "/api/conversation/profile",
            post(select_conversation_profile),
        )
        .route("/api/conversation/reconnect", post(reconnect_conversation))
        .route("/api/conversation/cancel", post(cancel_conversation))
        .route("/api/conversation/verify", post(verify_conversation))
        .route("/api/conversation/verify/cancel", post(cancel_verification))
        .route("/api/conversation/plan", post(mutate_project_plan))
        .route(
            "/api/conversation/plan/draft-preview",
            post(preview_plan_draft),
        )
        .route("/api/conversation/plan/export", get(export_project_plan))
        .route(
            "/api/conversation/plan/import-preview",
            post(preview_saved_plan),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_authorization,
        ));
    Router::new()
        .route("/", get(live_index))
        .route("/live.css", get(live_styles))
        .route("/live.js", get(live_script))
        .route("/brand.svg", get(brand_icon))
        .merge(protected)
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(MAX_PROMPT_CHARS * 6 + 1024))
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}

async fn advisory_state(State(state): State<LiveState>) -> Response {
    match state.advisory {
        Some(hub) => Json(hub.snapshot()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn configure_jev(
    State(state): State<LiveState>,
    payload: std::result::Result<
        Json<JevConnectionRequest>,
        axum::extract::rejection::JsonRejection,
    >,
) -> Response {
    let Some(hub) = state.advisory else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid"})),
        )
            .into_response();
    };
    match hub.configure_jev(request).await {
        Ok(view) => Json(view).into_response(),
        Err(error) => {
            let (status, code) = match error {
                JevConnectionError::Invalid => (StatusCode::BAD_REQUEST, "invalid"),
                JevConnectionError::Unavailable => (StatusCode::BAD_REQUEST, "not_configured"),
                JevConnectionError::Failed => (StatusCode::BAD_GATEWAY, "connection_failed"),
                JevConnectionError::Storage => (StatusCode::BAD_GATEWAY, "storage_unavailable"),
                JevConnectionError::Busy => (StatusCode::CONFLICT, "busy"),
                JevConnectionError::Limit => (StatusCode::TOO_MANY_REQUESTS, "session_limit"),
            };
            (status, Json(serde_json::json!({"error":code}))).into_response()
        }
    }
}

async fn configure_laya(
    State(state): State<LiveState>,
    payload: std::result::Result<
        Json<LayaConnectionRequest>,
        axum::extract::rejection::JsonRejection,
    >,
) -> Response {
    let Some(hub) = state.advisory else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid"})),
        )
            .into_response();
    };
    match hub.configure_laya(request).await {
        Ok(view) => Json(view).into_response(),
        Err(error) => {
            let (status, code) = match error {
                LayaConnectionError::Invalid => (StatusCode::BAD_REQUEST, "invalid"),
                LayaConnectionError::Unavailable => (StatusCode::BAD_REQUEST, "not_configured"),
                LayaConnectionError::Failed => (StatusCode::BAD_GATEWAY, "connection_failed"),
                LayaConnectionError::Busy => (StatusCode::CONFLICT, "busy"),
                LayaConnectionError::Limit => (StatusCode::TOO_MANY_REQUESTS, "session_limit"),
            };
            (status, Json(serde_json::json!({"error":code}))).into_response()
        }
    }
}

async fn submit_advisory(
    State(state): State<LiveState>,
    payload: std::result::Result<Json<AdvisoryRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Some(hub) = state.advisory else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid_json"})),
        )
            .into_response();
    };
    match hub.submit_with_conversation(request, state.conversation.as_ref()) {
        Ok(job) => (StatusCode::ACCEPTED, Json(job)).into_response(),
        Err(error) => {
            let (status, code) = match error {
                AdvisorySubmitError::Invalid => (StatusCode::BAD_REQUEST, "invalid"),
                AdvisorySubmitError::Unavailable => (StatusCode::BAD_REQUEST, "not_configured"),
                AdvisorySubmitError::Conflict => (StatusCode::CONFLICT, "conflict"),
                AdvisorySubmitError::Busy => (StatusCode::CONFLICT, "busy"),
                AdvisorySubmitError::Limit => (StatusCode::TOO_MANY_REQUESTS, "session_limit"),
            };
            (status, Json(serde_json::json!({"error": code}))).into_response()
        }
    }
}

fn execution_error(error: ExecutionError) -> Response {
    let (status, code) = match error {
        ExecutionError::Unavailable => (StatusCode::BAD_REQUEST, "unavailable"),
        ExecutionError::Stale => (StatusCode::CONFLICT, "stale"),
        ExecutionError::Conflict => (StatusCode::CONFLICT, "conflict"),
        ExecutionError::Busy => (StatusCode::CONFLICT, "busy"),
    };
    (status, Json(serde_json::json!({"error":code}))).into_response()
}
async fn prepare_execution(
    State(state): State<LiveState>,
    payload: Result<Json<PrepareExecutionRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Some(hub) = state.advisory else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return execution_error(ExecutionError::Unavailable);
    };
    match hub.prepare_execution_with_conversation(request.request_id, state.conversation.as_ref()) {
        Ok(preview) => Json(preview).into_response(),
        Err(error) => execution_error(error),
    }
}
async fn confirm_execution(
    State(state): State<LiveState>,
    payload: Result<Json<ConfirmExecutionRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let (Some(hub), Some(conversation)) = (state.advisory, state.conversation) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return execution_error(ExecutionError::Unavailable);
    };
    match hub.confirm_execution(request, &conversation) {
        Ok(snapshot) => (StatusCode::ACCEPTED, Json(snapshot)).into_response(),
        Err(error) => execution_error(error),
    }
}

async fn inspect_controls(
    State(state): State<LiveState>,
    payload: Result<Json<ControlProbeRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Some(hub) = state.advisory else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid"})),
        )
            .into_response();
    };
    match hub.inspect_controls(request).await {
        Ok(result) => Json(result).into_response(),
        Err(error) => {
            let (status, code) = match error {
                ControlProbeError::Invalid => (StatusCode::BAD_REQUEST, "invalid"),
                ControlProbeError::Unavailable => (StatusCode::UNPROCESSABLE_ENTITY, "unavailable"),
                ControlProbeError::Busy => (StatusCode::CONFLICT, "busy"),
                ControlProbeError::Conflict => (StatusCode::CONFLICT, "conflict"),
                ControlProbeError::Limit => (StatusCode::TOO_MANY_REQUESTS, "limit"),
                ControlProbeError::Failed => (StatusCode::BAD_GATEWAY, "failed"),
                ControlProbeError::Timeout => (StatusCode::GATEWAY_TIMEOUT, "timeout"),
            };
            (status, Json(serde_json::json!({"error":code}))).into_response()
        }
    }
}

async fn agent_catalog_state(State(state): State<LiveState>) -> Response {
    match state.advisory.and_then(|hub| hub.agent_catalog()) {
        Some(snapshot) => Json(snapshot).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn register_agent(
    State(state): State<LiveState>,
    payload: Result<Json<RegisterAgentRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Some(hub) = state.advisory else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return catalog_error(CatalogError::Invalid);
    };
    match tokio::task::spawn_blocking(move || hub.register_agent(request))
        .await
        .unwrap_or(Err(CatalogError::WriteFailed))
    {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => catalog_error(error),
    }
}
fn catalog_error(error: CatalogError) -> Response {
    let (status, code) = match error {
        CatalogError::Disabled => (StatusCode::FORBIDDEN, "disabled"),
        CatalogError::Invalid => (StatusCode::BAD_REQUEST, "invalid"),
        CatalogError::Conflict => (StatusCode::CONFLICT, "conflict"),
        CatalogError::WriteFailed => (StatusCode::INTERNAL_SERVER_ERROR, "write_failed"),
    };
    (status, Json(serde_json::json!({"error":code}))).into_response()
}

async fn catalog_state(State(state): State<LiveState>) -> Response {
    match state.advisory.and_then(|hub| hub.catalog()) {
        Some(snapshot) => Json(snapshot).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn save_profile(
    State(state): State<LiveState>,
    payload: Result<Json<SaveProfileRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Some(hub) = state.advisory else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid"})),
        )
            .into_response();
    };
    match tokio::task::spawn_blocking(move || hub.save_profile(request))
        .await
        .unwrap_or(Err(CatalogError::WriteFailed))
    {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => catalog_error(error),
    }
}

async fn live_index() -> Html<&'static str> {
    Html(LIVE_HTML)
}

async fn live_styles() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        LIVE_CSS,
    )
}

async fn brand_icon() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")],
        BRAND_SVG,
    )
}

async fn live_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        LIVE_JS,
    )
}

async fn live_timeline(State(state): State<LiveState>) -> Response {
    match load_timeline(&state.store, state.limit).await {
        Ok(sessions) => Html(render_timeline(&sessions)).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Debug, Serialize)]
struct PermissionSnapshot {
    interactive: bool,
    pending: Vec<PendingPermissionView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trusted: Option<TrustedSessionSnapshot>,
}

async fn live_permissions(State(state): State<LiveState>) -> Response {
    let snapshot = PermissionSnapshot {
        interactive: state.permissions.is_some(),
        pending: state
            .permissions
            .as_ref()
            .map_or_else(Vec::new, InteractivePermissionBroker::pending),
        trusted: state
            .permissions
            .as_ref()
            .and_then(InteractivePermissionBroker::trusted_state),
    };
    Json(snapshot).into_response()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustedPrepareRequest {
    pending_id: Uuid,
    revision: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustedConfirmRequest {
    challenge_id: Uuid,
    revision: u64,
}

#[derive(Debug, Serialize)]
struct TrustedSessionErrorBody {
    error: &'static str,
}

async fn trusted_session_state(State(state): State<LiveState>) -> Response {
    let Some(permissions) = state.permissions else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(snapshot) = permissions.trusted_state() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    Json(snapshot).into_response()
}

async fn prepare_trusted_session(
    State(state): State<LiveState>,
    request: Request<Body>,
) -> Response {
    let Some(permissions) = state.permissions else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(body) = to_bytes(request.into_body(), MAX_PROMPT_CHARS * 6 + 1024).await else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(request) = serde_json::from_slice::<TrustedPrepareRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match permissions.prepare_trusted_session(request.pending_id, request.revision) {
        Ok(challenge) => (StatusCode::CREATED, Json(challenge)).into_response(),
        Err(error) => trusted_error_response(error),
    }
}

async fn confirm_trusted_session(
    State(state): State<LiveState>,
    request: Request<Body>,
) -> Response {
    let Some(permissions) = state.permissions else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(body) = to_bytes(request.into_body(), MAX_PROMPT_CHARS * 6 + 1024).await else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(request) = serde_json::from_slice::<TrustedConfirmRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match permissions.confirm_trusted_session(request.challenge_id, request.revision) {
        Ok(snapshot) => (StatusCode::ACCEPTED, Json(snapshot)).into_response(),
        Err(error) => trusted_error_response(error),
    }
}

async fn revoke_trusted_session(State(state): State<LiveState>) -> Response {
    let Some(permissions) = state.permissions else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match permissions.revoke_trusted_session() {
        Ok(snapshot) => (StatusCode::ACCEPTED, Json(snapshot)).into_response(),
        Err(error) => trusted_error_response(error),
    }
}

fn trusted_error_response(error: TrustedSessionError) -> Response {
    let (status, code) = match error {
        TrustedSessionError::Disabled => (StatusCode::NOT_FOUND, "disabled"),
        TrustedSessionError::MissingPending | TrustedSessionError::MissingChallenge => {
            (StatusCode::GONE, "missing")
        }
        TrustedSessionError::StaleRevision => (StatusCode::CONFLICT, "stale_revision"),
        TrustedSessionError::AlreadyActive => (StatusCode::CONFLICT, "already_active"),
        TrustedSessionError::Expired => (StatusCode::GONE, "expired"),
        TrustedSessionError::ReceiverClosed => (StatusCode::GONE, "host_unavailable"),
    };
    (status, Json(TrustedSessionErrorBody { error: code })).into_response()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConversationPromptRequest {
    #[serde(default)]
    plan_link: Option<PlanLink>,
    turn_id: Uuid,
    text: String,
    profile_revision: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConversationCancelRequest {
    turn_id: Uuid,
}

#[derive(Debug, Serialize)]
struct ConversationErrorBody {
    error: &'static str,
}

async fn live_conversation(State(state): State<LiveState>) -> Response {
    state
        .conversation
        .map(|conversation| Json(conversation.snapshot()).into_response())
        .unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectProfileRequest {
    revision: u64,
    profile_id: Option<String>,
    confirm: bool,
}

async fn select_conversation_profile(
    State(state): State<LiveState>,
    payload: Result<Json<SelectProfileRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Some(conversation) = state.conversation else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ConversationErrorBody {
                error: "invalid_profile",
            }),
        )
            .into_response();
    };
    match conversation.select_profile(request.revision, request.profile_id, request.confirm) {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => {
            let (status, code) = match error {
                ProfileSelectionError::Unavailable => (StatusCode::NOT_FOUND, "unavailable"),
                ProfileSelectionError::Busy => (StatusCode::CONFLICT, "busy"),
                ProfileSelectionError::Stale => (StatusCode::CONFLICT, "stale_profile"),
                ProfileSelectionError::Unknown | ProfileSelectionError::Unconfirmed => {
                    (StatusCode::BAD_REQUEST, "invalid_profile")
                }
            };
            (status, Json(ConversationErrorBody { error: code })).into_response()
        }
    }
}

async fn verify_conversation(
    State(state): State<LiveState>,
    payload: Result<Json<VerificationRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Some(conversation) = state.conversation else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match conversation.submit_verification(request) {
        Ok(snapshot) => (StatusCode::ACCEPTED, Json(snapshot)).into_response(),
        Err(error) => verification_error(error),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelVerificationRequest {
    request_id: uuid::Uuid,
}
async fn cancel_verification(
    State(state): State<LiveState>,
    payload: Result<Json<CancelVerificationRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Some(conversation) = state.conversation else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match conversation.cancel_verification(request.request_id) {
        Ok(snapshot) => (StatusCode::ACCEPTED, Json(snapshot)).into_response(),
        Err(error) => verification_error(error),
    }
}
fn verification_error(error: VerificationError) -> Response {
    let (status, error) = match error {
        VerificationError::Unavailable => (StatusCode::NOT_FOUND, "unavailable"),
        VerificationError::Unconfirmed => (StatusCode::BAD_REQUEST, "confirmation_required"),
        VerificationError::Busy => (StatusCode::CONFLICT, "busy"),
        VerificationError::Stale => (StatusCode::CONFLICT, "stale_turn"),
        VerificationError::Conflict => (StatusCode::CONFLICT, "idempotency_conflict"),
        VerificationError::Limit => (StatusCode::TOO_MANY_REQUESTS, "verification_limit"),
        VerificationError::HostUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "host_unavailable"),
    };
    (status, Json(ConversationErrorBody { error })).into_response()
}

async fn export_project_plan(State(state): State<LiveState>) -> Response {
    let Some(hub) = state.conversation else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match hub.export_plan() {
        Ok(archive) => (
            [(
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"urdia-plan.json\"",
            )],
            Json(archive),
        )
            .into_response(),
        Err(_) => (
            StatusCode::CONFLICT,
            Json(ConversationErrorBody {
                error: "plan_export_unavailable",
            }),
        )
            .into_response(),
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanDraftPreviewRequest {
    response: String,
}
async fn preview_plan_draft(
    State(state): State<LiveState>,
    payload: Result<Json<PlanDraftPreviewRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if state.conversation.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Ok(Json(request)) = payload
        && let Some(draft) = PlanDraft::from_response(&request.response)
    {
        return Json(draft).into_response();
    }
    (
        StatusCode::BAD_REQUEST,
        Json(ConversationErrorBody {
            error: "invalid_plan_draft",
        }),
    )
        .into_response()
}

async fn preview_saved_plan(
    State(state): State<LiveState>,
    payload: Result<Json<PlanArchive>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if state.conversation.is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(Json(archive)) = payload else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ConversationErrorBody {
                error: "invalid_saved_plan",
            }),
        )
            .into_response();
    };
    match archive.validate() {
        Ok(()) => Json(archive).into_response(),
        Err(_) => (
            StatusCode::BAD_REQUEST,
            Json(ConversationErrorBody {
                error: "invalid_saved_plan",
            }),
        )
            .into_response(),
    }
}

async fn mutate_project_plan(
    State(state): State<LiveState>,
    payload: Result<Json<PlanRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Some(conversation) = state.conversation else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ConversationErrorBody {
                error: "invalid_plan",
            }),
        )
            .into_response();
    };
    match conversation.mutate_plan(request) {
        Ok(snapshot) => (StatusCode::ACCEPTED, Json(snapshot)).into_response(),
        Err(error) => {
            let (status, error) = match error {
                PlanError::Invalid => (StatusCode::BAD_REQUEST, "invalid_plan"),
                PlanError::Busy => (StatusCode::CONFLICT, "busy"),
                PlanError::Changed | PlanError::Blocked | PlanError::Conflict => {
                    (StatusCode::CONFLICT, "plan_changed_or_blocked")
                }
                PlanError::Limit => (StatusCode::TOO_MANY_REQUESTS, "plan_operation_limit"),
            };
            (status, Json(ConversationErrorBody { error })).into_response()
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReconnectionRequest {
    revision: u64,
    confirm_context_reset: bool,
}

async fn reconnect_conversation(
    State(state): State<LiveState>,
    payload: Result<Json<ReconnectionRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Some(conversation) = state.conversation else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(Json(request)) = payload else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    // Pending receipts/scopes must never be carried across a replacement.
    if state
        .permissions
        .as_ref()
        .is_some_and(|broker| !broker.pending().is_empty() || broker.trusted_state().is_some())
    {
        return StatusCode::CONFLICT.into_response();
    }
    match conversation.request_reconnection(request.revision, request.confirm_context_reset) {
        Ok(snapshot) => (StatusCode::ACCEPTED, Json(snapshot)).into_response(),
        Err(error) => {
            let status = match error {
                ReconnectionError::Unavailable => StatusCode::NOT_FOUND,
                ReconnectionError::Unconfirmed => StatusCode::BAD_REQUEST,
                ReconnectionError::Queue => StatusCode::SERVICE_UNAVAILABLE,
                ReconnectionError::Limit => StatusCode::TOO_MANY_REQUESTS,
                _ => StatusCode::CONFLICT,
            };
            (
                status,
                Json(ConversationErrorBody {
                    error: "reconnection_not_admitted",
                }),
            )
                .into_response()
        }
    }
}

async fn submit_conversation_prompt(
    State(state): State<LiveState>,
    request: Result<Json<ConversationPromptRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Ok(Json(request)) = request else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ConversationErrorBody {
                error: "invalid_prompt",
            }),
        )
            .into_response();
    };
    let Some(conversation) = state.conversation else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match conversation.submit_planned_prompt_with_revision(
        request.turn_id,
        request.text,
        request.profile_revision,
        request.plan_link,
    ) {
        Ok(snapshot) => (StatusCode::ACCEPTED, Json(snapshot)).into_response(),
        Err(ConversationSubmitError::EmptyPrompt | ConversationSubmitError::PromptLimit) => (
            StatusCode::BAD_REQUEST,
            Json(ConversationErrorBody {
                error: "invalid_prompt",
            }),
        )
            .into_response(),
        Err(ConversationSubmitError::PlanChanged) => (
            StatusCode::CONFLICT,
            Json(ConversationErrorBody {
                error: "plan_changed_or_blocked",
            }),
        )
            .into_response(),
        Err(ConversationSubmitError::ProfileRevision) => (
            StatusCode::CONFLICT,
            Json(ConversationErrorBody {
                error: "stale_profile",
            }),
        )
            .into_response(),
        Err(ConversationSubmitError::PrimaryDisconnected) => (
            StatusCode::CONFLICT,
            Json(ConversationErrorBody {
                error: "original_agent_disconnected",
            }),
        )
            .into_response(),
        Err(ConversationSubmitError::Busy { .. }) => (
            StatusCode::CONFLICT,
            Json(ConversationErrorBody { error: "busy" }),
        )
            .into_response(),
        Err(ConversationSubmitError::Conflict { .. }) => (
            StatusCode::CONFLICT,
            Json(ConversationErrorBody {
                error: "idempotency_conflict",
            }),
        )
            .into_response(),
        Err(ConversationSubmitError::CommandReceiverClosed) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ConversationErrorBody {
                error: "host_unavailable",
            }),
        )
            .into_response(),
        Err(ConversationSubmitError::CommandQueueFull) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ConversationErrorBody { error: "host_busy" }),
        )
            .into_response(),
        Err(ConversationSubmitError::SessionLimit) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ConversationErrorBody {
                error: "session_turn_limit",
            }),
        )
            .into_response(),
    }
}

async fn cancel_conversation(
    State(state): State<LiveState>,
    Json(request): Json<ConversationCancelRequest>,
) -> Response {
    let Some(conversation) = state.conversation else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match conversation.request_cancel(request.turn_id) {
        Ok(snapshot) => (StatusCode::ACCEPTED, Json(snapshot)).into_response(),
        Err(ConversationUpdateError::Missing) => StatusCode::NOT_FOUND.into_response(),
        Err(ConversationUpdateError::InvalidState { .. }) => (
            StatusCode::CONFLICT,
            Json(ConversationErrorBody {
                error: "turn_not_cancellable",
            }),
        )
            .into_response(),
        Err(ConversationUpdateError::CommandReceiverClosed) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ConversationErrorBody {
                error: "host_unavailable",
            }),
        )
            .into_response(),
        Err(ConversationUpdateError::CommandQueueFull) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ConversationErrorBody { error: "host_busy" }),
        )
            .into_response(),
        Err(ConversationUpdateError::Terminal | ConversationUpdateError::ResponseLimit) => (
            StatusCode::CONFLICT,
            Json(ConversationErrorBody {
                error: "turn_not_cancellable",
            }),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PermissionDecisionRequest {
    revision: u64,
    decision: PermissionDecision,
}

async fn resolve_permission(
    State(state): State<LiveState>,
    AxumPath(id): AxumPath<Uuid>,
    Json(request): Json<PermissionDecisionRequest>,
) -> Response {
    let Some(permissions) = state.permissions else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match permissions.resolve(id, request.revision, request.decision) {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(PermissionResolveError::StaleRevision { .. }) => StatusCode::CONFLICT.into_response(),
        Err(PermissionResolveError::Missing | PermissionResolveError::ReceiverClosed) => {
            StatusCode::GONE.into_response()
        }
    }
}

async fn require_authorization(
    State(state): State<LiveState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if authorized(request.headers(), &state.token) {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|candidate| candidate == token)
}

async fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

async fn security_headers(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; base-uri 'none'; connect-src 'self'; frame-ancestors 'none'; frame-src 'self'; img-src 'self' data:; object-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'",
        ),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    response
}

/// Render a complete standalone timeline with no network resources or scripts.
///
/// Event payloads are projected through an allowlist. Prompt text, agent text,
/// workspace paths, permission fingerprints, and unknown fields are omitted.
#[must_use]
pub fn render_timeline(sessions: &[TimelineSession]) -> String {
    let mut html = String::with_capacity(24_000);
    html.push_str(HTML_START);
    let _ = write!(
        html,
        "<header class=\"hero\"><div><p class=\"eyebrow\">LOCAL EVENT HISTORY</p><h1>Urdia timeline</h1><p class=\"lede\">A quiet, read-only view of agent sessions and permission receipts.</p></div><div class=\"metric\"><strong>{}</strong><span>sessions</span></div></header>",
        sessions.len()
    );

    if sessions.is_empty() {
        html.push_str("<main><section class=\"empty\"><span>◇</span><h2>No sessions yet</h2><p>Run an agent session, then generate this timeline again.</p></section></main>");
    } else {
        html.push_str("<main class=\"sessions\">");
        for (index, session) in sessions.iter().enumerate() {
            render_session(&mut html, session, index == 0);
        }
        html.push_str("</main>");
    }
    html.push_str(HTML_END);
    html
}

fn render_session(html: &mut String, session: &TimelineSession, open: bool) {
    let data = &session.summary.session_data;
    let provider = field(data, "provider_name")
        .or_else(|| field(data, "provider"))
        .unwrap_or("Unknown provider");
    let provider_id = field(data, "provider").unwrap_or("unknown");
    let inference = field(data, "inference").unwrap_or("unknown");
    let status = session_status(&session.events);
    let provider_mark = provider
        .chars()
        .next()
        .unwrap_or('U')
        .to_uppercase()
        .to_string();
    let open_attribute = if open { " open" } else { "" };
    let _ = write!(
        html,
        "<details class=\"session\"{open_attribute}><summary><div class=\"session-main\"><span class=\"provider-mark\">{}</span><div><h2>{}</h2><p><code>{}</code> · {} events · {}</p></div></div><div class=\"session-state\"><span class=\"badge badge-{}\">{}</span><time>{}</time></div></summary>",
        escape(&provider_mark),
        escape(provider),
        escape(provider_id),
        session.summary.event_count,
        escape(inference),
        css_token(status),
        escape(match status {
            "incomplete" => "Session not closed",
            "completed" => "Session closed",
            "replaced" => "Session replaced",
            other => other,
        }),
        escape(&format_timestamp(session.summary.updated_at_ms)),
    );
    html.push_str("<div class=\"session-body\">");
    if session
        .events
        .iter()
        .any(|stored| stored.event.kind == EventKind::Error)
    {
        html.push_str("<p>This session contains an earlier error. Check individual events and the current task separately.</p>");
    }
    html.push_str("<div class=\"session-meta\">");
    render_meta(
        html,
        "Session",
        &short_session_id(&session.summary.session_id.to_string()),
    );
    render_meta(
        html,
        "Started",
        &format_timestamp(session.summary.started_at_ms),
    );
    render_meta(
        html,
        "Authentication",
        field(data, "authentication").unwrap_or("unknown"),
    );
    render_meta(html, "Inference", inference);
    html.push_str("</div><ol class=\"events\">");
    for event in &session.events {
        render_event(html, event);
    }
    html.push_str("</ol></div></details>");
}

fn render_meta(html: &mut String, label: &str, value: &str) {
    let _ = write!(
        html,
        "<div><span>{}</span><strong>{}</strong></div>",
        escape(label),
        escape(value)
    );
}

fn render_event(html: &mut String, stored: &StoredEvent) {
    let label = event_label(&stored.event.kind);
    let detail = event_detail(&stored.event.kind, &stored.event.data);
    let _ = write!(
        html,
        "<li class=\"event event-{}\"><span class=\"event-dot\"></span><div class=\"event-card\"><div class=\"event-top\"><strong>{}</strong><time>{}</time></div><p>{}</p><span class=\"sequence\">#{}</span></div></li>",
        css_token(label),
        escape(label),
        escape(&format_timestamp(stored.event.occurred_at_ms)),
        escape(&detail),
        stored.sequence,
    );
}

fn event_label(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::SessionStarted => "Session started",
        EventKind::PromptSubmitted => "Prompt submitted",
        EventKind::AgentText => "Agent response",
        EventKind::Permission => "Permission receipt",
        EventKind::SessionFinished => "Session finished",
        EventKind::Error => "Session error",
    }
}

fn event_detail(kind: &EventKind, data: &Value) -> String {
    match kind {
        EventKind::SessionStarted => {
            let provider = field(data, "provider_name")
                .or_else(|| field(data, "provider"))
                .unwrap_or("Unknown provider");
            let protocols = data
                .get("protocols")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "transport unknown".to_owned());
            format!("{provider} connected through {protocols}.")
        }
        EventKind::PromptSubmitted => content_summary(data, "Prompt"),
        EventKind::AgentText => {
            let mut summary = content_summary(data, "Response");
            if let Some(reason) = field(data, "stop_reason") {
                let _ = write!(summary, " Stop reason: {reason}.");
            }
            summary
        }
        EventKind::Permission => {
            let outcome = field(data, "outcome").unwrap_or("unknown");
            let kind = field(data, "kind").unwrap_or("other");
            let revision = data.get("revision").and_then(Value::as_u64).unwrap_or(0);
            let locations = data
                .get("location_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            format!(
                "{} for {} at revision {}; {} reported location(s).",
                humanize(outcome),
                humanize(kind),
                revision,
                locations
            )
        }
        EventKind::SessionFinished if field(data, "status") == Some("replaced") => {
            "Session replaced. The new agent conversation receives no earlier transcript.".into()
        }
        EventKind::SessionFinished => format!(
            "Session {}.",
            humanize(field(data, "status").unwrap_or("finished"))
        ),
        EventKind::Error => {
            let stage = field(data, "stage").unwrap_or("unknown stage");
            match data.get("failure_kind").and_then(|value| {
                serde_json::from_value::<urdia_core::ProviderFailureKind>(value.clone()).ok()
            }) {
                Some(kind) => format!(
                    "An error was recorded during {stage}. {} Private provider details were omitted.",
                    kind.message()
                ),
                None => format!("An error was recorded during {stage}; details remain hidden."),
            }
        }
    }
}

fn content_summary(data: &Value, noun: &str) -> String {
    if data.get("content_persisted").and_then(Value::as_bool) == Some(false) {
        let characters = data.get("characters").and_then(Value::as_u64).unwrap_or(0);
        format!("{noun} content was not retained ({characters} characters).")
    } else {
        format!("{noun} content is retained locally but hidden in this privacy-safe view.")
    }
}

fn session_status(events: &[StoredEvent]) -> &str {
    events
        .iter()
        .rev()
        .find_map(|stored| {
            (stored.event.kind == EventKind::SessionFinished)
                .then(|| field(&stored.event.data, "status"))
                .flatten()
        })
        .unwrap_or("incomplete")
}

fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value.get(name).and_then(Value::as_str)
}

fn short_session_id(value: &str) -> String {
    value.chars().take(8).collect()
}

fn humanize(value: &str) -> String {
    value.replace('_', " ")
}

fn css_token(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn format_timestamp(milliseconds: i64) -> String {
    let nanos = i128::from(milliseconds).saturating_mul(1_000_000);
    OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .ok()
        .and_then(|timestamp| timestamp.format(&Rfc3339).ok())
        .unwrap_or_else(|| "unknown time".to_owned())
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

const HTML_START: &str = r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="dark"><title>Urdia timeline</title><style>
:root{--ink:#edf4e8;--muted:#9cad9e;--panel:#151c18;--line:#2a352e;--green:#9ee6ae;--mint:#d8ffd9;--coral:#ff9b7a;--gold:#e7c875;--bg:#0c110e}*{box-sizing:border-box}body{margin:0;background:radial-gradient(circle at 80% 0,#1b2b20 0,transparent 34rem),var(--bg);color:var(--ink);font:15px/1.55 Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;min-height:100vh}body:before{content:"";position:fixed;inset:0;pointer-events:none;opacity:.025;background-image:linear-gradient(#fff 1px,transparent 1px),linear-gradient(90deg,#fff 1px,transparent 1px);background-size:36px 36px}.hero{max-width:1120px;margin:auto;padding:76px 28px 42px;display:flex;align-items:end;justify-content:space-between;gap:32px}.eyebrow{color:var(--green);font-size:12px;font-weight:800;letter-spacing:.2em;margin:0 0 12px}.hero h1{font-family:Georgia,serif;font-size:clamp(42px,7vw,78px);font-weight:500;letter-spacing:-.055em;line-height:.94;margin:0}.lede{max-width:570px;color:var(--muted);font-size:17px;margin:20px 0 0}.metric{background:var(--mint);color:#0d1810;border-radius:50%;width:126px;height:126px;display:flex;flex-direction:column;align-items:center;justify-content:center;flex:0 0 auto;transform:rotate(4deg);box-shadow:0 18px 60px #0008}.metric strong{font:700 40px/1 Georgia,serif}.metric span{font-size:11px;text-transform:uppercase;letter-spacing:.12em;margin-top:7px}.sessions{max-width:1120px;margin:auto;padding:0 28px 90px;display:grid;gap:16px}.session{background:color-mix(in srgb,var(--panel) 92%,transparent);border:1px solid var(--line);border-radius:22px;overflow:hidden;box-shadow:0 18px 55px #0003}.session[open]{border-color:#496052}.session summary{list-style:none;cursor:pointer;padding:23px 25px;display:flex;justify-content:space-between;align-items:center;gap:24px}.session summary::-webkit-details-marker{display:none}.session-main,.session-state{display:flex;align-items:center;gap:15px}.provider-mark{display:grid;place-items:center;width:46px;height:46px;border-radius:14px;background:#26352b;color:var(--green);font:700 22px Georgia,serif}.session h2{font-size:17px;margin:0 0 3px}.session p{color:var(--muted);margin:0}.session code{color:#cad7cd;font-size:12px}.session-state{justify-content:end}.session-state time{color:var(--muted);font-size:12px;max-width:165px;text-align:right}.badge{font-size:11px;font-weight:800;text-transform:uppercase;letter-spacing:.08em;padding:7px 10px;border-radius:999px;background:#26332b;color:#c8d4ca}.badge-completed{background:#1c3925;color:var(--green)}.badge-failed{background:#41231f;color:var(--coral)}.session-body{border-top:1px solid var(--line);padding:22px 25px 28px}.session-meta{display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-bottom:28px}.session-meta div{background:#0f1511;border:1px solid #222d26;border-radius:12px;padding:12px 14px}.session-meta span{display:block;color:#75867a;font-size:10px;text-transform:uppercase;letter-spacing:.1em}.session-meta strong{display:block;font-size:13px;margin-top:4px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.events{list-style:none;margin:0;padding:0 0 0 10px}.event{position:relative;padding:0 0 15px 27px;border-left:1px solid #344238}.event:last-child{border-left-color:transparent;padding-bottom:0}.event-dot{position:absolute;left:-5px;top:17px;width:9px;height:9px;border-radius:50%;background:var(--green);box-shadow:0 0 0 5px #19221c}.event-permission-receipt .event-dot{background:var(--gold)}.event-session-error .event-dot{background:var(--coral)}.event-card{position:relative;background:#111813;border:1px solid #263229;border-radius:14px;padding:14px 48px 14px 16px}.event-top{display:flex;justify-content:space-between;gap:20px}.event-top strong{font-size:13px}.event-top time,.sequence{color:#718076;font-size:11px}.event-card p{font-size:13px;margin:7px 0 0}.sequence{position:absolute;right:15px;bottom:12px}.empty{max-width:720px;margin:50px auto;text-align:center;border:1px dashed #3a493e;border-radius:24px;padding:70px 20px}.empty span{color:var(--green);font-size:40px}.empty h2{font-family:Georgia,serif;font-size:32px;margin:12px 0 6px}.empty p{color:var(--muted)}footer{max-width:1120px;margin:auto;padding:0 28px 35px;color:#607067;font-size:11px;text-transform:uppercase;letter-spacing:.09em}@media(max-width:760px){.hero{padding-top:45px;align-items:start}.metric{width:88px;height:88px}.metric strong{font-size:29px}.session summary{align-items:start}.session-state{flex-direction:column;align-items:end}.session-state time{display:none}.session-meta{grid-template-columns:1fr 1fr}.event-top{display:block}.event-top time{display:block;margin-top:3px}}@media(max-width:500px){.hero{display:block}.metric{display:none}.session summary{display:block}.session-state{align-items:start;margin:12px 0 0}.session-main{align-items:start}.session-meta{grid-template-columns:1fr}.session-body{padding-left:16px;padding-right:16px}}
</style></head><body>"#;

const HTML_END: &str = r#"<footer>Generated locally · sensitive event fields omitted · no network resources</footer></body></html>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::Method;
    use serde_json::json;
    use std::path::PathBuf;
    use tower::ServiceExt;
    use urdia_core::{
        Event, PermissionBroker, PermissionLocation, PermissionOperation, PermissionOperationKind,
        PermissionRequest, SensitiveValue, SessionId, WorkspacePath,
    };

    #[tokio::test]
    async fn verification_api_authenticates_redacts_and_requires_opt_in() {
        let (hub, _receiver) = ConversationHub::new();
        let app = live_router_with_conversation(
            SqliteEventStore::in_memory().unwrap(),
            "test-token".into(),
            50,
            hub,
        );
        for uri in [
            "/api/conversation/verify",
            "/api/conversation/verify/cancel",
        ] {
            for (token, body, expected) in [
                (None, "SECRET_PAYLOAD", StatusCode::UNAUTHORIZED),
                (
                    Some("Bearer test-token"),
                    r#"{"request_id":"00000000-0000-0000-0000-000000000001","turn_id":"00000000-0000-0000-0000-000000000002","confirm_execution":true,"contract":"SECRET_PAYLOAD"}"#,
                    StatusCode::BAD_REQUEST,
                ),
                (
                    Some("Bearer test-token"),
                    r#"{"request_id":"00000000-0000-0000-0000-000000000001","command":"SECRET_PAYLOAD"}"#,
                    StatusCode::BAD_REQUEST,
                ),
            ] {
                let mut builder = Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .header(header::CONTENT_TYPE, "application/json");
                if let Some(token) = token {
                    builder = builder.header(header::AUTHORIZATION, token);
                }
                let response = app
                    .clone()
                    .oneshot(builder.body(Body::from(body)).unwrap())
                    .await
                    .unwrap();
                assert_eq!(response.status(), expected);
                assert!(!response_text(response).await.contains("SECRET_PAYLOAD"));
            }
        }
        let body = serde_json::to_string(&json!({"request_id": uuid::Uuid::new_v4(), "turn_id": uuid::Uuid::new_v4(), "confirm_execution": true})).unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/verify")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    struct OfflineAdvice;
    #[async_trait::async_trait]
    impl AdvisoryProvider for OfflineAdvice {
        fn engines(&self) -> Vec<AdvisoryEngine> {
            vec![AdvisoryEngine {
                id: "offline".into(),
                configured: true,
                remote: false,
            }]
        }
        async fn recommend(&self, _: &str, _: String) -> Option<AdvisoryResult> {
            None
        }
    }

    #[tokio::test]
    async fn execution_routes_authenticate_before_parsing_and_reject_browser_targets() {
        let (conversation, _receiver) = ConversationHub::new();
        let app = live_router_with_options(
            SqliteEventStore::in_memory().unwrap(),
            "test-token".into(),
            50,
            None,
            Some(conversation),
            Some(AdvisoryHub::new(Arc::new(OfflineAdvice))),
        );
        for route in ["/api/advisory/prepare", "/api/advisory/execute"] {
            for (token, expected) in [
                (None, StatusCode::UNAUTHORIZED),
                (Some("Bearer test-token"), StatusCode::BAD_REQUEST),
            ] {
                let mut request = Request::builder()
                    .method(Method::POST)
                    .uri(route)
                    .header(header::CONTENT_TYPE, "application/json");
                if let Some(token) = token {
                    request = request.header(header::AUTHORIZATION, token);
                }
                let response = app.clone().oneshot(request.body(Body::from(r#"{"profile_id":"SECRET_SENTINEL","objective":"PRIVATE_TASK","permissions":"allow"}"#)).unwrap()).await.unwrap();
                assert_eq!(response.status(), expected);
                let text = response_text(response).await;
                assert!(!text.contains("SECRET_SENTINEL") && !text.contains("PRIVATE_TASK"));
            }
        }
    }

    #[tokio::test]
    async fn control_probe_api_authenticates_and_redacts_invalid_payloads() {
        let app = live_router_with_options(
            SqliteEventStore::in_memory().unwrap(),
            "test-token".into(),
            50,
            None,
            None,
            Some(AdvisoryHub::new(Arc::new(OfflineAdvice))),
        );
        for (token, body, expected) in [
            (None, "not json", StatusCode::UNAUTHORIZED),
            (
                Some("Bearer test-token"),
                r#"{"request_id":"00000000-0000-0000-0000-000000000001","provider":"test","confirm_start":true,"command":"SECRET_SENTINEL"}"#,
                StatusCode::BAD_REQUEST,
            ),
            (
                Some("Bearer test-token"),
                r#"{"request_id":"00000000-0000-0000-0000-000000000001","provider":"test","model":null,"confirm_start":true}"#,
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            let mut builder = Request::builder()
                .method(Method::POST)
                .uri("/api/profiles/inspect")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(token) = token {
                builder = builder.header(header::AUTHORIZATION, token);
            }
            let response = app
                .clone()
                .oneshot(builder.body(Body::from(body)).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            assert!(!response_text(response).await.contains("SECRET_SENTINEL"));
        }
    }

    #[tokio::test]
    async fn jev_connection_authenticates_redacts_and_requires_remote_consent() {
        let app = live_router_with_options(
            SqliteEventStore::in_memory().unwrap(),
            "test-token".into(),
            50,
            None,
            None,
            Some(AdvisoryHub::new(Arc::new(OfflineAdvice))),
        );
        for (token, body, expected) in [
            (None, "not json", StatusCode::UNAUTHORIZED),
            (
                Some("Bearer test-token"),
                r#"{"action":"connect","request_id":"00000000-0000-0000-0000-000000000001","api_key":"SECRET_SENTINEL","remember":false,"allow_remote":false}"#,
                StatusCode::BAD_REQUEST,
            ),
            (
                Some("Bearer test-token"),
                r#"{"action":"connect","request_id":"00000000-0000-0000-0000-000000000001","api_key":"SECRET_SENTINEL","remember":false,"allow_remote":true,"endpoint":"PRIVATE_VALUE"}"#,
                StatusCode::BAD_REQUEST,
            ),
            (
                Some("Bearer test-token"),
                r#"{"action":"restore","request_id":"00000000-0000-0000-0000-000000000001","allow_remote":false}"#,
                StatusCode::BAD_REQUEST,
            ),
            (
                Some("Bearer test-token"),
                r#"{"action":"disconnect"}"#,
                StatusCode::BAD_REQUEST,
            ),
        ] {
            let mut request = Request::builder()
                .method(Method::POST)
                .uri("/api/advisory/jev")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(token) = token {
                request = request.header(header::AUTHORIZATION, token);
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::from(body)).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            let text = response_text(response).await;
            assert!(!text.contains("SECRET_SENTINEL") && !text.contains("PRIVATE_VALUE"));
        }
    }

    #[tokio::test]
    async fn agent_registration_authenticates_redacts_and_requires_opt_in() {
        let app = live_router_with_options(
            SqliteEventStore::in_memory().unwrap(),
            "test-token".into(),
            50,
            None,
            None,
            Some(AdvisoryHub::new(Arc::new(OfflineAdvice))),
        );
        for (token, body, expected) in [
            (None, "not json", StatusCode::UNAUTHORIZED),
            (
                Some("Bearer test-token"),
                r#"{"revision":0,"agent":{"id":"test","name":"Test","command":"SECRET_SENTINEL","args":[],"env":{"key":"PRIVATE_VALUE"}},"confirm_launch":true}"#,
                StatusCode::BAD_REQUEST,
            ),
            (
                Some("Bearer test-token"),
                r#"{"revision":0,"agent":{"id":"test","name":"Test","command":"SECRET_SENTINEL","args":[]},"confirm_launch":true}"#,
                StatusCode::FORBIDDEN,
            ),
        ] {
            let mut builder = Request::builder()
                .method(Method::POST)
                .uri("/api/agents")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(token) = token {
                builder = builder.header(header::AUTHORIZATION, token);
            }
            let response = app
                .clone()
                .oneshot(builder.body(Body::from(body)).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            let text = response_text(response).await;
            assert!(!text.contains("SECRET_SENTINEL") && !text.contains("PRIVATE_VALUE"));
        }
    }

    #[tokio::test]
    async fn catalog_api_authenticates_and_rejects_commands_keys_paths_and_disabled_writes() {
        let app = live_router_with_options(
            SqliteEventStore::in_memory().unwrap(),
            "test-token".into(),
            50,
            None,
            None,
            Some(AdvisoryHub::new(Arc::new(OfflineAdvice))),
        );
        for (token, body, expected) in [
            (None, "not json", StatusCode::UNAUTHORIZED),
            (
                Some("Bearer test-token"),
                r#"{"revision":0,"path":"SECRET_SENTINEL","command":"PRIVATE_VALUE"}"#,
                StatusCode::BAD_REQUEST,
            ),
            (
                Some("Bearer test-token"),
                r#"{"revision":0,"profile":{"id":"test","name":"Test","provider":"qwen","agent":"Agent","inference_provider":"Unknown","model":"Model","effort":null,"model_selector":null,"effort_selector":null},"make_default":false}"#,
                StatusCode::FORBIDDEN,
            ),
        ] {
            let mut builder = Request::builder()
                .method(Method::POST)
                .uri("/api/profiles")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(token) = token {
                builder = builder.header(header::AUTHORIZATION, token);
            }
            let response = app
                .clone()
                .oneshot(builder.body(Body::from(body)).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            let text = response_text(response).await;
            assert!(!text.contains("SECRET_SENTINEL") && !text.contains("PRIVATE_VALUE"));
        }
    }

    #[tokio::test]
    async fn local_connection_api_authenticates_before_parsing_and_redacts_invalid_secrets() {
        let app = live_router_with_options(
            SqliteEventStore::in_memory().unwrap(),
            "test-token".into(),
            50,
            None,
            None,
            Some(AdvisoryHub::new(Arc::new(OfflineAdvice))),
        );
        for (token, body, expected) in [
            (None, "not-json", StatusCode::UNAUTHORIZED),
            (
                Some("Bearer test-token"),
                r#"{"action":"connect","endpoint":"PRIVATE_VALUE","api_key":"SECRET_SENTINEL","unknown":true}"#,
                StatusCode::BAD_REQUEST,
            ),
            (
                Some("Bearer test-token"),
                r#"{"action":"connect","endpoint":"http://127.0.0.1:1","api_key":"SECRET_SENTINEL"}"#,
                StatusCode::BAD_REQUEST,
            ),
        ] {
            let mut builder = Request::builder()
                .method(Method::POST)
                .uri("/api/advisory/laya")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(token) = token {
                builder = builder.header(header::AUTHORIZATION, token);
            }
            let response = app
                .clone()
                .oneshot(builder.body(Body::from(body)).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            let text = response_text(response).await;
            assert!(!text.contains("SECRET_SENTINEL"));
            assert!(!text.contains("PRIVATE_VALUE"));
        }
    }

    #[tokio::test]
    async fn advisory_api_authenticates_before_parsing_and_never_echoes_objective_or_bad_values() {
        let store = SqliteEventStore::in_memory().unwrap();
        let app = live_router_with_options(
            store,
            "test-token".into(),
            50,
            None,
            None,
            Some(AdvisoryHub::new(Arc::new(OfflineAdvice))),
        );
        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/advisory")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert!(response_text(unauthorized).await.is_empty());
        let bad = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/advisory")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"request_id":"PRIVATE_BAD_VALUE"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
        assert!(!response_text(bad).await.contains("PRIVATE_BAD_VALUE"));
        let body = serde_json::to_vec(&json!({ "request_id": Uuid::new_v4(), "engine": "offline", "objective": "PRIVATE_ADVICE_OBJECTIVE" })).unwrap();
        let accepted = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/advisory")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::ACCEPTED);
        assert!(
            !response_text(accepted)
                .await
                .contains("PRIVATE_ADVICE_OBJECTIVE")
        );
        let snapshot = app
            .oneshot(
                Request::builder()
                    .uri("/api/advisory")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(snapshot.status(), StatusCode::OK);
        assert!(
            !response_text(snapshot)
                .await
                .contains("PRIVATE_ADVICE_OBJECTIVE")
        );
    }

    fn fixture() -> TimelineSession {
        let session_id = SessionId::new();
        let started = Event::new(
            session_id.clone(),
            EventKind::SessionStarted,
            json!({
                "provider": "qwen<script>alert(1)</script>",
                "provider_name": "<Qwen Code>",
                "protocols": ["acp-v1", "stdio"],
                "authentication": "unknown",
                "inference": "remote",
                "workspace": "/home/private/SECRET_WORKSPACE"
            }),
        );
        let prompt = Event::new(
            session_id.clone(),
            EventKind::PromptSubmitted,
            json!({"text": "SECRET_PROMPT_SENTINEL"}),
        );
        let permission = Event::new(
            session_id.clone(),
            EventKind::Permission,
            json!({
                "revision": 3,
                "kind": "execute",
                "outcome": "rejected_once",
                "location_count": 1,
                "operation_fingerprint": "SECRET_FINGERPRINT"
            }),
        );
        TimelineSession::new(
            StoredSession {
                session_id,
                first_sequence: 1,
                last_sequence: 3,
                event_count: 3,
                started_at_ms: started.occurred_at_ms,
                updated_at_ms: permission.occurred_at_ms,
                session_data: started.data.clone(),
            },
            vec![
                StoredEvent {
                    sequence: 1,
                    event: started,
                },
                StoredEvent {
                    sequence: 2,
                    event: prompt,
                },
                StoredEvent {
                    sequence: 3,
                    event: permission,
                },
            ],
        )
    }

    #[test]
    fn timeline_escapes_labels_and_omits_sensitive_fields() {
        let html = render_timeline(&[fixture()]);
        assert!(html.contains("&lt;Qwen Code&gt;"));
        assert!(!html.contains("<span class=\"provider-mark\"><"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("SECRET_PROMPT_SENTINEL"));
        assert!(!html.contains("SECRET_WORKSPACE"));
        assert!(!html.contains("SECRET_FINGERPRINT"));
        assert!(html.contains("rejected once for execute at revision 3"));
    }

    #[test]
    fn timeline_distinguishes_task_errors_from_session_closure() {
        let mut session = fixture();
        assert!(render_timeline(&[session.clone()]).contains("Session not closed"));
        let session_id = session.summary.session_id.clone();
        session.events.push(StoredEvent {
            sequence: 4,
            event: Event::new(
                session_id.clone(),
                EventKind::Error,
                json!({"detail": "SECRET_ERROR"}),
            ),
        });
        session.events.push(StoredEvent {
            sequence: 5,
            event: Event::new(
                session_id,
                EventKind::SessionFinished,
                json!({"status": "completed"}),
            ),
        });
        let html = render_timeline(&[session]);
        assert!(html.contains("Session closed"));
        assert!(html.contains("This session contains an earlier error"));
        assert!(!html.contains("SECRET_ERROR"));
    }

    #[test]
    fn timeline_provider_failure_kind_is_allowlisted_and_native_payload_is_never_rendered() {
        for category in [
            json!("authentication"),
            json!("rate_or_quota"),
            json!("PRIVATE_FAILURE_SENTINEL <img src=x>"),
            json!({"message":"PRIVATE_FAILURE_SENTINEL"}),
        ] {
            let mut session = fixture();
            session.events.push(StoredEvent {sequence:4,event:Event::new(session.summary.session_id.clone(),EventKind::Error,
                json!({"stage":"prompt","failure_kind":category,"message":"PRIVATE_FAILURE_SENTINEL","data":{"secret":"PRIVATE_FAILURE_SENTINEL"}}))});
            let html = render_timeline(&[session]);
            assert!(!html.contains("PRIVATE_FAILURE_SENTINEL"));
            assert!(!html.contains("<img"));
            if category == json!("authentication") {
                assert!(html.contains("authentication problem"));
            }
            if category == json!("rate_or_quota") {
                assert!(html.contains("usage or rate limit"));
            }
        }
    }

    #[test]
    fn timeline_is_standalone_and_script_free() {
        let html = render_timeline(&[fixture()]);
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
        assert!(!html.contains("<script"));
        assert!(html.contains("<style>"));
    }

    async fn response_text(response: Response) -> String {
        String::from_utf8(
            to_bytes(response.into_body(), 256 * 1024)
                .await
                .expect("response body should be bounded")
                .to_vec(),
        )
        .expect("test responses should be UTF-8")
    }

    async fn live_fixture() -> (Router, &'static str) {
        let store = SqliteEventStore::in_memory().expect("in-memory event store");
        let session_id = SessionId::new();
        store
            .append(Event::new(
                session_id.clone(),
                EventKind::SessionStarted,
                json!({
                    "provider_name": "Qwen <script>SECRET_SCRIPT</script>",
                    "provider": "qwen",
                    "protocols": ["acp-v1"],
                    "workspace": "/SECRET/WORKSPACE"
                }),
            ))
            .await
            .expect("append session start");
        store
            .append(Event::new(
                session_id,
                EventKind::PromptSubmitted,
                json!({
                    "text": "SECRET_PROMPT_SENTINEL",
                    "content_persisted": true
                }),
            ))
            .await
            .expect("append private prompt");
        (
            live_router(store, "test-token".to_owned(), 50),
            "test-token",
        )
    }

    fn private_permission_request(revision: u64) -> PermissionRequest {
        PermissionRequest {
            id: Uuid::new_v4(),
            session_id: SessionId::new(),
            provider_session_id: "SECRET_PROVIDER_SESSION".to_owned(),
            workspace: WorkspacePath::new(std::env::current_dir().unwrap()).unwrap(),
            revision,
            operation: PermissionOperation {
                tool_call_id: "SECRET_TOOL_CALL".to_owned(),
                title: Some("SECRET_TITLE".to_owned()),
                name: Some("SECRET_TOOL_NAME".to_owned()),
                kind: PermissionOperationKind::Execute,
                arguments: Some(SensitiveValue::new(json!({
                    "command": "SECRET_ARGUMENT_SENTINEL"
                }))),
                locations: vec![PermissionLocation {
                    path: PathBuf::from("/SECRET/TOOL/PATH"),
                    line: Some(7),
                }],
            },
        }
    }

    async fn wait_for_permission(broker: &InteractivePermissionBroker) -> PendingPermissionView {
        for _ in 0..100 {
            if let Some(permission) = broker.pending().into_iter().next() {
                return permission;
            }
            tokio::task::yield_now().await;
        }
        panic!("permission should become pending");
    }

    #[tokio::test]
    async fn live_server_rejects_non_loopback_bind_before_listening() {
        let store = SqliteEventStore::in_memory().expect("in-memory event store");
        let error = LiveUiServer::bind(store, "0.0.0.0:0".parse().unwrap(), 50)
            .await
            .expect_err("wildcard bind must be rejected");
        assert!(error.to_string().contains("must be loopback"));
    }

    #[tokio::test]
    async fn live_server_uses_an_ephemeral_loopback_port_and_redacts_debug_token() {
        let store = SqliteEventStore::in_memory().expect("in-memory event store");
        let server = LiveUiServer::bind(store, "127.0.0.1:0".parse().unwrap(), 50)
            .await
            .expect("loopback bind should succeed");
        assert!(server.address().ip().is_loopback());
        assert_ne!(server.address().port(), 0);
        assert!(
            server
                .access_url()
                .starts_with(&format!("http://{}/#token=", server.address()))
        );
        let token = server.access_url().split("#token=").nth(1).unwrap();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(!format!("{server:?}").contains(token));
    }

    #[tokio::test]
    async fn live_shell_contains_neither_token_nor_event_data() {
        let (app, token) = live_fixture().await;
        let response = app
            .clone()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert!(
            response
                .headers()
                .contains_key(header::CONTENT_SECURITY_POLICY)
        );
        let body = response_text(response).await;
        assert!(!body.contains(token));
        assert!(!body.contains("SECRET_PROMPT_SENTINEL"));
        assert!(!body.contains("/SECRET/WORKSPACE"));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/permissions")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response_text(response).await,
            r#"{"interactive":false,"pending":[]}"#
        );
    }

    #[tokio::test]
    async fn live_timeline_requires_bearer_and_returns_only_projected_html() {
        let (app, token) = live_fixture().await;
        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/timeline?token=test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert!(response_text(unauthorized).await.is_empty());

        let rejected = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/timeline")
                    .header(header::AUTHORIZATION, "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/timeline")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body = response_text(response).await;
        assert!(body.contains("Qwen &lt;script&gt;SECRET_SCRIPT&lt;/script&gt;"));
        assert!(!body.contains("<script>SECRET_SCRIPT</script>"));
        assert!(!body.contains("SECRET_PROMPT_SENTINEL"));
        assert!(!body.contains("/SECRET/WORKSPACE"));
    }

    #[tokio::test]
    async fn live_api_has_no_mutation_route() {
        let (app, token) = live_fixture().await;
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/timeline")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn interactive_api_redacts_and_consumes_a_decision_exactly_once() {
        let store = SqliteEventStore::in_memory().expect("in-memory event store");
        let broker = InteractivePermissionBroker::new();
        let decision_task = tokio::spawn({
            let broker = broker.clone();
            async move { broker.authorize(private_permission_request(12)).await }
        });
        let pending = wait_for_permission(&broker).await;
        let app =
            live_router_with_permissions(store, "test-token".to_owned(), 50, Some(broker.clone()));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/permissions")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_text(response).await;
        assert!(body.contains("\"interactive\":true"));
        assert!(body.contains("\"kind\":\"execute\""));
        for secret in [
            "SECRET_PROVIDER_SESSION",
            "SECRET_TOOL_CALL",
            "SECRET_TITLE",
            "SECRET_TOOL_NAME",
            "SECRET_ARGUMENT_SENTINEL",
            "/SECRET/TOOL/PATH",
        ] {
            assert!(!body.contains(secret));
        }

        let path = format!("/api/permissions/{}/decision", pending.id);
        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(&path)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let stale = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(&path)
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!(
                        "{{\"revision\":{},\"decision\":\"allow_once\"}}",
                        pending.revision + 1
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::CONFLICT);
        assert_eq!(broker.pending(), vec![pending.clone()]);

        let accepted = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(&path)
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!(
                        "{{\"revision\":{},\"decision\":\"allow_once\"}}",
                        pending.revision
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::ACCEPTED);
        assert_eq!(
            decision_task
                .await
                .expect("decision task should join")
                .expect("decision should arrive"),
            PermissionDecision::AllowOnce
        );

        let duplicate = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(&path)
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!(
                        "{{\"revision\":{},\"decision\":\"reject_once\"}}",
                        pending.revision
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(duplicate.status(), StatusCode::GONE);
    }

    #[tokio::test]
    async fn trusted_api_uses_two_step_confirmation_and_revoke_is_idempotent() {
        let store = SqliteEventStore::in_memory().expect("in-memory event store");
        let broker = InteractivePermissionBroker::with_trusted_session("Public Qwen");
        let decision_task = tokio::spawn({
            let broker = broker.clone();
            async move { broker.authorize(private_permission_request(21)).await }
        });
        let pending = wait_for_permission(&broker).await;
        let app =
            live_router_with_permissions(store, "test-token".to_owned(), 50, Some(broker.clone()));

        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/trusted-session/prepare")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let state = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/trusted-session")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(state.status(), StatusCode::OK);
        let state_body = response_text(state).await;
        assert!(state_body.contains("Public Qwen"));
        assert!(state_body.contains("awaiting_request"));
        for secret in [
            "SECRET_PROVIDER_SESSION",
            "SECRET_ARGUMENT_SENTINEL",
            "/SECRET/TOOL/PATH",
            "SECRET_TOOL_CALL",
        ] {
            assert!(!state_body.contains(secret));
        }

        let prepared = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/trusted-session/prepare")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!(
                        "{{\"pending_id\":\"{}\",\"revision\":{}}}",
                        pending.id, pending.revision
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(prepared.status(), StatusCode::CREATED);
        let challenge: TrustedSessionChallenge =
            serde_json::from_str(&response_text(prepared).await).expect("challenge JSON");
        assert_eq!(challenge.provider_label, "Public Qwen");
        assert_eq!(
            challenge.capabilities,
            TRUSTED_SESSION_CAPABILITIES.to_vec()
        );
        assert_eq!(challenge.ttl_seconds, TRUSTED_SESSION_TTL_SECONDS);
        assert!(challenge.workspace.starts_with('/'));

        let confirmed = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/trusted-session/confirm")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!(
                        "{{\"challenge_id\":\"{}\",\"revision\":{}}}",
                        challenge.id, challenge.revision
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(confirmed.status(), StatusCode::ACCEPTED);
        let confirmed_body = response_text(confirmed).await;
        assert!(confirmed_body.contains("\"status\":\"active\""));
        let decision = decision_task
            .await
            .expect("authorization task should join")
            .expect("trusted decision should arrive");
        assert!(decision.trusted_grant().is_some());

        let duplicate = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/trusted-session/confirm")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!(
                        "{{\"challenge_id\":\"{}\",\"revision\":{}}}",
                        challenge.id, challenge.revision
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(duplicate.status(), StatusCode::GONE);

        for _ in 0..2 {
            let revoked = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/api/trusted-session/revoke")
                        .header(header::AUTHORIZATION, "Bearer test-token")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(revoked.status(), StatusCode::ACCEPTED);
            assert!(
                response_text(revoked)
                    .await
                    .contains("\"status\":\"revoked\"")
            );
        }
    }

    #[tokio::test]
    async fn standalone_ui_has_no_trusted_controller_or_mutating_endpoints() {
        let store = SqliteEventStore::in_memory().expect("in-memory event store");
        let app = live_router(store, "test-token".to_owned(), 50);
        for path in [
            "/api/trusted-session",
            "/api/trusted-session/prepare",
            "/api/trusted-session/confirm",
            "/api/trusted-session/revoke",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(if path.ends_with("session") {
                            Method::GET
                        } else {
                            Method::POST
                        })
                        .uri(path)
                        .header(header::AUTHORIZATION, "Bearer test-token")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }

    #[tokio::test]
    async fn conversation_auth_runs_before_json_and_content_requires_token() {
        let store = SqliteEventStore::in_memory().expect("in-memory event store");
        let (conversation, _commands) = ConversationHub::with_context(ConversationContext {
            provider_id: "current-agent".to_owned(),
            provider_name: "Current agent".to_owned(),
            workspace_name: "private-project".to_owned(),
            profile: None,
            startup_controls: None,
        });
        let app = live_router_with_conversation(store, "test-token".to_owned(), 50, conversation);
        let unauthorized_post = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/prompt")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized_post.status(), StatusCode::UNAUTHORIZED);

        let unauthorized_get = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/conversation")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized_get.status(), StatusCode::UNAUTHORIZED);
        assert!(response_text(unauthorized_get).await.is_empty());

        let authorized = app
            .oneshot(
                Request::builder()
                    .uri("/api/conversation")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);
        assert_eq!(
            response_text(authorized).await,
            r#"{"project_plan":{"revision":0,"plan":null},"sequence":0,"active_turn_id":null,"turns":[],"context":{"provider_id":"current-agent","provider_name":"Current agent","workspace_name":"private-project"}}"#
        );
    }

    #[tokio::test]
    async fn generated_plan_preview_is_private_authenticated_and_inert() {
        let (hub, mut commands) = ConversationHub::new();
        let app = live_router_with_conversation(
            SqliteEventStore::in_memory().unwrap(),
            "test-token".into(),
            50,
            hub.clone(),
        );
        let before = hub.snapshot();
        for (authorized, text, status) in [
            (
                false,
                r#"{"objective":"goal","steps":[{"title":"one","scope":"file","acceptance":"test"}]}"#,
                StatusCode::UNAUTHORIZED,
            ),
            (
                true,
                r#"{"objective":"goal","steps":[{"title":"one","scope":"file","acceptance":"test"}]}"#,
                StatusCode::OK,
            ),
            (true, "PRIVATE_SECRET malformed", StatusCode::BAD_REQUEST),
            (
                true,
                r#"{"objective":"PRIVATE_SECRET","steps":[],"execute":true}"#,
                StatusCode::BAD_REQUEST,
            ),
        ] {
            let mut request = Request::builder()
                .method(Method::POST)
                .uri("/api/conversation/plan/draft-preview")
                .header(header::CONTENT_TYPE, "application/json");
            if authorized {
                request = request.header(header::AUTHORIZATION, "Bearer test-token");
            }
            let result = app
                .clone()
                .oneshot(
                    request
                        .body(Body::from(serde_json::json!({"response":text}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(result.status(), status);
            assert_eq!(result.headers()[header::CACHE_CONTROL], "no-store");
            let body = response_text(result).await;
            assert!(!body.contains("PRIVATE_SECRET"));
        }
        assert_eq!(hub.snapshot().sequence, before.sequence);
        assert_eq!(hub.snapshot().project_plan.revision, 0);
        assert!(hub.snapshot().project_plan.plan.is_none());
        assert!(commands.try_recv().is_err());
    }

    #[tokio::test]
    async fn saved_plan_preview_export_and_recovery_are_authenticated_and_inert() {
        let (hub, mut commands) = ConversationHub::new();
        hub.mutate_plan(PlanRequest {
            request_id: Uuid::new_v4(),
            revision: 0,
            operation: PlanOperation::Create {
                objective: "PRIVATE_OBJECTIVE".into(),
                steps: vec![PlanStepDefinition {
                    title: "step".into(),
                    scope: "PRIVATE_SCOPE".into(),
                    acceptance: "check".into(),
                }],
            },
        })
        .unwrap();
        let app = live_router_with_conversation(
            SqliteEventStore::in_memory().unwrap(),
            "test-token".into(),
            50,
            hub.clone(),
        );
        let exported = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/conversation/plan/export")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(exported.status(), StatusCode::OK);
        assert_eq!(exported.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(
            exported.headers()[header::CONTENT_DISPOSITION],
            "attachment; filename=\"urdia-plan.json\""
        );
        let archive: PlanArchive = serde_json::from_slice(
            &to_bytes(exported.into_body(), MAX_PLAN_ARCHIVE_BYTES)
                .await
                .unwrap(),
        )
        .unwrap();
        let before = hub.snapshot();
        for authenticated in [false, true] {
            let mut request = Request::builder()
                .method(Method::POST)
                .uri("/api/conversation/plan/import-preview")
                .header(header::CONTENT_TYPE, "application/json");
            if authenticated {
                request = request.header(header::AUTHORIZATION, "Bearer test-token");
            }
            let result = app
                .clone()
                .oneshot(
                    request
                        .body(Body::from(serde_json::to_vec(&archive).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                result.status(),
                if authenticated {
                    StatusCode::OK
                } else {
                    StatusCode::UNAUTHORIZED
                }
            );
        }
        let invalid = json!({"source_plan_id":"SECRET_INVALID_UUID"});
        let rejected = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/plan/import-preview")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&invalid).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
        assert!(!response_text(rejected).await.contains("SECRET"));
        assert!(hub.snapshot() == before);
        assert!(commands.try_recv().is_err());
        let unauthorized = app
            .oneshot(
                Request::builder()
                    .uri("/api/conversation/plan/export")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        let (fresh, mut fresh_commands) = ConversationHub::new();
        let recovered = fresh
            .mutate_plan(PlanRequest {
                request_id: Uuid::new_v4(),
                revision: 0,
                operation: PlanOperation::Restore {
                    archive,
                    start_step: 0,
                    confirm_restore: true,
                },
            })
            .unwrap();
        assert_eq!(recovered.turns.len(), 0);
        assert!(fresh_commands.try_recv().is_err());
        assert!(!format!("{recovered:?}").contains("PRIVATE"));
    }

    #[tokio::test]
    async fn project_plan_api_is_authenticated_and_rejects_source_values_safely() {
        let store = SqliteEventStore::in_memory().unwrap();
        let (hub, mut commands) = ConversationHub::new();
        let app = live_router_with_conversation(store, "test-token".into(), 50, hub.clone());
        for (uri, body) in [
            (
                "/api/conversation/plan",
                json!({"request_id":"SECRET_BAD_UUID","revision":0,"operation":{"kind":"clear","plan_id":"SECRET"}}),
            ),
            (
                "/api/conversation/prompt",
                json!({"turn_id":Uuid::new_v4(),"text":"SECRET_PROMPT","plan_link":{"plan_id":"SECRET_BAD_UUID","revision":0,"step_index":0}}),
            ),
            (
                "/api/conversation/plan",
                json!({"request_id":Uuid::new_v4(),"revision":0,"operation":{"kind":"create","objective":"SECRET","steps":[],"unexpected":"SECRET"}}),
            ),
        ] {
            for authenticated in [false, true] {
                let mut request = Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .header(header::CONTENT_TYPE, "application/json");
                if authenticated {
                    request = request.header(header::AUTHORIZATION, "Bearer test-token");
                }
                let response = app
                    .clone()
                    .oneshot(
                        request
                            .body(Body::from(serde_json::to_vec(&body).unwrap()))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    if authenticated {
                        StatusCode::BAD_REQUEST
                    } else {
                        StatusCode::UNAUTHORIZED
                    }
                );
                let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
                assert!(!String::from_utf8_lossy(&bytes).contains("SECRET"));
            }
        }
        let body = json!({"request_id":Uuid::new_v4(),"revision":0,"operation":{"kind":"create","objective":"recoverable objective","steps":[{"title":"one","scope":"two files","acceptance":"review tests"}]}});
        for _ in 0..2 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/api/conversation/plan")
                        .header(header::AUTHORIZATION, "Bearer test-token")
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            let snapshot: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap())
                    .unwrap();
            assert_eq!(snapshot["project_plan"]["revision"], 1);
            assert_eq!(
                snapshot["project_plan"]["plan"]["objective"],
                "recoverable objective"
            );
        }
        assert!(commands.try_recv().is_err());
    }

    #[tokio::test]
    async fn conversation_prompt_is_idempotent_and_busy_is_explicit() {
        let store = SqliteEventStore::in_memory().expect("in-memory event store");
        let (conversation, mut commands) = ConversationHub::new();
        let app =
            live_router_with_conversation(store, "test-token".to_owned(), 50, conversation.clone());
        let turn_id = Uuid::new_v4();
        let prompt = serde_json::to_vec(&json!({
            "turn_id": turn_id,
            "text": "SECRET_PROMPT_SENTINEL"
        }))
        .unwrap();
        let accepted = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/prompt")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(prompt.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::ACCEPTED);
        assert_eq!(
            commands.recv().await,
            Some(ConversationCommand::Prompt {
                turn_id,
                text: "SECRET_PROMPT_SENTINEL".to_owned()
            })
        );

        let duplicate = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/prompt")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(prompt))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(duplicate.status(), StatusCode::ACCEPTED);
        assert!(commands.try_recv().is_err());

        let conflicting = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/prompt")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "turn_id": turn_id,
                            "text": "different"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(conflicting.status(), StatusCode::CONFLICT);

        let busy = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/prompt")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "turn_id": Uuid::new_v4(),
                            "text": "second"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(busy.status(), StatusCode::CONFLICT);

        conversation
            .mark_running(turn_id)
            .expect("host should mark turn running");
        conversation
            .publish_delta(turn_id, "SECRET_RESPONSE_SENTINEL")
            .expect("host response should be published");
        let snapshot = app_for_conversation(&conversation, "test-token");
        let response = snapshot
            .oneshot(
                Request::builder()
                    .uri("/api/conversation")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_text(response).await;
        assert!(body.contains("SECRET_PROMPT_SENTINEL"));
        assert!(body.contains("SECRET_RESPONSE_SENTINEL"));
    }

    #[tokio::test]
    async fn conversation_json_is_strict_and_prompt_limits_are_enforced() {
        let store = SqliteEventStore::in_memory().expect("in-memory event store");
        let (conversation, _commands) = ConversationHub::new();
        let app = live_router_with_conversation(store, "test-token".to_owned(), 50, conversation);
        let unknown = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/prompt")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "turn_id": Uuid::new_v4(),
                            "text": "hello",
                            "unexpected": true
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);

        let oversized = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/prompt")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "turn_id": Uuid::new_v4(),
                            "text": "x".repeat(MAX_PROMPT_CHARS + 1)
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(oversized.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn conversation_cancel_is_idempotent_and_host_confirmed() {
        let store = SqliteEventStore::in_memory().expect("in-memory event store");
        let (conversation, mut commands) = ConversationHub::new();
        let app =
            live_router_with_conversation(store, "test-token".to_owned(), 50, conversation.clone());
        let turn_id = Uuid::new_v4();
        let body = serde_json::to_vec(&json!({
            "turn_id": turn_id,
            "text": "cancel me"
        }))
        .unwrap();
        let prompt = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/prompt")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(prompt.status(), StatusCode::ACCEPTED);
        assert!(matches!(
            commands.recv().await,
            Some(ConversationCommand::Prompt { .. })
        ));

        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/cancel")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let cancel_body = serde_json::to_vec(&json!({ "turn_id": turn_id })).unwrap();
        let cancel = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/cancel")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(cancel_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cancel.status(), StatusCode::ACCEPTED);
        assert_eq!(
            commands.recv().await,
            Some(ConversationCommand::Cancel { turn_id })
        );
        let duplicate = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/conversation/cancel")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(cancel_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(duplicate.status(), StatusCode::ACCEPTED);
        assert!(commands.try_recv().is_err());

        conversation
            .cancel(turn_id)
            .expect("host should confirm cancellation");
        let snapshot = app
            .oneshot(
                Request::builder()
                    .uri("/api/conversation")
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            response_text(snapshot)
                .await
                .contains("\"status\":\"cancelled\"")
        );
    }

    #[test]
    fn live_chat_uses_safe_dom_text_rendering_and_keeps_token_client_side() {
        assert!(LIVE_HTML.contains("conversation-history"));
        assert!(LIVE_JS.contains("textContent"));
        assert!(!LIVE_JS.contains("innerHTML"));
        assert!(LIVE_JS.contains("sessionStorage"));
        assert!(LIVE_JS.contains("window.location.hash"));
        assert!(LIVE_JS.contains("challenge && challenge.workspace"));
        assert!(LIVE_JS.contains("is not a sandbox"));
    }

    #[tokio::test]
    async fn reconnection_endpoint_authenticates_before_strict_consent_and_is_idempotent() {
        let (hub, mut commands) = ConversationHub::new();
        hub.enable_reconnection();
        let turn = Uuid::new_v4();
        hub.submit_prompt(turn, "fixture".into()).unwrap();
        commands.try_recv().unwrap();
        hub.fail(turn, "fixed".into()).unwrap();
        let revision = hub.snapshot().reconnection.unwrap().revision;
        let app = app_for_conversation(&hub, "test-token");
        for (auth, body, expected) in [
            (false, "not-json".to_string(), StatusCode::UNAUTHORIZED),
            (
                true,
                format!(r#"{{"revision":{revision},"confirm_context_reset":false}}"#),
                StatusCode::BAD_REQUEST,
            ),
            (
                true,
                format!(
                    r#"{{"revision":{revision},"confirm_context_reset":true,"key":"private"}}"#
                ),
                StatusCode::BAD_REQUEST,
            ),
            (
                true,
                format!(r#"{{"revision":{revision},"confirm_context_reset":true}}"#),
                StatusCode::ACCEPTED,
            ),
            (
                true,
                format!(r#"{{"revision":{revision},"confirm_context_reset":true}}"#),
                StatusCode::ACCEPTED,
            ),
        ] {
            let mut request = Request::builder()
                .method(Method::POST)
                .uri("/api/conversation/reconnect")
                .header(header::CONTENT_TYPE, "application/json");
            if auth {
                request = request.header(header::AUTHORIZATION, "Bearer test-token");
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::from(body)).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
        assert_eq!(
            commands.try_recv().unwrap(),
            ConversationCommand::Reconnect { revision }
        );
        assert!(commands.try_recv().is_err());
    }

    #[tokio::test]
    async fn reconnection_refuses_pending_permissions_and_any_trusted_broker() {
        for trusted in [false, true] {
            let (hub, mut commands) = ConversationHub::new();
            hub.enable_reconnection();
            let turn = Uuid::new_v4();
            hub.submit_prompt(turn, "fixture".into()).unwrap();
            commands.try_recv().unwrap();
            hub.fail(turn, "fixed".into()).unwrap();
            let revision = hub.snapshot().reconnection.unwrap().revision;
            let broker = if trusted {
                InteractivePermissionBroker::with_trusted_session("fixture")
            } else {
                InteractivePermissionBroker::new()
            };
            let permission_task = if !trusted {
                Some(tokio::spawn({
                    let broker = broker.clone();
                    async move { broker.authorize(private_permission_request(1)).await }
                }))
            } else {
                None
            };
            if !trusted {
                wait_for_permission(&broker).await;
            }
            let app = live_router_with_options(
                SqliteEventStore::in_memory().unwrap(),
                "test-token".into(),
                50,
                Some(broker),
                Some(hub),
                None,
            );
            let response = app
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/api/conversation/reconnect")
                        .header(header::AUTHORIZATION, "Bearer test-token")
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(format!(
                            r#"{{"revision":{revision},"confirm_context_reset":true}}"#
                        )))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CONFLICT);
            assert!(commands.try_recv().is_err());
            if let Some(task) = permission_task {
                task.abort();
                let _ = task.await;
            }
        }
    }

    fn app_for_conversation(conversation: &ConversationHub, token: &str) -> Router {
        live_router_with_conversation(
            SqliteEventStore::in_memory().expect("in-memory event store"),
            token.to_owned(),
            50,
            conversation.clone(),
        )
    }
}
