use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use urdia_acp::{AcpCommand, AcpProvider};
use urdia_core::{
    AgentProvider, AgentUpdate, AgentUpdateSink, PermissionBroker, PermissionDecision,
    PermissionReceipt, PermissionReceiptOutcome, PermissionReceiptSink, Prompt, Result,
    SessionRequest, WorkspacePath,
};
use urdia_ui::InteractivePermissionBroker;

struct AllowOnceBroker;

#[derive(Default)]
struct RecordingUpdates(Mutex<Vec<AgentUpdate>>);

impl AgentUpdateSink for RecordingUpdates {
    fn publish(&self, update: AgentUpdate) {
        self.0.lock().expect("update lock").push(update);
    }
}

#[async_trait]
impl PermissionBroker for AllowOnceBroker {
    async fn authorize(
        &self,
        request: urdia_core::PermissionRequest,
    ) -> Result<PermissionDecision> {
        assert_eq!(request.revision, 1);
        assert_eq!(request.operation.tool_call_id, "fake-tool-1");
        assert_eq!(request.operation.name.as_deref(), Some("fake-shell"));
        assert_eq!(
            request
                .operation
                .arguments
                .as_ref()
                .expect("arguments should be preserved")
                .expose()["secret"],
            "FAKE_AGENT_SECRET_SENTINEL"
        );
        Ok(PermissionDecision::AllowOnce)
    }
}

#[derive(Default)]
struct RecordingSink(Mutex<Vec<PermissionReceipt>>);

#[async_trait]
impl PermissionReceiptSink for RecordingSink {
    async fn record(&self, receipt: PermissionReceipt) -> Result<()> {
        self.0.lock().expect("receipt lock").push(receipt);
        Ok(())
    }
}

#[tokio::test]
async fn fake_agent_completes_deterministic_acp_v1_turn() {
    let executable = std::env::var_os("CARGO_BIN_EXE_urdia-fake-agent")
        .map(PathBuf::from)
        .expect("Cargo should expose the fake-agent binary to integration tests");
    let workspace = tempdir().expect("temporary workspace should be created");
    let workspace = WorkspacePath::new(workspace.path()).expect("workspace should canonicalize");
    let provider = AcpProvider::new(
        AcpCommand::new(executable)
            .startup_timeout(std::time::Duration::from_secs(10))
            .turn_timeout(std::time::Duration::from_secs(10)),
    );
    let session = provider
        .start_session(SessionRequest::new(workspace))
        .await
        .expect("ACP v1 initialize and session/new should complete");
    let result = session
        .prompt(Prompt::new("contract"))
        .await
        .expect("ACP v1 prompt should complete");
    assert_eq!(result.text, "fake-agent: contract");
    assert_eq!(result.stop_reason.as_deref(), Some("EndTurn"));
}

#[tokio::test]
async fn idle_cancellation_is_idempotent_and_keeps_session_alive() {
    let workspace = tempdir().expect("temporary workspace should be created");
    let workspace = WorkspacePath::new(workspace.path()).expect("workspace should canonicalize");
    let provider = AcpProvider::new(
        AcpCommand::new(fake_agent_executable())
            .startup_timeout(std::time::Duration::from_secs(10))
            .turn_timeout(std::time::Duration::from_secs(10)),
    );
    let session = provider
        .start_session(SessionRequest::new(workspace))
        .await
        .expect("fake agent should initialize");

    session
        .cancel()
        .await
        .expect("idle cancellation should succeed");
    session
        .cancel()
        .await
        .expect("repeated idle cancellation should succeed");
    let result = session
        .prompt(Prompt::new("after-idle-cancel"))
        .await
        .expect("session should remain usable");
    assert_eq!(result.text, "fake-agent: after-idle-cancel");
}

