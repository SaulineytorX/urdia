//! Session-memory operator plans. Definitions never grant execution or file access.
use crate::ConversationTurnStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStepDefinition {
    pub title: String,
    pub scope: String,
    pub acceptance: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanLink {
    pub plan_id: Uuid,
    pub revision: u64,
    pub step_index: usize,
}
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanOperation {
    Create {
        objective: String,
        steps: Vec<PlanStepDefinition>,
    },
    Restore {
        archive: PlanArchive,
        start_step: usize,
        confirm_restore: bool,
    },
    Clear {
        plan_id: Uuid,
    },
    Review {
        plan_id: Uuid,
        step_index: usize,
        turn_id: Uuid,
        confirm_review: bool,
    },
}
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub request_id: Uuid,
    pub revision: u64,
    pub operation: PlanOperation,
}
/// Portable user-controlled content, never live task or permission authority.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanArchive {
    pub schema_version: u8,
    pub source_plan_id: Uuid,
    pub source_workspace: Option<String>,
    pub exported_unix_seconds: i64,
    pub objective: String,
    pub steps: Vec<ArchivedStep>,
}
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchivedStep {
    #[serde(default)]
    pub inherited_history: bool,
    pub definition: PlanStepDefinition,
    pub operator_reviewed: bool,
    pub last_task: Option<ArchivedTask>,
}
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchivedTask {
    pub turn_id: Uuid,
    pub status: ConversationTurnStatus,
    pub agent: Option<String>,
    pub elapsed_ms: Option<u64>,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct PlanRecovery {
    pub archive: PlanArchive,
    pub start_step: usize,
}
pub const MAX_PLAN_ARCHIVE_BYTES: usize = 98304;
fn bounded(s: &str, n: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= n
}
fn valid_outline(objective: &str, steps: &[PlanStepDefinition]) -> bool {
    bounded(objective, 1024)
        && !steps.is_empty()
        && steps.len() <= 8
        && objective.len()
            + steps
                .iter()
                .map(|s| s.title.len() + s.scope.len() + s.acceptance.len())
                .sum::<usize>()
            <= 65536
        && steps.iter().all(|s| {
            bounded(&s.title, 120) && bounded(&s.scope, 2000) && bounded(&s.acceptance, 2000)
        })
}
/// Untrusted generated outline: validation establishes shape, never quality or authority.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanDraft {
    pub objective: String,
    pub steps: Vec<PlanStepDefinition>,
}
impl PlanDraft {
    pub fn from_response(response: &str) -> Option<Self> {
        if response.len() > 98304 {
            return None;
        }
        let text = response.trim();
        let text = if let Some(body) = text
            .strip_prefix("```json\n")
            .or_else(|| text.strip_prefix("```\n"))
        {
            body.strip_suffix("```")?.trim()
        } else {
            text
        };
        let draft: Self = serde_json::from_str(text).ok()?;
        valid_outline(&draft.objective, &draft.steps).then_some(draft)
    }
}

impl PlanArchive {
    pub fn validate(&self) -> Result<(), PlanError> {
        let definitions: Vec<_> = self.steps.iter().map(|s| s.definition.clone()).collect();
        if self.schema_version != 1
            || !valid_outline(&self.objective, &definitions)
            || !(0..=253402300799).contains(&self.exported_unix_seconds)
            || self
                .source_workspace
                .as_ref()
                .is_some_and(|s| !bounded(s, 255))
            || self.steps.iter().any(|s| {
                s.last_task.as_ref().is_some_and(|t| {
                    t.agent.as_ref().is_some_and(|s| !bounded(s, 200))
                        || t.elapsed_ms.is_some_and(|n| n > 9007199254740991)
                })
            })
        {
            return Err(PlanError::Invalid);
        }
        if serde_json::to_vec(self)
            .map_err(|_| PlanError::Invalid)?
            .len()
            > MAX_PLAN_ARCHIVE_BYTES
        {
            return Err(PlanError::Limit);
        }
        Ok(())
    }
}

