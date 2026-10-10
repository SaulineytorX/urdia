//! Explicit, bounded, prompt-free ACP selector inspection. Never grants permissions.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use urdia_core::{
    AgentProvider, PermissionReceiptSink, ProviderRegistry, SessionControls, SessionRequest,
    WorkspacePath,
};
use urdia_ui::{
    ControlProbeError as Error, ControlProbeRequest as Request, ControlProbeResult as Response,
};
use uuid::Uuid;
type Outcome = std::result::Result<Response, Error>;
struct Record {
    provider: String,
    model: Option<String>,
    result: Option<Outcome>,
}
#[derive(Default)]
struct State {
    records: BTreeMap<Uuid, Record>,
    active: bool,
}
pub(crate) struct ControlProbe {
    providers: Mutex<BTreeMap<String, Arc<dyn AgentProvider>>>,
    workspace: WorkspacePath,
    receipts: Arc<dyn PermissionReceiptSink>,
    state: Mutex<State>,
    deadline: Duration,
}
struct Completion {
    host: Arc<ControlProbe>,
    id: Uuid,
}
impl Drop for Completion {
    fn drop(&mut self) {
        if let Ok(mut state) = self.host.state.lock()
            && let Some(record) = state.records.get_mut(&self.id)
            && record.result.is_none()
        {
            record.result = Some(Err(Error::Failed));
            state.active = false;
        }
    }
}
impl ControlProbe {
    pub fn new(
        registry: &ProviderRegistry,
        workspace: WorkspacePath,
        receipts: Arc<dyn PermissionReceiptSink>,
    ) -> Self {
        Self {
            providers: Mutex::new(
                registry
                    .list()
                    .into_iter()
                    .filter(|info| info.availability.is_available())
                    .filter_map(|info| {
                        registry
                            .resolve(&info.id)
                            .ok()
                            .map(|provider| (info.id, provider))
                    })
                    .collect(),
            ),
            workspace,
            receipts,
            state: Mutex::new(State::default()),
            deadline: Duration::from_secs(30),
        }
    }
    /// Append-only, explicitly persisted launches are eligible for inspection only.
    pub fn stage_provider(&self, provider: Arc<dyn AgentProvider>) {
        let info = provider.info();
        if info.availability.is_available() {
            self.providers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry(info.id)
                .or_insert(provider);
        }
    }
    pub async fn inspect(self: &Arc<Self>, request: Request) -> Outcome {
        let controls = SessionControls {
            model: request.model.clone(),
            effort: None,
        };
        if !request.confirm_start || request.provider.len() > 128 || controls.validate().is_err() {
            return Err(Error::Invalid);
        }
        let provider = self
            .providers
            .lock()
            .map_err(|_| Error::Failed)?
            .get(&request.provider)
            .ok_or(Error::Unavailable)?
            .clone();
        {
            let mut state = self.state.lock().map_err(|_| Error::Failed)?;
            if let Some(record) = state.records.get(&request.request_id) {
                if record.provider != request.provider || record.model != request.model {
                    return Err(Error::Conflict);
                }
                return record.result.clone().unwrap_or(Err(Error::Busy));
            }
            if state.active {
                return Err(Error::Busy);
            }
            if state.records.len() >= 8 {
                return Err(Error::Limit);
            }
            state.records.insert(
                request.request_id,
                Record {
                    provider: request.provider.clone(),
                    model: request.model.clone(),
                    result: None,
                },
            );
            state.active = true;
        }
        let this = Arc::clone(self);
        // Finish/cache even if the HTTP client disconnects. Dropped sessions close ACP.
        tokio::spawn(async move {
            let _completion = Completion {
                host: this.clone(),
                id: request.request_id,
            };
            let result = tokio::time::timeout(this.deadline, async {
                let session = provider
                    .start_session(
                        SessionRequest::new(this.workspace.clone())
                            .controls(controls)
                            .permission_receipt_sink(this.receipts.clone()),
                    )
                    .await
                    .map_err(|_| Error::Failed)?;
                let options = session.control_options().ok_or(Error::Unavailable)?;
                // Validate third-party trait implementations too; never trust arbitrary catalogs.
                let valid = |values: &Vec<String>| {
                    values.len() <= 128
                        && values.iter().all(|v| urdia_core::valid_control_id(v))
                        && values
                            .iter()
                            .collect::<std::collections::BTreeSet<_>>()
                            .len()
                            == values.len()
                };
                if options.models.is_empty() && options.efforts.is_empty() {
                    return Err(Error::Unavailable);
                }
                if !valid(&options.models)
                    || !valid(&options.efforts)
                    || options
                        .current
                        .model
                        .as_ref()
                        .is_some_and(|v| !options.models.contains(v))
                    || options
                        .current
                        .effort
                        .as_ref()
                        .is_some_and(|v| !options.efforts.contains(v))
                    || request
                        .model
                        .as_ref()
                        .is_some_and(|v| options.current.model.as_ref() != Some(v))
                {
                    return Err(Error::Failed);
                }
                Ok(Response {
                    request_id: request.request_id,
                    provider: request.provider.clone(),
                    requested_model: request.model.clone(),
                    options,
                })
            })
            .await
            .unwrap_or(Err(Error::Timeout));
            if let Ok(mut state) = this.state.lock() {
                state.active = false;
                if let Some(record) = state.records.get_mut(&request.request_id) {
                    record.result = Some(result.clone());
                }
            }
            result
        })
        .await
        .unwrap_or(Err(Error::Failed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use urdia_core::{
        AgentSession, ControlOptions, DiscardPermissionReceiptSink, Prompt, PromptResult,
        ProviderInfo, Result, SessionControlState, SessionId,
    };
    struct Provider {
        starts: Arc<AtomicUsize>,
        closed: Arc<AtomicUsize>,
        malformed: bool,
        delay: Duration,
    }
    #[async_trait]
    impl AgentProvider for Provider {
        fn info(&self) -> ProviderInfo {
            let mut info = urdia_provider_qwen::QwenProvider::new().info();
            info.id = "probe".into();
            info.availability = urdia_core::ProviderAvailability::Available;
            info
        }
        async fn start_session(&self, request: SessionRequest) -> Result<Box<dyn AgentSession>> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            let model = request.controls.model.unwrap_or("a".into());
            Ok(Box::new(Session {
                id: SessionId::new(),
                closed: self.closed.clone(),
                options: ControlOptions {
                    models: if self.malformed {
                        vec!["PRIVATE\nINVALID".into()]
                    } else {
                        vec!["a".into(), "b".into()]
                    },
                    efforts: vec![if model == "b" { "medium" } else { "low" }.into()],
                    current: SessionControlState {
                        model: Some(model.clone()),
                        effort: Some(if model == "b" { "medium" } else { "low" }.into()),
                    },
                },
            }))
        }
    }
    struct Session {
        id: SessionId,
        closed: Arc<AtomicUsize>,
        options: ControlOptions,
    }
    impl Drop for Session {
        fn drop(&mut self) {
            self.closed.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[async_trait]
    impl AgentSession for Session {
        fn id(&self) -> &SessionId {
            &self.id
        }
        fn control_options(&self) -> Option<ControlOptions> {
            Some(self.options.clone())
        }
        async fn prompt(&self, _: Prompt) -> Result<PromptResult> {
            panic!("inspection must never send a prompt")
        }
        async fn cancel(&self) -> Result<()> {
            Ok(())
        }
    }
    fn fixture(
        delay: Duration,
        malformed: bool,
    ) -> (
        Arc<ControlProbe>,
        Arc<AtomicUsize>,
        Arc<AtomicUsize>,
        tempfile::TempDir,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let starts = Arc::new(AtomicUsize::new(0));
        let closed = Arc::new(AtomicUsize::new(0));
        let mut registry = ProviderRegistry::new();
        registry
            .register_owned(Provider {
                starts: starts.clone(),
                closed: closed.clone(),
                malformed,
                delay,
            })
            .unwrap();
        let probe = Arc::new(ControlProbe::new(
            &registry,
            WorkspacePath::new(dir.path()).unwrap(),
            Arc::new(DiscardPermissionReceiptSink),
        ));
        (probe, starts, closed, dir)
    }
    fn request(model: Option<&str>) -> Request {
        Request {
            request_id: Uuid::new_v4(),
            provider: "probe".into(),
            model: model.map(str::to_owned),
            confirm_start: true,
        }
    }
    #[tokio::test]
    async fn replay_is_scope_bound_prompt_free_and_sessions_close() {
        let (probe, starts, closed, _dir) = fixture(Duration::ZERO, false);
        let original = request(Some("b"));
        let result = probe.inspect(original.clone()).await.unwrap();
        assert_eq!(result.options.efforts, vec!["medium"]);
        probe.inspect(original.clone()).await.unwrap();
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(closed.load(Ordering::SeqCst), 1);
        let mut changed = original;
        changed.model = Some("a".into());
        assert!(matches!(probe.inspect(changed).await, Err(Error::Conflict)));
        let mut unconfirmed = request(None);
        unconfirmed.confirm_start = false;
        assert!(matches!(
            probe.inspect(unconfirmed).await,
            Err(Error::Invalid)
        ));
        for _ in 1..8 {
            probe.inspect(request(None)).await.unwrap();
        }
        assert!(matches!(
            probe.inspect(request(None)).await,
            Err(Error::Limit)
        ));
        assert_eq!(starts.load(Ordering::SeqCst), 8);
    }
    #[tokio::test]
    async fn concurrent_probes_and_disconnected_clients_are_bounded() {
        let (probe, starts, closed, _dir) = fixture(Duration::from_millis(50), false);
        let original = request(None);
        let saved = original.clone();
        let host = probe.clone();
        let call = tokio::spawn(async move { host.inspect(original).await });
        while starts.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        assert!(matches!(
            probe.inspect(saved.clone()).await,
            Err(Error::Busy)
        ));
        assert!(matches!(
            probe.inspect(request(None)).await,
            Err(Error::Busy)
        ));
        call.abort();
        tokio::time::sleep(Duration::from_millis(100)).await;
        probe.inspect(saved).await.unwrap();
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(closed.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn malformed_options_and_timeouts_fail_redacted_and_are_cached() {
        let (probe, _, closed, _dir) = fixture(Duration::ZERO, true);
        assert!(matches!(
            probe.inspect(request(None)).await,
            Err(Error::Failed)
        ));
        assert_eq!(closed.load(Ordering::SeqCst), 1);
        let (mut probe, starts, _, _dir) = fixture(Duration::from_millis(50), false);
        Arc::get_mut(&mut probe).unwrap().deadline = Duration::from_millis(5);
        let original = request(None);
        assert!(matches!(
            probe.inspect(original.clone()).await,
            Err(Error::Timeout)
        ));
        assert!(matches!(probe.inspect(original).await, Err(Error::Timeout)));
        assert_eq!(starts.load(Ordering::SeqCst), 1);
    }
}
