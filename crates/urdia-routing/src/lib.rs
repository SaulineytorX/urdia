//! Host-validated advisory routing for Urdia.
//!
//! A decision provider may recommend one opaque candidate. This crate validates
//! the full result and turns every failure into an explicit fallback. It never
//! starts an agent, grants a permission, or activates a trusted session.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use urdia_core::{
    Decision, DecisionInput, DecisionProvider, ESCALATION_CANDIDATE_ID, ProbabilitySemantics,
    Result, UrdiaError,
};
use uuid::Uuid;

const MAX_CANDIDATES: usize = 64;
const MAX_CANDIDATE_ID_BYTES: usize = 64;
const MAX_DESCRIPTION_CHARS: usize = 1_024;
const MAX_OBJECTIVE_CHARS: usize = 16_384;
const MAX_STATE_BYTES: usize = 128 * 1_024;
const PROBABILITY_EPSILON: f64 = 0.000_1;

/// One host-prepared route exposed to a semantic decision provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteCandidate {
    /// Case-local opaque identifier. It must not encode provider meaning.
    pub id: String,
    /// Bounded English description of the route and its host-owned effect.
    pub description: String,
}

/// A bounded advisory routing request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteRequest {
    pub objective: String,
    #[serde(default)]
    pub state: Value,
    pub candidates: Vec<RouteCandidate>,
}

/// Host policy applied after the decision provider returns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutePolicy {
    /// Optional threshold for providers whose confidence has been calibrated.
    pub min_confidence: Option<f32>,
}

/// Why the host refused to use a semantic recommendation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteFallbackReason {
    ProviderError,
    NoSelection,
    LowConfidence,
    InvalidDecision,
}

/// Validated advisory output. `selected` remains an opaque candidate ID.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteResult {
    pub request_id: Uuid,
    pub decision_provider: String,
    pub selected: Option<String>,
    pub probabilities: BTreeMap<String, f32>,
    pub confidence: Option<f32>,
    pub probability_semantics: ProbabilitySemantics,
    pub candidate_fingerprint: String,
    pub fallback_reason: Option<RouteFallbackReason>,
}

impl RouteResult {
    /// Produce a safe audit shape without objective, state, or descriptions.
    #[must_use]
    pub fn audit_record(&self) -> RouteAuditRecord {
        RouteAuditRecord {
            schema_version: 1,
            request_id: self.request_id,
            decision_provider: self.decision_provider.clone(),
            selected: self.selected.clone(),
            confidence: self.confidence,
            probability_semantics: self.probability_semantics.clone(),
            candidate_fingerprint: self.candidate_fingerprint.clone(),
            fallback_reason: self.fallback_reason,
        }
    }
}

/// Content-free record suitable for future persistence after its schema lands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteAuditRecord {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub decision_provider: String,
    pub selected: Option<String>,
    pub confidence: Option<f32>,
    pub probability_semantics: ProbabilitySemantics,
    pub candidate_fingerprint: String,
    pub fallback_reason: Option<RouteFallbackReason>,
}

/// Validates input and provider output around one decision provider.
#[derive(Clone)]
pub struct RouteResolver {
    provider: Arc<dyn DecisionProvider>,
    policy: RoutePolicy,
}

impl RouteResolver {
    #[must_use]
    pub fn new(provider: Arc<dyn DecisionProvider>) -> Self {
        Self {
            provider,
            policy: RoutePolicy::default(),
        }
    }

    /// Set a post-decision confidence threshold.
    pub fn with_policy(mut self, policy: RoutePolicy) -> Result<Self> {
        validate_policy(policy)?;
        self.policy = policy;
        Ok(self)
    }