#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct PlanTask {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<crate::ConversationProfile>,
    pub turn_id: Uuid,
    pub status: ConversationTurnStatus,
    pub agent: Option<String>,
    pub elapsed_ms: Option<u64>,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct PlanStep {
    pub definition: PlanStepDefinition,
    pub reviewed: bool,
    pub attempts: u32,
    pub task: Option<PlanTask>,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct ProjectPlan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery: Option<PlanRecovery>,
    pub id: Uuid,
    pub objective: String,
    pub steps: Vec<PlanStep>,
}
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct PlanSnapshot {
    pub revision: u64,
    pub plan: Option<ProjectPlan>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanError {
    Invalid,
    Busy,
    Changed,
    Conflict,
    Limit,
    Blocked,
}
#[derive(Default)]
pub(crate) struct PlanStore {
    revision: u64,
    plan: Option<ProjectPlan>,
    accepted: HashMap<Uuid, PlanRequest>,
}
impl PlanStore {
    pub fn snapshot(&self) -> PlanSnapshot {
        PlanSnapshot {
            revision: self.revision,
            plan: self.plan.clone(),
        }
    }
    pub fn mutate(&mut self, request: PlanRequest, busy: bool) -> Result<(), PlanError> {
        if let Some(previous) = self.accepted.get(&request.request_id) {
            return if previous == &request {
                Ok(())
            } else {
                Err(PlanError::Conflict)
            };
        }
        if request.revision != self.revision {
            return Err(PlanError::Changed);
        }
        if busy {
            return Err(PlanError::Busy);
        }
        if self.accepted.len() >= 32 {
            return Err(PlanError::Limit);
        }
        match &request.operation {
            PlanOperation::Create { objective, steps } => {
                self.create(objective, steps)?;
            }
            PlanOperation::Restore {
                archive,
                start_step,
                confirm_restore,
            } => {
                if !confirm_restore || *start_step >= archive.steps.len() {
                    return Err(PlanError::Invalid);
                }
                archive.validate()?;
                let definitions: Vec<_> = archive.steps[*start_step..]
                    .iter()
                    .map(|s| s.definition.clone())
                    .collect();
                self.create(&archive.objective, &definitions)?;
                self.plan.as_mut().unwrap().recovery = Some(PlanRecovery {
                    archive: archive.clone(),
                    start_step: *start_step,
                });
            }
            PlanOperation::Clear { plan_id } => {
                if self.plan.as_ref().map(|p| p.id) != Some(*plan_id) {
                    return Err(PlanError::Changed);
                }
                self.plan = None;
            }
            PlanOperation::Review {
                plan_id,
                step_index,
                turn_id,
                confirm_review,
            } => {
                if !confirm_review {
                    return Err(PlanError::Invalid);
                }
                let plan = self
                    .plan
                    .as_mut()
                    .filter(|p| p.id == *plan_id)
                    .ok_or(PlanError::Changed)?;
                let step = plan.steps.get_mut(*step_index).ok_or(PlanError::Invalid)?;
                if step.reviewed
                    || !step.task.as_ref().is_some_and(|t| {
                        t.turn_id == *turn_id && t.status == ConversationTurnStatus::Completed
                    })
                {
                    return Err(PlanError::Blocked);
                }
                step.reviewed = true;
            }
        }
        self.accepted.insert(request.request_id, request);
        self.revision += 1;
        Ok(())
    }
    fn create(&mut self, objective: &str, steps: &[PlanStepDefinition]) -> Result<(), PlanError> {
        if !valid_outline(objective, steps) {
            return Err(PlanError::Invalid);
        }
        if self.plan.is_some() {
            return Err(PlanError::Changed);
        }
        self.plan = Some(ProjectPlan {
            id: Uuid::new_v4(),
            objective: objective.to_owned(),
            recovery: None,
            steps: steps
                .iter()
                .cloned()
                .map(|definition| PlanStep {
                    definition,
                    reviewed: false,
                    attempts: 0,
                    task: None,
                })
                .collect(),
        });
        Ok(())
    }
    pub fn export(&self, source_workspace: Option<String>) -> Result<PlanArchive, PlanError> {
        let plan = self.plan.as_ref().ok_or(PlanError::Changed)?;
        let mut archive = PlanArchive {
            schema_version: 1,
            source_plan_id: plan.id,
            source_workspace,
            exported_unix_seconds: time::OffsetDateTime::now_utc().unix_timestamp(),
            objective: plan.objective.clone(),
            steps: plan
                .steps
                .iter()
                .map(|s| ArchivedStep {
                    inherited_history: false,
                    definition: s.definition.clone(),
                    operator_reviewed: s.reviewed,
                    last_task: s.task.as_ref().map(|t| ArchivedTask {
                        turn_id: t.turn_id,
                        status: t.status,
                        agent: t.agent.clone(),
                        elapsed_ms: t.elapsed_ms,
                    }),
                })
                .collect(),
        };
        if let Some(recovery) = &plan.recovery {
            for (index, step) in archive.steps.iter_mut().enumerate() {
                if step.last_task.is_none() && !step.operator_reviewed {
                    let previous = &recovery.archive.steps[recovery.start_step + index];
                    step.last_task = previous.last_task.clone();
                    step.operator_reviewed = previous.operator_reviewed;
                    step.inherited_history = true;
                }
            }
            let mut earlier = recovery.archive.steps[..recovery.start_step].to_vec();
            for step in &mut earlier {
                step.inherited_history = true;
            }
            earlier.append(&mut archive.steps);
            archive.steps = earlier;
        }
        archive.validate()?;
        Ok(archive)
    }
    pub fn validate_link(&self, link: &PlanLink, text: &str) -> Result<(), PlanError> {
        let plan = self
            .plan
            .as_ref()
            .filter(|p| p.id == link.plan_id && self.revision == link.revision)
            .ok_or(PlanError::Changed)?;
        let step = plan.steps.get(link.step_index).ok_or(PlanError::Invalid)?;
        if step.reviewed
            || plan.steps[..link.step_index].iter().any(|s| !s.reviewed)
            || step.task.as_ref().is_some_and(|t| {
                !matches!(
                    t.status,
                    ConversationTurnStatus::Failed | ConversationTurnStatus::Cancelled
                )
            })
        {
            return Err(PlanError::Blocked);
        }
        if text != self.prompt(link.step_index).ok_or(PlanError::Invalid)? {
            return Err(PlanError::Invalid);
        }
        Ok(())
    }
    pub fn prompt(&self, index: usize) -> Option<String> {
        let p = self.plan.as_ref()?;
        let s = &p.steps.get(index)?.definition;
        Some(format!(
            "Project objective: {}\nStep {}/{}: {}\nScope / files allowed: {}\nAcceptance criteria: {}\nWork only on this step. Explain the changes and checks performed. Do not start later steps. Urdia permissions still require the operator's decision.",
            p.objective,
            index + 1,
            p.steps.len(),
            s.title,
            s.scope,
            s.acceptance
        ))
    }
    // Called only after the validated host command is successfully enqueued under the same lock.
    pub fn bind(&mut self, link: &PlanLink, turn_id: Uuid) {
        let step = &mut self.plan.as_mut().unwrap().steps[link.step_index];
        step.attempts += 1;
        step.task = Some(PlanTask {
            profile: None,
            turn_id,
            status: ConversationTurnStatus::Queued,
            agent: None,
            elapsed_ms: None,
        });
        self.revision += 1;
    }
    pub fn observe(&mut self, task: PlanTask) {
        if let Some(plan) = &mut self.plan {
            for step in &mut plan.steps {
                if step
                    .task
                    .as_ref()
                    .is_some_and(|t| t.turn_id == task.turn_id)
                {
                    step.task = Some(task.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn create(revision: u64) -> PlanRequest {
        PlanRequest {
            request_id: Uuid::new_v4(),
            revision,
            operation: PlanOperation::Create {
                objective: "private objective".into(),
                steps: vec![
                    PlanStepDefinition {
                        title: "step".into(),
                        scope: "private scope".into(),
                        acceptance: "check".into()
                    };
                    2
                ],
            },
        }
    }
    #[test]
    fn generated_drafts_accept_only_a_single_bounded_outline() {
        let good = r#"{"objective":"Five-number calculator","steps":[{"title":"Implement","scope":"calculator.py only","acceptance":"Four operations on five finite numbers"},{"title":"Verify","scope":"test_calculator.py only","acceptance":"Run unit tests including zero divisors"}]}"#;
        assert!(PlanDraft::from_response(good).is_some());
        assert!(PlanDraft::from_response(&format!("```json\n{good}\n```")).is_some());
        for bad in [
            format!("Here is the plan: {good}"),
            format!("{good} {good}"),
            good[..good.len() - 2].to_owned(),
            good.replace("Five-number calculator", ""),
            good.replace("\"objective\":", "\"permissions\":true,\"objective\":"),
            good.replace("calculator.py only", &"x".repeat(2001)),
            "x".repeat(98305),
            r#"{"objective":"PRIVATE_SECRET","steps":[]}"#.into(),
        ] {
            assert!(PlanDraft::from_response(&bad).is_none());
        }
        let draft = PlanDraft::from_response(good).unwrap();
        let too_many = PlanDraft {
            objective: draft.objective,
            steps: vec![draft.steps[0].clone(); 9],
        };
        assert!(PlanDraft::from_response(&serde_json::to_string(&too_many).unwrap()).is_none());
    }

    #[test]
    fn portable_recovery_preserves_notes_but_never_restores_execution_authority() {
        let mut old = PlanStore::default();
        old.mutate(create(0), false).unwrap();
        let old_id = old.plan.as_ref().unwrap().id;
        let turn_id = Uuid::new_v4();
        old.bind(
            &PlanLink {
                plan_id: old_id,
                revision: 1,
                step_index: 0,
            },
            turn_id,
        );
        old.observe(PlanTask {
            profile: None,
            turn_id,
            status: ConversationTurnStatus::Completed,
            agent: Some("old agent".into()),
            elapsed_ms: Some(50),
        });
        old.mutate(
            PlanRequest {
                request_id: Uuid::new_v4(),
                revision: 2,
                operation: PlanOperation::Review {
                    plan_id: old_id,
                    step_index: 0,
                    turn_id,
                    confirm_review: true,
                },
            },
            false,
        )
        .unwrap();
        let archive = old.export(Some("old project".into())).unwrap();
        assert!(archive.steps[0].operator_reviewed);
        assert_eq!(
            archive.steps[0].last_task.as_ref().unwrap().turn_id,
            turn_id
        );
        let mut fresh = PlanStore::default();
        let request = PlanRequest {
            request_id: Uuid::new_v4(),
            revision: 0,
            operation: PlanOperation::Restore {
                archive: archive.clone(),
                start_step: 1,
                confirm_restore: true,
            },
        };
        fresh.mutate(request.clone(), false).unwrap();
        let plan = fresh.plan.as_ref().unwrap();
        assert_ne!(plan.id, old_id);
        assert_eq!(plan.steps.len(), 1);
        assert!(!plan.steps[0].reviewed);
        assert_eq!(plan.steps[0].attempts, 0);
        assert!(plan.steps[0].task.is_none());
        assert_eq!(plan.recovery.as_ref().unwrap().start_step, 1);
        assert!(
            fresh
                .validate_link(
                    &PlanLink {
                        plan_id: old_id,
                        revision: 1,
                        step_index: 0
                    },
                    &fresh.prompt(0).unwrap()
                )
                .is_err()
        );
        let before = fresh.snapshot();
        fresh.mutate(request, true).unwrap();
        assert!(fresh.snapshot() == before);
        let saved_again = fresh.export(Some("new project".into())).unwrap();
        assert_eq!(saved_again.steps.len(), 2);
        assert!(saved_again.steps[0].inherited_history);
        assert_eq!(
            saved_again.steps[0].last_task.as_ref().unwrap().turn_id,
            turn_id
        );
        assert_eq!(saved_again.source_workspace.as_deref(), Some("new project"));
        let mut all = PlanStore::default();
        all.mutate(
            PlanRequest {
                request_id: Uuid::new_v4(),
                revision: 0,
                operation: PlanOperation::Restore {
                    archive,
                    start_step: 0,
                    confirm_restore: true,
                },
            },
            false,
        )
        .unwrap();
        assert!(
            all.plan
                .as_ref()
                .unwrap()
                .steps
                .iter()
                .all(|s| !s.reviewed && s.task.is_none())
        );
        assert!(all.export(None).unwrap().steps[0].inherited_history);
        assert_eq!(
            all.mutate(
                PlanRequest {
                    request_id: Uuid::new_v4(),
                    revision: 1,
                    operation: PlanOperation::Review {
                        plan_id: all.plan.as_ref().unwrap().id,
                        step_index: 0,
                        turn_id,
                        confirm_review: true
                    }
                },
                false
            ),
            Err(PlanError::Blocked)
        );
    }
    #[test]
    fn archive_validation_and_restore_consent_are_bounded_and_inert() {
        let mut s = PlanStore::default();
        s.mutate(create(0), false).unwrap();
        let archive = s.export(None).unwrap();
        for (start_step, confirm_restore) in [(0, false), (2, true)] {
            let mut fresh = PlanStore::default();
            assert_eq!(
                fresh.mutate(
                    PlanRequest {
                        request_id: Uuid::new_v4(),
                        revision: 0,
                        operation: PlanOperation::Restore {
                            archive: archive.clone(),
                            start_step,
                            confirm_restore
                        }
                    },
                    false
                ),
                Err(PlanError::Invalid)
            );
            assert!(fresh.plan.is_none());
            assert_eq!(fresh.revision, 0);
        }
        for variant in 0..5 {
            let mut a = archive.clone();
            match variant {
                0 => a.schema_version = 2,
                1 => a.exported_unix_seconds = -1,
                2 => a.source_workspace = Some("x".repeat(256)),
                3 => a.objective = "".into(),
                _ => {
                    a.steps[0].last_task = Some(ArchivedTask {
                        turn_id: Uuid::new_v4(),
                        status: ConversationTurnStatus::Completed,
                        agent: Some("x".repeat(201)),
                        elapsed_ms: None,
                    })
                }
            }
            assert_eq!(a.validate(), Err(PlanError::Invalid));
        }
        let mut oversized = archive;
        oversized.steps.resize(8, oversized.steps[0].clone());
        for step in &mut oversized.steps {
            step.definition.scope = "\u{0001}".repeat(2000);
            step.definition.acceptance = "\u{0001}".repeat(2000);
        }
        assert_eq!(oversized.validate(), Err(PlanError::Limit));
        let encoded = serde_json::to_value(s.export(None).unwrap()).unwrap();
        let mut unexpected = encoded;
        unexpected["steps"][0]["command"] = serde_json::json!("SECRET_COMMAND");
        assert!(serde_json::from_value::<PlanArchive>(unexpected).is_err());
    }

    #[test]
    fn mutations_are_bounded_revision_owned_and_exactly_replayable() {
        let mut s = PlanStore::default();
        let request = create(0);
        assert_eq!(s.mutate(request.clone(), true), Err(PlanError::Busy));
        s.mutate(request.clone(), false).unwrap();
        let before = s.snapshot();
        s.mutate(request.clone(), true).unwrap();
        assert!(s.snapshot() == before);
        let mut conflict = request;
        conflict.revision = 1;
        assert_eq!(s.mutate(conflict, false), Err(PlanError::Conflict));
        assert_eq!(s.mutate(create(0), false), Err(PlanError::Changed));
        assert_eq!(
            s.mutate(
                PlanRequest {
                    request_id: Uuid::new_v4(),
                    revision: 1,
                    operation: PlanOperation::Clear {
                        plan_id: Uuid::new_v4()
                    }
                },
                false
            ),
            Err(PlanError::Changed)
        );
        for _ in 0..15 {
            let p = s.snapshot();
            s.mutate(
                PlanRequest {
                    request_id: Uuid::new_v4(),
                    revision: p.revision,
                    operation: PlanOperation::Clear {
                        plan_id: p.plan.unwrap().id,
                    },
                },
                false,
            )
            .unwrap();
            s.mutate(create(s.revision), false).unwrap();
        }
        let id = s.plan.as_ref().unwrap().id;
        s.mutate(
            PlanRequest {
                request_id: Uuid::new_v4(),
                revision: s.revision,
                operation: PlanOperation::Clear { plan_id: id },
            },
            false,
        )
        .unwrap();
        assert_eq!(s.mutate(create(s.revision), false), Err(PlanError::Limit));
    }
    #[test]
    fn outline_limits_and_multibyte_payloads_are_validated_before_retention() {
        for invalid in [0, 9] {
            let mut s = PlanStore::default();
            let mut r = create(0);
            if let PlanOperation::Create { steps, .. } = &mut r.operation {
                steps.resize(
                    invalid,
                    PlanStepDefinition {
                        title: "x".into(),
                        scope: "x".into(),
                        acceptance: "x".into(),
                    },
                );
            }
            assert_eq!(s.mutate(r, false), Err(PlanError::Invalid));
            assert!(s.plan.is_none());
        }
        let mut s = PlanStore::default();
        let mut r = create(0);
        if let PlanOperation::Create { steps, .. } = &mut r.operation {
            steps.resize(8, steps[0].clone());
            for step in steps {
                step.scope = "🦉".repeat(2000);
                step.acceptance = "🦉".repeat(2000);
            }
        }
        assert_eq!(s.mutate(r, false), Err(PlanError::Invalid));
        let mut r = create(0);
        if let PlanOperation::Create { objective, .. } = &mut r.operation {
            *objective = " ".into();
        }
        assert_eq!(s.mutate(r, false), Err(PlanError::Invalid));
    }
    #[test]
    fn exact_link_and_completed_operator_review_gate_later_steps() {
        let mut s = PlanStore::default();
        s.mutate(create(0), false).unwrap();
        let id = s.plan.as_ref().unwrap().id;
        let link = PlanLink {
            plan_id: id,
            revision: 1,
            step_index: 0,
        };
        let prompt = s.prompt(0).unwrap();
        assert_eq!(s.validate_link(&link, "edited"), Err(PlanError::Invalid));
        let later = PlanLink {
            step_index: 1,
            ..link.clone()
        };
        assert_eq!(
            s.validate_link(&later, &s.prompt(1).unwrap()),
            Err(PlanError::Blocked)
        );
        s.validate_link(&link, &prompt).unwrap();
        let turn_id = Uuid::new_v4();
        s.bind(&link, turn_id);
        let review = |turn_id, confirm_review| PlanRequest {
            request_id: Uuid::new_v4(),
            revision: 2,
            operation: PlanOperation::Review {
                plan_id: id,
                step_index: 0,
                turn_id,
                confirm_review,
            },
        };
        assert_eq!(
            s.mutate(review(turn_id, true), false),
            Err(PlanError::Blocked)
        );
        s.observe(PlanTask {
            profile: None,
            turn_id,
            status: ConversationTurnStatus::Failed,
            agent: None,
            elapsed_ms: Some(9),
        });
        let retry = PlanLink {
            revision: 2,
            ..link.clone()
        };
        s.validate_link(&retry, &prompt).unwrap();
        s.observe(PlanTask {
            profile: None,
            turn_id,
            status: ConversationTurnStatus::Completed,
            agent: Some("agent".into()),
            elapsed_ms: Some(10),
        });
        assert_eq!(s.validate_link(&retry, &prompt), Err(PlanError::Blocked));
        assert_eq!(
            s.mutate(review(Uuid::new_v4(), true), false),
            Err(PlanError::Blocked)
        );
        assert_eq!(
            s.mutate(review(turn_id, false), false),
            Err(PlanError::Invalid)
        );
        s.mutate(review(turn_id, true), false).unwrap();
        s.validate_link(
            &PlanLink {
                revision: 3,
                ..later
            },
            &s.prompt(1).unwrap(),
        )
        .unwrap();
    }
}
