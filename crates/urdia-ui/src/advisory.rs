//! Bounded advice and host-reviewed confirmation tickets. Permissions stay separate.
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Host-prepared connection metadata. Configured never means verified.
#[derive(Clone, Serialize)]
pub struct AdvisoryEngine {
    pub id: String,
    pub configured: bool,
    pub remote: bool,
}

/// Host-validated, content-free recommendation.
#[derive(Clone, Serialize)]
pub struct AdvisoryResult {
    pub selected_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_profile: Option<crate::ConversationProfile>,
    pub probabilities: BTreeMap<String, f32>,
    pub review_probability: Option<f32>,
    pub confidence: Option<f32>,
    pub probability_semantics: String,
    pub fallback_reason: Option<String>,
}

/// Session-only local connection operations. Never derive Debug for credentials.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum LayaConnectionRequest {
    Connect { endpoint: String, api_key: String },
    Check,
    Disconnect,
}

#[derive(Clone, Serialize)]
pub struct LayaConnectionView {
    pub endpoint: Option<String>,
    pub status: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayaConnectionError {
    Invalid,
    Unavailable,
    Failed,
    Busy,
    Limit,
}

/// Jev credentials are accepted only by an authenticated explicit host operation.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum JevConnectionRequest {
    Connect {
        request_id: Uuid,
        api_key: String,
        remember: bool,
        allow_remote: bool,
    },
    Check {
        request_id: Uuid,
        allow_remote: bool,
    },
    Restore {
        request_id: Uuid,
        allow_remote: bool,
    },
    Disconnect,
    Forget,
}
#[derive(Clone, Serialize)]
pub struct JevConnectionView {
    pub status: &'static str,
    pub remembered: bool,
    pub storage_kind: Option<&'static str>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JevConnectionError {
    Invalid,
    Unavailable,
    Failed,
    Storage,
    Busy,
    Limit,
}

/// Host implementation owns adapter configuration and candidate preparation.
#[async_trait]
pub trait AdvisoryProvider: Send + Sync {
    /// Whether host-pinned profiles can be dispatched; no decision or permission.
    fn automatic_execution_available(&self) -> bool {
        false
    }
    fn candidate_kind(&self) -> &'static str {
        "provider"
    }
    fn engines(&self) -> Vec<AdvisoryEngine>;
    async fn inspect_controls(
        &self,
        _: crate::ControlProbeRequest,
    ) -> Result<crate::ControlProbeResult, crate::ControlProbeError> {
        Err(crate::ControlProbeError::Unavailable)
    }
    fn agent_catalog(&self) -> Option<crate::AgentCatalogSnapshot> {
        None
    }
    fn register_agent(
        &self,
        _: crate::RegisterAgentRequest,
    ) -> Result<crate::AgentCatalogSnapshot, crate::CatalogError> {
        Err(crate::CatalogError::Disabled)
    }
    fn catalog(&self) -> Option<crate::CatalogSnapshot> {
        None
    }
    fn save_profile(
        &self,
        _: crate::SaveProfileRequest,
    ) -> Result<crate::CatalogSnapshot, crate::CatalogError> {
        Err(crate::CatalogError::Disabled)
    }

    /// Validate immutable host configuration; no agent startup or model call.
    fn execution_target(&self, _: &AdvisoryResult) -> Option<ExecutionTarget> {
        None
    }

    fn jev_connection(&self) -> Option<JevConnectionView> {
        None
    }
    async fn configure_jev(
        &self,
        _: JevConnectionRequest,
    ) -> Result<JevConnectionView, JevConnectionError> {
        Err(JevConnectionError::Unavailable)
    }

    fn laya_connection(&self) -> Option<LayaConnectionView> {
        None
    }
    async fn configure_laya(
        &self,
        _: LayaConnectionRequest,
    ) -> Result<LayaConnectionView, LayaConnectionError> {
        Err(LayaConnectionError::Unavailable)
    }
    async fn recommend(&self, engine: &str, objective: String) -> Option<AdvisoryResult>;
}

/// Browser request with an idempotency key. It cannot supply candidates or keys.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvisoryRequest {
    #[serde(default)]
    pub plan_link: Option<crate::PlanLink>,
    pub request_id: Uuid,
    pub engine: String,
    pub objective: String,
    #[serde(default)]
    pub allow_remote: bool,
}

#[derive(Clone, Serialize)]
pub struct AdvisoryJob {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_link: Option<crate::PlanLink>,
    pub request_id: Uuid,
    pub engine: String,
    pub status: &'static str,
    /// Host elapsed time from admission to terminal outcome, including adapter retries.
    pub elapsed_ms: Option<u64>,
    pub result: Option<AdvisoryResult>,
}

#[derive(Serialize)]
pub struct AdvisorySnapshot {
    pub automatic_execution_available: bool,
    pub manual_execution_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub laya_connection: Option<LayaConnectionView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jev_connection: Option<JevConnectionView>,
    pub candidate_kind: &'static str,
    pub engines: Vec<AdvisoryEngine>,
    pub latest: Option<AdvisoryJob>,
}

/// Redacted errors, suitable for a browser response.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AdvisorySubmitError {
    Invalid,
    Unavailable,
    Conflict,
    Busy,
    Limit,
}

