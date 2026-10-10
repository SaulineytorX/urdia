//! Local TypeLLM routing for Urdia.
//!
//! The adapter talks to Urdia's authenticated loopback bridge. TypeLLM and
//! SGLang remain outside the Rust process, and every result stays advisory.

use async_trait::async_trait;
use reqwest::{Client, Response, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::{Debug, Formatter};
use std::fs::File;
use std::io::Read;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;
use urdia_core::{
    Decision, DecisionInput, DecisionProvider, ESCALATION_CANDIDATE_ID, ProbabilitySemantics,
    Result, UrdiaError,
};

pub const PROVIDER_ID: &str = "typellm";
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:18767";
pub const MAX_PROVIDER_CANDIDATES: usize = 23;
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;
pub const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

const MAX_ATTEMPTS: usize = 3;
const MAX_RETRY_DELAY: Duration = Duration::from_secs(2);
const RETRY_BACKOFF: Duration = Duration::from_millis(50);
const TOP_PROBABILITY_EPSILON: f64 = 0.000_1;
const ROUNDING_ERROR_PER_PROBABILITY: f64 = 0.000_05;
const MAX_CANDIDATE_ID_BYTES: usize = 256;
const MAX_API_KEY_BYTES: u64 = 4 * 1024;
const MAX_MODEL_BYTES: usize = 512;

#[derive(Clone)]
pub struct TypeLlmDecisionProvider {
    client: Client,
    api_key: String,
    endpoint: Url,
    model: String,
}

impl Debug for TypeLlmDecisionProvider {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TypeLlmDecisionProvider")
            .field("model", &self.model)
            .field("endpoint", &self.endpoint)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

impl TypeLlmDecisionProvider {
    /// Read the model and bridge credential from environment or systemd.
    pub fn from_env() -> Result<Self> {
        let model = env::var("URDIA_TYPELLM_MODEL")
            .map_err(|_| TypeLlmFailure::Configuration.into_urdia_error())?;
        let api_key = load_api_key(
            env::var_os("URDIA_TYPELLM_API_KEY"),
            env::var_os("CREDENTIALS_DIRECTORY"),
        )
        .map_err(TypeLlmFailure::into_urdia_error)?;
        let endpoint = match env::var_os("URDIA_TYPELLM_URL") {
            Some(value) => value
                .into_string()
                .map_err(|_| TypeLlmFailure::Configuration.into_urdia_error())?
                .parse::<Url>()
                .map_err(|_| TypeLlmFailure::Configuration.into_urdia_error())?,
            None => Url::parse(DEFAULT_ENDPOINT).expect("built-in TypeLLM endpoint must parse"),
        };
        Self::build(api_key, model, endpoint, REQUEST_TIMEOUT)
            .map_err(TypeLlmFailure::into_urdia_error)
    }

    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Result<Self> {
        let endpoint = Url::parse(DEFAULT_ENDPOINT).expect("built-in TypeLLM endpoint must parse");
        Self::build(api_key.into(), model.into(), endpoint, REQUEST_TIMEOUT)
            .map_err(TypeLlmFailure::into_urdia_error)
    }

    pub fn with_endpoint(
        api_key: impl Into<String>,
        model: impl Into<String>,
        endpoint: impl AsRef<str>,
    ) -> Result<Self> {
        let endpoint = Url::parse(endpoint.as_ref())
            .map_err(|_| TypeLlmFailure::Configuration.into_urdia_error())?;
        Self::build(api_key.into(), model.into(), endpoint, REQUEST_TIMEOUT)
            .map_err(TypeLlmFailure::into_urdia_error)
    }

    #[must_use]
    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    fn build(
        api_key: String,
        model: String,
        endpoint: Url,
        timeout: Duration,
    ) -> std::result::Result<Self, TypeLlmFailure> {
        if api_key.trim().is_empty()
            || api_key != api_key.trim()
            || model.trim().is_empty()
            || model != model.trim()
            || model.len() > MAX_MODEL_BYTES
            || !model.contains('@')
            || !model.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'.' | b'_' | b'/' | b'-' | b':' | b'@')
            })
        {
            return Err(TypeLlmFailure::Configuration);
        }
        validate_endpoint(&endpoint)?;
        let client = Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .https_only(endpoint.scheme() == "https")
            .build()
            .map_err(|_| TypeLlmFailure::Configuration)?;
        Ok(Self {
            client,
            api_key,
            endpoint,
            model,
        })
    }

    async fn decide_inner(
        &self,
        input: DecisionInput,
    ) -> std::result::Result<Decision, TypeLlmFailure> {
        let candidates = validate_candidates(&input)?;
        let mut expected_ids = candidates.iter().cloned().collect::<BTreeSet<_>>();
        expected_ids.insert(ESCALATION_CANDIDATE_ID.to_owned());
        let request = BridgeRequest {
            model: self.model.clone(),
            objective: input.objective,
            candidates,
            state: input.state,
        };
        let response = self.send_with_retries(&request).await?;
        validate_response(response, &self.model, &expected_ids)
    }

    async fn send_with_retries(
        &self,
        request: &BridgeRequest,
    ) -> std::result::Result<BridgeResponse, TypeLlmFailure> {
        let body = serde_json::to_vec(request).map_err(|_| TypeLlmFailure::InvalidInput)?;
        if body.len() > MAX_REQUEST_BYTES {
            return Err(TypeLlmFailure::RequestTooLarge);
        }
        let url = endpoint_for_request(&self.endpoint);
        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .client
                .post(url.clone())
                .bearer_auth(&self.api_key)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.clone())
                .send()
                .await
                .map_err(|error| {
                    if error.is_timeout() {
                        TypeLlmFailure::Timeout
                    } else {
                        TypeLlmFailure::Transport
                    }
                })?;
            if response.status().is_success() {
                let body = read_bounded(response).await?;
                return serde_json::from_slice(&body).map_err(|_| TypeLlmFailure::InvalidResponse);
            }
            let status = response.status();
            if is_retryable(status) && attempt + 1 < MAX_ATTEMPTS {
                sleep(retry_delay(response.headers(), attempt)).await;
                continue;
            }
            return Err(TypeLlmFailure::ServiceStatus(status));
        }
        Err(TypeLlmFailure::Transport)
    }
}

