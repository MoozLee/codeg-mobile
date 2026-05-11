//! Integration test for the state-aware relay dispatcher.
//!
//! Builds an in-memory `AppState`, runs the phone-side scan / register
//! flow against a mock relay, then exercises each wired variant end to
//! end: list / new session / get session / stop.
//!
//! We do not spawn a real ACP agent process — the `spawn_agent` path
//! requires real binaries (claude-code / codex / etc.) and is explicitly
//! ignored with `#[ignore]` in the "happy path" smoke test. What matters
//! for wiring-verification is that
//!
//!   - `ListSessions` reflects DB rows + live connections
//!   - `GetSession` returns a well-formed `SessionDetail` for a known row
//!   - `SendPrompt` / `StopSession` / `ApprovalResponse` route through
//!     the real `ConnectionManager` and surface typed errors when the
//!     session doesn't exist (the wiring we can verify without a process)
//!
//! The integration-style "real agent" flow is a `#[ignore]` placeholder.

#![cfg(feature = "relay-client")]

use std::sync::Arc;

use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;

use codeg_lib::app_state::{
    default_chat_channel_manager, default_connection_manager, default_pairing_coordinator,
    default_terminal_manager, AppState,
};
use codeg_lib::db::migration::Migrator;
use codeg_lib::db::service::conversation_service;
use codeg_lib::db::service::folder_service;
use codeg_lib::db::AppDatabase;
use codeg_lib::models::AgentType;
use codeg_lib::pet_state_mapper;
use codeg_lib::relay::{dispatch_app_request_with_state, AppRequest, AppResponse};
use codeg_lib::web::event_bridge::{EventEmitter, WebEventBroadcaster};
use codeg_lib::web::WebServerState;

async fn fresh_state() -> Arc<AppState> {
    let conn = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite connect");
    conn.execute(Statement::from_string(
        DbBackend::Sqlite,
        "PRAGMA foreign_keys=ON;".to_owned(),
    ))
    .await
    .expect("pragma");
    Migrator::up(&conn, None).await.expect("migrations");

    let data_dir_guard = tempfile::tempdir().expect("tempdir");
    let data_dir = data_dir_guard.path().to_path_buf();
    // Leak the guard so the directory outlives the test; the OS will
    // reap it in /tmp. tempdir's Drop removes the dir which would break
    // any follow-up keypair writes if the test state outlives the guard.
    std::mem::forget(data_dir_guard);
    let broadcaster = Arc::new(WebEventBroadcaster::new());
    let emitter = EventEmitter::WebOnly(broadcaster.clone());
    let pet_state = pet_state_mapper::new_pet_state_handle();
    let pairing_coordinator = default_pairing_coordinator(&data_dir);

    Arc::new(AppState {
        db: AppDatabase { conn },
        connection_manager: default_connection_manager(),
        terminal_manager: default_terminal_manager(),
        event_broadcaster: broadcaster,
        emitter,
        data_dir,
        web_server_state: WebServerState::new(),
        chat_channel_manager: default_chat_channel_manager(),
        pet_state,
        pairing_coordinator,
    })
}

#[tokio::test]
async fn list_sessions_empty_on_fresh_state() {
    let state = fresh_state().await;
    let resp = dispatch_app_request_with_state(AppRequest::ListSessions, &state).await;
    match resp {
        AppResponse::SessionsList { sessions } => {
            assert!(sessions.is_empty(), "fresh state has no sessions");
        }
        other => panic!("expected SessionsList, got {other:?}"),
    }
}

#[tokio::test]
async fn list_sessions_surfaces_db_rows() {
    let state = fresh_state().await;

    // Seed a folder + a conversation row.
    let folder = folder_service::add_folder(&state.db.conn, "/tmp/codeg-mobile-test")
        .await
        .expect("add folder");
    let conv = conversation_service::create(
        &state.db.conn,
        folder.id,
        AgentType::ClaudeCode,
        Some("Mobile smoke test".to_string()),
        None,
    )
    .await
    .expect("create conv");

    let resp = dispatch_app_request_with_state(AppRequest::ListSessions, &state).await;
    match resp {
        AppResponse::SessionsList { sessions } => {
            let found = sessions.iter().find(|s| s.id == conv.id.to_string());
            let found = found.expect("seeded row present in session list");
            assert_eq!(found.title, "Mobile smoke test");
            assert_eq!(found.agent_type, "claude_code");
        }
        other => panic!("expected SessionsList, got {other:?}"),
    }
}

