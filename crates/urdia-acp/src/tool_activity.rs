//! Bounded projection: raw tool identifiers remain private, all payloads are discarded.
use agent_client_protocol::schema::v1::{SessionUpdate, ToolCallStatus, ToolKind};
use std::collections::HashMap;
use urdia_core::{
    AgentToolActivity, AgentToolKind, AgentToolStatus, AgentUpdate, MAX_TOOL_ACTIVITIES,
    valid_control_id,
};
#[derive(Default)]
pub(super) struct ToolActivityTracker {
    calls: HashMap<String, AgentToolActivity>,
    limit_reported: bool,
}
impl ToolActivityTracker {
    pub fn observe(&mut self, update: &SessionUpdate) -> Option<AgentUpdate> {
        let (id, kind, status) = match update {
            SessionUpdate::ToolCall(call) => (
                call.tool_call_id.0.as_ref(),
                Some(call.kind),
                Some(call.status),
            ),
            SessionUpdate::ToolCallUpdate(call) => (
                call.tool_call_id.0.as_ref(),
                call.fields.kind,
                call.fields.status,
            ),
            _ => return None,
        };
        if !valid_control_id(id) {
            return None;
        }
        let previous = self.calls.get(id).copied();
        if let Some(previous) = previous {
            if previous.status.is_terminal() {
                return None;
            }
        } else if self.calls.len() >= MAX_TOOL_ACTIVITIES {
            if self.limit_reported {
                return None;
            }
            self.limit_reported = true;
            return Some(AgentUpdate::ToolActivityLimitReached);
        }
        let mut activity = previous.unwrap_or(AgentToolActivity {
            ordinal: (self.calls.len() + 1) as u16,
            kind: AgentToolKind::Other,
            status: AgentToolStatus::Unknown,
        });
        if let Some(kind) = kind {
            activity.kind = project_kind(kind);
        }
        if let Some(status) = status {
            let status = project_status(status);
            if status.rank() >= activity.status.rank() {
                activity.status = status;
            }
        }
        if previous == Some(activity) {
            return None;
        }
        self.calls.insert(id.to_owned(), activity);
        Some(AgentUpdate::ToolActivity(activity))
    }
}
fn project_kind(kind: ToolKind) -> AgentToolKind {
    match kind {
        ToolKind::Read => AgentToolKind::Read,
        ToolKind::Edit => AgentToolKind::Edit,
        ToolKind::Delete => AgentToolKind::Delete,
        ToolKind::Move => AgentToolKind::Move,
        ToolKind::Search => AgentToolKind::Search,
        ToolKind::Execute => AgentToolKind::Execute,
        ToolKind::Think => AgentToolKind::Think,
        ToolKind::Fetch => AgentToolKind::Fetch,
        ToolKind::SwitchMode => AgentToolKind::SwitchMode,
        _ => AgentToolKind::Other,
    }
}
fn project_status(status: ToolCallStatus) -> AgentToolStatus {
    match status {
        ToolCallStatus::Pending => AgentToolStatus::Pending,
        ToolCallStatus::InProgress => AgentToolStatus::InProgress,
        ToolCallStatus::Completed => AgentToolStatus::Completed,
        ToolCallStatus::Failed => AgentToolStatus::Failed,
        _ => AgentToolStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{ToolCall, ToolCallUpdate, ToolCallUpdateFields};
    #[test]
    fn partial_reports_are_redacted_deduplicated_and_terminal_states_do_not_regress() {
        let mut tracker = ToolActivityTracker::default();
        let initial = SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "PRIVATE_ID",
            ToolCallUpdateFields::new()
                .kind(ToolKind::Execute)
                .title("PRIVATE_TITLE")
                .raw_input(serde_json::json!({"key":"PRIVATE_INPUT"})),
        ));
        let update = tracker.observe(&initial).unwrap();
        assert_eq!(
            update,
            AgentUpdate::ToolActivity(AgentToolActivity {
                ordinal: 1,
                kind: AgentToolKind::Execute,
                status: AgentToolStatus::Unknown
            })
        );
        assert!(!format!("{update:?}").contains("PRIVATE"));
        assert!(tracker.observe(&initial).is_none());
        let progress = SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "PRIVATE_ID",
            ToolCallUpdateFields::new().status(ToolCallStatus::InProgress),
        ));
        tracker.observe(&progress).unwrap();
        let pending =
            SessionUpdate::ToolCall(ToolCall::new("PRIVATE_ID", "PRIVATE").kind(ToolKind::Execute));
        assert!(tracker.observe(&pending).is_none());
        let done = SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "PRIVATE_ID",
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Failed)
                .raw_output(serde_json::json!({"secret":"PRIVATE_RESULT"})),
        ));
        assert!(tracker.observe(&done).is_some());
        assert!(tracker.observe(&progress).is_none());
        assert!(tracker.observe(&done).is_none());
    }
    #[test]
    fn invalid_ids_and_overflow_cannot_grow_tracking_or_repeat_limit_events() {
        let mut tracker = ToolActivityTracker::default();
        for id in ["".to_string(), "x".repeat(1025), "bad\nsecret".into()] {
            assert!(
                tracker
                    .observe(&SessionUpdate::ToolCall(ToolCall::new(id, "private")))
                    .is_none()
            );
        }
        for i in 0..MAX_TOOL_ACTIVITIES {
            assert!(
                tracker
                    .observe(&SessionUpdate::ToolCall(ToolCall::new(
                        format!("call-{i}"),
                        "private"
                    )))
                    .is_some()
            );
        }
        assert_eq!(
            tracker.observe(&SessionUpdate::ToolCall(ToolCall::new("extra", "private"))),
            Some(AgentUpdate::ToolActivityLimitReached)
        );
        assert!(
            tracker
                .observe(&SessionUpdate::ToolCall(ToolCall::new(
                    "another", "private"
                )))
                .is_none()
        );
        assert_eq!(tracker.calls.len(), MAX_TOOL_ACTIVITIES);
        assert!(
            tracker
                .observe(&SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    "call-0",
                    ToolCallUpdateFields::new().status(ToolCallStatus::Completed)
                )))
                .is_some()
        );
    }
}
