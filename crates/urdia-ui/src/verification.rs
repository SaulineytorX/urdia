//! Fixed calculator evidence; never execution commands or model judgments.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Reviewed host-owned contracts; unknown values fail during request decoding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum VerificationContract {
    #[default]
    #[serde(rename = "calculator-add-v1")]
    Add,
    #[serde(rename = "calculator-five-v1")]
    Five,
    #[serde(rename = "python-unittest-v1")]
    PythonUnittest,
    #[serde(rename = "frontend-static-v1")]
    FrontendStatic,
    #[serde(rename = "frontend-counter-v1")]
    FrontendCounter,
}
impl VerificationContract {
    pub fn id(self) -> &'static str {
        match self {
            Self::Add => "calculator-add-v1",
            Self::Five => "calculator-five-v1",
            Self::PythonUnittest => "python-unittest-v1",
            Self::FrontendStatic => "frontend-static-v1",
            Self::FrontendCounter => "frontend-counter-v1",
        }
    }
}

/// Aggregate unittest observations, never names, exceptions or raw logs.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnittestCounts {
    pub tests_run: u32,
    pub failures: u32,
    pub errors: u32,
    pub skipped: u32,
    pub expected_failures: u32,
    pub unexpected_successes: u32,
}
impl UnittestCounts {
    fn valid(&self) -> bool {
        [
            self.tests_run,
            self.failures,
            self.errors,
            self.skipped,
            self.expected_failures,
            self.unexpected_successes,
        ]
        .iter()
        .all(|n| *n <= 1_000_000)
    }
    fn successful(&self) -> bool {
        self.tests_run > 0
            && self.failures == 0
            && self.errors == 0
            && self.skipped == 0
            && self.unexpected_successes == 0
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationCheck {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(default)]
    pub unittest: Option<UnittestCounts>,
    pub status: String,
    pub exit_code: i32,
    pub elapsed_ms: u64,
}
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontendCounts {
    pub viewports: u32,
    pub checks_run: u32,
    pub errors: u32,
    pub blocked_requests: u32,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationReport {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontend: Option<FrontendCounts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file_count: Option<u32>,
    pub contract: String,
    pub status: String,
    pub files_sha256_before: BTreeMap<String, String>,
    pub acceptance_files_unchanged: bool,
    pub checks: BTreeMap<String, VerificationCheck>,
}
impl VerificationReport {
    /// Fail closed on malformed or contradictory helper observations.
    pub fn valid(&self) -> bool {
        if matches!(
            self.contract.as_str(),
            "frontend-static-v1" | "frontend-counter-v1"
        ) {
            let expected = if self.contract == "frontend-counter-v1" {
                14
            } else {
                4
            };
            let Some(counts) = &self.frontend else {
                return false;
            };
            return self.schema_version == 4
                && self
                    .source_file_count
                    .is_some_and(|n| (1..=128).contains(&n))
                && self.files_sha256_before.len() == 1
                && self
                    .files_sha256_before
                    .get("frontend_sources")
                    .is_some_and(|h| {
                        h.len() == 64
                            && h.bytes()
                                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                    })
                && counts.viewports == 2
                && counts.checks_run <= expected
                && counts.errors <= 10000
                && counts.blocked_requests <= 10000
                && self.checks.len() == 2
                && ["desktop", "mobile"].iter().all(|name| {
                    self.checks.get(*name).is_some_and(|c| {
                        c.unittest.is_none()
                            && (if c.status == "passed" {
                                c.failure_reason.is_none()
                            } else {
                                c.failure_reason.as_deref().is_some_and(|r| {
                                    matches!(
                                        r,
                                        "load" | "layout" | "counter" | "runtime" | "timeout"
                                    )
                                })
                            })
                            && c.elapsed_ms <= 35000
                            && ((c.status == "passed" && c.exit_code == 0)
                                || (c.status == "failed" && c.exit_code == 1))
                    })
                })
                && matches!(self.status.as_str(), "passed" | "failed")
                && (self.status == "passed")
                    == (self.acceptance_files_unchanged
                        && counts.checks_run == expected
                        && counts.errors == 0
                        && counts.blocked_requests == 0
                        && self.checks.values().all(|c| c.status == "passed"));
        }
        if self.frontend.is_some() || self.checks.values().any(|c| c.failure_reason.is_some()) {
            return false;
        }
        if self.contract == "python-unittest-v1" {
            return self.schema_version == 3
                && self
                    .source_file_count
                    .is_some_and(|n| (1..=128).contains(&n))
                && matches!(self.status.as_str(), "passed" | "failed")
                && self.files_sha256_before.len() == 1
                && self
                    .files_sha256_before
                    .get("python_sources")
                    .is_some_and(|hash| {
                        hash.len() == 64
                            && hash
                                .bytes()
                                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                    })
                && self.checks.len() == 1
                && self.checks.get("project_tests").is_some_and(|check| {
                    check.elapsed_ms <= 40_000
                        && check.unittest.as_ref().is_none_or(UnittestCounts::valid)
                        && match check.status.as_str() {
                            "passed" => {
                                check.exit_code == 0
                                    && check
                                        .unittest
                                        .as_ref()
                                        .is_some_and(|c| c.successful() && c.expected_failures == 0)
                            }
                            "failed" => check.exit_code != 0 && check.exit_code != 124,
                            "timed_out" => check.exit_code == 124,
                            _ => false,
                        }
                })
                && (self.status == "passed")
                    == (self.acceptance_files_unchanged
                        && self.checks["project_tests"].status == "passed");
        }
        self.source_file_count.is_none()
            && matches!(self.schema_version, 1 | 2)
            && matches!(
                self.contract.as_str(),
                "calculator-add-v1" | "calculator-five-v1"
            )
            && matches!(self.status.as_str(), "passed" | "failed")
            && self.files_sha256_before.len() == 2
            && ["calculator.py", "test_calculator.py"].iter().all(|name| {
                self.files_sha256_before.get(*name).is_some_and(|hash| {
                    hash.len() == 64
                        && hash
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                })
            })
            && self.checks.len() == 2
            && ["host_contract", "project_tests"].iter().all(|name| {
                self.checks.get(*name).is_some_and(|check| {
                    check.elapsed_ms <= 40_000
                        && check.unittest.as_ref().is_none_or(UnittestCounts::valid)
                        && (check.status != "passed"
                            || check
                                .unittest
                                .as_ref()
                                .map_or(self.schema_version == 1, |counts| {
                                    counts.successful()
                                        && (*name != "host_contract"
                                            || counts.tests_run
                                                == if self.contract == "calculator-five-v1" {
                                                    12
                                                } else {
                                                    3
                                                })
                                }))
                        && match check.status.as_str() {
                            "passed" => check.exit_code == 0,
                            "timed_out" => check.exit_code == 124,
                            "failed" => check.exit_code != 0 && check.exit_code != 124,
                            _ => false,
                        }
                })
            })
            && (self.status == "passed")
                == (self.acceptance_files_unchanged
                    && self.checks.values().all(|check| check.status == "passed"))
    }
}
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct VerificationJob {
    pub contract: VerificationContract,
    pub request_id: Uuid,
    pub turn_id: Uuid,
    pub status: &'static str,
    pub report: Option<VerificationReport>,
    pub stale: bool,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct VerificationSnapshot {
    pub active: bool,
    pub latest: Option<VerificationJob>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationRequest {
    #[serde(default)]
    pub contract: VerificationContract,
    pub request_id: Uuid,
    pub turn_id: Uuid,
    pub confirm_execution: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationError {
    Unavailable,
    Unconfirmed,
    Stale,
    Busy,
    Conflict,
    Limit,
    HostUnavailable,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frontend_report_rejects_false_passes_and_wrong_scope() {
        let valid = serde_json::json!({"schema_version":4,"contract":"frontend-counter-v1","source_file_count":3,"status":"passed","files_sha256_before":{"frontend_sources":"a".repeat(64)},"acceptance_files_unchanged":true,"frontend":{"viewports":2,"checks_run":14,"errors":0,"blocked_requests":0},"checks":{"desktop":{"status":"passed","exit_code":0,"elapsed_ms":100},"mobile":{"status":"passed","exit_code":0,"elapsed_ms":100}}});
        assert!(
            serde_json::from_value::<VerificationReport>(valid.clone())
                .unwrap()
                .valid()
        );
        for field in ["checks_run", "viewports", "errors", "blocked_requests"] {
            let mut bad = valid.clone();
            bad["frontend"][field] = serde_json::json!(1);
            assert!(
                !serde_json::from_value::<VerificationReport>(bad)
                    .unwrap()
                    .valid()
            );
        }
        let mut changed = valid.clone();
        changed["acceptance_files_unchanged"] = serde_json::json!(false);
        assert!(
            !serde_json::from_value::<VerificationReport>(changed)
                .unwrap()
                .valid()
        );
        let mut unknown = valid;
        unknown["contract"] = serde_json::json!("frontend-other");
        assert!(
            !serde_json::from_value::<VerificationReport>(unknown)
                .unwrap()
                .valid()
        );
    }

    #[test]
    fn python_report_rejects_wrong_schema_empty_counts_and_contradictions() {
        let valid = serde_json::json!({"schema_version":3,"contract":"python-unittest-v1","source_file_count":2,"status":"passed","files_sha256_before":{"python_sources":"a".repeat(64)},"acceptance_files_unchanged":true,"checks":{"project_tests":{"status":"passed","exit_code":0,"elapsed_ms":100,"unittest":{"tests_run":1,"failures":0,"errors":0,"skipped":0,"expected_failures":0,"unexpected_successes":0}}}});
        assert!(
            serde_json::from_value::<VerificationReport>(valid.clone())
                .unwrap()
                .valid()
        );
        for (field, value) in [
            ("schema_version", serde_json::json!(2)),
            ("source_file_count", serde_json::json!(0)),
            ("acceptance_files_unchanged", serde_json::json!(false)),
        ] {
            let mut bad = valid.clone();
            bad[field] = value;
            assert!(
                !serde_json::from_value::<VerificationReport>(bad)
                    .unwrap()
                    .valid()
            );
        }
        for (field, value) in [
            ("tests_run", 0),
            ("errors", 1),
            ("skipped", 1),
            ("expected_failures", 1),
        ] {
            let mut bad = valid.clone();
            bad["checks"]["project_tests"]["unittest"][field] = serde_json::json!(value);
            assert!(
                !serde_json::from_value::<VerificationReport>(bad)
                    .unwrap()
                    .valid()
            );
        }
        let mut bad = valid;
        bad["raw_output"] = serde_json::json!("PRIVATE_OUTPUT");
        assert!(serde_json::from_value::<VerificationReport>(bad).is_err());
    }

    #[test]
    fn counted_reports_require_consistent_bounded_observations() {
        let counts = serde_json::json!({"tests_run":3,"failures":0,"errors":0,
            "skipped":0,"expected_failures":0,"unexpected_successes":0});
        let valid = serde_json::json!({
            "schema_version":2,"contract":"calculator-add-v1","status":"passed",
            "files_sha256_before":{"calculator.py":"0".repeat(64),"test_calculator.py":"1".repeat(64)},
            "acceptance_files_unchanged":true,
            "checks":{"host_contract":{"status":"passed","exit_code":0,"elapsed_ms":100,"unittest":counts},
                "project_tests":{"status":"passed","exit_code":0,"elapsed_ms":100,"unittest":counts}}
        });
        assert!(
            serde_json::from_value::<VerificationReport>(valid.clone())
                .unwrap()
                .valid()
        );
        for (field, value) in [
            ("tests_run", 0),
            ("failures", 1),
            ("errors", 1),
            ("skipped", 1),
            ("unexpected_successes", 1),
            ("tests_run", 1_000_001),
        ] {
            let mut bad = valid.clone();
            bad["checks"]["project_tests"]["unittest"][field] = serde_json::json!(value);
            assert!(
                !serde_json::from_value::<VerificationReport>(bad)
                    .unwrap()
                    .valid()
            );
        }
        let mut bad = valid.clone();
        bad["checks"]["host_contract"]["unittest"]["tests_run"] = serde_json::json!(33);
        assert!(
            !serde_json::from_value::<VerificationReport>(bad)
                .unwrap()
                .valid()
        );
        let mut bad = valid.clone();
        bad["checks"]["project_tests"]["unittest"] = serde_json::Value::Null;
        assert!(
            !serde_json::from_value::<VerificationReport>(bad)
                .unwrap()
                .valid()
        );
        for value in [
            serde_json::json!(-1),
            serde_json::json!(true),
            serde_json::json!(3.5),
        ] {
            let mut bad = valid.clone();
            bad["checks"]["project_tests"]["unittest"]["tests_run"] = value;
            assert!(serde_json::from_value::<VerificationReport>(bad).is_err());
        }
    }

    #[test]
    fn report_validation_rejects_unknown_and_contradictory_evidence() {
        let valid = serde_json::json!({
            "schema_version":1,"contract":"calculator-add-v1","status":"passed",
            "files_sha256_before":{"calculator.py":"0".repeat(64),"test_calculator.py":"1".repeat(64)},
            "acceptance_files_unchanged":true,
            "checks":{"host_contract":{"status":"passed","exit_code":0,"elapsed_ms":100},"project_tests":{"status":"passed","exit_code":0,"elapsed_ms":100}}
        });
        assert!(
            serde_json::from_value::<VerificationReport>(valid.clone())
                .unwrap()
                .valid()
        );
        for (field, value) in [
            ("status", serde_json::json!("failed")),
            ("contract", serde_json::json!("arbitrary")),
            ("acceptance_files_unchanged", serde_json::json!(false)),
        ] {
            let mut bad = valid.clone();
            bad[field] = value;
            assert!(
                !serde_json::from_value::<VerificationReport>(bad)
                    .unwrap()
                    .valid()
            );
        }
        let mut bad = valid.clone();
        bad["checks"]["project_tests"]["exit_code"] = serde_json::json!(1);
        assert!(
            !serde_json::from_value::<VerificationReport>(bad)
                .unwrap()
                .valid()
        );
        let mut bad = valid;
        bad["raw_output"] = serde_json::json!("SECRET");
        assert!(serde_json::from_value::<VerificationReport>(bad).is_err());
    }
}