#[tokio::test]
async fn fake_agent_streams_ordered_chunks_before_each_final_result() {
    let workspace = tempdir().expect("temporary workspace should be created");
    let workspace = WorkspacePath::new(workspace.path()).expect("workspace should canonicalize");
    let provider = AcpProvider::new(
        AcpCommand::new(fake_agent_executable())
            .arg("--stream")
            .startup_timeout(std::time::Duration::from_secs(10))
            .turn_timeout(std::time::Duration::from_secs(10)),
    );
    let session = provider
        .start_session(SessionRequest::new(workspace))
        .await
        .expect("streaming fake agent should initialize");

    let first_updates = Arc::new(RecordingUpdates::default());
    let first_result = session
        .prompt_with_updates(Prompt::new("first-stream"), first_updates.clone())
        .await
        .expect("first streaming prompt should complete");
    let first_updates_snapshot = first_updates.0.lock().expect("update lock").clone();
    assert!(first_updates_snapshot.len() >= 2);
    assert_eq!(updates_text(&first_updates_snapshot), first_result.text);

    let second_updates = Arc::new(RecordingUpdates::default());
    let second_result = session
        .prompt_with_updates(Prompt::new("second-stream"), second_updates.clone())
        .await
        .expect("second streaming prompt should complete");
    let second_updates_snapshot = second_updates.0.lock().expect("update lock").clone();
    assert!(second_updates_snapshot.len() >= 2);
    assert_eq!(updates_text(&second_updates_snapshot), second_result.text);
    assert_eq!(
        updates_text(&first_updates_snapshot),
        "fake-agent: first-stream"
    );
}

#[tokio::test]
async fn fake_agent_allow_once_is_bound_and_receipted_before_execution() {
    let workspace = tempdir().expect("temporary workspace should be created");
    let workspace = WorkspacePath::new(workspace.path()).expect("workspace should canonicalize");
    let sink = Arc::new(RecordingSink::default());
    let provider = AcpProvider::new(
        AcpCommand::new(fake_agent_executable())
            .arg("--request-permission")
            .startup_timeout(std::time::Duration::from_secs(10))
            .turn_timeout(std::time::Duration::from_secs(10)),
    );
    let session = provider
        .start_session(
            SessionRequest::new(workspace)
                .permission_broker(Arc::new(AllowOnceBroker))
                .permission_receipt_sink(sink.clone()),
        )
        .await
        .expect("permission fake agent should initialize");
    let result = session
        .prompt(Prompt::new("permission-contract"))
        .await
        .expect("allow-once prompt should complete");
    assert_eq!(result.text, "fake-agent: permission-contract");

    let receipts = sink.0.lock().expect("receipt lock");
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].outcome, PermissionReceiptOutcome::AllowedOnce);
    let persisted_shape = serde_json::to_string(&receipts[0]).expect("receipt should serialize");
    assert!(!persisted_shape.contains("FAKE_AGENT_SECRET_SENTINEL"));
    assert!(!persisted_shape.contains("echo"));
}

#[tokio::test]
async fn fake_agent_waits_for_an_exact_once_interactive_decision() {
    let workspace = tempdir().expect("temporary workspace should be created");
    let workspace = WorkspacePath::new(workspace.path()).expect("workspace should canonicalize");
    let sink = Arc::new(RecordingSink::default());
    let broker = InteractivePermissionBroker::new();
    let provider = AcpProvider::new(
        AcpCommand::new(fake_agent_executable())
            .arg("--request-permission")
            .startup_timeout(std::time::Duration::from_secs(10))
            .turn_timeout(std::time::Duration::from_secs(10)),
    );
    let session = provider
        .start_session(
            SessionRequest::new(workspace)
                .permission_broker(Arc::new(broker.clone()))
                .permission_receipt_sink(sink.clone()),
        )
        .await
        .expect("interactive fake agent should initialize");
    let prompt_task =
        tokio::spawn(async move { session.prompt(Prompt::new("interactive-contract")).await });

    let pending = loop {
        if let Some(pending) = broker.pending().into_iter().next() {
            break pending;
        }
        tokio::task::yield_now().await;
    };
    assert_eq!(pending.revision, 1);
    broker
        .resolve(pending.id, pending.revision, PermissionDecision::AllowOnce)
        .expect("one interactive decision should be accepted");
    assert!(
        broker
            .resolve(pending.id, pending.revision, PermissionDecision::RejectOnce,)
            .is_err()
    );

    let result = prompt_task
        .await
        .expect("prompt task should join")
        .expect("interactive prompt should complete");
    assert_eq!(result.text, "fake-agent: interactive-contract");
    let receipts = sink.0.lock().expect("receipt lock");
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].outcome, PermissionReceiptOutcome::AllowedOnce);
}

