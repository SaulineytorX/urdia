//! Sanitize at the transport boundary, before errors reach UI/logging/persistence.
use agent_client_protocol::Error;
use urdia_core::{ProviderFailureKind as Kind, UrdiaError};

pub(super) fn sanitize(error: &Error) -> UrdiaError {
    UrdiaError::ProviderFailure(classify(error))
}

fn classify(error: &Error) -> Kind {
    if i32::from(error.code) == -32000 {
        return Kind::Authentication;
    }
    if let Some(kind) = error
        .data
        .as_ref()
        .and_then(|data| data.get("urdia_failure_kind"))
        .and_then(|value| value.as_str())
        .filter(|text| text.len() <= 32)
        .and_then(|text| {
            serde_json::from_value::<Kind>(serde_json::Value::String(text.to_owned())).ok()
        })
    {
        return kind;
    }
    if let Some(status) = error
        .data
        .as_ref()
        .and_then(|data| data.get("httpStatusCode"))
        .and_then(|value| value.as_i64())
    {
        match status {
            401 => return Kind::Authentication,
            403 => return Kind::AccessDenied,
            429 => return Kind::RateOrQuota,
            500..=599 => return Kind::Connection,
            _ => {}
        }
    }
    // Best-effort provider reports, not independently verified account state.
    // Scan bounded message fields only, never arbitrary nested payloads.
    for text in [
        Some(error.message.as_str()),
        error.data.as_ref().and_then(|data| data.as_str()),
        error
            .data
            .as_ref()
            .and_then(|data| data.get("message"))
            .and_then(|value| value.as_str()),
        error
            .data
            .as_ref()
            .and_then(|data| data.get("error"))
            .and_then(|error| error.get("message"))
            .and_then(|value| value.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        if text.len() > 4096 {
            continue;
        }
        let text = text.to_ascii_lowercase();
        for (kind, phrases) in [
            (
                Kind::Authentication,
                &[
                    "authentication required",
                    "not authenticated",
                    "unauthorized",
                    "incorrect api key",
                    "invalid api key",
                    "http 401",
                    "api error: 401",
                ][..],
            ),
            (
                Kind::AccessDenied,
                &["http 403", "access denied", "permission denied by provider"],
            ),
            (
                Kind::RateOrQuota,
                &[
                    "http 429",
                    "rate limit exceeded",
                    "rate_limit_exceeded",
                    "quota exceeded",
                    "usage limit exceeded",
                    "insufficient_quota",
                ],
            ),
            (
                Kind::ModelUnavailable,
                &[
                    "model not found",
                    "model_not_found",
                    "model is unavailable",
                    "unsupported model",
                ],
            ),
            (
                Kind::ContextLimit,
                &[
                    "context length exceeded",
                    "context_length_exceeded",
                    "context window exceeded",
                ],
            ),
            (
                Kind::Connection,
                &[
                    "connection refused",
                    "connection reset",
                    "network connection failed",
                ],
            ),
        ] {
            if phrases.iter().any(|phrase| text.contains(phrase)) {
                return kind;
            }
        }
    }
    Kind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn protocol_codes_and_allowlisted_metadata_take_priority_and_drop_private_data() {
        let error = Error::auth_required().data(json!({"secret":"PRIVATE_NATIVE_SENTINEL"}));
        assert!(matches!(
            sanitize(&error),
            UrdiaError::ProviderFailure(Kind::Authentication)
        ));
        for (status, kind) in [
            (401, Kind::Authentication),
            (403, Kind::AccessDenied),
            (429, Kind::RateOrQuota),
            (503, Kind::Connection),
        ] {
            let error = Error::internal_error()
                .data(json!({"httpStatusCode":status,"secret":"PRIVATE_NATIVE_SENTINEL"}));
            assert_eq!(classify(&error), kind);
            assert!(
                !format!("{} {:?}", sanitize(&error), sanitize(&error))
                    .contains("PRIVATE_NATIVE_SENTINEL")
            );
        }
        assert_eq!(
            classify(
                &Error::internal_error()
                    .data(json!({"urdia_failure_kind":"model_unavailable","httpStatusCode":401}))
            ),
            Kind::ModelUnavailable
        );
        assert_eq!(
            classify(
                &Error::internal_error()
                    .data(json!({"urdia_failure_kind":"PRIVATE_NATIVE_SENTINEL"}))
            ),
            Kind::Unknown
        );
    }

    #[test]
    fn bounded_message_hints_are_diagnostic_only_and_unrecognized_text_stays_unknown() {
        for (message, kind) in [
            (
                "API Error: 401 Incorrect API key provided. PRIVATE_NATIVE_SENTINEL",
                Kind::Authentication,
            ),
            (
                "rate_limit_exceeded PRIVATE_NATIVE_SENTINEL",
                Kind::RateOrQuota,
            ),
            (
                "unsupported model PRIVATE_NATIVE_SENTINEL",
                Kind::ModelUnavailable,
            ),
            ("context_length_exceeded", Kind::ContextLimit),
            ("connection refused", Kind::Connection),
        ] {
            let error = Error::new(-32603, message);
            assert_eq!(classify(&error), kind);
            assert!(
                !format!("{} {:?}", sanitize(&error), sanitize(&error))
                    .contains("PRIVATE_NATIVE_SENTINEL")
            );
        }
        for message in [
            "401 items in a file",
            "unknown private detail",
            "timeout in quoted source",
        ] {
            assert_eq!(classify(&Error::new(-32603, message)), Kind::Unknown);
        }
        assert_eq!(
            classify(&Error::new(-32603, "x".repeat(4097) + " unauthorized")),
            Kind::Unknown
        );
        assert_eq!(
            classify(&Error::internal_error().data(json!({"prompt":"unauthorized"}))),
            Kind::Unknown
        );
        assert_eq!(
            classify(&Error::internal_error().data(
                json!({"error":{"message":"Incorrect API key provided. PRIVATE_NATIVE_SENTINEL"}})
            )),
            Kind::Authentication
        );
    }
}
