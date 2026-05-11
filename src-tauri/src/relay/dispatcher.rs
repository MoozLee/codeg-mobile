//! Application-layer request dispatcher for the relay link.
//!
//! The P3 scope only needed a smoke-test (`ping/pong`) path. P4 adds the
//! pairing variants so a freshly-scanned phone can register itself as a
//! trusted device. P5/P6/P7 extend these enums with session / prompt /
//! approval variants used by the phone frontend.
//!
//! Serialization uses `#[serde(tag = "type", rename_all = "snake_case")]`
//! so the TS side can read a discriminated union straight from JSON,
//! matching the existing pattern in `src/lib/types.ts`.
//!
//! Two dispatch paths live here:
//! - [`dispatch_app_request`] — pure fallback for builds that don't wire
//!   an [`AppState`] (early tests, pairing-only connections). Replies to
//!   `ping`; every other business variant falls through to `Error`.
//! - [`dispatch_app_request_with_state`] — the real, state-aware dispatch
//!   that talks to [`crate::acp::manager::ConnectionManager`], the DB, and
//!   the folder store. Mobile [`crate::relay::client::RelayClient`] uses
//!   this when started with an `Arc<AppState>`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::acp::types::PromptInputBlock;
use crate::app_state::AppState;
use crate::db::service::{conversation_service, folder_service};
use crate::models::AgentType;

/// Lightweight session summary sent to the phone's session list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MobileSessionSummary {
    pub id: String,
    pub title: String,
    pub agent_type: String,
    pub last_active_at: i64,
    pub status: String,
}

/// One message rendered on the phone's message stream. The wire format
/// stays intentionally narrow because the phone only needs role + text;
/// richer blocks (tool call payload, diff) arrive as role=tool text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MobileMessage {
    pub id: String,
    pub role: String,
    pub text: String,
    pub timestamp: i64,
}

/// Messages the phone may send to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppRequest {
    /// Smoke-test request. The daemon replies with [`AppResponse::Pong`]
    /// carrying the same payload, so a phone can confirm the relay + E2EE
    /// pipeline is healthy.
    Ping { payload: String },
    /// First message a freshly-scanned phone sends after the E2EE handshake.
    /// The daemon persists a `paired_devices` row and replies with
    /// [`AppResponse::DeviceRegisterAck`].
    DeviceRegister { nickname: String },
    /// Ask the daemon for all sessions it knows about (union of live
    /// ACP sessions + persisted DB conversations).
    ListSessions,
    /// Fetch the full message timeline for a single session.
    GetSession { session_id: String },
    /// Send a free-text prompt to an existing session.
    SendPrompt { session_id: String, text: String },
    /// Stop an in-flight agent response.
    StopSession { session_id: String },
    /// Create a new session.
    NewSession { agent_type: String, cwd: String },
    /// Approve / reject an outstanding permission request surfaced by
    /// [`AppResponse::ApprovalRequired`].
    ApprovalResponse { request_id: String, allow: bool },
}

/// Messages the daemon sends back to the phone.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppResponse {
    Pong {
        payload: String,
    },
    /// Response to [`AppRequest::DeviceRegister`]. The `device_id` is the
    /// stable uuid the daemon assigned; the phone saves it (with its
    /// ephemeral keypair and the daemon public key) to its device profile.
    DeviceRegisterAck {
        device_id: String,
        server_version: String,
    },
    /// Reserved slot for later phases: any handler error surfaces here so
    /// the phone can render it without knowing the specific command
    /// vocabulary.
    Error {
        message: String,
    },
    /// Response to [`AppRequest::ListSessions`].
    SessionsList {
        sessions: Vec<MobileSessionSummary>,
    },
    /// Response to [`AppRequest::GetSession`].
    SessionDetail {
        session_id: String,
        title: String,
        agent_type: String,
        messages: Vec<MobileMessage>,
    },
    /// Response to [`AppRequest::SendPrompt`].
    PromptAck {
        message_id: String,
    },
    /// Server-pushed delta for streaming assistant tokens or late-arriving
    /// user / tool messages.
    MessageDelta {
        session_id: String,
        role: String,
        delta: String,
    },
    /// Response to [`AppRequest::StopSession`].
    StopAck {
        session_id: String,
    },
    /// Agent asked for permission; phone should surface an approval UI.
    ApprovalRequired {
        request_id: String,
        session_id: String,
        summary: String,
    },
    /// Response to [`AppRequest::NewSession`].
    NewSessionCreated {
        session_id: String,
    },
}