#[derive(Clone, Serialize)]
pub struct ExecutionTarget {
    pub profile: crate::ConversationProfile,
    pub provider_id: String,
    pub workspace_name: String,
    pub model_selector: String,
    pub effort_selector: Option<String>,
    pub permission_mode: String,
}
#[derive(Clone, Serialize)]
pub struct ExecutionPreview {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_link: Option<crate::PlanLink>,
    pub plan_id: Uuid,
    pub request_id: Uuid,
    pub objective: String,
    pub target: ExecutionTarget,
    pub valid_for_seconds: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareExecutionRequest {
    pub request_id: Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmExecutionRequest {
    pub plan_id: Uuid,
    pub turn_id: Uuid,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExecutionError {
    Unavailable,
    Stale,
    Conflict,
    Busy,
}
struct Plan {
    preview: ExecutionPreview,
    created: Instant,
    accepted: Option<Uuid>,
}

struct Record {
    plan_link: Option<crate::PlanLink>,
    admitted: Instant,
    objective: String,
    allow_remote: bool,
    job: AdvisoryJob,
    plan: Option<Plan>,
}

#[derive(Default)]
struct State {
    records: BTreeMap<Uuid, Record>,
    latest: Option<Uuid>,
    active: bool,
    connection_attempts: usize,
}

/// One concurrent request, at most 64 accepted keys, and a bounded deadline.
/// Completed requests are replayed without repeating an API call.
#[derive(Clone)]
pub struct AdvisoryHub {
    provider: Arc<dyn AdvisoryProvider>,
    state: Arc<Mutex<State>>,
    deadline: Duration,
}

impl AdvisoryHub {
    pub fn new(provider: Arc<dyn AdvisoryProvider>) -> Self {
        Self {
            provider,
            state: Arc::new(Mutex::new(State::default())),
            deadline: Duration::from_secs(35),
        }
    }

    pub async fn inspect_controls(
        &self,
        request: crate::ControlProbeRequest,
    ) -> Result<crate::ControlProbeResult, crate::ControlProbeError> {
        self.provider.inspect_controls(request).await
    }
    pub fn agent_catalog(&self) -> Option<crate::AgentCatalogSnapshot> {
        self.provider.agent_catalog()
    }
    pub fn register_agent(
        &self,
        request: crate::RegisterAgentRequest,
    ) -> Result<crate::AgentCatalogSnapshot, crate::CatalogError> {
        self.provider.register_agent(request)
    }
    pub fn catalog(&self) -> Option<crate::CatalogSnapshot> {
        self.provider.catalog()
    }
    pub fn save_profile(
        &self,
        request: crate::SaveProfileRequest,
    ) -> Result<crate::CatalogSnapshot, crate::CatalogError> {
        self.provider.save_profile(request)
    }

    pub fn snapshot(&self) -> AdvisorySnapshot {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        AdvisorySnapshot {
            automatic_execution_available: self.provider.automatic_execution_available(),
            manual_execution_available: state
                .latest
                .and_then(|id| state.records.get(&id))
                .filter(|record| {
                    record.plan.as_ref().is_none_or(|plan| {
                        plan.accepted.is_none() && plan.created.elapsed() < Duration::from_secs(300)
                    })
                })
                .and_then(|record| record.job.result.as_ref())
                .filter(|result| result.fallback_reason.is_none())
                .is_some_and(|result| self.provider.execution_target(result).is_some()),
            laya_connection: self.provider.laya_connection(),
            jev_connection: self.provider.jev_connection(),
            candidate_kind: self.provider.candidate_kind(),
            engines: self.provider.engines(),
            latest: state
                .latest
                .and_then(|id| state.records.get(&id))
                .map(|record| record.job.clone()),
        }
    }

    /// A five-minute, host-owned ticket bound to the original recommendation/task.
    pub fn prepare_execution(&self, request_id: Uuid) -> Result<ExecutionPreview, ExecutionError> {
        self.prepare_execution_with_conversation(request_id, None)
    }

    pub fn prepare_execution_with_conversation(
        &self,
        request_id: Uuid,
        conversation: Option<&crate::ConversationHub>,
    ) -> Result<ExecutionPreview, ExecutionError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.active || state.latest != Some(request_id) {
            return Err(ExecutionError::Stale);
        }
        let record = state
            .records
            .get_mut(&request_id)
            .ok_or(ExecutionError::Unavailable)?;
        if let Some(link) = &record.plan_link {
            conversation
                .ok_or(ExecutionError::Unavailable)?
                .validate_plan_step(link, &record.objective)
                .map_err(|e| {
                    if e == crate::PlanError::Busy {
                        ExecutionError::Busy
                    } else {
                        ExecutionError::Stale
                    }
                })?;
        }
        let result = record
            .job
            .result
            .as_ref()
            .filter(|r| r.fallback_reason.is_none())
            .ok_or(ExecutionError::Unavailable)?;
        let target = self
            .provider
            .execution_target(result)
            .ok_or(ExecutionError::Unavailable)?;
        if let Some(plan) = &record.plan {
            if plan.accepted.is_some() || plan.created.elapsed() >= Duration::from_secs(300) {
                return Err(ExecutionError::Stale);
            }
            let mut preview = plan.preview.clone();
            preview.valid_for_seconds = 300_u64.saturating_sub(plan.created.elapsed().as_secs());
            return Ok(preview);
        }
        let preview = ExecutionPreview {
            plan_link: record.plan_link.clone(),
            plan_id: Uuid::new_v4(),
            request_id,
            objective: record.objective.clone(),
            target,
            valid_for_seconds: 300,
        };
        record.plan = Some(Plan {
            preview: preview.clone(),
            created: Instant::now(),
            accepted: None,
        });
        Ok(preview)
    }

    /// Queue only the host-retained objective/target, exactly once. No permissions change.
    pub fn confirm_execution(
        &self,
        request: ConfirmExecutionRequest,
        conversation: &crate::ConversationHub,
    ) -> Result<crate::ConversationSnapshot, ExecutionError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let request_id = state
            .records
            .iter()
            .find_map(|(id, record)| {
                record
                    .plan
                    .as_ref()
                    .filter(|p| p.preview.plan_id == request.plan_id)
                    .map(|_| *id)
            })
            .ok_or(ExecutionError::Unavailable)?;
        let latest = state.latest;
        let active = state.active;
        let record = state
            .records
            .get_mut(&request_id)
            .ok_or(ExecutionError::Unavailable)?;
        let plan = record.plan.as_mut().ok_or(ExecutionError::Unavailable)?;
        if let Some(accepted) = plan.accepted {
            return if accepted == request.turn_id {
                Ok(conversation.snapshot())
            } else {
                Err(ExecutionError::Conflict)
            };
        }
        if active
            || latest != Some(request_id)
            || plan.created.elapsed() >= Duration::from_secs(300)
        {
            return Err(ExecutionError::Stale);
        }
        let target = record
            .job
            .result
            .as_ref()
            .filter(|r| r.fallback_reason.is_none())
            .and_then(|result| self.provider.execution_target(result))
            .ok_or(ExecutionError::Unavailable)?;
        // Configuration is immutable for this process; still validate availability again.
        if target.provider_id != plan.preview.target.provider_id
            || target.profile.id != plan.preview.target.profile.id
            || target.model_selector != plan.preview.target.model_selector
            || target.effort_selector != plan.preview.target.effort_selector
        {
            return Err(ExecutionError::Stale);
        }
        let snapshot = conversation
            .submit_recommended_with_plan(
                request.turn_id,
                record.objective.clone(),
                target.profile.id,
                record.plan_link.clone(),
            )
            .map_err(|error| match error {
                crate::ConversationSubmitError::Conflict { .. } => ExecutionError::Conflict,
                crate::ConversationSubmitError::PlanChanged => ExecutionError::Stale,
                crate::ConversationSubmitError::Busy { .. }
                | crate::ConversationSubmitError::CommandQueueFull => ExecutionError::Busy,
                _ => ExecutionError::Unavailable,
            })?;
        plan.accepted = Some(request.turn_id);
        Ok(snapshot)
    }

    /// Serialize changes with advice and bound connection tests per host lifetime.
    pub async fn configure_jev(
        &self,
        request: JevConnectionRequest,
    ) -> Result<JevConnectionView, JevConnectionError> {
        match &request {
            JevConnectionRequest::Connect {
                api_key,
                allow_remote,
                ..
            } => {
                if !allow_remote
                    || api_key.is_empty()
                    || api_key.len() > 4096
                    || api_key.bytes().any(|v| !(33..=126).contains(&v))
                {
                    return Err(JevConnectionError::Invalid);
                }
            }
            JevConnectionRequest::Check { allow_remote, .. }
            | JevConnectionRequest::Restore { allow_remote, .. }
                if !allow_remote =>
            {
                return Err(JevConnectionError::Invalid);
            }
            _ => {}
        }
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.active {
                return Err(JevConnectionError::Busy);
            }
            state.active = true;
        }
        struct Release(Arc<Mutex<State>>);
        impl Drop for Release {
            fn drop(&mut self) {
                self.0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .active = false;
            }
        }
        let _release = Release(self.state.clone());
        tokio::time::timeout(self.deadline, self.provider.configure_jev(request))
            .await
            .unwrap_or(Err(JevConnectionError::Failed))
    }