#[tokio::test]
async fn hanging_agent_is_stopped_by_configured_turn_timeout() {
    let executable = fake_agent_executable();
    let workspace = tempdir().expect("temporary workspace should be created");
    let workspace = WorkspacePath::new(workspace.path()).expect("workspace should canonicalize");
    let provider = AcpProvider::new(
        AcpCommand::new(executable)
            .arg("--hang")
            .startup_timeout(std::time::Duration::from_secs(10))
            .turn_timeout(std::time::Duration::from_millis(100)),
    );
    let session = provider
        .start_session(SessionRequest::new(workspace))
        .await
        .expect("hanging fake agent should still initialize");

    let result = session.prompt(Prompt::new("timeout")).await;
    assert!(
        result
            .expect_err("prompt should use the configured timeout")
            .to_string()
            .contains("timed out")
    );

    // The timeout interrupts the worker, so a later command cannot reach the
    // child process after the SDK connection has been dropped.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let follow_up = session.prompt(Prompt::new("after-timeout")).await;
    assert!(
        follow_up
            .expect_err("worker should be stopped after timeout")
            .to_string()
            .contains("no longer running")
    );
}

#[tokio::test]
async fn active_prompt_deadline_does_not_reset_on_rejected_commands() {
    let executable = fake_agent_executable();
    let workspace = tempdir().expect("temporary workspace should be created");
    let workspace = WorkspacePath::new(workspace.path()).expect("workspace should canonicalize");
    let provider = AcpProvider::new(
        AcpCommand::new(executable)
            .arg("--hang")
            .startup_timeout(std::time::Duration::from_secs(10))
            .turn_timeout(std::time::Duration::from_millis(150)),
    );
    let session: Arc<dyn urdia_core::AgentSession> = Arc::from(
        provider
            .start_session(SessionRequest::new(workspace))
            .await
            .expect("hanging fake agent should initialize"),
    );
    let prompt_session = Arc::clone(&session);
    let started = std::time::Instant::now();
    let prompt_task =
        tokio::spawn(async move { prompt_session.prompt(Prompt::new("deadline")).await });

    let busy_session = Arc::clone(&session);
    let busy_task = tokio::spawn(async move {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(450);
        while std::time::Instant::now() < deadline {
            let _ = busy_session.prompt(Prompt::new("busy")).await;
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    });

    let result = tokio::time::timeout(std::time::Duration::from_secs(1), prompt_task)
        .await
        .expect("fixed deadline should finish before the test timeout")
        .expect("prompt task should join")
        .expect_err("the fixed deadline should stop the hanging prompt");
    busy_task.abort();
    assert!(result.to_string().contains("timed out"));
    assert!(
        started.elapsed() < std::time::Duration::from_millis(350),
        "prompt deadline was extended by rejected commands"
    );
}

#[tokio::test]
async fn cancel_aware_agent_keeps_session_alive_after_cancel() {
    let executable = fake_agent_executable();
    let workspace = tempdir().expect("temporary workspace should be created");
    let workspace = WorkspacePath::new(workspace.path()).expect("workspace should canonicalize");
    let provider = AcpProvider::new(
        AcpCommand::new(executable)
            .arg("--hang")
            .arg("--cancel-aware")
            .startup_timeout(std::time::Duration::from_secs(10))
            .turn_timeout(std::time::Duration::from_secs(10)),
    );
    let session = Arc::new(
        provider
            .start_session(SessionRequest::new(workspace))
            .await
            .expect("hanging fake agent should initialize"),
    );
    let prompt_session = Arc::clone(&session);
    let prompt_task =
        tokio::spawn(async move { prompt_session.prompt(Prompt::new("cancel")).await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    session
        .cancel()
        .await
        .expect("cancel should acknowledge and stop the worker");
    let prompt_result = prompt_task
        .await
        .expect("prompt task should join after cancellation");
    assert!(
        prompt_result
            .expect_err("active prompt should be interrupted")
            .to_string()
            .contains("cancelled")
    );

    let follow_up = session.prompt(Prompt::new("after-cancel")).await;
    assert_eq!(
        follow_up
            .expect("a cooperative cancellation should preserve the session")
            .text,
        "fake-agent: after-cancel"
    );
}

fn fake_agent_executable() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_urdia-fake-agent")
        .map(PathBuf::from)
        .expect("Cargo should expose the fake-agent binary to integration tests")
}

fn updates_text(updates: &[AgentUpdate]) -> String {
    updates
        .iter()
        .map(|update| match update {
            AgentUpdate::TextDelta { text } => text.as_str(),
            _ => "",
        })
        .collect()
}

#[tokio::test]
async fn startup_controls_switch_model_refresh_effort_and_preserve_turns() {
    let directory = tempdir().unwrap();
    let provider = AcpProvider::new(AcpCommand::new(fake_agent_executable()).arg("--controls"));
    let session = provider
        .start_session(
            SessionRequest::new(WorkspacePath::new(directory.path()).unwrap()).controls(
                urdia_core::SessionControls {
                    model: Some("model-b(openai)".into()),
                    effort: Some("none".into()),
                },
            ),
        )
        .await
        .unwrap();
    let state = session.startup_controls().unwrap();
    assert_eq!(state.model.as_deref(), Some("model-b(openai)"));
    assert_eq!(state.effort.as_deref(), Some("none"));
    assert_eq!(
        session
            .prompt(Prompt::new("configured"))
            .await
            .unwrap()
            .text,
        "fake-agent: configured"
    );
    assert!(!format!("{state:?}").contains("model-b"));
}

#[tokio::test]
async fn startup_controls_reject_stale_effort_and_mismatched_confirmation() {
    for (flag, effort) in [("--controls", "medium"), ("--controls-mismatch", "none")] {
        let directory = tempdir().unwrap();
        let provider =
            AcpProvider::new(AcpCommand::new(fake_agent_executable()).args(["--controls", flag]));
        let result = provider
            .start_session(
                SessionRequest::new(WorkspacePath::new(directory.path()).unwrap()).controls(
                    urdia_core::SessionControls {
                        model: Some("model-b(openai)".into()),
                        effort: Some(effort.into()),
                    },
                ),
            )
            .await;
        let error = match result {
            Ok(_) => panic!("must reject configuration before returning a session"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("unconfirmed"));
        assert!(!error.to_string().contains("model-b"));
    }
}

#[tokio::test]
async fn legacy_agents_remain_usable_but_reject_explicit_controls() {
    let directory = tempdir().unwrap();
    let provider = AcpProvider::new(AcpCommand::new(fake_agent_executable()));
    let workspace = WorkspacePath::new(directory.path()).unwrap();
    let session = provider
        .start_session(SessionRequest::new(workspace.clone()))
        .await
        .unwrap();
    assert!(session.startup_controls().is_none());
    assert!(
        provider
            .start_session(
                SessionRequest::new(workspace).controls(urdia_core::SessionControls {
                    model: Some("SECRET_CONTROL_SENTINEL".into()),
                    effort: None,
                })
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn configuration_rpc_cannot_outlive_the_startup_deadline() {
    let directory = tempdir().unwrap();
    let provider = AcpProvider::new(
        AcpCommand::new(fake_agent_executable())
            .args(["--controls", "--controls-hang"])
            .startup_timeout(std::time::Duration::from_millis(200)),
    );
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        provider.start_session(
            SessionRequest::new(WorkspacePath::new(directory.path()).unwrap()).controls(
                urdia_core::SessionControls {
                    model: Some("model-b(openai)".into()),
                    effort: None,
                },
            ),
        ),
    )
    .await
    .expect("startup must be bounded");
    let error = match result {
        Ok(_) => panic!("hanging configuration must not return a session"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("timed out") || error.to_string().contains("unconfirmed"));
}

#[tokio::test]
async fn live_controls_scope_drift_and_removed_selectors_guard_the_next_prompt() {
    for flag in [
        "--controls-drift",
        "--controls-wrong-session",
        "--controls-drop",
    ] {
        let directory = tempdir().unwrap();
        let provider =
            AcpProvider::new(AcpCommand::new(fake_agent_executable()).args(["--controls", flag]));
        let session = provider
            .start_session(
                SessionRequest::new(WorkspacePath::new(directory.path()).unwrap()).controls(
                    urdia_core::SessionControls {
                        model: Some("model-b(openai)".into()),
                        effort: Some("none".into()),
                    },
                ),
            )
            .await
            .unwrap();
        session.prompt(Prompt::new("first")).await.unwrap();
        let live = session.live_controls().unwrap();
        let wrong_session = flag == "--controls-wrong-session";
        assert_eq!(live.revision, if wrong_session { 0 } else { 1 });
        assert_eq!(live.changed_since_startup, !wrong_session);
        assert_eq!(live.explicit_configuration_matches, wrong_session);
        assert_eq!(
            session.prompt(Prompt::new("second")).await.is_ok(),
            wrong_session
        );
        assert_eq!(
            session.startup_controls().unwrap().model.as_deref(),
            Some("model-b(openai)")
        );
        assert!(!format!("{live:?}").contains("model-"));
        if flag == "--controls-drop" {
            let current = live.current.unwrap();
            assert!(current.model.is_none() && current.effort.is_none());
        }
    }
}

#[tokio::test]
async fn tool_reports_are_session_scoped_redacted_and_reset_for_each_active_turn() {
    for flag in ["--tool-activity", "--tools-wrong-session"] {
        let directory = tempdir().unwrap();
        let provider = AcpProvider::new(AcpCommand::new(fake_agent_executable()).args([flag]));
        let session = provider
            .start_session(SessionRequest::new(
                WorkspacePath::new(directory.path()).unwrap(),
            ))
            .await
            .unwrap();
        for _ in 0..2 {
            let sink = Arc::new(RecordingUpdates::default());
            let result = session
                .prompt_with_updates(Prompt::new("tools"), sink.clone())
                .await
                .unwrap();
            assert_eq!(result.text, "fake-agent: tools");
            let updates = sink.0.lock().unwrap();
            assert!(!format!("{updates:?}").contains("TOOL_PRIVATE"));
            let tools: Vec<_> = updates
                .iter()
                .filter_map(|update| match update {
                    AgentUpdate::ToolActivity(activity) => Some(*activity),
                    _ => None,
                })
                .collect();
            if flag == "--tools-wrong-session" {
                assert!(tools.is_empty());
            } else {
                assert_eq!(tools.len(), 9);
                assert_eq!(tools[0].ordinal, 1);
                assert_eq!(tools[8].ordinal, 3);
                assert_eq!(tools[8].status, urdia_core::AgentToolStatus::Failed);
            }
        }
    }
}

// Offline native final-state burst through the Python bridge and real ACP SDK.
#[cfg(unix)]
#[tokio::test]
async fn codex_final_suffix_reaches_stream_and_result_across_repeated_turns() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempdir().expect("fixture directory");
    let executable = directory.path().join("codex-fixture");
    std::fs::write(&executable, r#"#!/usr/bin/env python3
import json,sys
if '--version' in sys.argv:
 print('codex-cli 0.159.2');sys.exit(0)
count=0
for line in sys.stdin:
 m=json.loads(line); method=m.get('method'); i=m.get('id'); frames=[]
 if method=='initialize': frames=[{'id':i,'result':{}}]
 elif method=='thread/start': frames=[{'id':i,'result':{'thread':{'id':'thread'},'model':'fixture','reasoningEffort':'low','approvalPolicy':'on-request','approvalsReviewer':'user','sandbox':{'type':'readOnly'}}}]
 elif method=='turn/start':
  count+=1; turn='turn-'+str(count); text='{"objective":"fixture","steps":[]}'
  frames=[{'method':'turn/started','params':{'threadId':'thread','turn':{'id':turn}}},{'id':i,'result':{'turn':{'id':turn}}}]
  for chunk in text[:-2]: frames.append({'method':'item/agentMessage/delta','params':{'threadId':'thread','turnId':turn,'itemId':'message','delta':chunk}})
  frames.extend([{'method':'item/completed','params':{'threadId':'thread','turnId':turn,'item':{'id':'message','type':'agentMessage','text':text}}},{'method':'turn/completed','params':{'threadId':'thread','turn':{'id':turn,'status':'completed'}}}])
 if frames: sys.stdout.write(''.join(json.dumps(f)+'\n' for f in frames));sys.stdout.flush()
"#).expect("write native fixture");
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
        .expect("executable fixture");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/codex-acp.py");
    let provider = AcpProvider::new(
        AcpCommand::new("python3")
            .arg("-I")
            .arg("-B")
            .arg(script.to_string_lossy())
            .arg("--codex")
            .arg(executable.to_string_lossy())
            .arg("--model")
            .arg("fixture")
            .arg("--effort")
            .arg("low")
            .startup_timeout(std::time::Duration::from_secs(10))
            .turn_timeout(std::time::Duration::from_secs(10)),
    );
    let session = provider
        .start_session(SessionRequest::new(
            WorkspacePath::new(directory.path()).expect("workspace"),
        ))
        .await
        .expect("offline Codex bridge initialization");
    for _ in 0..3 {
        let updates = Arc::new(RecordingUpdates::default());
        let result = session
            .prompt_with_updates(Prompt::new("offline"), updates.clone())
            .await
            .expect("completed native burst");
        assert_eq!(result.text, r#"{"objective":"fixture","steps":[]}"#);
        assert_eq!(
            updates_text(&updates.0.lock().expect("updates")),
            result.text
        );
        assert_eq!(result.stop_reason.as_deref(), Some("EndTurn"));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn native_codex_failed_turn_reaches_core_as_redacted_authentication_kind() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempdir().unwrap();
    let executable = directory.path().join("codex-error-fixture");
    std::fs::write(&executable, r#"#!/usr/bin/env python3
import json,sys
if '--version' in sys.argv:
 print('codex-cli 0.159.2');sys.exit(0)
def emit(value):print(json.dumps(value),flush=True)
for line in sys.stdin:
 m=json.loads(line);method=m.get('method');i=m.get('id')
 if method=='initialize':emit({'id':i,'result':{}})
 elif method=='thread/start':emit({'id':i,'result':{'thread':{'id':'thread'},'model':'fixture','reasoningEffort':'low','approvalPolicy':'on-request','approvalsReviewer':'user','sandbox':{'type':'readOnly'}}})
 elif method=='turn/start':
  emit({'method':'turn/started','params':{'threadId':'thread','turn':{'id':'turn'}}})
  emit({'id':i,'result':{'turn':{'id':'turn'}}})
  emit({'method':'turn/completed','params':{'threadId':'thread','turn':{'id':'turn','status':'failed','error':{'codexErrorInfo':'unauthorized','message':'PRIVATE_NATIVE_SENTINEL'}}}})
 elif method=='turn/interrupt':emit({'id':i,'result':{}})
"#).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/codex-acp.py");
    let provider = AcpProvider::new(
        AcpCommand::new("python3")
            .arg("-I")
            .arg("-B")
            .arg(script.to_string_lossy())
            .arg("--codex")
            .arg(executable.to_string_lossy())
            .arg("--model")
            .arg("fixture")
            .arg("--effort")
            .arg("low")
            .startup_timeout(std::time::Duration::from_secs(10))
            .turn_timeout(std::time::Duration::from_secs(10)),
    );
    let session = provider
        .start_session(SessionRequest::new(
            WorkspacePath::new(directory.path()).unwrap(),
        ))
        .await
        .unwrap();
    let error = session
        .prompt(Prompt::new("offline failure"))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        urdia_core::UrdiaError::ProviderFailure(urdia_core::ProviderFailureKind::Authentication)
    ));
    assert!(!format!("{error:?} {error}").contains("PRIVATE_NATIVE_SENTINEL"));
}