/// Route an incoming request against the default (non-pairing) dispatch
/// table. The `ping` path stays for smoke tests. Pairing dispatch has its
/// own handler in [`crate::pairing`] which writes to the DB and cannot live
/// in this pure function. Business variants (list/get/send/etc) require
/// an `AppState`; callers that can provide one should use
/// [`dispatch_app_request_with_state`] instead. Here we return a
/// placeholder `Error` so the phone sees a well-formed response rather
/// than a decode crash.
pub fn dispatch_app_request(req: AppRequest) -> AppResponse {
    match req {
        AppRequest::Ping { payload } => AppResponse::Pong { payload },
        AppRequest::DeviceRegister { .. } => AppResponse::Error {
            message: "device_register is only accepted on a pairing session".into(),
        },
        AppRequest::ListSessions
        | AppRequest::GetSession { .. }
        | AppRequest::SendPrompt { .. }
        | AppRequest::StopSession { .. }
        | AppRequest::NewSession { .. }
        | AppRequest::ApprovalResponse { .. } => AppResponse::Error {
            message: "handler not yet wired on this daemon build".into(),
        },
    }
}

/// State-aware dispatch — the real entry point used by the mobile relay
/// client on a wired daemon. Each arm maps onto an existing `_core`
/// business function (see `commands/conversations.rs`,
/// `acp::manager::ConnectionManager`) or the session store. Errors get
/// wrapped into [`AppResponse::Error`] so the phone never has to decode a
/// typed error enum — one variant, one string.
pub async fn dispatch_app_request_with_state(
    req: AppRequest,
    state: &Arc<AppState>,
) -> AppResponse {
    match req {
        AppRequest::Ping { payload } => AppResponse::Pong { payload },
        AppRequest::DeviceRegister { .. } => AppResponse::Error {
            message: "device_register is only accepted on a pairing session".into(),
        },
        AppRequest::ListSessions => handle_list_sessions(state).await,
        AppRequest::GetSession { session_id } => handle_get_session(state, &session_id).await,
        AppRequest::SendPrompt { session_id, text } => {
            handle_send_prompt(state, &session_id, text).await
        }
        AppRequest::StopSession { session_id } => handle_stop_session(state, &session_id).await,
        AppRequest::NewSession { agent_type, cwd } => {
            handle_new_session(state, &agent_type, &cwd).await
        }
        AppRequest::ApprovalResponse { request_id, allow } => {
            handle_approval_response(state, &request_id, allow).await
        }
    }
}

/// Return a merged view of DB-persisted conversations + live connections.
/// A DB row is always present once a prompt has been sent; purely-spawned
/// live connections (no prompt yet) surface with id = connection_id so
/// the phone can still find them. Sorted newest-first.
async fn handle_list_sessions(state: &Arc<AppState>) -> AppResponse {
    let mut sessions: Vec<MobileSessionSummary> = match conversation_service::list_all(
        &state.db.conn,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|row| MobileSessionSummary {
                id: row.id.to_string(),
                title: row.title.unwrap_or_default(),
                agent_type: serde_json::to_value(row.agent_type)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default(),
                last_active_at: row.updated_at.timestamp(),
                status: row.status,
            })
            .collect(),
        Err(err) => {
            return AppResponse::Error {
                message: format!("list_sessions: {err}"),
            };
        }
    };

    // Surface live connections that haven't produced a DB row yet (agent
    // was just spawned, no prompt). Skip any whose state already has a
    // linked conversation_id — that row is already in the list above.
    let live = state.connection_manager.list_connections().await;
    for info in live {
        let already_listed = sessions.iter().any(|s| s.id == info.id);
        if already_listed {
            continue;
        }
        if let Some(st) = state.connection_manager.get_state(&info.id).await {
            let guard = st.read().await;
            if guard.conversation_id.is_some() {
                continue;
            }
            sessions.push(MobileSessionSummary {
                id: info.id.clone(),
                title: String::new(),
                agent_type: serde_json::to_value(info.agent_type)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default(),
                last_active_at: chrono::Utc::now().timestamp(),
                status: match info.status {
                    crate::acp::types::ConnectionStatus::Connecting => "connecting".into(),
                    crate::acp::types::ConnectionStatus::Connected => "connected".into(),
                    crate::acp::types::ConnectionStatus::Prompting => "prompting".into(),
                    crate::acp::types::ConnectionStatus::Disconnected => "disconnected".into(),
                    crate::acp::types::ConnectionStatus::Error => "error".into(),
                },
            });
        }
    }

    sessions.sort_by_key(|s| std::cmp::Reverse(s.last_active_at));
    AppResponse::SessionsList { sessions }
}