    pub async fn configure_laya(
        &self,
        request: LayaConnectionRequest,
    ) -> Result<LayaConnectionView, LayaConnectionError> {
        if let LayaConnectionRequest::Connect { endpoint, api_key } = &request
            && (endpoint.len() > 2048
                || api_key.is_empty()
                || api_key.len() > 4096
                || api_key.bytes().any(|byte| !(33..=126).contains(&byte)))
        {
            return Err(LayaConnectionError::Invalid);
        }
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.active {
                return Err(LayaConnectionError::Busy);
            }
            if state.connection_attempts >= 16
                && !matches!(request, LayaConnectionRequest::Disconnect)
            {
                return Err(LayaConnectionError::Limit);
            }
            if !matches!(request, LayaConnectionRequest::Disconnect) {
                state.connection_attempts += 1;
            }
            state.active = true;
        }
        struct Release(Arc<Mutex<State>>);
        impl Drop for Release {
            fn drop(&mut self) {
                self.0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .active = false;
            }
        }
        let _release = Release(self.state.clone());
        tokio::time::timeout(self.deadline, self.provider.configure_laya(request))
            .await
            .unwrap_or(Err(LayaConnectionError::Failed))
    }

    pub fn submit(&self, request: AdvisoryRequest) -> Result<AdvisoryJob, AdvisorySubmitError> {
        self.submit_with_conversation(request, None)
    }

    pub fn submit_with_conversation(
        &self,
        request: AdvisoryRequest,
        conversation: Option<&crate::ConversationHub>,
    ) -> Result<AdvisoryJob, AdvisorySubmitError> {
        if request.objective.trim().is_empty() || request.objective.chars().count() > 16_384 {
            return Err(AdvisorySubmitError::Invalid);
        }
        let job = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(record) = state.records.get(&request.request_id) {
                return if record.objective == request.objective
                    && record.plan_link == request.plan_link
                    && record.job.engine == request.engine
                    && record.allow_remote == request.allow_remote
                {
                    Ok(record.job.clone())
                } else {
                    Err(AdvisorySubmitError::Conflict)
                };
            }
            if let Some(link) = &request.plan_link {
                conversation
                    .ok_or(AdvisorySubmitError::Invalid)?
                    .validate_plan_step(link, &request.objective)
                    .map_err(|e| {
                        if e == crate::PlanError::Busy {
                            AdvisorySubmitError::Busy
                        } else {
                            AdvisorySubmitError::Conflict
                        }
                    })?;
            }
            if !self.provider.engines().iter().any(|engine| {
                engine.id == request.engine
                    && engine.configured
                    && (!engine.remote || request.allow_remote)
            }) {
                return Err(AdvisorySubmitError::Unavailable);
            }
            if state.active {
                return Err(AdvisorySubmitError::Busy);
            }
            if state.records.len() >= 64 {
                return Err(AdvisorySubmitError::Limit);
            }
            let job = AdvisoryJob {
                plan_link: request.plan_link.clone(),
                request_id: request.request_id,
                engine: request.engine.clone(),
                status: "running",
                elapsed_ms: None,
                result: None,
            };
            state.records.insert(
                request.request_id,
                Record {
                    plan_link: request.plan_link.clone(),
                    admitted: Instant::now(),
                    objective: request.objective.clone(),
                    allow_remote: request.allow_remote,
                    job: job.clone(),
                    plan: None,
                },
            );
            state.latest = Some(request.request_id);
            state.active = true;
            job
        };
        let hub = self.clone();
        // Continue after an HTTP disconnect so a browser retry cannot double-charge.
        tokio::spawn(async move {
            let provider = hub.provider.clone();
            let mut worker = tokio::spawn(async move {
                provider.recommend(&request.engine, request.objective).await
            });
            let result = match tokio::time::timeout(hub.deadline, &mut worker).await {
                Ok(Ok(result)) => result,
                _ => {
                    worker.abort();
                    None
                }
            };
            let mut state = hub
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(record) = state.records.get_mut(&request.request_id) {
                record.job.status = if result.is_some() {
                    "completed"
                } else {
                    "failed"
                };
                record.job.elapsed_ms = Some(
                    record
                        .admitted
                        .elapsed()
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64,
                );
                record.job.result = result;
            }
            state.active = false;
        });
        Ok(job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Notify;

    struct ConnectionFake {
        release: Notify,
    }
    #[async_trait]
    impl AdvisoryProvider for ConnectionFake {
        fn engines(&self) -> Vec<AdvisoryEngine> {
            vec![]
        }
        async fn recommend(&self, _: &str, _: String) -> Option<AdvisoryResult> {
            None
        }
        async fn configure_laya(
            &self,
            _: LayaConnectionRequest,
        ) -> Result<LayaConnectionView, LayaConnectionError> {
            self.release.notified().await;
            Ok(LayaConnectionView {
                endpoint: None,
                status: "disconnected",
            })
        }
    }
    #[tokio::test]
    async fn connection_mutations_are_serialized_and_cancellation_releases_slot() {
        let provider = Arc::new(ConnectionFake {
            release: Notify::new(),
        });
        let hub = AdvisoryHub::new(provider.clone());
        let worker_hub = hub.clone();
        let task = tokio::spawn(async move {
            worker_hub
                .configure_laya(LayaConnectionRequest::Check)
                .await
        });
        tokio::task::yield_now().await;
        assert!(matches!(
            hub.configure_laya(LayaConnectionRequest::Disconnect).await,
            Err(LayaConnectionError::Busy)
        ));
        assert!(matches!(
            hub.configure_jev(JevConnectionRequest::Disconnect).await,
            Err(JevConnectionError::Busy)
        ));
        assert!(matches!(
            hub.configure_jev(JevConnectionRequest::Check {
                request_id: Uuid::new_v4(),
                allow_remote: false
            })
            .await,
            Err(JevConnectionError::Invalid)
        ));
        task.abort();
        let _ = task.await;
        provider.release.notify_one();
        assert!(
            hub.configure_laya(LayaConnectionRequest::Disconnect)
                .await
                .is_ok()
        );
    }
    #[tokio::test]
    async fn bounded_connections_never_echo_secrets_and_disconnect_survives_limit() {
        let provider = Arc::new(ConnectionFake {
            release: Notify::new(),
        });
        let hub = AdvisoryHub::new(provider.clone());
        for key in ["", "key\nSECRET", "key with spaces"] {
            assert!(matches!(
                hub.configure_laya(LayaConnectionRequest::Connect {
                    endpoint: "http://127.0.0.1:1".into(),
                    api_key: key.into()
                })
                .await,
                Err(LayaConnectionError::Invalid)
            ));
        }
        for _ in 0..16 {
            provider.release.notify_one();
            assert!(
                hub.configure_laya(LayaConnectionRequest::Check)
                    .await
                    .is_ok()
            );
        }
        assert!(matches!(
            hub.configure_laya(LayaConnectionRequest::Check).await,
            Err(LayaConnectionError::Limit)
        ));
        provider.release.notify_one();
        assert!(
            hub.configure_laya(LayaConnectionRequest::Disconnect)
                .await
                .is_ok()
        );
        assert!(
            !serde_json::to_string(&hub.snapshot())
                .unwrap()
                .contains("SECRET")
        );
    }

    struct Executable;
    #[async_trait]
    impl AdvisoryProvider for Executable {
        fn engines(&self) -> Vec<AdvisoryEngine> {
            vec![AdvisoryEngine {
                id: "fake".into(),
                configured: true,
                remote: false,
            }]
        }
        fn execution_target(&self, result: &AdvisoryResult) -> Option<ExecutionTarget> {
            Some(ExecutionTarget {
                profile: result.selected_profile.clone()?,
                provider_id: "dedicated".into(),
                workspace_name: "Example".into(),
                model_selector: "exact-model".into(),
                effort_selector: Some("low".into()),
                permission_mode: "one-shot approval".into(),
            })
        }
        async fn recommend(&self, _: &str, _: String) -> Option<AdvisoryResult> {
            Some(AdvisoryResult {
                selected_provider: Some("dedicated".into()),
                selected_profile: Some(crate::ConversationProfile {
                    id: "target".into(),
                    name: "Target".into(),
                    agent: "Agent".into(),
                    inference_provider: "Unknown".into(),
                    model: "Label".into(),
                    effort: Some("Low".into()),
                    selection: "advisory".into(),
                    model_effort_verified: false,
                }),
                probabilities: BTreeMap::new(),
                review_probability: None,
                confidence: None,
                probability_semantics: "Unknown".into(),
                fallback_reason: None,
            })
        }
    }
    fn planned_request(conversation: &crate::ConversationHub) -> AdvisoryRequest {
        conversation
            .mutate_plan(crate::PlanRequest {
                request_id: Uuid::new_v4(),
                revision: 0,
                operation: crate::PlanOperation::Create {
                    objective: "Calculator".into(),
                    steps: vec![crate::PlanStepDefinition {
                        title: "Backend".into(),
                        scope: "calculator.py".into(),
                        acceptance: "Tests pass".into(),
                    }],
                },
            })
            .unwrap();
        let plan = conversation.snapshot().project_plan;
        AdvisoryRequest {plan_link:Some(crate::PlanLink {plan_id:plan.plan.unwrap().id,revision:plan.revision,step_index:0}),
            objective:"Project objective: Calculator\nStep 1/1: Backend\nScope / files allowed: calculator.py\nAcceptance criteria: Tests pass\nWork only on this step. Explain the changes and checks performed. Do not start later steps. Urdia permissions still require the operator's decision.".into(),
            ..request(Uuid::new_v4(),"unused")}
    }
    fn copy_request(r: &AdvisoryRequest) -> AdvisoryRequest {
        AdvisoryRequest {
            request_id: r.request_id,
            engine: r.engine.clone(),
            objective: r.objective.clone(),
            allow_remote: r.allow_remote,
            plan_link: r.plan_link.clone(),
        }
    }
    #[tokio::test]
    async fn recommended_execution_binds_the_reviewed_step_and_replays_without_dispatch() {
        let hub = AdvisoryHub::new(Arc::new(Executable));
        let (conversation, mut commands) =
            crate::ConversationHub::with_context(crate::ConversationContext {
                provider_id: "default".into(),
                provider_name: "Default agent".into(),
                workspace_name: "project".into(),
                profile: None,
                startup_controls: None,
            });
        let r = planned_request(&conversation);
        let id = r.request_id;
        hub.submit_with_conversation(copy_request(&r), Some(&conversation))
            .unwrap();
        settle(&hub).await;
        let preview = hub
            .prepare_execution_with_conversation(id, Some(&conversation))
            .unwrap();
        assert_eq!(preview.plan_link, r.plan_link);
        assert_eq!(preview.objective, r.objective);
        assert!(commands.try_recv().is_err());
        let turn_id = Uuid::new_v4();
        let confirm = || ConfirmExecutionRequest {
            plan_id: preview.plan_id,
            turn_id,
        };
        hub.confirm_execution(confirm(), &conversation).unwrap();
        hub.confirm_execution(confirm(), &conversation).unwrap();
        assert!(
            matches!(commands.try_recv(),Ok(crate::ConversationCommand::Recommended {profile_id,text,..}) if profile_id=="target" && text==r.objective)
        );
        assert!(commands.try_recv().is_err());
        let snapshot = conversation.snapshot().project_plan;
        let step = &snapshot.plan.as_ref().unwrap().steps[0];
        assert_eq!(step.attempts, 1);
        assert_eq!(step.task.as_ref().unwrap().turn_id, turn_id);
        assert!(!step.reviewed);
        assert!(step.task.as_ref().unwrap().agent.is_none()); // queued advice is not the default launch
        conversation.set_execution(
            turn_id,
            crate::ConversationContext {
                provider_id: preview.target.provider_id.clone(),
                provider_name: "Actual selected agent".into(),
                workspace_name: preview.target.workspace_name.clone(),
                profile: Some(preview.target.profile.clone()),
                startup_controls: None,
            },
        );
        assert_eq!(
            conversation.snapshot().project_plan.plan.unwrap().steps[0]
                .task
                .as_ref()
                .unwrap()
                .profile
                .as_ref()
                .unwrap()
                .id,
            "target"
        );
        conversation.complete(turn_id).unwrap();
        let link = r.plan_link.clone().unwrap();
        conversation
            .mutate_plan(crate::PlanRequest {
                request_id: Uuid::new_v4(),
                revision: snapshot.revision,
                operation: crate::PlanOperation::Review {
                    plan_id: link.plan_id,
                    step_index: 0,
                    turn_id,
                    confirm_review: true,
                },
            })
            .unwrap();
        hub.submit_with_conversation(copy_request(&r), Some(&conversation))
            .unwrap();
        hub.confirm_execution(confirm(), &conversation).unwrap();
        assert!(commands.try_recv().is_err());
        assert!(
            !serde_json::to_string(&hub.snapshot())
                .unwrap()
                .contains("Tests pass")
        );
        let mut conflict = copy_request(&r);
        conflict.plan_link = None;
        assert!(matches!(
            hub.submit_with_conversation(conflict, Some(&conversation)),
            Err(AdvisorySubmitError::Conflict)
        ));
    }
    #[tokio::test]
    async fn linked_advice_requires_fresh_exact_step_and_stale_tickets_cannot_execute() {
        let hub = AdvisoryHub::new(Arc::new(Executable));
        let (conversation, mut commands) = crate::ConversationHub::new();
        let r = planned_request(&conversation);
        assert!(matches!(
            hub.submit(copy_request(&r)),
            Err(AdvisorySubmitError::Invalid)
        ));
        let mut changed = copy_request(&r);
        changed.objective.push_str("edited");
        assert!(matches!(
            hub.submit_with_conversation(changed, Some(&conversation)),
            Err(AdvisorySubmitError::Conflict)
        ));
        assert!(hub.snapshot().latest.is_none());
        let ordinary = Uuid::new_v4();
        conversation
            .submit_prompt(ordinary, "other task".into())
            .unwrap();
        commands.try_recv().unwrap();
        assert!(matches!(
            hub.submit_with_conversation(copy_request(&r), Some(&conversation)),
            Err(AdvisorySubmitError::Busy)
        ));
        assert!(hub.snapshot().latest.is_none());
        conversation.complete(ordinary).unwrap();
        hub.submit_with_conversation(copy_request(&r), Some(&conversation))
            .unwrap();
        settle(&hub).await;
        assert!(matches!(
            hub.prepare_execution(r.request_id),
            Err(ExecutionError::Unavailable)
        ));
        let preview = hub
            .prepare_execution_with_conversation(r.request_id, Some(&conversation))
            .unwrap();
        conversation
            .mutate_plan(crate::PlanRequest {
                request_id: Uuid::new_v4(),
                revision: 1,
                operation: crate::PlanOperation::Clear {
                    plan_id: r.plan_link.as_ref().unwrap().plan_id,
                },
            })
            .unwrap();
        hub.submit_with_conversation(copy_request(&r), Some(&conversation))
            .unwrap(); // exact advice replay makes no call
        assert!(matches!(
            hub.prepare_execution_with_conversation(r.request_id, Some(&conversation)),
            Err(ExecutionError::Stale)
        ));
        assert!(matches!(
            hub.confirm_execution(
                ConfirmExecutionRequest {
                    plan_id: preview.plan_id,
                    turn_id: Uuid::new_v4()
                },
                &conversation
            ),
            Err(ExecutionError::Stale)
        ));
        assert!(commands.try_recv().is_err());
        assert_eq!(conversation.snapshot().turns.len(), 1);
    }
    #[tokio::test]
    async fn confirmed_advice_and_manual_step_admission_have_one_winner() {
        for _ in 0..8 {
            let hub = AdvisoryHub::new(Arc::new(Executable));
            let (conversation, mut commands) = crate::ConversationHub::new();
            let r = planned_request(&conversation);
            hub.submit_with_conversation(copy_request(&r), Some(&conversation))
                .unwrap();
            settle(&hub).await;
            let preview = hub
                .prepare_execution_with_conversation(r.request_id, Some(&conversation))
                .unwrap();
            let barrier = Arc::new(std::sync::Barrier::new(3));
            let b = barrier.clone();
            let h = hub.clone();
            let c = conversation.clone();
            let recommend = std::thread::spawn(move || {
                b.wait();
                h.confirm_execution(
                    ConfirmExecutionRequest {
                        plan_id: preview.plan_id,
                        turn_id: Uuid::new_v4(),
                    },
                    &c,
                )
                .is_ok()
            });
            let b = barrier.clone();
            let c = conversation.clone();
            let manual = std::thread::spawn(move || {
                b.wait();
                c.submit_planned_prompt_with_revision(
                    Uuid::new_v4(),
                    r.objective,
                    None,
                    r.plan_link,
                )
                .is_ok()
            });
            barrier.wait();
            assert_ne!(recommend.join().unwrap(), manual.join().unwrap());
            commands.try_recv().unwrap();
            assert!(commands.try_recv().is_err());
            assert_eq!(
                conversation.snapshot().project_plan.plan.unwrap().steps[0].attempts,
                1
            );
        }
    }

    #[tokio::test]
    async fn execution_requires_review_and_confirmation_and_replays_once() {
        let hub = AdvisoryHub::new(Arc::new(Executable));
        let (conversation, mut commands) = crate::ConversationHub::new();
        let id = Uuid::new_v4();
        hub.submit(request(id, "ORIGINAL_PRIVATE_TASK")).unwrap();
        settle(&hub).await;
        let finished = hub.snapshot().latest.unwrap();
        assert!(finished.elapsed_ms.is_some());
        assert_eq!(
            hub.submit(request(id, "ORIGINAL_PRIVATE_TASK"))
                .unwrap()
                .elapsed_ms,
            finished.elapsed_ms
        );
        let preview = hub.prepare_execution(id).unwrap();
        assert_eq!(preview.objective, "ORIGINAL_PRIVATE_TASK");
        assert!(commands.try_recv().is_err());
        let turn_id = Uuid::new_v4();
        let confirm = || ConfirmExecutionRequest {
            plan_id: preview.plan_id,
            turn_id,
        };
        hub.confirm_execution(confirm(), &conversation).unwrap();
        hub.confirm_execution(confirm(), &conversation).unwrap();
        assert!(
            matches!(commands.try_recv(), Ok(crate::ConversationCommand::Recommended { profile_id, text, .. }) if profile_id == "target" && text == "ORIGINAL_PRIVATE_TASK")
        );
        assert!(commands.try_recv().is_err());
        assert!(matches!(
            hub.confirm_execution(
                ConfirmExecutionRequest {
                    plan_id: preview.plan_id,
                    turn_id: Uuid::new_v4()
                },
                &conversation
            ),
            Err(ExecutionError::Conflict)
        ));
        assert!(
            conversation
                .submit_prompt(turn_id, "ORIGINAL_PRIVATE_TASK".into())
                .is_err()
        );
        assert!(
            conversation
                .submit_recommended(
                    turn_id,
                    "ORIGINAL_PRIVATE_TASK".into(),
                    "different-target".into()
                )
                .is_err()
        );
        assert!(
            !serde_json::to_string(&hub.snapshot())
                .unwrap()
                .contains("ORIGINAL_PRIVATE_TASK")
        );
    }
    #[tokio::test]
    async fn busy_confirmation_is_retryable_but_expired_stale_and_fallback_advice_are_blocked() {
        let hub = AdvisoryHub::new(Arc::new(Executable));
        let (conversation, mut commands) = crate::ConversationHub::new();
        let id = Uuid::new_v4();
        hub.submit(request(id, "task")).unwrap();
        settle(&hub).await;
        let preview = hub.prepare_execution(id).unwrap();
        let turn_id = Uuid::new_v4();
        let active = Uuid::new_v4();
        conversation.submit_prompt(active, "manual".into()).unwrap();
        commands.try_recv().unwrap();
        assert!(matches!(
            hub.confirm_execution(
                ConfirmExecutionRequest {
                    plan_id: preview.plan_id,
                    turn_id
                },
                &conversation
            ),
            Err(ExecutionError::Busy)
        ));
        conversation.complete(active).unwrap();
        hub.state
            .lock()
            .unwrap()
            .records
            .get_mut(&id)
            .unwrap()
            .plan
            .as_mut()
            .unwrap()
            .created = Instant::now() - Duration::from_secs(301);
        assert!(matches!(
            hub.confirm_execution(
                ConfirmExecutionRequest {
                    plan_id: preview.plan_id,
                    turn_id
                },
                &conversation
            ),
            Err(ExecutionError::Stale)
        ));
        assert!(commands.try_recv().is_err());
        let next = Uuid::new_v4();
        hub.submit(request(next, "next task")).unwrap();
        settle(&hub).await;
        assert!(matches!(
            hub.prepare_execution(id),
            Err(ExecutionError::Stale)
        ));
        hub.state
            .lock()
            .unwrap()
            .records
            .get_mut(&next)
            .unwrap()
            .job
            .result
            .as_mut()
            .unwrap()
            .fallback_reason = Some("no_selection".into());
        assert!(matches!(
            hub.prepare_execution(next),
            Err(ExecutionError::Unavailable)
        ));
    }

    struct Fake {
        calls: AtomicUsize,
        release: Notify,
    }
    #[async_trait]
    impl AdvisoryProvider for Fake {
        fn engines(&self) -> Vec<AdvisoryEngine> {
            vec![AdvisoryEngine {
                id: "fake".into(),
                configured: true,
                remote: false,
            }]
        }
        async fn recommend(&self, _: &str, _: String) -> Option<AdvisoryResult> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.release.notified().await;
            None
        }
    }
    fn request(id: Uuid, objective: &str) -> AdvisoryRequest {
        AdvisoryRequest {
            plan_link: None,
            request_id: id,
            engine: "fake".into(),
            objective: objective.into(),
            allow_remote: false,
        }
    }

    async fn settle(hub: &AdvisoryHub) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while hub
                .snapshot()
                .latest
                .as_ref()
                .is_some_and(|job| job.status == "running")
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("advice must reach a terminal state");
    }

    struct Immediate {
        remote: bool,
    }
    #[async_trait]
    impl AdvisoryProvider for Immediate {
        fn engines(&self) -> Vec<AdvisoryEngine> {
            vec![AdvisoryEngine {
                id: "fake".into(),
                configured: true,
                remote: self.remote,
            }]
        }
        async fn recommend(&self, _: &str, _: String) -> Option<AdvisoryResult> {
            None
        }
    }

    #[tokio::test]
    async fn remote_consent_is_host_enforced_and_accepted_ids_are_bounded() {
        let hub = AdvisoryHub::new(Arc::new(Immediate { remote: true }));
        assert!(matches!(
            hub.submit(request(Uuid::new_v4(), "task")),
            Err(AdvisorySubmitError::Unavailable)
        ));
        for _ in 0..64 {
            let mut next = request(Uuid::new_v4(), "task");
            next.allow_remote = true;
            assert!(hub.submit(next).is_ok());
            settle(&hub).await;
        }
        let mut next = request(Uuid::new_v4(), "task");
        next.allow_remote = true;
        assert!(matches!(hub.submit(next), Err(AdvisorySubmitError::Limit)));
    }

    #[tokio::test]
    async fn deadline_releases_the_busy_slot_and_keeps_failed_requests_idempotent() {
        let provider = Arc::new(Fake {
            calls: AtomicUsize::new(0),
            release: Notify::new(),
        });
        let mut hub = AdvisoryHub::new(provider.clone());
        hub.deadline = Duration::from_millis(5);
        let id = Uuid::new_v4();
        hub.submit(request(id, "timeout task")).unwrap();
        settle(&hub).await;
        let finished = hub.snapshot().latest.unwrap();
        assert_eq!(finished.status, "failed");
        assert!(finished.elapsed_ms.is_some());
        let calls = provider.calls.load(Ordering::SeqCst);
        assert!(calls <= 1);
        let replay = hub.submit(request(id, "timeout task")).unwrap();
        assert_eq!(replay.elapsed_ms, finished.elapsed_ms);
        assert_eq!(provider.calls.load(Ordering::SeqCst), calls);
        assert!(hub.submit(request(Uuid::new_v4(), "next")).is_ok());
    }

    #[tokio::test]
    async fn duplicate_and_conflicting_keys_never_repeat_calls_and_busy_is_bounded() {
        let provider = Arc::new(Fake {
            calls: AtomicUsize::new(0),
            release: Notify::new(),
        });
        let hub = AdvisoryHub::new(provider.clone());
        let id = Uuid::new_v4();
        assert!(
            hub.submit(request(id, "PRIVATE_OBJECTIVE"))
                .unwrap()
                .elapsed_ms
                .is_none()
        );
        assert!(hub.submit(request(id, "PRIVATE_OBJECTIVE")).is_ok());
        assert!(matches!(
            hub.submit(request(id, "changed")),
            Err(AdvisorySubmitError::Conflict)
        ));
        assert!(matches!(
            hub.submit(request(Uuid::new_v4(), "second")),
            Err(AdvisorySubmitError::Busy)
        ));
        tokio::time::timeout(Duration::from_secs(1), async {
            while provider.calls.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the first provider call must start");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert!(
            !serde_json::to_string(&hub.snapshot())
                .unwrap()
                .contains("PRIVATE_OBJECTIVE")
        );
        provider.release.notify_one();
        settle(&hub).await;
        assert_eq!(hub.snapshot().latest.unwrap().status, "failed");
        assert!(hub.submit(request(id, "PRIVATE_OBJECTIVE")).is_ok());
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
        assert!(hub.submit(request(Uuid::new_v4(), "next")).is_ok());
    }

    #[tokio::test]
    async fn invalid_input_and_unconfigured_engines_never_reach_the_provider() {
        let provider = Arc::new(Fake {
            calls: AtomicUsize::new(0),
            release: Notify::new(),
        });
        let hub = AdvisoryHub::new(provider.clone());
        for objective in ["".to_owned(), " ".to_owned(), "x".repeat(16_385)] {
            assert!(matches!(
                hub.submit(request(Uuid::new_v4(), &objective)),
                Err(AdvisorySubmitError::Invalid)
            ));
        }
        let mut unknown = request(Uuid::new_v4(), "valid");
        unknown.engine = "unconfigured".into();
        assert!(matches!(
            hub.submit(unknown),
            Err(AdvisorySubmitError::Unavailable)
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    }
}
