//! Host-only Jev key lifecycle. Explicit tests, no project objective or permission changes.
use crate::credential_store::{CredentialStore, OsCredentialStore};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use urdia_core::{DecisionInput, DecisionProvider};
use urdia_decision_jev::JevDecisionProvider;
use urdia_routing::RouteResolver;
use urdia_ui::{
    JevConnectionError as Error, JevConnectionRequest as Request, JevConnectionView as View,
};
use uuid::Uuid;
struct Connection {
    provider: Arc<JevDecisionProvider>,
    status: &'static str,
    remembered: bool,
}
#[derive(Default)]
struct State {
    connection: Option<Connection>,
    receipts: BTreeMap<Uuid, (u64, Result<(), Error>)>,
}
pub struct LocalJev {
    #[cfg(test)]
    endpoint: Option<String>,
    state: Mutex<State>,
    store: Arc<dyn CredentialStore>,
}
impl LocalJev {
    pub fn new(load_host_configuration: bool) -> Self {
        let provider = load_host_configuration
            .then(JevDecisionProvider::from_env)
            .and_then(Result::ok);
        Self {
            #[cfg(test)]
            endpoint: None,
            state: Mutex::new(State {
                connection: provider.map(|p| Connection {
                    provider: Arc::new(p),
                    status: "configured",
                    remembered: false,
                }),
                ..State::default()
            }),
            store: Arc::new(OsCredentialStore::detect()),
        }
    }
    pub fn view(&self) -> View {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        View {
            status: state
                .connection
                .as_ref()
                .map_or("disconnected", |p| p.status),
            remembered: state.connection.as_ref().is_some_and(|p| p.remembered),
            storage_kind: self.store.kind(),
        }
    }
    pub fn resolver(&self) -> Option<RouteResolver> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .connection
            .as_ref()
            .map(|p| RouteResolver::new(p.provider.clone()))
    }
    pub fn profile_resolver(&self) -> Option<RouteResolver> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .connection
            .as_ref()
            .map(|connection| {
                RouteResolver::new(Arc::new(
                    connection
                        .provider
                        .as_ref()
                        .clone()
                        .with_profile_preferences(),
                ))
            })
    }
    pub async fn configure(&self, request: Request) -> Result<View, Error> {
        match &request {
            Request::Connect {
                api_key,
                allow_remote,
                ..
            } if !allow_remote
                || api_key.is_empty()
                || api_key.len() > 4096
                || api_key.bytes().any(|v| !(33..=126).contains(&v)) =>
            {
                return Err(Error::Invalid);
            }
            Request::Check { allow_remote, .. } | Request::Restore { allow_remote, .. }
                if !allow_remote =>
            {
                return Err(Error::Invalid);
            }
            _ => {}
        }
        let (id, fingerprint) = match &request {
            Request::Connect {
                request_id,
                api_key,
                remember,
                ..
            } => {
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                ("connect", api_key, remember).hash(&mut hash);
                (Some(*request_id), hash.finish())
            }
            Request::Check { request_id, .. } => (Some(*request_id), 1),
            Request::Restore { request_id, .. } => (Some(*request_id), 2),
            _ => (None, 0),
        };
        if let Some(id) = id {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some((old, result)) = state.receipts.get(&id) {
                if *old != fingerprint {
                    return Err(Error::Invalid);
                }
                let result = *result;
                drop(state);
                return result.map(|()| self.view());
            }
            if state.receipts.len() >= 16 {
                return Err(Error::Limit);
            }
            // Reserve before any external effect. Cancellation never permits a repeated paid probe.
            state.receipts.insert(id, (fingerprint, Err(Error::Failed)));
        }
        let result = self.apply(request).await;
        if let Some(id) = id {
            self.state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .receipts
                .insert(id, (fingerprint, result));
        }
        result.map(|()| self.view())
    }
    async fn apply(&self, request: Request) -> Result<(), Error> {
        let (provider, remember) = match request {
            Request::Disconnect => {
                self.clear();
                return Ok(());
            }
            Request::Forget => {
                self.store.delete().await.map_err(|()| Error::Storage)?;
                self.clear();
                return Ok(());
            }
            Request::Check { .. } => {
                let state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let p = state.connection.as_ref().ok_or(Error::Unavailable)?;
                (p.provider.clone(), p.remembered)
            }
            Request::Restore { .. } => {
                let key = self
                    .store
                    .read()
                    .await
                    .map_err(|()| Error::Storage)?
                    .ok_or(Error::Unavailable)?;
                (Arc::new(self.provider(key)?), true)
            }
            Request::Connect {
                api_key, remember, ..
            } => {
                if remember && self.store.kind().is_none() {
                    return Err(Error::Storage);
                }
                if remember && api_key.len() > self.store.max_key_bytes() {
                    return Err(Error::Invalid);
                }
                let provider = Arc::new(self.provider(api_key.clone())?);
                self.probe(&provider).await?;
                if remember {
                    self.store
                        .write(&api_key)
                        .await
                        .map_err(|()| Error::Storage)?;
                }
                self.state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .connection = Some(Connection {
                    provider,
                    status: "connected",
                    remembered: remember,
                });
                return Ok(());
            }
        };
        self.probe(&provider).await?;
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .connection = Some(Connection {
            provider,
            status: "connected",
            remembered: remember,
        });
        Ok(())
    }
    fn clear(&self) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .connection = None;
    }
    fn provider(&self, key: String) -> Result<JevDecisionProvider, Error> {
        #[cfg(test)]
        if let Some(endpoint) = &self.endpoint {
            return JevDecisionProvider::with_endpoint(key, endpoint).map_err(|_| Error::Invalid);
        }
        JevDecisionProvider::new(key).map_err(|_| Error::Invalid)
    }
    async fn probe(&self, provider: &JevDecisionProvider) -> Result<(), Error> {
        provider.decide(DecisionInput { objective:"Connection check only. Select connection_check. This authorizes no action.".into(), candidates:vec!["connection_check".into()], state:serde_json::json!({"candidate_descriptions":{"connection_check":"Confirm this authenticated typed endpoint responds. No project task is included."}}) }).await.map(|_| ()).map_err(|_| Error::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[derive(Default)]
    struct Vault {
        key: Mutex<Option<String>>,
        fail: bool,
    }
    #[async_trait]
    impl CredentialStore for Vault {
        fn kind(&self) -> Option<&'static str> {
            Some("test_vault")
        }
        fn max_key_bytes(&self) -> usize {
            32
        }
        async fn read(&self) -> Result<Option<String>, ()> {
            if self.fail {
                Err(())
            } else {
                Ok(self.key.lock().unwrap().clone())
            }
        }
        async fn write(&self, key: &str) -> Result<(), ()> {
            if self.fail {
                Err(())
            } else {
                *self.key.lock().unwrap() = Some(key.into());
                Ok(())
            }
        }
        async fn delete(&self) -> Result<(), ()> {
            if self.fail {
                Err(())
            } else {
                *self.key.lock().unwrap() = None;
                Ok(())
            }
        }
    }
    async fn fixture(
        vault: Arc<Vault>,
        success: bool,
    ) -> (LocalJev, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = vec![0; 16384];
                let _ = stream.read(&mut bytes).await.unwrap();
                calls.fetch_add(1, Ordering::SeqCst);
                let body = serde_json::json!({"model":"jev-1.13.0","answers":{"route":{"type":"choice","choice":"connection_check","probabilities":{"connection_check":1.0,"__urdia_escalate__":0.0},"confidence":1.0}},"usage":{"input_tokens":1,"output_tokens":1}}).to_string();
                let status = if success {
                    "200 OK"
                } else {
                    "401 Unauthorized"
                };
                stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
        });
        let mut local = LocalJev::new(false);
        local.store = vault;
        local.endpoint = Some(format!("http://{address}"));
        (local, count, task)
    }
    fn connect(id: Uuid, remember: bool) -> Request {
        Request::Connect {
            request_id: id,
            api_key: "TEST_JEV_KEY_SENTINEL".into(),
            remember,
            allow_remote: true,
        }
    }
    #[tokio::test]
    async fn remembered_key_replays_without_another_probe_and_survives_new_host() {
        let vault = Arc::new(Vault::default());
        let (local, count, task) = fixture(vault.clone(), true).await;
        let id = Uuid::new_v4();
        let view = local.configure(connect(id, true)).await.unwrap();
        assert!(view.remembered);
        assert!(
            !serde_json::to_string(&view)
                .unwrap()
                .contains("TEST_JEV_KEY_SENTINEL")
        );
        local.configure(connect(id, true)).await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            local.configure(connect(id, false)).await.err(),
            Some(Error::Invalid)
        );
        local.configure(Request::Disconnect).await.unwrap();
        assert!(local.resolver().is_none());
        assert!(vault.key.lock().unwrap().is_some());
        let (restarted, second_count, second_task) = fixture(vault.clone(), true).await;
        assert!(restarted.resolver().is_none());
        assert_eq!(second_count.load(Ordering::SeqCst), 0);
        restarted
            .configure(Request::Restore {
                request_id: Uuid::new_v4(),
                allow_remote: true,
            })
            .await
            .unwrap();
        assert!(restarted.view().remembered);
        restarted.configure(Request::Forget).await.unwrap();
        assert!(vault.key.lock().unwrap().is_none());
        assert!(restarted.resolver().is_none());
        task.abort();
        second_task.abort();
    }
    #[tokio::test]
    async fn session_only_failure_and_limits_never_silently_persist_or_repeat() {
        let vault = Arc::new(Vault::default());
        let (local, count, task) = fixture(vault.clone(), true).await;
        for _ in 0..16 {
            local
                .configure(connect(Uuid::new_v4(), false))
                .await
                .unwrap();
        }
        assert!(vault.key.lock().unwrap().is_none());
        assert!(!local.view().remembered);
        assert_eq!(
            local.configure(connect(Uuid::new_v4(), true)).await.err(),
            Some(Error::Limit)
        );
        assert_eq!(count.load(Ordering::SeqCst), 16);
        local.configure(Request::Disconnect).await.unwrap();
        local.configure(Request::Forget).await.unwrap();
        task.abort();
        let (failed, count, task) = fixture(vault, false).await;
        let id = Uuid::new_v4();
        assert_eq!(
            failed.configure(connect(id, true)).await.err(),
            Some(Error::Failed)
        );
        assert_eq!(
            failed.configure(connect(id, true)).await.err(),
            Some(Error::Failed)
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(failed.resolver().is_none());
        task.abort();
    }
    #[tokio::test]
    async fn failed_storage_is_not_reported_as_remembered_or_connected() {
        let vault = Arc::new(Vault {
            fail: true,
            ..Vault::default()
        });
        let (local, count, task) = fixture(vault, true).await;
        assert_eq!(
            local
                .configure(Request::Connect {
                    request_id: Uuid::new_v4(),
                    api_key: "x".repeat(33),
                    remember: true,
                    allow_remote: true
                })
                .await
                .err(),
            Some(Error::Invalid)
        );
        assert_eq!(count.load(Ordering::SeqCst), 0);
        let id = Uuid::new_v4();
        assert_eq!(
            local.configure(connect(id, true)).await.err(),
            Some(Error::Storage)
        );
        assert_eq!(
            local.configure(connect(id, true)).await.err(),
            Some(Error::Storage)
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(local.view().status, "disconnected");
        assert!(!local.view().remembered);
        task.abort();
    }
}