    /// Return a validated recommendation or an explicit fallback.
    pub async fn resolve(&self, request: RouteRequest) -> Result<RouteResult> {
        validate_route_request(&request)?;
        let request_id = Uuid::new_v4();
        let candidate_fingerprint = candidate_fingerprint(&request.candidates);
        let candidate_ids = request
            .candidates
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>();
        let descriptions = request
            .candidates
            .iter()
            .map(|candidate| (candidate.id.clone(), candidate.description.clone()))
            .collect::<BTreeMap<_, _>>();
        let state = json!({
            "context": request.state,
            "candidate_descriptions": descriptions,
        });
        let provider_id = self.provider.id().to_owned();
        let decision = self
            .provider
            .decide(DecisionInput {
                objective: request.objective,
                candidates: candidate_ids.clone(),
                state,
            })
            .await;

        let decision = match decision {
            Ok(decision) => decision,
            Err(_) => {
                return Ok(fallback_result(
                    request_id,
                    provider_id,
                    candidate_fingerprint,
                    RouteFallbackReason::ProviderError,
                ));
            }
        };
        Ok(validate_decision(
            request_id,
            provider_id,
            candidate_fingerprint,
            &candidate_ids,
            decision,
            self.policy,
        ))
    }
}

fn validate_policy(policy: RoutePolicy) -> Result<()> {
    if policy
        .min_confidence
        .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err(UrdiaError::InvalidInput(
            "routing confidence threshold must be between zero and one".to_owned(),
        ));
    }
    Ok(())
}

/// Validate a complete request before any decision provider is contacted.
///
/// Evaluation harnesses use this preflight for every case so a malformed
/// later case cannot cause a partially paid or partially recorded live run.
pub fn validate_route_request(request: &RouteRequest) -> Result<()> {
    let objective_chars = request.objective.chars().count();
    if request.objective.trim().is_empty() || objective_chars > MAX_OBJECTIVE_CHARS {
        return Err(UrdiaError::InvalidInput(
            "routing objective is empty or exceeds its size limit".to_owned(),
        ));
    }
    if request.candidates.is_empty() || request.candidates.len() > MAX_CANDIDATES {
        return Err(UrdiaError::InvalidInput(format!(
            "routing requires between 1 and {MAX_CANDIDATES} candidates"
        )));
    }
    if serde_json::to_vec(&request.state)
        .map_err(|_| UrdiaError::InvalidInput("routing state is invalid".to_owned()))?
        .len()
        > MAX_STATE_BYTES
    {
        return Err(UrdiaError::InvalidInput(
            "routing state exceeds its size limit".to_owned(),
        ));
    }
    let mut ids = BTreeSet::new();
    for candidate in &request.candidates {
        if !valid_candidate_id(&candidate.id)
            || candidate.id == ESCALATION_CANDIDATE_ID
            || !ids.insert(candidate.id.as_str())
        {
            return Err(UrdiaError::InvalidInput(
                "routing candidate IDs must be unique opaque ASCII identifiers".to_owned(),
            ));
        }
        if candidate.description.trim().is_empty()
            || candidate.description.chars().count() > MAX_DESCRIPTION_CHARS
        {
            return Err(UrdiaError::InvalidInput(
                "routing candidate description is empty or exceeds its size limit".to_owned(),
            ));
        }
    }
    Ok(())
}

