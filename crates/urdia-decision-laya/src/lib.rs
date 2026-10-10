//! Local Laya routing for Urdia.
//!
//! The adapter speaks Laya's Jev-compatible `/v1/systemone` protocol, but owns
//! its model and response validation separately from the remote Jev adapter.
//! Laya is advisory: this crate never authorizes an operation or chooses a
//! permission policy.

use async_trait::async_trait;
use reqwest::{Client, Response, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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

/// The stable Urdia provider ID for this adapter.
pub const PROVIDER_ID: &str = "laya";
/// The Laya checkpoint selected for English routing decisions.
pub const MODEL: &str = "english";
/// The response model emitted by Laya's System One runtime.
pub const RESPONSE_MODEL: &str = "laya-rl-agent";
/// The default local English Laya service origin.
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:18766";
/// The single question ID sent in each System One request.
pub const ROUTE_QUESTION_ID: &str = "route";
/// The maximum response body retained by the adapter.
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;
/// The maximum serialized request body sent to the local service.
pub const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
/// The timeout applied to each HTTP attempt.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

const MAX_ATTEMPTS: usize = 3;
const MAX_RETRY_DELAY: Duration = Duration::from_secs(2);
const RETRY_BACKOFF: Duration = Duration::from_millis(50);
const TOP_PROBABILITY_EPSILON: f64 = 0.000_1;
const ROUNDING_ERROR_PER_PROBABILITY: f64 = 0.000_05;
const MAX_CANDIDATE_ID_BYTES: usize = 256;
const MAX_DESCRIPTION_CHARS: usize = 1_024;
const MAX_API_KEY_BYTES: u64 = 4 * 1_024;

/// A Laya decision provider using a fixed local endpoint and host-supplied key.
///
/// Use [`Self::from_env`] for the service deployment. Tests and compatible
/// local services can use [`Self::with_endpoint`] with an explicitly injected
/// key and numeric loopback endpoint. The key is omitted from [`Debug`].
#[derive(Clone)]
pub struct LayaDecisionProvider {
    client: Client,
    api_key: String,
    endpoint: Url,
}

impl Debug for LayaDecisionProvider {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LayaDecisionProvider")
            .field("model", &MODEL)
            .field("response_model", &RESPONSE_MODEL)
            .field("endpoint", &self.endpoint)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

impl LayaDecisionProvider {
    /// Read `URDIA_LAYA_API_KEY`, or systemd's `laya_key` credential, and use
    /// the configured numeric loopback endpoint.
    pub fn from_env() -> Result<Self> {
        let api_key = load_api_key(
            env::var_os("URDIA_LAYA_API_KEY"),
            env::var_os("CREDENTIALS_DIRECTORY"),
        )
        .map_err(LayaFailure::into_urdia_error)?;
        let endpoint = match env::var_os("URDIA_LAYA_URL") {
            Some(value) => value
                .into_string()
                .map_err(|_| LayaFailure::Configuration.into_urdia_error())?
                .parse::<Url>()
                .map_err(|_| LayaFailure::Configuration.into_urdia_error())?,
            None => Url::parse(DEFAULT_ENDPOINT).expect("built-in Laya endpoint must parse"),
        };
        Self::build(api_key, endpoint, REQUEST_TIMEOUT).map_err(LayaFailure::into_urdia_error)
    }

    /// Build a provider against the default local service endpoint.
    pub fn new(api_key: impl Into<String>) -> Result<Self> {
        let endpoint = Url::parse(DEFAULT_ENDPOINT).expect("built-in Laya endpoint must parse");
        Self::build(api_key.into(), endpoint, REQUEST_TIMEOUT)
            .map_err(LayaFailure::into_urdia_error)
    }

    /// Build a provider against an explicitly injected numeric loopback endpoint.
    ///
    /// Endpoint injection is deliberately restricted to an IP loopback host so
    /// a production credential cannot silently be sent to an arbitrary origin.
    pub fn with_endpoint(api_key: impl Into<String>, endpoint: impl AsRef<str>) -> Result<Self> {
        let endpoint = Url::parse(endpoint.as_ref())
            .map_err(|_| LayaFailure::Configuration.into_urdia_error())?;
        Self::build(api_key.into(), endpoint, REQUEST_TIMEOUT)
            .map_err(LayaFailure::into_urdia_error)
    }

    /// Alias for [`Self::with_endpoint`] that makes test construction explicit.
    pub fn for_loopback_endpoint(
        api_key: impl Into<String>,
        endpoint: impl AsRef<str>,
    ) -> Result<Self> {
        Self::with_endpoint(api_key, endpoint)
    }

    /// Return the configured endpoint without credentials.
    #[must_use]
    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }

    /// Return the model selected for the adapter.
    #[must_use]
    pub const fn model(&self) -> &'static str {
        MODEL
    }

    fn build(
        api_key: String,
        endpoint: Url,
        timeout: Duration,
    ) -> std::result::Result<Self, LayaFailure> {
        if api_key.trim().is_empty() || api_key != api_key.trim() {
            return Err(LayaFailure::Configuration);
        }
        validate_endpoint(&endpoint)?;

        let client = Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .https_only(endpoint.scheme() == "https")
            .build()
            .map_err(|_| LayaFailure::Configuration)?;

        Ok(Self {
            client,
            api_key,
            endpoint,
        })
    }

    async fn decide_inner(
        &self,
        input: DecisionInput,
    ) -> std::result::Result<Decision, LayaFailure> {
        let candidates = validate_candidates(&input)?;
        let descriptions = extract_candidate_descriptions(&input.state, &candidates);
        let criteria = criteria_for(&candidates, &descriptions);
        let expected_ids = criteria.keys().cloned().collect::<BTreeSet<_>>();

        let request = SystemOneRequest {
            state: request_state(&input, &descriptions),
            model: MODEL,
            questions: BTreeMap::from([(
                ROUTE_QUESTION_ID.to_owned(),
                ChoiceQuestion {
                    question_type: "choice",
                    instructions: routing_instructions(),
                    criteria,
                },
            )]),
        };

        let response = self.send_with_retries(&request).await?;
        validate_response(response, &expected_ids)
    }

    async fn send_with_retries(
        &self,
        request: &SystemOneRequest,
    ) -> std::result::Result<SystemOneResponse, LayaFailure> {
        let body = serde_json::to_vec(request).map_err(|_| LayaFailure::InvalidInput)?;
        if body.len() > MAX_REQUEST_BYTES {
            return Err(LayaFailure::RequestTooLarge);
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
                        LayaFailure::Timeout
                    } else {
                        LayaFailure::Transport
                    }
                })?;

            if response.status().is_success() {
                let body = read_bounded(response).await?;
                return serde_json::from_slice(&body).map_err(|_| LayaFailure::InvalidResponse);
            }

            let status = response.status();
            if is_retryable(status) && attempt + 1 < MAX_ATTEMPTS {
                let delay = retry_delay(response.headers(), attempt);
                sleep(delay).await;
                continue;
            }

            return Err(LayaFailure::ServiceStatus(status));
        }

        Err(LayaFailure::Transport)
    }
}