/// Load a single conversation's detail. The `session_id` parameter is
/// interpreted first as an i32 conversation id (DB row) and falls back to
/// a live connection id if parsing fails or the row doesn't exist.
/// Messages come from the parser-backed `get_folder_conversation_core`
/// for persisted rows; for live-only connections we return an empty stub
/// (the phone will rehydrate via message_delta pushes).
async fn handle_get_session(state: &Arc<AppState>, session_id: &str) -> AppResponse {
    if let Ok(cid) = session_id.parse::<i32>() {
        match crate::commands::conversations::get_folder_conversation_core(&state.db.conn, cid)
            .await
        {
            Ok(detail) => {
                let messages = turns_to_mobile_messages(&detail.turns);
                return AppResponse::SessionDetail {
                    session_id: session_id.to_string(),
                    title: detail.summary.title.clone().unwrap_or_default(),
                    agent_type: serde_json::to_value(detail.summary.agent_type)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                    messages,
                };
            }
            Err(err) => {
                return AppResponse::Error {
                    message: format!("get_session: {err}"),
                };
            }
        }
    }

    // Fallback: the phone sent a connection_id (live-only session).
    if let Some(st) = state.connection_manager.get_state(session_id).await {
        let guard = st.read().await;
        return AppResponse::SessionDetail {
            session_id: session_id.to_string(),
            title: String::new(),
            agent_type: serde_json::to_value(guard.agent_type)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default(),
            messages: Vec::new(),
        };
    }

    AppResponse::Error {
        message: format!("session not found: {session_id}"),
    }
}

/// Map a mobile `session_id` onto a live connection id. Accepts either a
/// conversation_id (i32 string) or a raw connection_id. Returns None if
/// neither resolves.
async fn resolve_connection_id(state: &Arc<AppState>, session_id: &str) -> Option<String> {
    if let Ok(cid) = session_id.parse::<i32>() {
        if let Some(conn_id) = state
            .connection_manager
            .find_connection_by_conversation_id(cid)
            .await
        {
            return Some(conn_id);
        }
    }
    // Direct connection_id fallback.
    if state.connection_manager.get_state(session_id).await.is_some() {
        return Some(session_id.to_string());
    }
    None
}

async fn handle_send_prompt(state: &Arc<AppState>, session_id: &str, text: String) -> AppResponse {
    let Some(conn_id) = resolve_connection_id(state, session_id).await else {
        return AppResponse::Error {
            message: format!("send_prompt: no live connection for session {session_id}"),
        };
    };

    // The folder/conversation link was established when this session was
    // first created; passing both as None re-uses the existing link.
    let blocks = vec![PromptInputBlock::Text { text }];
    let message_id = uuid::Uuid::new_v4().to_string();
    match state
        .connection_manager
        .send_prompt_linked(&state.db, &conn_id, blocks, None, None)
        .await
    {
        Ok(()) => AppResponse::PromptAck { message_id },
        Err(err) => AppResponse::Error {
            message: format!("send_prompt: {err}"),
        },
    }
}

async fn handle_stop_session(state: &Arc<AppState>, session_id: &str) -> AppResponse {
    let Some(conn_id) = resolve_connection_id(state, session_id).await else {
        return AppResponse::Error {
            message: format!("stop_session: no live connection for session {session_id}"),
        };
    };
    match state
        .connection_manager
        .cancel(&state.db.conn, &conn_id)
        .await
    {
        Ok(()) => AppResponse::StopAck {
            session_id: session_id.to_string(),
        },
        Err(err) => AppResponse::Error {
            message: format!("stop_session: {err}"),
        },
    }
}

