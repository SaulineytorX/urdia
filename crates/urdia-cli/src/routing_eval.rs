//! Reproducible, safe routing evaluation for the command-line interface.
//!
//! The evaluator deliberately keeps the fixture's task text and host context
//! in memory only. Reports contain opaque case IDs, route IDs, fingerprints,
//! and aggregate measurements so a report can be shared without reproducing
//! the prompt, state, candidate descriptions, or provider response.

use super::{MAX_EVALUATION_BYTES, RouterKind, read_bounded_json, route_resolver};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use urdia_core::{ProbabilitySemantics, Result, UrdiaError};
use urdia_routing::{
    RouteCandidate, RouteFallbackReason, RouteRequest, RouteResolver, validate_route_request,
};

/// The fixture partition used by a routing evaluation run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum EvaluationSplit {
    All,
    Development,
    #[value(name = "held-out", alias = "held_out")]
    #[serde(rename = "held-out", alias = "held_out")]
    HeldOut,
    Adversarial,
}

impl EvaluationSplit {
    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Development => "development",
            Self::HeldOut => "held-out",
            Self::Adversarial => "adversarial",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CorpusVersion {
    V1,
    V2,
    V3,
    V4,
    V5,
}

#[derive(Clone, Debug, Deserialize)]
struct EvaluationCorpus {
    #[serde(skip)]
    fixture_fingerprint: Option<String>,
    schema_version: String,
    #[serde(default)]
    corpus_id: Option<String>,
    language: String,
    model: EvaluationModel,
    candidate_id_scope: String,
    privacy: EvaluationPrivacy,
    cases: Vec<EvaluationCase>,
}

#[derive(Clone, Debug, Deserialize)]
struct EvaluationPrivacy {
    persist_raw_response: bool,
    persist_api_key: bool,
    fixture_contains_real_secrets: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct EvaluationModel {
    provider: String,
    model: String,
    version_pinned: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct EvaluationCase {
    id: String,
    input: EvaluationInput,
    candidates: Vec<EvaluationCandidate>,
    expected: EvaluationExpected,
    #[serde(default, alias = "evaluation_split")]
    split: Option<EvaluationSplit>,
}

#[derive(Clone, Debug, Deserialize)]
struct EvaluationInput {
    objective: String,
    state: Value,
}

#[derive(Clone, Debug, Deserialize)]
struct EvaluationCandidate {
    id: String,
    kind: String,
    description: String,
    host_effect: String,
    #[serde(default, alias = "is_escalation", alias = "escalates")]
    escalation: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
struct EvaluationExpected {
    acceptable_routes: Vec<String>,
    must_escalate: bool,
    #[serde(default)]
    risk_confidence_profile: Option<RiskConfidenceProfile>,
    semantic_only_authorization: SemanticOnlyAuthorization,
}

#[derive(Clone, Debug, Deserialize)]
struct RiskConfidenceProfile {
    risk_band: RiskBand,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RiskBand {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskBand {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct SemanticOnlyAuthorization {
    decision_is_advisory: bool,
    may_activate_trusted_session: bool,
    may_grant_permission: bool,
    host_must_recheck_scope: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
struct EvaluationObservation {
    case_id: String,
    run_index: usize,
    split: EvaluationSplit,
    risk_band: RiskBand,
    selected: Option<String>,
    fallback: Option<RouteFallbackReason>,
    confidence: Option<f32>,
    probability_semantics: ProbabilitySemantics,
    candidate_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    evaluation_input_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host_eligible_selection: Option<bool>,
    acceptable: bool,
    expected_escalation: bool,
    escalation: bool,
    operational_success: bool,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
struct ConfidenceSummary {
    count: usize,
    min: Option<f32>,
    mean: Option<f32>,
    max: Option<f32>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
struct OutcomeStability {
    groups: usize,
    stable_groups: usize,
    stable_rate: f64,
    dominant_outcome_frequency: f64,
    dominant_outcome_repeats: usize,
    repeats: usize,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
struct EvaluationAggregate {
    observations: usize,
    cases: usize,
    acceptable: usize,
    acceptable_rate: f64,
    expected_escalations: usize,
    escalations: usize,
    escalation_rate: f64,
    escalation_true_positives: usize,
    escalation_precision: Option<f64>,
    escalation_recall: Option<f64>,
    selected: usize,
    selection_rate: f64,
    covered: usize,
    coverage_rate: f64,
    semantic_abstentions: usize,
    semantic_abstention_rate: f64,
    operational_successes: usize,
    operational_success_rate: f64,
    fallback_counts: BTreeMap<String, usize>,
    confidence: ConfidenceSummary,
    outcome_stability: OutcomeStability,
}

#[derive(Clone, Debug, Serialize)]
struct EvaluationReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    fixture_fingerprint: Option<String>,
    schema_version: String,
    corpus_id: Option<String>,
    router: String,
    model: Option<String>,
    split: EvaluationSplit,
    repeats: usize,
    min_confidence: Option<f32>,
    cases: usize,
    observations: usize,
    /// Compatibility aliases for the first evaluator report.
    acceptable_route_accuracy: f64,
    escalation_precision: Option<f64>,
    escalation_recall: Option<f64>,
    coverage: f64,
    operational_success_rate: f64,
    observations_detail: Vec<EvaluationObservation>,
    aggregate: EvaluationAggregate,
    by_split: BTreeMap<String, EvaluationAggregate>,
    by_risk: BTreeMap<String, EvaluationAggregate>,
}

/// Evaluate a fixture with the configured provider and print a sanitized report.
pub(crate) async fn evaluate_routing(
    fixture: &Path,
    router: RouterKind,
    live: bool,
    min_confidence: Option<f32>,
    split: EvaluationSplit,
    repeats: u8,
) -> Result<()> {
    if router == RouterKind::Jev && !live {
        return Err(UrdiaError::InvalidInput(
            "Jev evaluation sends fixture data to TypeSafe; repeat with --live to opt in"
                .to_owned(),
        ));
    }
    if router != RouterKind::Jev && live {
        return Err(UrdiaError::InvalidInput(
            "--live is only valid with --router jev".to_owned(),
        ));
    }
    let corpus = load_corpus(fixture)?;
    validate_provider_model(&corpus, router)?;
    let resolver = route_resolver(router, min_confidence)?;
    let report = evaluate_with_resolver(
        &corpus,
        &resolver,
        split,
        usize::from(repeats),
        router_name(router),
        matches!(
            router,
            RouterKind::Jev | RouterKind::Laya | RouterKind::Typellm
        )
        .then_some(corpus.model.model.clone()),
        min_confidence,
    )
    .await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|_| UrdiaError::InvalidInput("serialize evaluation report".to_owned()))?
    );
    Ok(())
}

fn load_corpus(fixture: &Path) -> Result<EvaluationCorpus> {
    let value = read_bounded_json(fixture, MAX_EVALUATION_BYTES, "routing fixture")?;
    parse_corpus(value)
}

fn parse_corpus(value: Value) -> Result<EvaluationCorpus> {
    let binding = if value["schema_version"] == "urdia.profile-routing-evaluation/v5" {
        Some(urdia_routing::routing_context_fingerprint(&RouteRequest {
            objective: "urdia-profile-corpus-freeze-v1".into(),
            state: value.clone(),
            candidates: vec![RouteCandidate {
                id: "freeze".into(),
                description: "Whole fixture provenance; not executable".into(),
            }],
        })?)
    } else {
        None
    };
    let mut corpus: EvaluationCorpus = serde_json::from_value(value)
        .map_err(|_| UrdiaError::InvalidInput("routing fixture shape is invalid".to_owned()))?;
    corpus.fixture_fingerprint = binding;
    validate_corpus(corpus)
}

fn validate_corpus(corpus: EvaluationCorpus) -> Result<EvaluationCorpus> {
    let version = corpus_version(&corpus.schema_version)?;
    let expected_corpus_id = match version {
        CorpusVersion::V1 => "urdia-routing-v1",
        CorpusVersion::V2 => "urdia-routing-v2",
        CorpusVersion::V3 => "urdia-routing-v3",
        CorpusVersion::V4 => "urdia-routing-v4",
        CorpusVersion::V5 => "urdia-profile-routing-v5",
    };
    if corpus.language != "en"
        || corpus.corpus_id.as_deref() != Some(expected_corpus_id)
        || corpus.candidate_id_scope != "case_local"
        || corpus.privacy.persist_raw_response
        || corpus.privacy.persist_api_key
        || corpus.privacy.fixture_contains_real_secrets
        || corpus.cases.is_empty()
    {
        return Err(UrdiaError::InvalidInput(
            "routing fixture version or case set is invalid".to_owned(),
        ));
    }
    if version == CorpusVersion::V1 && corpus.cases.len() != 15 {
        return Err(UrdiaError::InvalidInput(
            "routing fixture v1 must contain exactly 15 cases".to_owned(),
        ));
    }
    if matches!(
        version,
        CorpusVersion::V2 | CorpusVersion::V3 | CorpusVersion::V4 | CorpusVersion::V5
    ) {
        let (version_label, expected_cases, expected_split_counts) = match version {
            CorpusVersion::V2 => ("v2", 27, [15, 6, 6]),
            CorpusVersion::V3 => ("v3", 31, [15, 8, 8]),
            CorpusVersion::V4 => ("v4", 24, [0, 12, 12]),
            CorpusVersion::V5 => ("v5", 24, [8, 8, 8]),
            CorpusVersion::V1 => unreachable!("v1 is handled above"),
        };
        if corpus.cases.len() != expected_cases {
            return Err(UrdiaError::InvalidInput(format!(
                "routing fixture {version_label} must contain exactly {expected_cases} cases"
            )));
        }
        let mut split_counts = [0_usize; 3];
        for case in &corpus.cases {
            match case.split {
                Some(EvaluationSplit::Development) => split_counts[0] += 1,
                Some(EvaluationSplit::HeldOut) => split_counts[1] += 1,
                Some(EvaluationSplit::Adversarial) => split_counts[2] += 1,
                Some(EvaluationSplit::All) | None => {}
            }
        }
        if split_counts != expected_split_counts {
            return Err(UrdiaError::InvalidInput(format!(
                "routing fixture {version_label} must contain {} development, {} held-out, and {} adversarial cases",
                expected_split_counts[0], expected_split_counts[1], expected_split_counts[2]
            )));
        }
    }
    let mut case_ids = BTreeSet::new();
    for case in &corpus.cases {
        if !valid_case_id(&case.id) {
            return Err(UrdiaError::InvalidInput(
                "routing fixture case IDs must use lowercase ASCII letters, digits, and underscores"
                    .to_owned(),
            ));
        }
        if !case_ids.insert(case.id.as_str()) {
            return Err(UrdiaError::InvalidInput(
                "routing fixture case IDs must be unique".to_owned(),
            ));
        }
        let candidate_ids = case
            .candidates
            .iter()
            .map(|candidate| candidate.id.as_str())
            .collect::<BTreeSet<_>>();
        if candidate_ids.len() != case.candidates.len() {
            return Err(UrdiaError::InvalidInput(format!(
                "routing fixture case `{}` contains duplicate candidate IDs",
                case.id
            )));
        }
        let safety = &case.expected.semantic_only_authorization;
        if case.expected.acceptable_routes.is_empty()
            || case
                .expected
                .acceptable_routes
                .iter()
                .any(|id| !candidate_ids.contains(id.as_str()))
            || !safety.decision_is_advisory
            || safety.may_activate_trusted_session
            || safety.may_grant_permission
            || !safety.host_must_recheck_scope
            || case.expected.risk_confidence_profile.as_ref().is_none()
        {
            return Err(UrdiaError::InvalidInput(format!(
                "routing fixture case `{}` violates the evaluation safety contract",
                case.id
            )));
        }
        match version {
            CorpusVersion::V1 => {
                if case.split.is_some()
                    || case
                        .candidates
                        .iter()
                        .any(|candidate| candidate.escalation.is_some())
                {
                    return Err(UrdiaError::InvalidInput(
                        "routing fixture v1 must not contain split or candidate escalation fields"
                            .to_owned(),
                    ));
                }
            }
            CorpusVersion::V2 | CorpusVersion::V3 | CorpusVersion::V4 | CorpusVersion::V5 => {
                if case.split.is_none()
                    || case
                        .candidates
                        .iter()
                        .any(|candidate| candidate.escalation.is_none())
                {
                    return Err(UrdiaError::InvalidInput(
                        "routing fixture v2+ requires an explicit split and candidate escalation boolean"
                            .to_owned(),
                    ));
                }
            }
        }
        if case.expected.acceptable_routes.iter().any(|acceptable| {
            case.candidates
                .iter()
                .find(|candidate| candidate.id == *acceptable)
                .is_none_or(|candidate| {
                    candidate_escalation(candidate, version) != case.expected.must_escalate
                })
        }) {
            return Err(UrdiaError::InvalidInput(format!(
                "routing fixture case `{}` has an inconsistent escalation label",
                case.id
            )));
        }
        if version == CorpusVersion::V5 {
            let eligible = super::profile_evaluation::eligible(&case.input.state)?;
            let profile_ids: BTreeSet<_> = case.input.state["profiles"]
                .as_object()
                .ok_or_else(|| UrdiaError::InvalidInput("profile state is invalid".into()))?
                .keys()
                .map(String::as_str)
                .collect();
            let candidate_profiles: BTreeSet<_> = case
                .candidates
                .iter()
                .filter(|candidate| candidate.escalation == Some(false))
                .map(|candidate| candidate.id.as_str())
                .collect();
            if profile_ids != candidate_profiles
                || case.candidates.iter().any(|candidate| {
                    !matches!(
                        candidate.kind.as_str(),
                        "execution_profile" | "operator_review"
                    )
                })
                || case
                    .expected
                    .acceptable_routes
                    .iter()
                    .any(|id| !case.expected.must_escalate && !eligible.contains(id))
            {
                return Err(UrdiaError::InvalidInput(
                    "profile labels contradict fixture eligibility".into(),
                ));
            }
        }
        validate_route_request(&route_request(case)).map_err(|_| {
            UrdiaError::InvalidInput(format!(
                "routing fixture case `{}` has an invalid route request",
                case.id
            ))
        })?;
    }
    Ok(corpus)
}

fn valid_case_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn corpus_version(schema_version: &str) -> Result<CorpusVersion> {
    match schema_version {
        "urdia.routing-evaluation/v1" => Ok(CorpusVersion::V1),
        "urdia.routing-evaluation/v2" => Ok(CorpusVersion::V2),
        "urdia.routing-evaluation/v3" => Ok(CorpusVersion::V3),
        "urdia.routing-evaluation/v4" => Ok(CorpusVersion::V4),
        "urdia.profile-routing-evaluation/v5" => Ok(CorpusVersion::V5),
        _ => Err(UrdiaError::InvalidInput(
            "unsupported routing fixture version".to_owned(),
        )),
    }
}

fn validate_provider_model(corpus: &EvaluationCorpus, router: RouterKind) -> Result<()> {
    let configured_typellm_model = std::env::var("URDIA_TYPELLM_MODEL").ok();
    validate_provider_model_with_typellm(corpus, router, configured_typellm_model.as_deref())
}

fn validate_provider_model_with_typellm(
    corpus: &EvaluationCorpus,
    router: RouterKind,
    configured_typellm_model: Option<&str>,
) -> Result<()> {
    match router {
        RouterKind::Jev
            if corpus.model.provider != "typesafe"
                || corpus.model.model != urdia_decision_jev::MODEL
                || !corpus.model.version_pinned =>
        {
            return Err(UrdiaError::InvalidInput(
                "routing fixture does not pin the adapter's exact Jev model".to_owned(),
            ));
        }
        RouterKind::Laya
            if corpus.model.provider != "laya"
                || corpus.model.model != urdia_decision_laya::MODEL
                || corpus.model.version_pinned =>
        {
            return Err(UrdiaError::InvalidInput(
                "routing fixture does not describe the local Laya adapter model".to_owned(),
            ));
        }
        RouterKind::Typellm
            if corpus.model.provider != "typellm"
                || !corpus.model.version_pinned
                || configured_typellm_model != Some(corpus.model.model.as_str()) =>
        {
            return Err(UrdiaError::InvalidInput(
                "routing fixture does not pin the configured TypeLLM model".to_owned(),
            ));
        }
        RouterKind::Deterministic | RouterKind::Jev | RouterKind::Laya | RouterKind::Typellm => {}
    }
    Ok(())
}

async fn evaluate_with_resolver(
    corpus: &EvaluationCorpus,
    resolver: &RouteResolver,
    requested_split: EvaluationSplit,
    repeats: usize,
    provider_label: &str,
    model: Option<String>,
    min_confidence: Option<f32>,
) -> Result<EvaluationReport> {
    if !(1..=10).contains(&repeats) {
        return Err(UrdiaError::InvalidInput(
            "routing evaluation repeats must be between one and ten".to_owned(),
        ));
    }
    let version = corpus_version(&corpus.schema_version)?;
    if version == CorpusVersion::V1 && requested_split != EvaluationSplit::All {
        return Err(UrdiaError::InvalidInput(
            "routing fixture v1 only supports --split all".to_owned(),
        ));
    }
    let selected_cases = corpus
        .cases
        .iter()
        .filter(|case| {
            requested_split == EvaluationSplit::All || case.split == Some(requested_split)
        })
        .collect::<Vec<_>>();
    if selected_cases.is_empty() {
        return Err(UrdiaError::InvalidInput(
            "routing evaluation split contains no cases".to_owned(),
        ));
    }

    let mut observations = Vec::with_capacity(selected_cases.len() * repeats);
    for run_index in 1..=repeats {
        for case in &selected_cases {
            observations.push(evaluate_case(case, resolver, run_index, version).await?);
        }
    }
    let aggregate = aggregate(&observations, repeats);
    let by_split = group_aggregates(&observations, repeats, |observation| {
        observation.split.as_str().to_owned()
    });
    let by_risk = group_aggregates(&observations, repeats, |observation| {
        observation.risk_band.as_str().to_owned()
    });
    let (acceptable_route_accuracy, escalation_precision, escalation_recall, coverage) =
        compatibility_metrics(&observations);
    Ok(EvaluationReport {
        fixture_fingerprint: corpus.fixture_fingerprint.clone(),
        schema_version: corpus.schema_version.clone(),
        corpus_id: corpus.corpus_id.clone(),
        router: provider_label.to_owned(),
        model,
        split: requested_split,
        repeats,
        min_confidence,
        cases: selected_cases.len(),
        observations: observations.len(),
        acceptable_route_accuracy,
        escalation_precision,
        escalation_recall,
        coverage,
        operational_success_rate: aggregate.operational_success_rate,
        observations_detail: observations,
        aggregate,
        by_split,
        by_risk,
    })
}

const fn router_name(router: RouterKind) -> &'static str {
    match router {
        RouterKind::Deterministic => "deterministic",
        RouterKind::Jev => "jev",
        RouterKind::Laya => "laya",
        RouterKind::Typellm => "typellm",
    }
}

async fn evaluate_case(
    case: &EvaluationCase,
    resolver: &RouteResolver,
    run_index: usize,
    version: CorpusVersion,
) -> Result<EvaluationObservation> {
    let route = resolver.resolve(route_request(case)).await?;
    let acceptable = route
        .selected
        .as_ref()
        .is_some_and(|id| case.expected.acceptable_routes.contains(id));
    let escalation = route.fallback_reason.is_some_and(is_semantic_fallback)
        || route.selected.as_ref().is_some_and(|id| {
            case.candidates
                .iter()
                .find(|candidate| &candidate.id == id)
                .is_some_and(|candidate| candidate_escalation(candidate, version))
        });
    let operational_success = !matches!(
        route.fallback_reason,
        Some(RouteFallbackReason::ProviderError | RouteFallbackReason::InvalidDecision)
    );
    Ok(EvaluationObservation {
        case_id: case.id.clone(),
        run_index,
        split: case.split.unwrap_or(EvaluationSplit::All),
        risk_band: case
            .expected
            .risk_confidence_profile
            .as_ref()
            .expect("validated corpus requires a risk profile")
            .risk_band,
        selected: route.selected.clone(),
        fallback: route.fallback_reason,
        confidence: route.confidence,
        probability_semantics: route.probability_semantics,
        candidate_fingerprint: route.candidate_fingerprint,
        evaluation_input_fingerprint: (version == CorpusVersion::V5)
            .then(|| urdia_routing::routing_context_fingerprint(&route_request(case)))
            .transpose()?,
        host_eligible_selection: if version == CorpusVersion::V5 {
            let eligible = super::profile_evaluation::eligible(&case.input.state)?;
            Some(route.selected.as_ref().is_some_and(|id| {
                eligible.contains(id)
                    || case
                        .candidates
                        .iter()
                        .any(|candidate| candidate.id == *id && candidate.escalation == Some(true))
            }))
        } else {
            None
        },
        acceptable,
        expected_escalation: case.expected.must_escalate,
        escalation,
        operational_success,
    })
}

fn route_request(case: &EvaluationCase) -> RouteRequest {
    RouteRequest {
        objective: case.input.objective.clone(),
        state: case.input.state.clone(),
        candidates: case
            .candidates
            .iter()
            .map(|candidate| RouteCandidate {
                id: candidate.id.clone(),
                description: candidate.description.clone(),
            })
            .collect(),
    }
}

fn candidate_escalation(candidate: &EvaluationCandidate, version: CorpusVersion) -> bool {
    match version {
        CorpusVersion::V1 => candidate_is_escalation_v1(candidate),
        CorpusVersion::V2 | CorpusVersion::V3 | CorpusVersion::V4 | CorpusVersion::V5 => {
            candidate.escalation.unwrap_or(false)
        }
    }
}

fn candidate_is_escalation_v1(candidate: &EvaluationCandidate) -> bool {
    matches!(
        candidate.kind.as_str(),
        "refactor_plan"
            | "operator_review"
            | "clarify"
            | "unsupported_capability"
            | "deny_destructive"
            | "one_shot_review"
            | "network_review"
            | "safe_analysis"
            | "block_secret_egress"
            | "provider_fallback"
            | "decompose_review"
    ) || matches!(
        candidate.host_effect.as_str(),
        "review_only"
            | "clarification"
            | "unsupported"
            | "deny"
            | "one_shot_review"
            | "network_review"
            | "secret_block"
            | "provider_fallback"
            | "decompose"
    )
}

const fn is_semantic_fallback(reason: RouteFallbackReason) -> bool {
    matches!(
        reason,
        RouteFallbackReason::NoSelection | RouteFallbackReason::LowConfidence
    )
}

fn aggregate(observations: &[EvaluationObservation], repeats: usize) -> EvaluationAggregate {
    let total = observations.len();
    let acceptable = observations.iter().filter(|item| item.acceptable).count();
    let expected_escalations = observations
        .iter()
        .filter(|item| item.expected_escalation)
        .count();
    let escalations = observations.iter().filter(|item| item.escalation).count();
    let escalation_true_positives = observations
        .iter()
        .filter(|item| item.escalation && item.expected_escalation)
        .count();
    let selected = observations
        .iter()
        .filter(|item| item.selected.is_some())
        .count();
    let semantic_abstentions = observations
        .iter()
        .filter(|item| item.fallback.is_some_and(is_semantic_fallback))
        .count();
    let covered = selected + semantic_abstentions;
    let operational_successes = observations
        .iter()
        .filter(|item| item.operational_success)
        .count();
    let mut fallback_counts = BTreeMap::new();
    for reason in observations.iter().filter_map(|item| item.fallback) {
        *fallback_counts
            .entry(fallback_reason_name(reason).to_owned())
            .or_insert(0) += 1;
    }
    EvaluationAggregate {
        observations: total,
        cases: observations
            .iter()
            .map(|item| item.case_id.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        acceptable,
        acceptable_rate: rate(acceptable, total),
        expected_escalations,
        escalations,
        escalation_rate: rate(escalations, total),
        escalation_true_positives,
        escalation_precision: (escalations > 0)
            .then(|| escalation_true_positives as f64 / escalations as f64),
        escalation_recall: (expected_escalations > 0)
            .then(|| escalation_true_positives as f64 / expected_escalations as f64),
        selected,
        selection_rate: rate(selected, total),
        covered,
        coverage_rate: rate(covered, total),
        semantic_abstentions,
        semantic_abstention_rate: rate(semantic_abstentions, total),
        operational_successes,
        operational_success_rate: rate(operational_successes, total),
        fallback_counts,
        confidence: confidence_summary(observations),
        outcome_stability: outcome_stability(observations, repeats),
    }
}

fn group_aggregates<F>(
    observations: &[EvaluationObservation],
    repeats: usize,
    key: F,
) -> BTreeMap<String, EvaluationAggregate>
where
    F: Fn(&EvaluationObservation) -> String,
{
    let mut groups: BTreeMap<String, Vec<EvaluationObservation>> = BTreeMap::new();
    for observation in observations {
        groups
            .entry(key(observation))
            .or_default()
            .push(observation.clone());
    }
    groups
        .into_iter()
        .map(|(key, observations)| (key, aggregate(&observations, repeats)))
        .collect()
}

fn confidence_summary(observations: &[EvaluationObservation]) -> ConfidenceSummary {
    let values = observations
        .iter()
        .filter_map(|item| item.confidence)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return ConfidenceSummary::default();
    }
    ConfidenceSummary {
        count: values.len(),
        min: values.iter().copied().reduce(f32::min),
        mean: Some(values.iter().sum::<f32>() / values.len() as f32),
        max: values.iter().copied().reduce(f32::max),
    }
}

fn outcome_stability(observations: &[EvaluationObservation], repeats: usize) -> OutcomeStability {
    let mut grouped: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for observation in observations {
        grouped
            .entry(observation.case_id.as_str())
            .or_default()
            .push(outcome_key(observation));
    }
    if grouped.is_empty() {
        return OutcomeStability {
            repeats,
            ..OutcomeStability::default()
        };
    }
    let mut stable_groups = 0;
    let mut dominant_repeats = 0;
    for outcomes in grouped.values() {
        let mut counts = HashMap::<&str, usize>::new();
        for outcome in outcomes {
            *counts.entry(outcome.as_str()).or_insert(0) += 1;
        }
        let dominant = counts.values().copied().max().unwrap_or_default();
        dominant_repeats += dominant;
        if dominant == outcomes.len() {
            stable_groups += 1;
        }
    }
    let groups = grouped.len();
    OutcomeStability {
        groups,
        stable_groups,
        stable_rate: stable_groups as f64 / groups as f64,
        dominant_outcome_frequency: if observations.is_empty() {
            0.0
        } else {
            dominant_repeats as f64 / observations.len() as f64
        },
        dominant_outcome_repeats: dominant_repeats,
        repeats,
    }
}

fn outcome_key(observation: &EvaluationObservation) -> String {
    match (&observation.selected, observation.fallback) {
        (Some(selected), fallback) => format!("selected:{selected}:fallback:{fallback:?}"),
        (None, Some(fallback)) => format!("fallback:{fallback:?}"),
        (None, None) => "abstention".to_owned(),
    }
}

fn compatibility_metrics(
    observations: &[EvaluationObservation],
) -> (f64, Option<f64>, Option<f64>, f64) {
    let total = observations.len() as f64;
    let acceptable = observations.iter().filter(|item| item.acceptable).count() as f64;
    let predicted = observations.iter().filter(|item| item.escalation).count();
    let expected = observations
        .iter()
        .filter(|item| item.expected_escalation)
        .count();
    let true_positive = observations
        .iter()
        .filter(|item| item.escalation && item.expected_escalation)
        .count();
    let covered = observations
        .iter()
        .filter(|item| item.selected.is_some() || item.fallback.is_some_and(is_semantic_fallback))
        .count() as f64;
    (
        rate_f64(acceptable, total),
        (predicted > 0).then(|| true_positive as f64 / predicted as f64),
        (expected > 0).then(|| true_positive as f64 / expected as f64),
        rate_f64(covered, total),
    )
}

fn fallback_reason_name(reason: RouteFallbackReason) -> &'static str {
    match reason {
        RouteFallbackReason::ProviderError => "provider_error",
        RouteFallbackReason::NoSelection => "no_selection",
        RouteFallbackReason::LowConfidence => "low_confidence",
        RouteFallbackReason::InvalidDecision => "invalid_decision",
    }
}

fn rate(numerator: usize, denominator: usize) -> f64 {
    rate_f64(numerator as f64, denominator as f64)
}

fn rate_f64(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use urdia_core::{Decision, DecisionInput, DecisionProvider};

    #[derive(Clone)]
    struct ScriptedProvider {
        id: &'static str,
        script: Arc<Mutex<Vec<Result<Decision>>>>,
    }

    #[async_trait]
    impl DecisionProvider for ScriptedProvider {
        fn id(&self) -> &str {
            self.id
        }

        async fn decide(&self, _input: DecisionInput) -> Result<Decision> {
            self.script.lock().expect("script lock").remove(0)
        }
    }

    fn decision(selected: Option<&str>) -> Decision {
        Decision {
            selected: selected.map(ToOwned::to_owned),
            probabilities: BTreeMap::from([("c_one".to_owned(), 0.7), ("c_two".to_owned(), 0.3)]),
            confidence: Some(0.7),
            probability_semantics: ProbabilitySemantics::Calibrated,
        }
    }

    fn test_case(id: &str, split: Option<EvaluationSplit>) -> EvaluationCase {
        EvaluationCase {
            id: id.to_owned(),
            input: EvaluationInput {
                objective: format!("secret objective {id}"),
                state: json!({"secret_state": id}),
            },
            candidates: vec![
                EvaluationCandidate {
                    id: "c_one".to_owned(),
                    kind: "history_read".to_owned(),
                    description: "secret description one".to_owned(),
                    host_effect: "read_only".to_owned(),
                    escalation: split.map(|_| false),
                },
                EvaluationCandidate {
                    id: "c_two".to_owned(),
                    kind: "operator_review".to_owned(),
                    description: "secret description two".to_owned(),
                    host_effect: "review_only".to_owned(),
                    escalation: split.map(|_| true),
                },
            ],
            expected: EvaluationExpected {
                acceptable_routes: vec!["c_one".to_owned()],
                must_escalate: false,
                risk_confidence_profile: Some(RiskConfidenceProfile {
                    risk_band: RiskBand::Low,
                }),
                semantic_only_authorization: SemanticOnlyAuthorization {
                    decision_is_advisory: true,
                    may_activate_trusted_session: false,
                    may_grant_permission: false,
                    host_must_recheck_scope: true,
                },
            },
            split,
        }
    }

    fn corpus(version: CorpusVersion, cases: Vec<EvaluationCase>) -> EvaluationCorpus {
        EvaluationCorpus {
            fixture_fingerprint: None,
            schema_version: match version {
                CorpusVersion::V1 => "urdia.routing-evaluation/v1".to_owned(),
                CorpusVersion::V2 => "urdia.routing-evaluation/v2".to_owned(),
                CorpusVersion::V3 => "urdia.routing-evaluation/v3".to_owned(),
                CorpusVersion::V4 => "urdia.routing-evaluation/v4".to_owned(),
                CorpusVersion::V5 => "urdia.profile-routing-evaluation/v5".to_owned(),
            },
            corpus_id: Some(
                match version {
                    CorpusVersion::V1 => "urdia-routing-v1",
                    CorpusVersion::V2 => "urdia-routing-v2",
                    CorpusVersion::V3 => "urdia-routing-v3",
                    CorpusVersion::V4 => "urdia-routing-v4",
                    CorpusVersion::V5 => "urdia-profile-routing-v5",
                }
                .to_owned(),
            ),
            language: "en".to_owned(),
            model: EvaluationModel {
                provider: "typesafe".to_owned(),
                model: urdia_decision_jev::MODEL.to_owned(),
                version_pinned: true,
            },
            candidate_id_scope: "case_local".to_owned(),
            privacy: EvaluationPrivacy {
                persist_raw_response: false,
                persist_api_key: false,
                fixture_contains_real_secrets: false,
            },
            cases,
        }
    }

    #[test]
    fn provider_metadata_is_validated_per_adapter() {
        let cases = vec![test_case("case_one", None)];
        let jev = corpus(CorpusVersion::V1, cases.clone());
        assert!(validate_provider_model(&jev, RouterKind::Jev).is_ok());
        assert!(validate_provider_model(&jev, RouterKind::Laya).is_err());

        let mut laya = jev.clone();
        laya.model = EvaluationModel {
            provider: "laya".to_owned(),
            model: urdia_decision_laya::MODEL.to_owned(),
            version_pinned: false,
        };
        assert!(validate_provider_model(&laya, RouterKind::Laya).is_ok());
        assert!(validate_provider_model(&laya, RouterKind::Jev).is_err());

        laya.model.version_pinned = true;
        assert!(validate_provider_model(&laya, RouterKind::Laya).is_err());

        let mut typellm = jev.clone();
        typellm.model = EvaluationModel {
            provider: "typellm".to_owned(),
            model: "Qwen/example@immutable-revision".to_owned(),
            version_pinned: true,
        };
        assert!(
            validate_provider_model_with_typellm(
                &typellm,
                RouterKind::Typellm,
                Some("Qwen/example@immutable-revision")
            )
            .is_ok()
        );
        assert!(
            validate_provider_model_with_typellm(
                &typellm,
                RouterKind::Typellm,
                Some("Qwen/other@revision")
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn scripted_provider_runs_sequentially_and_records_alternating_errors() {
        let cases = vec![test_case("case_one", None), test_case("case_two", None)];
        let script = Arc::new(Mutex::new(vec![
            Ok(decision(Some("c_one"))),
            Err(UrdiaError::Provider("RAW_SECRET".to_owned())),
            Ok(decision(Some("c_two"))),
            Ok(decision(None)),
        ]));
        let resolver = RouteResolver::new(Arc::new(ScriptedProvider {
            id: "scripted",
            script,
        }));
        let report = evaluate_with_resolver(
            &corpus(CorpusVersion::V1, cases),
            &resolver,
            EvaluationSplit::All,
            2,
            "scripted",
            None,
            None,
        )
        .await
        .expect("scripted evaluation");
        assert_eq!(report.observations, 4);
        assert_eq!(report.aggregate.fallback_counts["provider_error"], 1);
        assert_eq!(report.aggregate.fallback_counts["no_selection"], 1);
        assert_eq!(report.aggregate.operational_successes, 3);
        assert_eq!(report.acceptable_route_accuracy, 0.25);
        assert_eq!(report.escalation_precision, Some(0.0));
        assert_eq!(report.escalation_recall, None);
        assert_eq!(report.coverage, 0.75);
        assert_eq!(report.operational_success_rate, 0.75);
        assert_eq!(report.aggregate.outcome_stability.stable_rate, 0.0);
        assert_eq!(
            report
                .aggregate
                .outcome_stability
                .dominant_outcome_frequency,
            0.5
        );
        assert_eq!(report.observations_detail[0].run_index, 1);
        assert_eq!(report.observations_detail[2].run_index, 2);
    }

    #[tokio::test]
    async fn deterministic_outcomes_have_exact_stability_across_repeats() {
        let script = Arc::new(Mutex::new(
            (0..3).map(|_| Ok(decision(Some("c_one")))).collect(),
        ));
        let resolver = RouteResolver::new(Arc::new(ScriptedProvider {
            id: "deterministic",
            script,
        }));
        let report = evaluate_with_resolver(
            &corpus(CorpusVersion::V1, vec![test_case("case_one", None)]),
            &resolver,
            EvaluationSplit::All,
            3,
            "deterministic",
            None,
            None,
        )
        .await
        .expect("deterministic evaluation");
        assert_eq!(report.aggregate.outcome_stability.groups, 1);
        assert_eq!(
            report.aggregate.outcome_stability.dominant_outcome_repeats,
            3
        );
        assert_eq!(report.aggregate.outcome_stability.repeats, 3);
        assert_eq!(
            report
                .aggregate
                .outcome_stability
                .dominant_outcome_frequency,
            1.0
        );
    }

    #[test]
    fn duplicate_case_candidate_ids_are_rejected() {
        let mut cases = Vec::new();
        cases.extend((0..15).map(|index| {
            test_case(
                &format!("development_{index}"),
                Some(EvaluationSplit::Development),
            )
        }));
        cases.extend(
            (0..6).map(|index| {
                test_case(&format!("held_out_{index}"), Some(EvaluationSplit::HeldOut))
            }),
        );
        cases.extend((0..6).map(|index| {
            test_case(
                &format!("adversarial_{index}"),
                Some(EvaluationSplit::Adversarial),
            )
        }));
        cases[0].candidates[1].id = cases[0].candidates[0].id.clone();
        let error = validate_corpus(corpus(CorpusVersion::V2, cases))
            .expect_err("duplicate candidate IDs should be rejected");
        assert!(format!("{error}").contains("duplicate candidate IDs"));
    }

    #[test]
    fn v1_requires_exactly_fifteen_unique_cases_and_all_split() {
        let value: Value =
            serde_json::from_str(include_str!("../../../evaluations/routing/v1.json"))
                .expect("v1 JSON");
        let parsed: EvaluationCorpus = serde_json::from_value(value).expect("v1 shape");
        let validated = validate_corpus(parsed).expect("v1 corpus");
        assert_eq!(validated.cases.len(), 15);
        assert!(validated.cases.iter().all(|case| case.split.is_none()));
        let ids = validated
            .cases
            .iter()
            .map(|case| case.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), 15);
    }

    #[test]
    fn malformed_v1_count_and_duplicate_case_ids_are_rejected() {
        let value: Value =
            serde_json::from_str(include_str!("../../../evaluations/routing/v1.json"))
                .expect("v1 JSON");
        let mut short: EvaluationCorpus = serde_json::from_value(value.clone()).expect("v1 shape");
        short.cases.pop();
        assert!(
            format!(
                "{}",
                validate_corpus(short).expect_err("short v1 must fail")
            )
            .contains("exactly 15")
        );

        let mut duplicate: EvaluationCorpus = serde_json::from_value(value).expect("v1 shape");
        duplicate.cases[1].id = duplicate.cases[0].id.clone();
        assert!(
            format!(
                "{}",
                validate_corpus(duplicate).expect_err("duplicate case ID must fail")
            )
            .contains("case IDs must be unique")
        );
    }

    #[test]
    fn checked_in_v2_corpus_has_the_required_splits_and_labels() {
        let value: Value =
            serde_json::from_str(include_str!("../../../evaluations/routing/v2.json"))
                .expect("v2 JSON");
        let parsed: EvaluationCorpus = serde_json::from_value(value).expect("v2 shape");
        let validated = validate_corpus(parsed).expect("v2 corpus");
        assert_eq!(validated.cases.len(), 27);
        for (split, expected) in [
            (EvaluationSplit::Development, 15),
            (EvaluationSplit::HeldOut, 6),
            (EvaluationSplit::Adversarial, 6),
        ] {
            assert_eq!(
                validated
                    .cases
                    .iter()
                    .filter(|case| case.split == Some(split))
                    .count(),
                expected
            );
        }
    }

    #[test]
    fn checked_in_v3_corpus_has_the_required_splits_and_labels() {
        let value: Value =
            serde_json::from_str(include_str!("../../../evaluations/routing/v3.json"))
                .expect("v3 JSON");
        let parsed: EvaluationCorpus = serde_json::from_value(value).expect("v3 shape");
        let validated = validate_corpus(parsed).expect("v3 corpus");
        assert_eq!(validated.cases.len(), 31);
        assert_eq!(
            validated
                .cases
                .iter()
                .filter(|case| case.split == Some(EvaluationSplit::Development))
                .count(),
            15
        );
        assert_eq!(
            validated
                .cases
                .iter()
                .filter(|case| case.split == Some(EvaluationSplit::HeldOut))
                .count(),
            8
        );
        assert_eq!(
            validated
                .cases
                .iter()
                .filter(|case| case.split == Some(EvaluationSplit::Adversarial))
                .count(),
            8
        );
        assert!(validated.cases.iter().any(|case| {
            case.id == "missing_evidence_snapshot" && case.split == Some(EvaluationSplit::HeldOut)
        }));
        assert!(validated.cases.iter().any(|case| {
            case.id == "destructive_euphemism_prune"
                && case.split == Some(EvaluationSplit::Adversarial)
        }));
    }

    #[test]
    fn v3_rejects_wrong_case_count_and_split_counts() {
        let value: Value =
            serde_json::from_str(include_str!("../../../evaluations/routing/v3.json"))
                .expect("v3 JSON");
        let mut short: EvaluationCorpus = serde_json::from_value(value.clone()).expect("v3 shape");
        short.cases.pop();
        assert!(
            format!(
                "{}",
                validate_corpus(short).expect_err("short v3 must fail")
            )
            .contains("exactly 31")
        );

        let mut wrong_split: EvaluationCorpus = serde_json::from_value(value).expect("v3 shape");
        wrong_split.cases[30].split = Some(EvaluationSplit::Development);
        assert!(
            format!(
                "{}",
                validate_corpus(wrong_split).expect_err("wrong v3 split counts must fail")
            )
            .contains("15 development, 8 held-out, and 8 adversarial")
        );
    }

    fn profile_corpus() -> EvaluationCorpus {
        parse_corpus(
            serde_json::from_str(include_str!(
                "../../../evaluations/routing/v5-profiles.json"
            ))
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn frozen_profile_corpus_has_new_splits_and_consistent_host_eligibility() {
        let corpus = profile_corpus();
        assert_eq!(
            corpus.fixture_fingerprint.as_deref(),
            Some("23787cef784b8a05ee999757b6fc09e7554922acd3da07655d657172a036bb22")
        );
        assert_eq!(corpus.cases.len(), 24);
        for split in [
            EvaluationSplit::Development,
            EvaluationSplit::HeldOut,
            EvaluationSplit::Adversarial,
        ] {
            assert_eq!(
                corpus
                    .cases
                    .iter()
                    .filter(|case| case.split == Some(split))
                    .count(),
                8
            );
        }
        let historical: EvaluationCorpus =
            serde_json::from_str(include_str!("../../../evaluations/routing/v4.json")).unwrap();
        assert!(
            corpus
                .cases
                .iter()
                .all(|case| historical.cases.iter().all(|old| old.id != case.id))
        );
        for bad in [
            "automatic_execution",
            "revision_floor",
            "pin",
            "digest",
            "controls",
            "extra",
        ] {
            let mut corpus = profile_corpus();
            let case = &mut corpus.cases[0];
            let id = case.expected.acceptable_routes[0].clone();
            match bad {
                "automatic_execution" => {
                    case.input.state["policy"]["automatic_execution"] = json!(true)
                }
                "revision_floor" => case.input.state["policy"]["revision_floor"] = json!(0),
                "pin" => case.input.state["policy"]["pinned_profile"] = json!("missing"),
                "digest" => {
                    case.input.state["profiles"][&id]["launch_digest"] = json!("PRIVATE_SENTINEL")
                }
                "controls" => {
                    case.input.state["profiles"][&id]["requested_controls"]["effort"] =
                        json!("unsupported")
                }
                _ => case.input.state["profiles"][&id]["PRIVATE_SENTINEL"] = json!(true),
            }
            let error = validate_corpus(corpus).unwrap_err();
            assert!(!error.to_string().contains("PRIVATE_SENTINEL"));
        }
    }

    #[tokio::test]
    async fn profile_report_binds_context_and_flags_ineligible_raw_selection() {
        let corpus = profile_corpus();
        let case = corpus
            .cases
            .iter()
            .find(|case| case.id == "controls_changed_after_evidence")
            .unwrap();
        let ineligible = case.input.state["profiles"]
            .as_object()
            .unwrap()
            .keys()
            .find(|id| !case.expected.acceptable_routes.contains(id))
            .unwrap();
        let resolver = RouteResolver::new(Arc::new(ScriptedProvider {
            id: "scripted",
            script: Arc::new(Mutex::new(vec![Ok(decision(Some(ineligible)))])),
        }));
        let report = evaluate_case(case, &resolver, 1, CorpusVersion::V5)
            .await
            .unwrap();
        assert!(!report.acceptable);
        assert_eq!(report.host_eligible_selection, Some(false));
        assert_eq!(
            report.evaluation_input_fingerprint.as_ref().unwrap().len(),
            64
        );
        let before = urdia_routing::routing_context_fingerprint(&route_request(case)).unwrap();
        for field in [
            "requested_controls",
            "evidence_revision",
            "launch_digest",
            "pin",
        ] {
            let mut changed = case.clone();
            match field {
                "requested_controls" => {
                    changed.input.state["profiles"][ineligible]["requested_controls"]["effort"] =
                        json!("none")
                }
                "evidence_revision" => {
                    changed.input.state["profiles"][ineligible]["evidence_revision"] = json!(1000)
                }
                "launch_digest" => {
                    changed.input.state["profiles"][ineligible]["launch_digest"] =
                        json!("c".repeat(64))
                }
                _ => changed.input.state["policy"]["pinned_profile"] = json!(ineligible),
            }
            assert_ne!(
                before,
                urdia_routing::routing_context_fingerprint(&route_request(&changed)).unwrap()
            );
        }
        let json = serde_json::to_string(&report).unwrap();
        for private in [
            "declared_model",
            "evidence_controls",
            "requested_controls",
            case.input.objective.as_str(),
        ] {
            assert!(!json.contains(private));
        }
    }

    #[test]
    fn checked_in_v4_blind_corpus_has_only_new_test_splits() {
        let value: Value =
            serde_json::from_str(include_str!("../../../evaluations/routing/v4.json"))
                .expect("v4 JSON");
        let parsed: EvaluationCorpus = serde_json::from_value(value).expect("v4 shape");
        let validated = validate_corpus(parsed).expect("v4 corpus");
        assert_eq!(validated.cases.len(), 24);
        assert_eq!(
            validated
                .cases
                .iter()
                .filter(|case| case.split == Some(EvaluationSplit::Development))
                .count(),
            0
        );
        assert_eq!(
            validated
                .cases
                .iter()
                .filter(|case| case.split == Some(EvaluationSplit::HeldOut))
                .count(),
            12
        );
        assert_eq!(
            validated
                .cases
                .iter()
                .filter(|case| case.split == Some(EvaluationSplit::Adversarial))
                .count(),
            12
        );
        assert!(validated.cases.iter().all(|case| {
            case.expected
                .semantic_only_authorization
                .decision_is_advisory
                && !case
                    .expected
                    .semantic_only_authorization
                    .may_activate_trusted_session
                && !case
                    .expected
                    .semantic_only_authorization
                    .may_grant_permission
        }));

        let mut fixture_position_counts = [0_usize; 4];
        let mut lexical_position_counts = [0_usize; 4];
        for case in &validated.cases {
            let acceptable = case
                .expected
                .acceptable_routes
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            for (position, candidate) in case.candidates.iter().enumerate() {
                fixture_position_counts[position] +=
                    usize::from(acceptable.contains(candidate.id.as_str()));
            }
            let mut lexical = case.candidates.iter().collect::<Vec<_>>();
            lexical.sort_by(|left, right| left.id.cmp(&right.id));
            for (position, candidate) in lexical.into_iter().enumerate() {
                lexical_position_counts[position] +=
                    usize::from(acceptable.contains(candidate.id.as_str()));
            }
        }
        assert!(
            fixture_position_counts.iter().max().expect("maximum")
                - fixture_position_counts.iter().min().expect("minimum")
                <= 1
        );
        assert!(
            lexical_position_counts.iter().max().expect("maximum")
                - lexical_position_counts.iter().min().expect("minimum")
                <= 1
        );
    }

    #[test]
    fn v4_rejects_wrong_case_count_and_split_counts() {
        let value: Value =
            serde_json::from_str(include_str!("../../../evaluations/routing/v4.json"))
                .expect("v4 JSON");
        let mut short: EvaluationCorpus = serde_json::from_value(value.clone()).expect("v4 shape");
        short.cases.pop();
        assert!(
            format!(
                "{}",
                validate_corpus(short).expect_err("short v4 must fail")
            )
            .contains("exactly 24")
        );

        let mut wrong_split: EvaluationCorpus = serde_json::from_value(value).expect("v4 shape");
        wrong_split.cases[23].split = Some(EvaluationSplit::HeldOut);
        assert!(
            format!(
                "{}",
                validate_corpus(wrong_split).expect_err("wrong v4 split counts must fail")
            )
            .contains("0 development, 12 held-out, and 12 adversarial")
        );
    }

    #[test]
    fn invalid_case_ids_and_risk_bands_are_rejected_before_evaluation() {
        let value: Value =
            serde_json::from_str(include_str!("../../../evaluations/routing/v2.json"))
                .expect("v2 JSON");
        let mut invalid_id: EvaluationCorpus =
            serde_json::from_value(value.clone()).expect("v2 shape");
        invalid_id.cases[0].id = "Unsafe-ID".to_owned();
        assert!(
            format!(
                "{}",
                validate_corpus(invalid_id).expect_err("invalid case ID must fail")
            )
            .contains("lowercase ASCII")
        );

        let mut invalid_risk = value;
        invalid_risk["cases"][0]["expected"]["risk_confidence_profile"]["risk_band"] =
            Value::String("typo".to_owned());
        assert!(serde_json::from_value::<EvaluationCorpus>(invalid_risk).is_err());
    }

    #[tokio::test]
    async fn split_selection_rejects_v1_subsets_and_filters_v2() {
        let resolver = RouteResolver::new(Arc::new(ScriptedProvider {
            id: "scripted",
            script: Arc::new(Mutex::new(vec![Ok(decision(Some("c_one")))])),
        }));
        let v1_error = evaluate_with_resolver(
            &corpus(CorpusVersion::V1, vec![test_case("case_one", None)]),
            &resolver,
            EvaluationSplit::Development,
            1,
            "scripted",
            None,
            None,
        )
        .await
        .expect_err("v1 should be all-only");
        assert!(format!("{v1_error}").contains("only supports --split all"));

        let v2 = corpus(
            CorpusVersion::V2,
            vec![
                test_case("development_case", Some(EvaluationSplit::Development)),
                test_case("held_out_case", Some(EvaluationSplit::HeldOut)),
            ],
        );
        let report = evaluate_with_resolver(
            &v2,
            &resolver,
            EvaluationSplit::Development,
            1,
            "scripted",
            None,
            None,
        )
        .await
        .expect("development split");
        assert_eq!(report.cases, 1);
        assert_eq!(report.split, EvaluationSplit::Development);
    }

    #[tokio::test]
    async fn report_is_safe_and_contains_required_observation_fields() {
        let resolver = RouteResolver::new(Arc::new(ScriptedProvider {
            id: "scripted",
            script: Arc::new(Mutex::new(vec![Ok(decision(Some("c_one")))])),
        }));
        let report = evaluate_with_resolver(
            &corpus(CorpusVersion::V1, vec![test_case("secret_case", None)]),
            &resolver,
            EvaluationSplit::All,
            1,
            "scripted",
            None,
            None,
        )
        .await
        .expect("safe report");
        let serialized = serde_json::to_string(&report).expect("report JSON");
        assert!(!serialized.contains("secret objective"));
        assert!(!serialized.contains("secret_state"));
        assert!(!serialized.contains("secret description"));
        assert!(!serialized.contains("RAW_SECRET"));
        let observation = &report.observations_detail[0];
        assert_eq!(observation.split, EvaluationSplit::All);
        assert_eq!(observation.risk_band, RiskBand::Low);
        assert_eq!(
            observation.probability_semantics,
            ProbabilitySemantics::Calibrated
        );
        assert_eq!(observation.candidate_fingerprint.len(), 64);
    }
}
