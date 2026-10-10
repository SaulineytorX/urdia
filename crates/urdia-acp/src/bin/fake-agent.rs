//! Deterministic ACP v1 agent used by the contract test.

use agent_client_protocol::schema::v1::{
    AgentCapabilities, CancelNotification, ConfigOptionUpdate, ContentBlock, ContentChunk,
    InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse, PermissionOption,
    PermissionOptionKind, PromptRequest, PromptResponse, RequestPermissionOutcome,
    RequestPermissionRequest, SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse, StopReason, TextContent, ToolCall, ToolCallStatus,
    ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use agent_client_protocol::{Agent, Result, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

#[tokio::main]
async fn main() -> Result<()> {
    let hang_on_prompt = std::env::args().any(|arg| arg == "--hang");
    let failure_kind = std::env::args().find_map(|arg| match arg.as_str() {
        "--failure-auth" => Some("authentication"),
        "--failure-rate" => Some("rate_or_quota"),
        "--failure-model" => Some("model_unavailable"),
        "--failure-connection" => Some("connection"),
        "--failure-unknown" => Some("unknown"),
        _ => None,
    });
    let request_permission = std::env::args().any(|arg| arg == "--request-permission");
    let stream_response = std::env::args().any(|arg| arg == "--stream");
    let cancel_aware = std::env::args().any(|arg| arg == "--cancel-aware");
    let tool_activity = std::env::args().any(|arg| arg == "--tool-activity");
    let tools_wrong_session = std::env::args().any(|arg| arg == "--tools-wrong-session");
    let controls_drift = std::env::args().any(|arg| arg == "--controls-drift");
    let controls_wrong_session = std::env::args().any(|arg| arg == "--controls-wrong-session");
    let controls_drop = std::env::args().any(|arg| arg == "--controls-drop");
    let controls_enabled = std::env::args().any(|arg| arg == "--controls");
    let controls_hang = std::env::args().any(|arg| arg == "--controls-hang");
    let controls_mismatch = std::env::args().any(|arg| arg == "--controls-mismatch");
    let settings = Arc::new(std::sync::Mutex::new((
        "model-a(openai)".to_owned(),
        "xhigh".to_owned(),
    )));
    let first_prompt = Arc::new(AtomicBool::new(true));
    let cancel_requested = Arc::new(Notify::new());

    Agent
        .builder()
        .name("urdia-fake-agent")
        .on_receive_request(
            async |request: InitializeRequest, responder, _connection| {
                responder.respond(
                    InitializeResponse::new(request.protocol_version)
                        .agent_capabilities(AgentCapabilities::new()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let settings = Arc::clone(&settings);
                async move |_request: NewSessionRequest, responder, _connection| {
                    let mut response = NewSessionResponse::new("urdia-fake-session");
                    if controls_enabled {
                        response.config_options = Some(fake_controls(&settings.lock().unwrap()));
                    }
                    responder.respond(response)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: SetSessionConfigOptionRequest, responder, _connection| {
                if controls_hang { std::future::pending::<()>().await; }
                let value = serde_json::to_value(&request).unwrap()["value"].as_str().unwrap().to_owned();
                let mut settings = settings.lock().unwrap();
                match request.config_id.to_string().as_str() {
                    "model-control" => {
                        settings.0 = value;
                        settings.1 = if settings.0 == "model-b(openai)" { "default" } else { "xhigh" }.into();
                    }
                    "effort-control" => {
                        settings.1 = value;
                        if controls_mismatch { settings.0 = "model-a(openai)".into(); }
                    }
                    _ => return Err(agent_client_protocol::Error::invalid_params()),
                }
                responder.respond(SetSessionConfigOptionResponse::new(fake_controls(&settings)))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            {
                let cancel_requested = Arc::clone(&cancel_requested);
                async move |_notification: CancelNotification, _connection| {
                    cancel_requested.notify_waiters();
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: PromptRequest, responder, connection| {
                if let Some(kind) = failure_kind {
                    return responder.respond_with_error(agent_client_protocol::Error::new(
                        -32603, "PRIVATE_FAILURE_SENTINEL <img src=x onerror=alert(1)>"
                    ).data(serde_json::json!({"urdia_failure_kind":kind,"secret":"PRIVATE_FAILURE_SENTINEL"})));
                }
                let task_connection = connection.clone();
                let first_prompt = Arc::clone(&first_prompt);
                let cancel_requested = Arc::clone(&cancel_requested);
                connection.spawn(async move {
                    if controls_drift || controls_wrong_session || controls_drop {
                        let options = if controls_drop { Vec::new() } else {
                            fake_controls(&("model-a(openai)".into(), "xhigh".into()))
                        };
                        task_connection.send_notification(SessionNotification::new(
                            if controls_wrong_session { "unrelated-session".into() } else { request.session_id.clone() },
                            SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(options)),
                        ))?;
                    }
                    let prompt = request
                        .prompt
                        .into_iter()
                        .filter_map(|content| match content {
                            ContentBlock::Text(text) => Some(text.text),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    if tool_activity || tools_wrong_session {
                        let session_id = if tools_wrong_session { "wrong-tool-session".into() } else { request.session_id.clone() };
                        for (id, kind, final_status) in [
                            ("TOOL_PRIVATE_ID_READ", ToolKind::Read, ToolCallStatus::Completed),
                            ("TOOL_PRIVATE_ID_EDIT", ToolKind::Edit, ToolCallStatus::Completed),
                            ("TOOL_PRIVATE_ID_EXEC", ToolKind::Execute, ToolCallStatus::Failed),
                        ] {
                            task_connection.send_notification(SessionNotification::new(session_id.clone(),
                                SessionUpdate::ToolCall(ToolCall::new(id, "TOOL_PRIVATE_TITLE")
                                    .kind(kind).raw_input(serde_json::json!({"secret":"TOOL_PRIVATE_INPUT"})))) )?;
                            for status in [ToolCallStatus::InProgress, final_status] {
                                task_connection.send_notification(SessionNotification::new(session_id.clone(),
                                    SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(id, ToolCallUpdateFields::new()
                                        .status(status).raw_output(serde_json::json!({"secret":"TOOL_PRIVATE_OUTPUT"}))))))?;
                            }
                        }
                    }
                    let wait_for_cancellation = hang_on_prompt && first_prompt.swap(false, Ordering::AcqRel);
                    if wait_for_cancellation {
                        if cancel_aware {
                            cancel_requested.notified().await;
                            responder.respond(PromptResponse::new(StopReason::Cancelled))?;
                            return Ok(());
                        }
                        std::future::pending::<()>().await;
                    }
                    let permission_allowed = if request_permission {
                        let permission = task_connection
                            .send_request(RequestPermissionRequest::new(
                                request.session_id.clone(),
                                ToolCallUpdate::new(
                                    "fake-tool-1",
                                    ToolCallUpdateFields::new()
                                        .title("Run fake command")
                                        .name("fake-shell")
                                        .kind(ToolKind::Execute)
                                        .raw_input(serde_json::json!({
                                            "command": "echo",
                                            "secret": "FAKE_AGENT_SECRET_SENTINEL"
                                        })),
                                ),
                                vec![
                                    PermissionOption::new(
                                        "allow-always",
                                        "Allow always",
                                        PermissionOptionKind::AllowAlways,
                                    ),
                                    PermissionOption::new(
                                        "allow-once",
                                        "Allow once",
                                        PermissionOptionKind::AllowOnce,
                                    ),
                                    PermissionOption::new(
                                        "reject-once",
                                        "Reject once",
                                        PermissionOptionKind::RejectOnce,
                                    ),
                                ],
                            ))
                            .block_task()
                            .await?;
                        matches!(permission.outcome, RequestPermissionOutcome::Selected(selected) if selected.option_id.to_string() == "allow-once")
                    } else {
                        true
                    };
                    let response = if !permission_allowed {
                        "fake-agent: permission rejected".to_owned()
                    } else if prompt.is_empty() {
                        "fake-agent: empty prompt".to_owned()
                    } else {
                        format!("fake-agent: {prompt}")
                    };
                    let chunks = if stream_response {
                        deterministic_chunks(&response)
                    } else {
                        vec![response.as_str()]
                    };
                    for chunk in chunks {
                        task_connection.send_notification(SessionNotification::new(
                            request.session_id.clone(),
                            SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new(chunk)),
                            )),
                        ))?;
                        if stream_response {
                            tokio::task::yield_now().await;
                        }
                    }
                    responder.respond(PromptResponse::new(StopReason::EndTurn))?;
                    Ok(())
                })?;
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(Stdio::new())
        .await
}

fn deterministic_chunks(text: &str) -> Vec<&str> {
    let character_count = text.chars().count();
    if character_count < 2 {
        return vec![text];
    }
    let split_at = text
        .char_indices()
        .nth(character_count / 2)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    let (first, second) = text.split_at(split_at);
    vec![first, second]
}

fn fake_controls(
    settings: &(String, String),
) -> Vec<agent_client_protocol::schema::v1::SessionConfigOption> {
    let efforts = if settings.0 == "model-b(openai)" {
        vec!["none", "default"]
    } else {
        vec!["none", "medium", "xhigh"]
    };
    serde_json::from_value(serde_json::json!([
        {"id":"mode", "name":"Permission mode", "category":"mode", "type":"select", "currentValue":"safe", "options":[{"value":"safe","name":"Safe"}]},
        {"id":"model-control", "name":"Model", "category":"model", "type":"select", "currentValue":settings.0,
         "options":[{"group":"models", "name":"Models", "options":[{"value":"model-a(openai)","name":"A"},{"value":"model-b(openai)","name":"B"}]}]},
        {"id":"effort-control", "name":"Effort", "category":"thought_level", "type":"select", "currentValue":settings.1,
         "options":efforts.iter().map(|value| serde_json::json!({"value":value,"name":value})).collect::<Vec<_>>()}
    ])).unwrap()
}