fn valid_candidate_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_CANDIDATE_ID_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn validate_decision(
    request_id: Uuid,
    decision_provider: String,
    candidate_fingerprint: String,
    candidate_ids: &[String],
    decision: Decision,
    policy: RoutePolicy,
) -> RouteResult {
    let allowed = candidate_ids.iter().cloned().collect::<BTreeSet<_>>();
    let probability_keys = decision
        .probabilities
        .keys()
        .filter(|id| id.as_str() != ESCALATION_CANDIDATE_ID)
        .cloned()
        .collect::<BTreeSet<_>>();
    let expected_sum = decision
        .probabilities
        .values()
        .map(|value| f64::from(*value))
        .sum::<f64>();
    let probabilities_valid = probability_keys == allowed
        && decision
            .probabilities
            .keys()
            .all(|id| allowed.contains(id) || id.as_str() == ESCALATION_CANDIDATE_ID)
        && decision
            .probabilities
            .values()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        && (expected_sum - 1.0).abs() <= PROBABILITY_EPSILON;
    let confidence_valid = decision
        .confidence
        .is_none_or(|value| value.is_finite() && (0.0..=1.0).contains(&value));
    let selected_valid = decision
        .selected
        .as_ref()
        .is_none_or(|selected| allowed.contains(selected));

    if !probabilities_valid || !confidence_valid || !selected_valid {
        return fallback_result(
            request_id,
            decision_provider,
            candidate_fingerprint,
            RouteFallbackReason::InvalidDecision,
        );
    }
    if decision.selected.is_none() {
        return RouteResult {
            request_id,
            decision_provider,
            selected: None,
            probabilities: decision.probabilities,
            confidence: decision.confidence,
            probability_semantics: decision.probability_semantics,
            candidate_fingerprint,
            fallback_reason: Some(RouteFallbackReason::NoSelection),
        };
    }
    if policy
        .min_confidence
        .is_some_and(|minimum| decision.confidence.is_none_or(|value| value < minimum))
    {
        return RouteResult {
            request_id,
            decision_provider,
            selected: None,
            probabilities: decision.probabilities,
            confidence: decision.confidence,
            probability_semantics: decision.probability_semantics,
            candidate_fingerprint,
            fallback_reason: Some(RouteFallbackReason::LowConfidence),
        };
    }
    RouteResult {
        request_id,
        decision_provider,
        selected: decision.selected,
        probabilities: decision.probabilities,
        confidence: decision.confidence,
        probability_semantics: decision.probability_semantics,
        candidate_fingerprint,
        fallback_reason: None,
    }
}

fn fallback_result(
    request_id: Uuid,
    decision_provider: String,
    candidate_fingerprint: String,
    reason: RouteFallbackReason,
) -> RouteResult {
    RouteResult {
        request_id,
        decision_provider,
        selected: None,
        probabilities: BTreeMap::new(),
        confidence: None,
        probability_semantics: ProbabilitySemantics::Unknown,
        candidate_fingerprint,
        fallback_reason: Some(reason),
    }
}