#[async_trait]
impl DecisionProvider for TypeLlmDecisionProvider {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    async fn decide(&self, input: DecisionInput) -> Result<Decision> {
        self.decide_inner(input)
            .await
            .map_err(TypeLlmFailure::into_urdia_error)
    }
}

#[derive(Debug)]
enum TypeLlmFailure {
    Configuration,
    InvalidInput,
    Transport,
    Timeout,
    RequestTooLarge,
    ResponseTooLarge,
    InvalidResponse,
    ServiceStatus(StatusCode),
}

impl TypeLlmFailure {
    fn into_urdia_error(self) -> UrdiaError {
        match self {
            Self::InvalidInput => {
                UrdiaError::InvalidInput("TypeLLM decision input is invalid".into())
            }
            Self::Configuration => {
                UrdiaError::Provider("TypeLLM provider configuration is invalid".into())
            }
            Self::Transport => UrdiaError::Provider("TypeLLM request failed".into()),
            Self::Timeout => UrdiaError::Provider("TypeLLM request timed out".into()),
            Self::RequestTooLarge => {
                UrdiaError::Provider("TypeLLM request exceeded the size limit".into())
            }
            Self::ResponseTooLarge => {
                UrdiaError::Provider("TypeLLM response exceeded the size limit".into())
            }
            Self::InvalidResponse => UrdiaError::Provider("TypeLLM response is invalid".into()),
            Self::ServiceStatus(status) => {
                UrdiaError::Provider(format!("TypeLLM service returned HTTP {}", status.as_u16()))
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct BridgeRequest {
    model: String,
    objective: String,
    candidates: Vec<String>,
    state: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeResponse {
    model: String,
    route: BridgeRoute,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeRoute {
    value: String,
    probabilities: BTreeMap<String, f64>,
}

fn load_api_key(
    environment_key: Option<std::ffi::OsString>,
    credentials_directory: Option<std::ffi::OsString>,
) -> std::result::Result<String, TypeLlmFailure> {
    if let Some(key) = environment_key {
        return key.into_string().map_err(|_| TypeLlmFailure::Configuration);
    }
    let directory = credentials_directory.ok_or(TypeLlmFailure::Configuration)?;
    read_api_key_file(&PathBuf::from(directory).join("typellm_key"))
}

fn read_api_key_file(path: &Path) -> std::result::Result<String, TypeLlmFailure> {
    let file = File::open(path).map_err(|_| TypeLlmFailure::Configuration)?;
    let mut bytes = Vec::with_capacity(256);
    file.take(MAX_API_KEY_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| TypeLlmFailure::Configuration)?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_API_KEY_BYTES {
        return Err(TypeLlmFailure::Configuration);
    }
    let value = String::from_utf8(bytes).map_err(|_| TypeLlmFailure::Configuration)?;
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() {
        return Err(TypeLlmFailure::Configuration);
    }
    Ok(value.to_owned())
}

fn validate_endpoint(endpoint: &Url) -> std::result::Result<(), TypeLlmFailure> {
    if endpoint.scheme() != "http" && endpoint.scheme() != "https"
        || endpoint.username() != ""
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || endpoint.host().is_none()
    {
        return Err(TypeLlmFailure::Configuration);
    }
    let Some(host) = endpoint.host_str() else {
        return Err(TypeLlmFailure::Configuration);
    };
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    let Ok(address) = host.parse::<IpAddr>() else {
        return Err(TypeLlmFailure::Configuration);
    };
    if !address.is_loopback() {
        return Err(TypeLlmFailure::Configuration);
    }
    Ok(())
}

fn endpoint_for_request(endpoint: &Url) -> Url {
    let mut url = endpoint.clone();
    url.set_path("/v1/route");
    url.set_query(None);
    url.set_fragment(None);
    url
}

fn validate_candidates(input: &DecisionInput) -> std::result::Result<Vec<String>, TypeLlmFailure> {
    if input.objective.trim().is_empty()
        || input.candidates.is_empty()
        || input.candidates.len() > MAX_PROVIDER_CANDIDATES
    {
        return Err(TypeLlmFailure::InvalidInput);
    }
    let mut seen = BTreeSet::new();
    for candidate in &input.candidates {
        if candidate.is_empty()
            || candidate.len() > MAX_CANDIDATE_ID_BYTES
            || candidate == ESCALATION_CANDIDATE_ID
            || !candidate
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            || !seen.insert(candidate.clone())
        {
            return Err(TypeLlmFailure::InvalidInput);
        }
    }
    Ok(input.candidates.clone())
}

async fn read_bounded(mut response: Response) -> std::result::Result<Vec<u8>, TypeLlmFailure> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(TypeLlmFailure::ResponseTooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| TypeLlmFailure::Transport)?
    {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(TypeLlmFailure::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn retry_delay(headers: &reqwest::header::HeaderMap, attempt: usize) -> Duration {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| RETRY_BACKOFF.saturating_mul(2_u32.saturating_pow(attempt as u32)))
        .min(MAX_RETRY_DELAY)
}

fn validate_response(
    response: BridgeResponse,
    model: &str,
    expected_ids: &BTreeSet<String>,
) -> std::result::Result<Decision, TypeLlmFailure> {
    let route = response.route;
    if response.model != model
        || route.probabilities.len() != expected_ids.len()
        || !expected_ids.contains(&route.value)
        || !probabilities_match(&route, expected_ids)
    {
        return Err(TypeLlmFailure::InvalidResponse);
    }
    let sum = route.probabilities.values().sum::<f64>();
    let probabilities = route
        .probabilities
        .into_iter()
        .map(|(candidate, probability)| (candidate, (probability / sum) as f32))
        .collect();
    let selected = (route.value != ESCALATION_CANDIDATE_ID).then_some(route.value);
    Ok(Decision {
        selected,
        probabilities,
        confidence: None,
        probability_semantics: ProbabilitySemantics::ConstrainedTokenDistribution,
    })
}

fn probabilities_match(route: &BridgeRoute, expected_ids: &BTreeSet<String>) -> bool {
    let mut sum = 0.0;
    for candidate in expected_ids {
        let Some(probability) = route.probabilities.get(candidate) else {
            return false;
        };
        if !probability.is_finite() || !(0.0..=1.0).contains(probability) {
            return false;
        }
        sum += probability;
    }
    if route
        .probabilities
        .keys()
        .any(|id| !expected_ids.contains(id))
    {
        return false;
    }
    let Some(selected_probability) = route.probabilities.get(&route.value) else {
        return false;
    };
    let highest = route
        .probabilities
        .values()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let tolerance = ROUNDING_ERROR_PER_PROBABILITY * expected_ids.len() as f64 + 0.000_001;
    (sum - 1.0).abs() <= tolerance && *selected_probability + TOP_PROBABILITY_EPSILON >= highest
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    const KEY: &str = "typellm-test-key-SECRET_SENTINEL";
    const MODEL: &str = "Qwen/example@revision";

    #[derive(Clone)]
    struct TestState {
        requests: Arc<Mutex<Vec<(HeaderMap, Value)>>>,
        response: Value,
        statuses: Vec<StatusCode>,
    }

    async fn handler(
        State(state): State<TestState>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        let mut requests = state.requests.lock().await;
        let index = requests.len();
        requests.push((headers, body));
        let status = state
            .statuses
            .get(index)
            .copied()
            .unwrap_or_else(|| *state.statuses.last().unwrap());
        (status, Json(state.response.clone()))
    }

    async fn server(
        response: Value,
        status: StatusCode,
    ) -> (String, Arc<Mutex<Vec<(HeaderMap, Value)>>>) {
        server_with_statuses(response, vec![status]).await
    }

    async fn server_with_statuses(
        response: Value,
        statuses: Vec<StatusCode>,
    ) -> (String, Arc<Mutex<Vec<(HeaderMap, Value)>>>) {
        assert!(!statuses.is_empty());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = TestState {
            requests: requests.clone(),
            response,
            statuses,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .route("/v1/route", post(handler))
                    .with_state(state),
            )
            .await
            .unwrap();
        });
        (format!("http://{address}"), requests)
    }

    fn input() -> DecisionInput {
        DecisionInput {
            objective: "Choose a coding route".into(),
            candidates: vec!["c_alpha".into(), "c_beta".into()],
            state: json!({"candidate_descriptions": {"c_alpha": "Alpha", "c_beta": "Beta"}}),
        }
    }

    fn response(value: &str, alpha: f64, beta: f64, escalation: f64) -> Value {
        json!({"model": MODEL, "route": {"value": value, "probabilities": {
            "c_alpha": alpha, "c_beta": beta, "__urdia_escalate__": escalation
        }}})
    }

    #[tokio::test]
    async fn sends_authenticated_request_and_returns_distribution() {
        let (endpoint, requests) = server(response("c_alpha", 0.7, 0.2, 0.1), StatusCode::OK).await;
        let provider = TypeLlmDecisionProvider::with_endpoint(KEY, MODEL, endpoint).unwrap();
        let decision = provider.decide(input()).await.unwrap();
        assert_eq!(decision.selected.as_deref(), Some("c_alpha"));
        assert_eq!(decision.confidence, None);
        assert_eq!(
            decision.probability_semantics,
            ProbabilitySemantics::ConstrainedTokenDistribution
        );
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].1["model"], MODEL);
        assert_eq!(requests[0].1["candidates"].as_array().unwrap().len(), 2);
        assert_eq!(
            requests[0].0.get("authorization").unwrap(),
            "Bearer typellm-test-key-SECRET_SENTINEL"
        );
    }

    #[tokio::test]
    async fn escalation_returns_no_selection() {
        let (endpoint, _) = server(
            response(ESCALATION_CANDIDATE_ID, 0.1, 0.1, 0.8),
            StatusCode::OK,
        )
        .await;
        let provider = TypeLlmDecisionProvider::with_endpoint(KEY, MODEL, endpoint).unwrap();
        assert_eq!(provider.decide(input()).await.unwrap().selected, None);
    }

    #[tokio::test]
    async fn rejects_invalid_response_variants_without_exposing_them() {
        let variants = [
            json!({"model": "wrong-SECRET_SENTINEL", "route": {"value": "c_alpha", "probabilities": {"c_alpha": 0.7, "c_beta": 0.2, "__urdia_escalate__": 0.1}}}),
            json!({"model": MODEL, "route": {"value": "c_alpha", "probabilities": {"c_alpha": 0.8, "c_beta": 0.2}}}),
            json!({"model": MODEL, "route": {"value": "c_alpha", "probabilities": {"c_alpha": 0.6, "c_beta": 0.2, "__urdia_escalate__": 0.1}}}),
            json!({"model": MODEL, "route": {"value": "c_alpha", "probabilities": {"c_alpha": 0.2, "c_beta": 0.7, "__urdia_escalate__": 0.1}}}),
            json!({"model": MODEL, "route": {"value": "c_alpha", "probabilities": {"c_alpha": 0.6, "c_beta": 0.2, "__urdia_escalate__": 0.1, "extra": 0.1}}}),
        ];
        for bad in variants {
            let (endpoint, _) = server(bad, StatusCode::OK).await;
            let provider = TypeLlmDecisionProvider::with_endpoint(KEY, MODEL, endpoint).unwrap();
            let error = provider.decide(input()).await.unwrap_err().to_string();
            assert!(error.contains("response is invalid"));
            assert!(!error.contains("SECRET_SENTINEL"));
        }
    }

    #[tokio::test]
    async fn retries_server_errors_but_not_unauthorized() {
        let good = response("c_alpha", 0.7, 0.2, 0.1);
        let (endpoint, requests) = server_with_statuses(
            good.clone(),
            vec![StatusCode::INTERNAL_SERVER_ERROR, StatusCode::OK],
        )
        .await;
        let provider = TypeLlmDecisionProvider::with_endpoint(KEY, MODEL, endpoint).unwrap();
        assert!(provider.decide(input()).await.is_ok());
        assert_eq!(requests.lock().await.len(), 2);

        let (endpoint, requests) =
            server_with_statuses(good, vec![StatusCode::UNAUTHORIZED, StatusCode::OK]).await;
        let provider = TypeLlmDecisionProvider::with_endpoint(KEY, MODEL, endpoint).unwrap();
        assert!(provider.decide(input()).await.is_err());
        assert_eq!(requests.lock().await.len(), 1);
    }

    #[test]
    fn rejects_non_loopback_and_redacts_key() {
        assert!(TypeLlmDecisionProvider::with_endpoint(KEY, MODEL, "http://example.com").is_err());
        let provider = TypeLlmDecisionProvider::new(KEY, MODEL).unwrap();
        let debug = format!("{provider:?}");
        assert!(!debug.contains(KEY));
        assert!(debug.contains("<redacted>"));
    }

    #[tokio::test]
    async fn rejects_more_than_twenty_three_candidates_before_connecting() {
        let provider = TypeLlmDecisionProvider::new(KEY, MODEL).unwrap();
        let mut value = input();
        value.candidates = (0..24).map(|index| format!("c_{index}")).collect();
        assert!(matches!(
            provider.decide(value).await,
            Err(UrdiaError::InvalidInput(_))
        ));
    }

    #[test]
    fn validates_model_and_default_endpoint() {
        assert!(TypeLlmDecisionProvider::new(KEY, " ").is_err());
        assert!(TypeLlmDecisionProvider::new(KEY, "Qwen/mutable-alias").is_err());
        let provider = TypeLlmDecisionProvider::new(KEY, MODEL).unwrap();
        assert_eq!(provider.endpoint().as_str(), "http://127.0.0.1:18767/");
        assert_eq!(provider.model(), MODEL);
        assert_eq!(provider.id(), PROVIDER_ID);
    }

    #[test]
    fn loads_a_systemd_style_credential_with_trailing_newline() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("typellm_key"), "credential-value\r\n").unwrap();
        let loaded = load_api_key(None, Some(directory.path().as_os_str().to_owned())).unwrap();
        assert_eq!(loaded, "credential-value");
    }
}