/// Create a new live session. Spawns a fresh agent connection with an
/// auto-added folder at `cwd`. Returns the connection id as the mobile
/// session_id; the phone then calls `send_prompt` to bind a DB
/// conversation row on the first turn.
async fn handle_new_session(state: &Arc<AppState>, agent_type_raw: &str, cwd: &str) -> AppResponse {
    let agent_type: AgentType = match serde_json::from_value(serde_json::Value::String(
        agent_type_raw.to_string(),
    )) {
        Ok(at) => at,
        Err(err) => {
            return AppResponse::Error {
                message: format!("new_session: invalid agent_type '{agent_type_raw}': {err}"),
            };
        }
    };

    if cwd.trim().is_empty() {
        return AppResponse::Error {
            message: "new_session: cwd is required".into(),
        };
    }

    // Ensure the folder row exists so subsequent `send_prompt_linked`
    // calls from the phone can resolve the folder via
    // `find_connection_by_conversation_id`. `add_folder` is idempotent.
    if let Err(err) = folder_service::add_folder(&state.db.conn, cwd).await {
        return AppResponse::Error {
            message: format!("new_session: add_folder failed: {err}"),
        };
    }

    let runtime_env: BTreeMap<String, String> = BTreeMap::new();
    let owner_window_label = "mobile".to_string();
    // PathBuf canonicalization lives in `spawn_agent` via `working_dir`.
    let _ = PathBuf::from(cwd);

    match state
        .connection_manager
        .spawn_agent(
            agent_type,
            Some(cwd.to_string()),
            None,
            runtime_env,
            owner_window_label,
            state.emitter.clone(),
        )
        .await
    {
        Ok(conn_id) => AppResponse::NewSessionCreated {
            session_id: conn_id,
        },
        Err(err) => AppResponse::Error {
            message: format!("new_session: {err}"),
        },
    }
}

async fn handle_approval_response(
    state: &Arc<AppState>,
    request_id: &str,
    allow: bool,
) -> AppResponse {
    // Phone only speaks allow/deny — map onto canonical ACP option ids.
    // Agents differ on the exact id strings; callers already downstream of
    // the ACP manager will surface an error if the option isn't valid.
    let option_id = if allow { "allow_once" } else { "reject_once" };

    // Look up the connection by scanning live connections for the request.
    // `pending_permission.request_id` lives on SessionState; we iterate.
    let connections = state.connection_manager.list_connections().await;
    for info in connections {
        if let Some(st) = state.connection_manager.get_state(&info.id).await {
            let matches = {
                let guard = st.read().await;
                guard
                    .pending_permission
                    .as_ref()
                    .is_some_and(|p| p.request_id == request_id)
            };
            if matches {
                return match state
                    .connection_manager
                    .respond_permission(&info.id, request_id, option_id)
                    .await
                {
                    Ok(()) => AppResponse::PromptAck {
                        message_id: request_id.to_string(),
                    },
                    Err(err) => AppResponse::Error {
                        message: format!("approval_response: {err}"),
                    },
                };
            }
        }
    }

    AppResponse::Error {
        message: format!("approval_response: no pending permission for request {request_id}"),
    }
}

/// Flatten `MessageTurn` blocks into the phone's thin `MobileMessage`
/// wire format. Text / thinking blocks collapse into `text`; tool uses
/// and results become role="tool" rows so the phone can render them
/// with its tool-call component.
fn turns_to_mobile_messages(
    turns: &[crate::models::message::MessageTurn],
) -> Vec<MobileMessage> {
    use crate::models::message::{ContentBlock, TurnRole};
    let mut out = Vec::new();
    for turn in turns {
        let role = match turn.role {
            TurnRole::User => "user",
            TurnRole::Assistant => "assistant",
            TurnRole::System => "system",
        };
        let timestamp = turn.timestamp.timestamp();
        for (idx, block) in turn.blocks.iter().enumerate() {
            let (r, text) = match block {
                ContentBlock::Text { text } => (role, text.clone()),
                ContentBlock::Thinking { text } => (role, format!("[thinking] {text}")),
                ContentBlock::Image { .. } => (role, "[image]".to_string()),
                ContentBlock::ImageGeneration { revised_prompt, .. } => (
                    role,
                    format!(
                        "[image_generation] {}",
                        revised_prompt.clone().unwrap_or_default()
                    ),
                ),
                ContentBlock::ToolUse {
                    tool_name,
                    input_preview,
                    ..
                } => (
                    "tool",
                    format!(
                        "[call {tool_name}] {}",
                        input_preview.clone().unwrap_or_default()
                    ),
                ),
                ContentBlock::ToolResult {
                    output_preview,
                    is_error,
                    ..
                } => (
                    "tool",
                    format!(
                        "[{} result] {}",
                        if *is_error { "error" } else { "ok" },
                        output_preview.clone().unwrap_or_default()
                    ),
                ),
            };
            out.push(MobileMessage {
                id: format!("{}-{idx}", turn.id),
                role: r.into(),
                text,
                timestamp,
            });
        }
    }
    out
}