/// Bind the entire validated routing input, including objective and nested host state.
/// This digest is provenance, not authorization or proof that the state is true.
pub fn routing_context_fingerprint(request: &RouteRequest) -> Result<String> {
    validate_route_request(request)?;
    let value = serde_json::to_value(request)
        .map_err(|_| UrdiaError::InvalidInput("serialize routing context".into()))?;
    let bytes = serde_json::to_vec(&canonical_context(value))
        .map_err(|_| UrdiaError::InvalidInput("serialize routing context".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn canonical_context(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let sorted: BTreeMap<_, _> = object
                .into_iter()
                .map(|(key, value)| (key, canonical_context(value)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(array) => Value::Array(array.into_iter().map(canonical_context).collect()),
        other => other,
    }
}

fn candidate_fingerprint(candidates: &[RouteCandidate]) -> String {
    let mut ordered = candidates.to_vec();
    ordered.sort_by(|left, right| left.id.cmp(&right.id));
    let mut digest = Sha256::new();
    for candidate in ordered {
        for value in [candidate.id.as_bytes(), candidate.description.as_bytes()] {
            digest.update((value.len() as u64).to_be_bytes());
            digest.update(value);
        }
    }
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    #[derive(Clone)]
    struct FixedProvider {
        decision: std::result::Result<Decision, &'static str>,
    }

    #[async_trait]
    impl DecisionProvider for FixedProvider {
        fn id(&self) -> &str {
            "fixed"
        }

        async fn decide(&self, _input: DecisionInput) -> Result<Decision> {
            self.decision
                .clone()
                .map_err(|message| UrdiaError::Provider(message.to_owned()))
        }
    }

    fn request() -> RouteRequest {
        RouteRequest {
            objective: "Review the workspace".to_owned(),
            state: json!({"workspace": "local"}),
            candidates: vec![
                RouteCandidate {
                    id: "c_Q7m2La".to_owned(),
                    description: "Use the local review route.".to_owned(),
                },
                RouteCandidate {
                    id: "c_x4P9de".to_owned(),
                    description: "Ask the operator for clarification.".to_owned(),
                },
            ],
        }
    }

    fn decision(selected: Option<&str>, confidence: Option<f32>) -> Decision {
        Decision {
            selected: selected.map(ToOwned::to_owned),
            probabilities: BTreeMap::from([
                ("c_Q7m2La".to_owned(), 0.8),
                ("c_x4P9de".to_owned(), 0.2),
            ]),
            confidence,
            probability_semantics: ProbabilitySemantics::Calibrated,
        }
    }

    #[tokio::test]
    async fn accepts_a_complete_in_set_decision() {
        let resolver = RouteResolver::new(Arc::new(FixedProvider {
            decision: Ok(decision(Some("c_Q7m2La"), Some(0.9))),
        }));
        let result = resolver.resolve(request()).await.expect("route");
        assert_eq!(result.selected.as_deref(), Some("c_Q7m2La"));
        assert_eq!(result.fallback_reason, None);
        let audit = serde_json::to_string(&result.audit_record()).expect("audit JSON");
        assert!(!audit.contains("Review the workspace"));
        assert!(!audit.contains("local review route"));
    }

    #[tokio::test]
    async fn provider_errors_and_unknown_ids_fall_back() {
        let failed = RouteResolver::new(Arc::new(FixedProvider {
            decision: Err("SECRET"),
        }))
        .resolve(request())
        .await
        .expect("fallback");
        assert_eq!(
            failed.fallback_reason,
            Some(RouteFallbackReason::ProviderError)
        );

        let invalid = RouteResolver::new(Arc::new(FixedProvider {
            decision: Ok(Decision {
                selected: Some("outside".to_owned()),
                ..decision(Some("c_Q7m2La"), Some(0.9))
            }),
        }))
        .resolve(request())
        .await
        .expect("fallback");
        assert_eq!(
            invalid.fallback_reason,
            Some(RouteFallbackReason::InvalidDecision)
        );
        assert_eq!(invalid.selected, None);
    }

    #[tokio::test]
    async fn threshold_and_escalation_are_explicit_fallbacks() {
        let resolver = RouteResolver::new(Arc::new(FixedProvider {
            decision: Ok(decision(Some("c_Q7m2La"), Some(0.4))),
        }))
        .with_policy(RoutePolicy {
            min_confidence: Some(0.8),
        })
        .expect("policy");
        let low = resolver.resolve(request()).await.expect("route");
        assert_eq!(
            low.fallback_reason,
            Some(RouteFallbackReason::LowConfidence)
        );
        assert_eq!(low.selected, None);

        let escalated = RouteResolver::new(Arc::new(FixedProvider {
            decision: Ok(decision(None, Some(0.9))),
        }))
        .resolve(request())
        .await
        .expect("route");
        assert_eq!(
            escalated.fallback_reason,
            Some(RouteFallbackReason::NoSelection)
        );
    }

    #[test]
    fn full_context_binding_is_stable_for_object_order_and_changes_with_state() {
        let mut input = request();
        input.state = serde_json::from_str(r#"{"b":2,"a":{"d":4,"c":3}}"#).unwrap();
        let mut equivalent = input.clone();
        equivalent.state = serde_json::from_str(r#"{"a":{"c":3,"d":4},"b":2}"#).unwrap();
        assert_eq!(
            routing_context_fingerprint(&input).unwrap(),
            routing_context_fingerprint(&equivalent).unwrap()
        );
        equivalent.state["a"]["c"] = json!(99);
        assert_ne!(
            routing_context_fingerprint(&input).unwrap(),
            routing_context_fingerprint(&equivalent).unwrap()
        );
        assert_eq!(
            candidate_fingerprint(&input.candidates),
            candidate_fingerprint(&equivalent.candidates)
        );
    }

    #[test]
    fn validates_case_local_ids_and_stable_fingerprint() {
        let mut reversed = request();
        reversed.candidates.reverse();
        assert_eq!(
            candidate_fingerprint(&request().candidates),
            candidate_fingerprint(&reversed.candidates)
        );
        let mut duplicate = request();
        duplicate.candidates[1].id = duplicate.candidates[0].id.clone();
        assert!(validate_route_request(&duplicate).is_err());
    }
}
