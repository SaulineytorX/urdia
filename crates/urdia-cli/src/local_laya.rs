//! Session-only local decision connection. No process installation or privilege changes.
use std::sync::{Arc, Mutex};
use urdia_core::{DecisionInput, DecisionProvider};
use urdia_decision_laya::LayaDecisionProvider;
use urdia_routing::RouteResolver;
use urdia_ui::{LayaConnectionError, LayaConnectionRequest, LayaConnectionView};

struct Connection {
    provider: Arc<LayaDecisionProvider>,
    view: LayaConnectionView,
}
#[derive(Default)]
pub struct LocalLaya {
    connection: Mutex<Option<Connection>>,
}
impl LocalLaya {
    pub fn from_environment() -> Self {
        let connection = LayaDecisionProvider::from_env()
            .ok()
            .map(|provider| Connection {
                view: LayaConnectionView {
                    endpoint: Some(provider.endpoint().to_string()),
                    status: "configured",
                },
                provider: Arc::new(provider),
            });
        Self {
            connection: Mutex::new(connection),
        }
    }
    pub fn view(&self) -> LayaConnectionView {
        self.connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|entry| entry.view.clone())
            .unwrap_or(LayaConnectionView {
                endpoint: None,
                status: "disconnected",
            })
    }
    pub fn resolver(&self) -> Option<RouteResolver> {
        self.connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|entry| RouteResolver::new(entry.provider.clone()))
    }
    pub async fn configure(
        &self,
        request: LayaConnectionRequest,
    ) -> Result<LayaConnectionView, LayaConnectionError> {
        let provider = match request {
            LayaConnectionRequest::Disconnect => {
                *self
                    .connection
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
                return Ok(self.view());
            }
            LayaConnectionRequest::Check => self
                .connection
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .map(|entry| entry.provider.clone())
                .ok_or(LayaConnectionError::Unavailable)?,
            LayaConnectionRequest::Connect { endpoint, api_key } => Arc::new(
                LayaDecisionProvider::with_endpoint(api_key, endpoint)
                    .map_err(|_| LayaConnectionError::Invalid)?,
            ),
        };
        // /health in pinned Laya 0.3.20 is unauthenticated. Test the actual typed,
        // authenticated English endpoint instead; no project objective is sent.
        provider.decide(DecisionInput {
            objective: "Connection check only. Select connection_check. This authorizes no action.".into(),
            candidates: vec!["connection_check".into()],
            state: serde_json::json!({"candidate_descriptions":{"connection_check":"Confirm this local typed decision endpoint responds."}}),
        }).await.map_err(|_| LayaConnectionError::Failed)?;
        let view = LayaConnectionView {
            endpoint: Some(provider.endpoint().to_string()),
            status: "connected",
        };
        *self
            .connection
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Connection {
            provider,
            view: view.clone(),
        });
        Ok(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn invalid_origins_never_become_configured_and_disconnect_is_local() {
        let local = LocalLaya::default();
        for endpoint in [
            "https://example.com",
            "http://0.0.0.0:18766",
            "http://localhost:18766",
            "http://key@127.0.0.1:18766",
        ] {
            assert!(matches!(
                local
                    .configure(LayaConnectionRequest::Connect {
                        endpoint: endpoint.into(),
                        api_key: "SECRET_SENTINEL".into()
                    })
                    .await,
                Err(LayaConnectionError::Invalid)
            ));
            assert_eq!(local.view().status, "disconnected");
        }
        assert!(matches!(
            local.configure(LayaConnectionRequest::Check).await,
            Err(LayaConnectionError::Unavailable)
        ));
        assert_eq!(
            local
                .configure(LayaConnectionRequest::Disconnect)
                .await
                .unwrap()
                .status,
            "disconnected"
        );
    }
}