/// Map an [`AcpEvent`] into an [`AppResponse`] for the phone, if any.
/// Returns `None` for events the phone doesn't need to see — the full ACP
/// event vocabulary is much richer than the phone's wire format.
pub fn acp_event_to_app_response(event: &crate::acp::AcpEvent) -> Option<AppResponse> {
    use crate::acp::AcpEvent;
    match event {
        AcpEvent::ContentDelta { text } => Some(AppResponse::MessageDelta {
            session_id: String::new(), // filled in by the caller (connection owner)
            role: "assistant".into(),
            delta: text.clone(),
        }),
        AcpEvent::Thinking { text } => Some(AppResponse::MessageDelta {
            session_id: String::new(),
            role: "assistant".into(),
            delta: format!("[thinking] {text}"),
        }),
        AcpEvent::PermissionRequest {
            request_id,
            tool_call,
            ..
        } => {
            let summary = tool_call
                .get("title")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| "Permission required".to_string());
            Some(AppResponse::ApprovalRequired {
                request_id: request_id.clone(),
                session_id: String::new(),
                summary,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_roundtrip() {
        let req = AppRequest::Ping {
            payload: "hi".into(),
        };
        let res = dispatch_app_request(req);
        assert_eq!(
            res,
            AppResponse::Pong {
                payload: "hi".into()
            }
        );
    }

    #[test]
    fn request_serializes_as_tagged_union() {
        let req = AppRequest::Ping {
            payload: "x".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(
            json.contains("\"type\":\"ping\""),
            "expected snake_case tag, got: {json}"
        );
    }

    #[test]
    fn response_serializes_as_tagged_union() {
        let res = AppResponse::Pong {
            payload: "x".into(),
        };
        let json = serde_json::to_string(&res).unwrap();
        assert!(
            json.contains("\"type\":\"pong\""),
            "expected snake_case tag, got: {json}"
        );
    }

    #[test]
    fn device_register_roundtrip() {
        let req = AppRequest::DeviceRegister {
            nickname: "Lee's iPhone".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"device_register\""));
        let decoded: AppRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn device_register_rejects_on_default_dispatch() {
        let res = dispatch_app_request(AppRequest::DeviceRegister {
            nickname: "pixel".into(),
        });
        assert!(matches!(res, AppResponse::Error { .. }));
    }

    #[test]
    fn device_register_ack_serializes() {
        let res = AppResponse::DeviceRegisterAck {
            device_id: "uuid-x".into(),
            server_version: "0.12.1".into(),
        };
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"type\":\"device_register_ack\""));
        assert!(json.contains("\"device_id\":\"uuid-x\""));
    }

    #[test]
    fn list_sessions_roundtrip() {
        let req = AppRequest::ListSessions;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"list_sessions\""));
        let decoded: AppRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn get_session_roundtrip() {
        let req = AppRequest::GetSession {
            session_id: "abc".into(),
        };
        let decoded: AppRequest =
            serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn send_prompt_roundtrip() {
        let req = AppRequest::SendPrompt {
            session_id: "s1".into(),
            text: "hello".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"send_prompt\""));
        let decoded: AppRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn stop_session_roundtrip() {
        let req = AppRequest::StopSession {
            session_id: "s1".into(),
        };
        let decoded: AppRequest =
            serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn new_session_roundtrip() {
        let req = AppRequest::NewSession {
            agent_type: "claude_code".into(),
            cwd: "/tmp".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"new_session\""));
        let decoded: AppRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn approval_response_roundtrip() {
        let req = AppRequest::ApprovalResponse {
            request_id: "r1".into(),
            allow: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"approval_response\""));
        let decoded: AppRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn sessions_list_response_roundtrip() {
        let res = AppResponse::SessionsList {
            sessions: vec![MobileSessionSummary {
                id: "s1".into(),
                title: "T".into(),
                agent_type: "claude_code".into(),
                last_active_at: 12345,
                status: "in_progress".into(),
            }],
        };
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"type\":\"sessions_list\""));
        let decoded: AppResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, res);
    }

    #[test]
    fn message_delta_response_roundtrip() {
        let res = AppResponse::MessageDelta {
            session_id: "s".into(),
            role: "assistant".into(),
            delta: "hi".into(),
        };
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"type\":\"message_delta\""));
        let decoded: AppResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, res);
    }

    #[test]
    fn approval_required_roundtrip() {
        let res = AppResponse::ApprovalRequired {
            request_id: "req-1".into(),
            session_id: "s".into(),
            summary: "Run shell".into(),
        };
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"type\":\"approval_required\""));
        let decoded: AppResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, res);
    }

    #[test]
    fn unwired_variants_fall_through_to_error() {
        let req = AppRequest::ListSessions;
        let res = dispatch_app_request(req);
        assert!(matches!(res, AppResponse::Error { .. }));
    }
}
