//! Read-only observations: completion, operator review and checks are separate.
use crate::{ConversationTurnStatus, ProjectPlan, VerificationJob};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ProjectSummary {
    pub completed_steps: usize,
    pub reviewed_steps: usize,
    pub checked_steps: usize,
    pub timed_steps: usize,
    pub latest_attempt_ms: Option<u64>,
    pub inherited_steps: usize,
    pub steps: Vec<StepSummary>,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct StepSummary {
    pub title: String,
    pub attempts: u32,
    pub reviewed: bool,
    pub turn_id: Option<Uuid>,
    pub status: Option<ConversationTurnStatus>,
    pub agent: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub verification: Option<StepVerification>,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct StepVerification {
    pub contract: &'static str,
    pub status: &'static str,
    /// A later task may have changed the workspace; evidence is for this task only.
    pub historical: bool,
    pub tests_run: Option<u64>,
    pub elapsed_ms: Option<u64>,
}

pub(crate) fn summarize(
    plan: &ProjectPlan,
    jobs: &BTreeMap<Uuid, VerificationJob>,
    order: &[Uuid],
    latest_turn: Option<Uuid>,
    elapsed: &HashMap<Uuid, u64>,
) -> ProjectSummary {
    let steps: Vec<_> = plan
        .steps
        .iter()
        .map(|step| {
            let task = step.task.as_ref();
            let verification = task
                .and_then(|task| {
                    order
                        .iter()
                        .rev()
                        .filter_map(|id| jobs.get(id))
                        .find(|job| job.turn_id == task.turn_id)
                })
                .map(|job| {
                    let report = job
                        .report
                        .as_ref()
                        .filter(|r| r.valid() && r.contract == job.contract.id());
                    StepVerification {
                        contract: job.contract.id(),
                        status: if job.status == "passed"
                            && !report.is_some_and(|r| r.status == "passed")
                        {
                            "unavailable"
                        } else {
                            job.status
                        },
                        historical: latest_turn != Some(job.turn_id),
                        tests_run: report.and_then(|r| {
                            r.checks.values().try_fold(0u64, |total, check| {
                                check
                                    .unittest
                                    .as_ref()
                                    .map(|counts| total + u64::from(counts.tests_run))
                            })
                        }),
                        elapsed_ms: report.map(|r| r.checks.values().map(|c| c.elapsed_ms).sum()),
                    }
                });
            StepSummary {
                title: step.definition.title.clone(),
                attempts: step.attempts,
                reviewed: step.reviewed,
                turn_id: task.map(|t| t.turn_id),
                status: task.map(|t| t.status),
                agent: task.and_then(|t| t.agent.clone()),
                elapsed_ms: task.and_then(|t| elapsed.get(&t.turn_id).copied().or(t.elapsed_ms)),
                verification,
            }
        })
        .collect();
    ProjectSummary {
        completed_steps: steps
            .iter()
            .filter(|s| s.status == Some(ConversationTurnStatus::Completed))
            .count(),
        reviewed_steps: steps.iter().filter(|s| s.reviewed).count(),
        checked_steps: steps
            .iter()
            .filter(|s| {
                s.verification
                    .as_ref()
                    .is_some_and(|v| v.status == "passed")
            })
            .count(),
        timed_steps: steps.iter().filter(|s| s.elapsed_ms.is_some()).count(),
        latest_attempt_ms: steps
            .iter()
            .filter_map(|s| s.elapsed_ms)
            .reduce(u64::saturating_add),
        inherited_steps: plan.recovery.as_ref().map_or(0, |r| r.archive.steps.len()),
        steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PlanStep, PlanStepDefinition, PlanTask, VerificationCheck, VerificationContract,
        VerificationReport,
    };
    fn plan(task: Uuid) -> ProjectPlan {
        ProjectPlan {
            id: Uuid::new_v4(),
            objective: "Private objective".into(),
            recovery: None,
            steps: vec![PlanStep {
                definition: PlanStepDefinition {
                    title: "Build".into(),
                    scope: "Private scope".into(),
                    acceptance: "Private acceptance".into(),
                },
                reviewed: false,
                attempts: 2,
                task: Some(PlanTask {
                    profile: None,
                    turn_id: task,
                    status: ConversationTurnStatus::Completed,
                    agent: Some("Agent".into()),
                    elapsed_ms: Some(40),
                }),
            }],
        }
    }
    fn report() -> VerificationReport {
        VerificationReport {
            frontend: None,
            source_file_count: None,
            schema_version: 1,
            contract: "calculator-add-v1".into(),
            status: "passed".into(),
            acceptance_files_unchanged: true,
            files_sha256_before: BTreeMap::from([
                ("calculator.py".into(), "a".repeat(64)),
                ("test_calculator.py".into(), "b".repeat(64)),
            ]),
            checks: BTreeMap::from([
                (
                    "host_contract".into(),
                    VerificationCheck {
                        failure_reason: None,
                        unittest: None,
                        status: "passed".into(),
                        exit_code: 0,
                        elapsed_ms: 10,
                    },
                ),
                (
                    "project_tests".into(),
                    VerificationCheck {
                        failure_reason: None,
                        unittest: None,
                        status: "passed".into(),
                        exit_code: 0,
                        elapsed_ms: 20,
                    },
                ),
            ]),
        }
    }
    #[test]
    fn completion_review_checks_and_duration_are_separate_observations() {
        let id = Uuid::new_v4();
        let mut p = plan(id);
        let empty = summarize(&p, &BTreeMap::new(), &[], Some(id), &HashMap::new());
        assert_eq!(
            (
                empty.completed_steps,
                empty.reviewed_steps,
                empty.checked_steps
            ),
            (1, 0, 0)
        );
        assert_eq!(empty.latest_attempt_ms, Some(40));
        assert_eq!(empty.steps[0].attempts, 2);
        p.steps[0].reviewed = true;
        let request = Uuid::new_v4();
        let jobs = BTreeMap::from([(
            request,
            VerificationJob {
                request_id: request,
                turn_id: id,
                contract: VerificationContract::Add,
                status: "passed",
                report: Some(report()),
                stale: false,
            },
        )]);
        let checked = summarize(
            &p,
            &jobs,
            &[request],
            Some(Uuid::new_v4()),
            &HashMap::from([(id, 75)]),
        );
        assert_eq!(
            (
                checked.completed_steps,
                checked.reviewed_steps,
                checked.checked_steps
            ),
            (1, 1, 1)
        );
        assert_eq!(checked.latest_attempt_ms, Some(75));
        assert!(checked.steps[0].verification.as_ref().unwrap().historical);
        assert_eq!(
            checked.steps[0].verification.as_ref().unwrap().elapsed_ms,
            Some(30)
        );
        assert_eq!(
            checked.steps[0].verification.as_ref().unwrap().tests_run,
            None
        );
        let body = serde_json::to_string(&checked).unwrap();
        assert!(!body.contains("Private scope"));
        assert!(!body.contains("files_sha256"));
    }
    #[test]
    fn new_attempt_and_newer_cancelled_checks_cannot_inherit_a_pass() {
        let id = Uuid::new_v4();
        let mut p = plan(id);
        let first = Uuid::from_u128(9);
        let last = Uuid::from_u128(1);
        let mut jobs = BTreeMap::from([(
            first,
            VerificationJob {
                request_id: first,
                turn_id: id,
                contract: VerificationContract::Add,
                status: "passed",
                report: Some(report()),
                stale: false,
            },
        )]);
        jobs.insert(
            last,
            VerificationJob {
                request_id: last,
                turn_id: id,
                contract: VerificationContract::Add,
                status: "cancelled",
                report: None,
                stale: false,
            },
        );
        let result = summarize(&p, &jobs, &[first, last], Some(id), &HashMap::new());
        assert_eq!(result.checked_steps, 0);
        assert_eq!(
            result.steps[0].verification.as_ref().unwrap().status,
            "cancelled"
        );
        p.steps[0].task.as_mut().unwrap().turn_id = Uuid::new_v4();
        assert!(
            summarize(&p, &jobs, &[first, last], Some(id), &HashMap::new()).steps[0]
                .verification
                .is_none()
        );
        p.steps[0].task = None;
        let pending = summarize(&p, &jobs, &[first, last], Some(id), &HashMap::new());
        assert_eq!(pending.completed_steps, 0);
        assert_eq!(pending.latest_attempt_ms, None);
        jobs.get_mut(&first).unwrap().report = None;
        p = plan(id);
        assert_eq!(
            summarize(&p, &jobs, &[first], Some(id), &HashMap::new()).checked_steps,
            0
        );
    }
}