fn load_api_key(
    environment_key: Option<std::ffi::OsString>,
    credentials_directory: Option<std::ffi::OsString>,
) -> std::result::Result<String, LayaFailure> {
    if let Some(key) = environment_key {
        return key.into_string().map_err(|_| LayaFailure::Configuration);
    }
    let directory = credentials_directory.ok_or(LayaFailure::Configuration)?;
    read_api_key_file(&PathBuf::from(directory).join("laya_key"))
}

fn read_api_key_file(path: &Path) -> std::result::Result<String, LayaFailure> {
    let file = File::open(path).map_err(|_| LayaFailure::Configuration)?;
    let mut bytes = Vec::with_capacity(256);
    file.take(MAX_API_KEY_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| LayaFailure::Configuration)?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_API_KEY_BYTES {
        return Err(LayaFailure::Configuration);
    }
    String::from_utf8(bytes).map_err(|_| LayaFailure::Configuration)
}

#[async_trait]
impl DecisionProvider for LayaDecisionProvider {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    async fn decide(&self, input: DecisionInput) -> Result<Decision> {
        self.decide_inner(input)
            .await
            .map_err(LayaFailure::into_urdia_error)
    }
}

#[derive(Debug)]
enum LayaFailure {
    Configuration,
    InvalidInput,
    Transport,
    Timeout,
    RequestTooLarge,
    ResponseTooLarge,
    InvalidResponse,
    ServiceStatus(StatusCode),
}