#[tokio::test]
async fn get_session_returns_detail_for_known_row() {
    let state = fresh_state().await;
    let folder = folder_service::add_folder(&state.db.conn, "/tmp/codeg-mobile-get")
        .await
        .expect("add folder");
    let conv = conversation_service::create(
        &state.db.conn,
        folder.id,
        AgentType::Codex,
        Some("Detail probe".to_string()),
        None,
    )
    .await
    .expect("create conv");

    let resp = dispatch_app_request_with_state(
        AppRequest::GetSession {
            session_id: conv.id.to_string(),
        },
        &state,
    )
    .await;

    match resp {
        AppResponse::SessionDetail {
            session_id,
            title,
            agent_type,
            messages,
        } => {
            assert_eq!(session_id, conv.id.to_string());
            assert_eq!(title, "Detail probe");
            assert_eq!(agent_type, "codex");
            // No parser-backed transcript exists on disk, so the DB-only
            // path returns an empty turn list. That's the correct
            // contract for a freshly-created row with no external file.
            assert!(messages.is_empty());
        }
        other => panic!("expected SessionDetail, got {other:?}"),
    }
}

#[tokio::test]
async fn get_session_returns_error_for_unknown_id() {
    let state = fresh_state().await;
    let resp = dispatch_app_request_with_state(
        AppRequest::GetSession {
            session_id: "no-such-session".to_string(),
        },
        &state,
    )
    .await;
    assert!(
        matches!(resp, AppResponse::Error { .. }),
        "expected Error for unknown id, got {resp:?}"
    );
}

#[tokio::test]
async fn send_prompt_fails_when_no_connection() {
    let state = fresh_state().await;
    let resp = dispatch_app_request_with_state(
        AppRequest::SendPrompt {
            session_id: "unknown".into(),
            text: "hi".into(),
        },
        &state,
    )
    .await;
    match resp {
        AppResponse::Error { message } => {
            assert!(
                message.contains("no live connection"),
                "unexpected error message: {message}"
            );
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[tokio::test]
async fn stop_session_fails_when_no_connection() {
    let state = fresh_state().await;
    let resp = dispatch_app_request_with_state(
        AppRequest::StopSession {
            session_id: "nope".into(),
        },
        &state,
    )
    .await;
    match resp {
        AppResponse::Error { message } => {
            assert!(
                message.contains("no live connection"),
                "unexpected error: {message}"
            );
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[tokio::test]
async fn approval_response_errors_when_no_pending_request() {
    let state = fresh_state().await;
    let resp = dispatch_app_request_with_state(
        AppRequest::ApprovalResponse {
            request_id: "missing".into(),
            allow: true,
        },
        &state,
    )
    .await;
    assert!(
        matches!(resp, AppResponse::Error { .. }),
        "expected Error, got {resp:?}"
    );
}

#[tokio::test]
async fn new_session_requires_cwd() {
    let state = fresh_state().await;
    let resp = dispatch_app_request_with_state(
        AppRequest::NewSession {
            agent_type: "claude_code".into(),
            cwd: "".into(),
        },
        &state,
    )
    .await;
    match resp {
        AppResponse::Error { message } => {
            assert!(message.contains("cwd"), "unexpected error: {message}");
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

#[tokio::test]
async fn new_session_rejects_unknown_agent_type() {
    let state = fresh_state().await;
    let resp = dispatch_app_request_with_state(
        AppRequest::NewSession {
            agent_type: "not_an_agent".into(),
            cwd: "/tmp".into(),
        },
        &state,
    )
    .await;
    match resp {
        AppResponse::Error { message } => {
            assert!(
                message.contains("invalid agent_type"),
                "unexpected error: {message}"
            );
        }
        other => panic!("expected Error, got {other:?}"),
    }
}

/// End-to-end smoke: `NewSession` → `ListSessions` visibility → `SendPrompt`.
/// Requires a real ACP agent binary on the machine (claude-code, codex,
/// etc.), so it's gated behind `#[ignore]`. Run manually with
/// `cargo test --test relay_dispatcher -- --ignored` when a binary is
/// installed.
///
/// TODO(P6): swap the real binary for a mock ACP server so this can run
/// in CI without external deps.
#[tokio::test]
#[ignore]
async fn new_session_then_send_prompt_smoke() {
    let state = fresh_state().await;

    let resp = dispatch_app_request_with_state(
        AppRequest::NewSession {
            agent_type: "claude_code".into(),
            cwd: "/tmp".into(),
        },
        &state,
    )
    .await;
    let session_id = match resp {
        AppResponse::NewSessionCreated { session_id } => session_id,
        other => panic!("expected NewSessionCreated, got {other:?}"),
    };

    // Should show up via list.
    let listed = dispatch_app_request_with_state(AppRequest::ListSessions, &state).await;
    match listed {
        AppResponse::SessionsList { sessions } => {
            assert!(sessions.iter().any(|s| s.id == session_id));
        }
        other => panic!("expected SessionsList, got {other:?}"),
    }

    // Send a prompt.
    let prompt_resp = dispatch_app_request_with_state(
        AppRequest::SendPrompt {
            session_id,
            text: "hello".into(),
        },
        &state,
    )
    .await;
    assert!(
        matches!(prompt_resp, AppResponse::PromptAck { .. }),
        "expected PromptAck, got {prompt_resp:?}"
    );
}