impl LayaFailure {
    fn into_urdia_error(self) -> UrdiaError {
        match self {
            Self::InvalidInput => UrdiaError::InvalidInput("Laya decision input is invalid".into()),
            Self::Configuration => {
                UrdiaError::Provider("Laya provider configuration is invalid".into())
            }
            Self::Transport => UrdiaError::Provider("Laya request failed".into()),
            Self::Timeout => UrdiaError::Provider("Laya request timed out".into()),
            Self::RequestTooLarge => {
                UrdiaError::Provider("Laya request exceeded the size limit".into())
            }
            Self::ResponseTooLarge => {
                UrdiaError::Provider("Laya response exceeded the size limit".into())
            }
            Self::InvalidResponse => UrdiaError::Provider("Laya response is invalid".into()),
            Self::ServiceStatus(status) => {
                UrdiaError::Provider(format!("Laya service returned HTTP {}", status.as_u16()))
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct SystemOneRequest {
    state: Value,
    model: &'static str,
    questions: BTreeMap<String, ChoiceQuestion>,
}

#[derive(Debug, Serialize)]
struct ChoiceQuestion {
    #[serde(rename = "type")]
    question_type: &'static str,
    instructions: &'static str,
    criteria: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct SystemOneResponse {
    model: String,
    answers: BTreeMap<String, ChoiceAnswer>,
    routing: RoutingMetadata,
    usage: Usage,
}

#[derive(Debug, Deserialize)]
struct RoutingMetadata {
    model: String,
}

#[derive(Debug, Deserialize)]
struct ChoiceAnswer {
    #[serde(rename = "type")]
    answer_type: String,
    choice: String,
    probabilities: BTreeMap<String, f64>,
    confidence: f64,
}

#[derive(Debug, Deserialize)]
struct Usage {
    input_tokens: i64,
    output_tokens: i64,
}

fn validate_endpoint(endpoint: &Url) -> std::result::Result<(), LayaFailure> {
    if endpoint.scheme() != "http" && endpoint.scheme() != "https"
        || endpoint.username() != ""
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || endpoint.host().is_none()
    {
        return Err(LayaFailure::Configuration);
    }
    let Some(host) = endpoint.host_str() else {
        return Err(LayaFailure::Configuration);
    };
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    let Ok(address) = host.parse::<IpAddr>() else {
        return Err(LayaFailure::Configuration);
    };
    if !address.is_loopback() {
        return Err(LayaFailure::Configuration);
    }
    Ok(())
}

fn endpoint_for_request(endpoint: &Url) -> Url {
    let mut url = endpoint.clone();
    url.set_path("/v1/systemone");
    url.set_query(None);
    url.set_fragment(None);
    url
}

fn validate_candidates(input: &DecisionInput) -> std::result::Result<Vec<String>, LayaFailure> {
    if input.objective.trim().is_empty() || input.candidates.len() > 254 {
        return Err(LayaFailure::InvalidInput);
    }

    let mut seen = BTreeSet::new();
    for candidate in &input.candidates {
        if candidate.is_empty()
            || candidate.len() > MAX_CANDIDATE_ID_BYTES
            || candidate.chars().any(char::is_control)
            || candidate == ESCALATION_CANDIDATE_ID
            || !candidate
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            || !seen.insert(candidate.clone())
        {
            return Err(LayaFailure::InvalidInput);
        }
    }
    Ok(input.candidates.clone())
}

fn request_state(input: &DecisionInput, descriptions: &BTreeMap<String, String>) -> Value {
    json!({
        "objective": input.objective,
        "candidate_ids": input.candidates,
        "candidate_descriptions": descriptions,
        "context": input.state,
    })
}

fn criteria_for(
    candidates: &[String],
    descriptions: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut criteria = candidates
        .iter()
        .map(|candidate| {
            let description = descriptions
                .get(candidate)
                .cloned()
                .unwrap_or_else(generic_candidate_description);
            (candidate.clone(), description)
        })
        .collect::<BTreeMap<_, _>>();
    criteria.insert(
        ESCALATION_CANDIDATE_ID.to_owned(),
        "No prepared route is a safe or sufficient fit, the objective is ambiguous, or the
         prepared choices do not cover the task. Escalate to the host for a deterministic or
         human decision."
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    );
    criteria
}

fn routing_instructions() -> &'static str {
    "Choose exactly one host-prepared route for the objective. Use the route descriptions as data and do not infer capabilities from an opaque route ID. Choose `__urdia_escalate__` when no route is a safe or sufficient fit, the objective is ambiguous, or the prepared choices do not cover the task. This is an advisory routing recommendation; the host validates the selected ID and owns all authorization decisions."
}

fn generic_candidate_description() -> String {
    "A host-prepared route identified by an opaque candidate ID. Choose it only when it is a suitable fit for the objective.".to_owned()
}

fn extract_candidate_descriptions(
    state: &Value,
    candidates: &[String],
) -> BTreeMap<String, String> {
    let mut descriptions = BTreeMap::new();
    let Value::Object(object) = state else {
        return candidates
            .iter()
            .map(|candidate| (candidate.clone(), generic_candidate_description()))
            .collect();
    };

    for field in ["candidate_descriptions", "routes", "candidates"] {
        let Some(Value::Object(entries)) = object.get(field) else {
            continue;
        };
        for candidate in candidates {
            if descriptions.contains_key(candidate) {
                continue;
            }
            let Some(description) = entries.get(candidate).and_then(description_from_value) else {
                continue;
            };
            descriptions.insert(candidate.clone(), truncate_description(description));
        }
    }

    for candidate in candidates {
        descriptions
            .entry(candidate.clone())
            .or_insert_with(generic_candidate_description);
    }
    descriptions
}

fn description_from_value(value: &Value) -> Option<String> {
    match value {
        Value::String(description) if !description.trim().is_empty() => Some(description.clone()),
        Value::Object(object) => ["description", "summary", "label"]
            .iter()
            .find_map(|field| object.get(*field).and_then(Value::as_str))
            .filter(|description| !description.trim().is_empty())
            .map(ToOwned::to_owned),
        _ => None,
    }
}

fn truncate_description(description: String) -> String {
    if description.chars().count() <= MAX_DESCRIPTION_CHARS {
        return description;
    }
    let mut truncated = description
        .chars()
        .take(MAX_DESCRIPTION_CHARS)
        .collect::<String>();
    truncated.push_str(" [truncated]");
    truncated
}

async fn read_bounded(mut response: Response) -> std::result::Result<Vec<u8>, LayaFailure> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(LayaFailure::ResponseTooLarge);
    }

    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| LayaFailure::Transport)? {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(LayaFailure::ResponseTooLarge);
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
    let retry_after = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| RETRY_BACKOFF.saturating_mul(2_u32.saturating_pow(attempt as u32)));
    retry_after.min(MAX_RETRY_DELAY)
}

fn validate_response(
    response: SystemOneResponse,
    expected_ids: &BTreeSet<String>,
) -> std::result::Result<Decision, LayaFailure> {
    if response.model != RESPONSE_MODEL
        || response.routing.model != MODEL
        || response.usage.input_tokens < 0
        || response.usage.output_tokens < 0
        || response.answers.len() != 1
    {
        return Err(LayaFailure::InvalidResponse);
    }

    let Some(answer) = response.answers.get(ROUTE_QUESTION_ID) else {
        return Err(LayaFailure::InvalidResponse);
    };
    if answer.answer_type != "choice"
        || answer.probabilities.len() != expected_ids.len()
        || !expected_ids.contains(&answer.choice)
        || !probabilities_match(answer, expected_ids)
        || !answer.confidence.is_finite()
        || !(0.0..=1.0).contains(&answer.confidence)
    {
        return Err(LayaFailure::InvalidResponse);
    }

    let sum = answer.probabilities.values().sum::<f64>();
    let probabilities = answer
        .probabilities
        .iter()
        .map(|(candidate, probability)| (candidate.clone(), (*probability / sum) as f32))
        .collect();
    let selected = (answer.choice != ESCALATION_CANDIDATE_ID).then(|| answer.choice.clone());

    Ok(Decision {
        selected,
        probabilities,
        confidence: Some(answer.confidence as f32),
        // Laya's choice confidence is normalized entropy and is not a
        // calibration claim. Consumers must not compare it to Jev's semantics.
        probability_semantics: ProbabilitySemantics::Unknown,
    })
}

fn probabilities_match(answer: &ChoiceAnswer, expected_ids: &BTreeSet<String>) -> bool {
    if answer
        .probabilities
        .keys()
        .any(|candidate| !expected_ids.contains(candidate))
    {
        return false;
    }

    let mut sum = 0.0;
    for candidate in expected_ids {
        let Some(probability) = answer.probabilities.get(candidate) else {
            return false;
        };
        if !probability.is_finite() || !(0.0..=1.0).contains(probability) {
            return false;
        }
        sum += probability;
    }
    let Some(selected_probability) = answer.probabilities.get(&answer.choice) else {
        return false;
    };
    let highest_probability = answer
        .probabilities
        .values()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let sum_tolerance = ROUNDING_ERROR_PER_PROBABILITY * expected_ids.len() as f64 + 0.000_001;
    (sum - 1.0).abs() <= sum_tolerance
        && *selected_probability + TOP_PROBABILITY_EPSILON >= highest_probability
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::{Json, Router};
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    const KEY: &str = "laya-test-key-SECRET_SENTINEL";

    #[derive(Clone)]
    struct TestState {
        requests: Arc<Mutex<Vec<(HeaderMap, Value)>>>,
        response: Value,
        status: StatusCode,
    }

    async fn test_handler(
        State(state): State<TestState>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        state.requests.lock().await.push((headers, body));
        (state.status, Json(state.response.clone()))
    }

    async fn spawn_server(
        response: Value,
        status: StatusCode,
    ) -> (
        String,
        Arc<Mutex<Vec<(HeaderMap, Value)>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = TestState {
            requests: requests.clone(),
            response,
            status,
        };
        let router = Router::new()
            .route("/v1/systemone", post(test_handler))
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind loopback");
        let address = listener
            .local_addr()
            .expect("listener should have an address");
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("test server should serve");
        });
        (format!("http://{address}"), requests, task)
    }

    fn input() -> DecisionInput {
        DecisionInput {
            objective: "Review this Rust workspace".to_owned(),
            candidates: vec!["qwen".to_owned(), "codex".to_owned()],
            state: json!({
                "candidate_descriptions": {
                    "qwen": "Use the local Qwen ACP provider.",
                },
                "task": "Inspect the repository"
            }),
        }
    }

    fn response(choice: &str, probabilities: Value) -> Value {
        json!({
            "model": RESPONSE_MODEL,
            "answers": {
                ROUTE_QUESTION_ID: {
                    "type": "choice",
                    "choice": choice,
                    "probabilities": probabilities,
                    "confidence": 0.8,
                    "answer_confidence": 0.2,
                    "action": {"act_probability": 0.1}
                }
            },
            "usage": {"input_tokens": 42, "output_tokens": 0},
            "routing": {"model": MODEL, "reason": "explicit model"}
        })
    }

    #[test]
    fn debug_redacts_key_and_models_are_pinned() {
        let provider = LayaDecisionProvider::new(KEY).expect("provider should build");
        let debug = format!("{provider:?}");
        assert!(!debug.contains(KEY));
        assert!(debug.contains("<redacted>"));
        assert_eq!(provider.id(), PROVIDER_ID);
        assert_eq!(provider.model(), MODEL);
        assert_eq!(provider.endpoint().as_str(), "http://127.0.0.1:18766/");
    }

    #[test]
    fn empty_injected_key_is_rejected_without_echoing_it() {
        let error = LayaDecisionProvider::new("").expect_err("empty key should fail");
        assert!(format!("{error}").contains("configuration is invalid"));
    }

    #[test]
    fn loads_a_bounded_systemd_credential_without_exposing_it() {
        let directory = tempfile::tempdir().expect("temporary credential directory");
        let path = directory.path().join("laya_key");
        std::fs::write(&path, KEY).expect("credential should write");
        let loaded = load_api_key(None, Some(directory.path().as_os_str().to_owned()))
            .expect("systemd credential should load");
        assert_eq!(loaded, KEY);
        let provider = LayaDecisionProvider::new(loaded).expect("credential should be valid");
        assert!(!format!("{provider:?}").contains(KEY));

        std::fs::write(&path, vec![b'x'; MAX_API_KEY_BYTES as usize + 1])
            .expect("oversized credential should write");
        assert!(load_api_key(None, Some(directory.path().as_os_str().to_owned())).is_err());
    }

    #[test]
    fn explicit_environment_key_precedes_systemd_credential() {
        let loaded = load_api_key(
            Some(KEY.into()),
            Some("/directory/that/must/not/be-read".into()),
        )
        .expect("environment key should win");
        assert_eq!(loaded, KEY);
    }

    #[test]
    fn injected_endpoints_must_be_numeric_loopback_without_url_secrets() {
        for endpoint in [
            "http://localhost:18765",
            "http://example.test:18765",
            "http://192.0.2.1:18765",
            "http://127.0.0.1:18765?secret=sentinel",
            "http://user:pass@127.0.0.1:18765",
            "http://127.0.0.1:18765#secret",
        ] {
            assert!(
                LayaDecisionProvider::with_endpoint(KEY, endpoint).is_err(),
                "endpoint should be rejected: {endpoint}"
            );
        }
        assert!(LayaDecisionProvider::with_endpoint(KEY, "http://127.0.0.1:18765").is_ok());
        assert!(LayaDecisionProvider::with_endpoint(KEY, "http://[::1]:18765").is_ok());
    }

    #[tokio::test]
    async fn sends_english_checkpoint_contract_and_returns_unknown_semantics() {
        let server = spawn_server(
            response(
                "qwen",
                json!({"codex": 0.2, "__urdia_escalate__": 0.1, "qwen": 0.7}),
            ),
            StatusCode::OK,
        )
        .await;
        let provider = LayaDecisionProvider::with_endpoint(KEY, &server.0)
            .expect("loopback provider should build");
        let decision = provider.decide(input()).await.expect("Laya should decide");
        assert_eq!(decision.selected.as_deref(), Some("qwen"));
        assert_eq!(
            decision.probability_semantics,
            ProbabilitySemantics::Unknown
        );
        assert_eq!(decision.probabilities.get("qwen"), Some(&0.7));

        let requests = server.1.lock().await;
        assert_eq!(requests.len(), 1);
        let (headers, body) = &requests[0];
        assert_eq!(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer laya-test-key-SECRET_SENTINEL")
        );
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["questions"][ROUTE_QUESTION_ID]["type"], "choice");
        let instructions = body["questions"][ROUTE_QUESTION_ID]["instructions"]
            .as_str()
            .expect("instructions should be a string");
        assert!(instructions.contains("Choose exactly one"));
        assert!(instructions.contains("__urdia_escalate__"));
        let criteria = body["questions"][ROUTE_QUESTION_ID]["criteria"]
            .as_object()
            .expect("criteria should be an object");
        assert_eq!(criteria.len(), 3);
        assert_eq!(criteria["qwen"], "Use the local Qwen ACP provider.");
        assert!(criteria.contains_key("codex"));
        assert!(criteria.contains_key(ESCALATION_CANDIDATE_ID));
        assert_eq!(
            body["state"]["candidate_descriptions"]["qwen"],
            "Use the local Qwen ACP provider."
        );
        server.2.abort();
    }

    #[tokio::test]
    async fn escalation_returns_no_selected_candidate() {
        let server = spawn_server(
            response(
                ESCALATION_CANDIDATE_ID,
                json!({"codex": 0.1, "__urdia_escalate__": 0.8, "qwen": 0.1}),
            ),
            StatusCode::OK,
        )
        .await;
        let provider = LayaDecisionProvider::with_endpoint(KEY, &server.0)
            .expect("loopback provider should build");
        let decision = provider.decide(input()).await.expect("Laya should decide");
        assert_eq!(decision.selected, None);
        assert_eq!(decision.probabilities.len(), 3);
        server.2.abort();
    }

    #[tokio::test]
    async fn rejects_bad_models_probability_sets_and_confidence() {
        let cases = [
            response("qwen", json!({"codex": 0.2, "__urdia_escalate__": 0.1})),
            response(
                "unknown",
                json!({"codex": 0.2, "__urdia_escalate__": 0.1, "qwen": 0.7}),
            ),
            json!({
                "model": "wrong-runtime",
                "answers": {ROUTE_QUESTION_ID: {
                    "type": "choice", "choice": "qwen",
                    "probabilities": {"codex": 0.2, "__urdia_escalate__": 0.1, "qwen": 0.7},
                    "confidence": 0.8
                }},
                "routing": {"model": MODEL},
                "usage": {"input_tokens": 1, "output_tokens": 0}
            }),
            json!({
                "model": RESPONSE_MODEL,
                "answers": {ROUTE_QUESTION_ID: {
                    "type": "choice", "choice": "qwen",
                    "probabilities": {"codex": 0.2, "__urdia_escalate__": 0.1, "qwen": 0.7},
                    "confidence": 0.8
                }},
                "routing": {"model": "typed-decisions"},
                "usage": {"input_tokens": 1, "output_tokens": 0}
            }),
            json!({
                "model": RESPONSE_MODEL,
                "answers": {ROUTE_QUESTION_ID: {
                    "type": "noul", "choice": "qwen",
                    "probabilities": {"codex": 0.2, "__urdia_escalate__": 0.1, "qwen": 0.7},
                    "confidence": 0.8
                }},
                "routing": {"model": MODEL},
                "usage": {"input_tokens": 1, "output_tokens": 0}
            }),
            response(
                "qwen",
                json!({"codex": 0.2, "__urdia_escalate__": 0.1, "qwen": 0.6}),
            ),
            response(
                "qwen",
                json!({"codex": 0.2, "__urdia_escalate__": 0.1, "qwen": 0.7, "other": 0.0}),
            ),
            response(
                "qwen",
                json!({"codex": -0.1, "__urdia_escalate__": 0.2, "qwen": 0.9}),
            ),
            response(
                "codex",
                json!({"codex": 0.2, "__urdia_escalate__": 0.1, "qwen": 0.7}),
            ),
            json!({
                "model": RESPONSE_MODEL,
                "answers": {ROUTE_QUESTION_ID: {
                    "type": "choice", "choice": "qwen",
                    "probabilities": {"codex": 0.2, "__urdia_escalate__": 0.1, "qwen": 0.7},
                    "confidence": 1.5
                }},
                "routing": {"model": MODEL},
                "usage": {"input_tokens": 1, "output_tokens": 0}
            }),
        ];

        for case in cases {
            let server = spawn_server(case, StatusCode::OK).await;
            let provider = LayaDecisionProvider::with_endpoint(KEY, &server.0)
                .expect("loopback provider should build");
            let error = provider
                .decide(input())
                .await
                .expect_err("malformed response should fail");
            assert!(format!("{error}").contains("response is invalid"));
            server.2.abort();
        }
    }

    #[tokio::test]
    async fn retries_retryable_statuses_but_not_unauthorized() {
        let requests = Arc::new(Mutex::new(0_u32));
        let state = requests.clone();
        let app = Router::new().route(
            "/v1/systemone",
            post(move || {
                let state = state.clone();
                async move {
                    let mut requests = state.lock().await;
                    *requests += 1;
                    if *requests < 3 {
                        (
                            StatusCode::SERVICE_UNAVAILABLE,
                            Json(json!({"error": "hidden"})),
                        )
                            .into_response()
                    } else {
                        (
                            StatusCode::OK,
                            Json(response(
                                "qwen",
                                json!({"codex": 0.2, "__urdia_escalate__": 0.1, "qwen": 0.7}),
                            )),
                        )
                            .into_response()
                    }
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind loopback");
        let address = listener
            .local_addr()
            .expect("listener should have an address");
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("test server should serve");
        });
        let provider = LayaDecisionProvider::with_endpoint(KEY, format!("http://{address}"))
            .expect("loopback provider should build");
        provider
            .decide(input())
            .await
            .expect("retry should succeed");
        assert_eq!(*requests.lock().await, 3);
        task.abort();

        let server = spawn_server(json!({"error": "hidden"}), StatusCode::UNAUTHORIZED).await;
        let provider = LayaDecisionProvider::with_endpoint(KEY, &server.0)
            .expect("loopback provider should build");
        let error = provider
            .decide(input())
            .await
            .expect_err("unauthorized should fail");
        assert!(format!("{error}").contains("HTTP 401"));
        assert_eq!(server.1.lock().await.len(), 1);
        server.2.abort();
    }

    #[tokio::test]
    async fn does_not_expose_key_or_error_body() {
        let server = spawn_server(
            json!({"error": KEY, "state": "PRIVATE_STATE_SENTINEL"}),
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
        let provider = LayaDecisionProvider::with_endpoint(KEY, &server.0)
            .expect("loopback provider should build");
        let error = provider
            .decide(input())
            .await
            .expect_err("unprocessable response should fail");
        let rendered = format!("{error:?} {error}");
        assert!(!rendered.contains(KEY));
        assert!(!rendered.contains("PRIVATE_STATE_SENTINEL"));
        server.2.abort();
    }

    #[tokio::test]
    async fn rejects_oversized_success_body() {
        let server = spawn_server(
            Value::String("x".repeat(MAX_RESPONSE_BYTES + 1)),
            StatusCode::OK,
        )
        .await;
        let provider = LayaDecisionProvider::with_endpoint(KEY, &server.0)
            .expect("loopback provider should build");
        let error = provider
            .decide(input())
            .await
            .expect_err("oversized response should fail");
        assert!(format!("{error}").contains("size limit"));
        server.2.abort();
    }

    #[tokio::test]
    async fn rejects_oversized_request_before_connecting() {
        let provider = LayaDecisionProvider::with_endpoint(KEY, "http://127.0.0.1:1")
            .expect("loopback provider should build");
        let mut oversized = input();
        oversized.state = json!({"state": "x".repeat(MAX_REQUEST_BYTES)});
        let error = provider
            .decide(oversized)
            .await
            .expect_err("oversized request should fail before transport");
        assert!(format!("{error}").contains("size limit"));
        assert!(!format!("{error:?}").contains(KEY));
    }
}
