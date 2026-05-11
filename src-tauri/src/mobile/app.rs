//! Tauri Mobile (iOS/Android) entry point and Builder scaffold.
//!
//! This module is compiled only when the `mobile-runtime` feature is enabled.
//! It is a P0 skeleton: it wires up the required mobile plugins and leaves
//! the wider setup (config persistence, chat channels, ACP idle sweep, etc.)
//! for follow-up tasks in the mobile milestone.
//!
//! The shell starts an in-process Axum server (reusing `web::router`) so the
//! iOS webview can talk to the Rust backend the same way the desktop tauri
//! webview does in "Web Service" mode. The webview then loads the static
//! frontend bundle (mobile-tailored layout via Tailwind breakpoints).
//!
//! **Out of scope (handled by later sub-tasks):**
//! - Real web-server start address / port negotiation (P3 relay / P4 pairing)
//! - Bundled `tauri.ios.conf.json` with capabilities and permissions (P10)

use std::sync::Arc;

use sha2::Digest;
use tauri::Emitter;
use tauri_plugin_deep_link::DeepLinkExt;

use crate::app_state::{
    default_chat_channel_manager, default_connection_manager, default_pairing_coordinator,
    AppState,
};
use crate::db;
use crate::pet_state_mapper;
use crate::web::event_bridge::{EventEmitter, WebEventBroadcaster};
use crate::web::WebServerState;

/// Tauri mobile entry point. Wired from `lib.rs` via `#[cfg_attr(mobile, tauri::mobile_entry_point)]`.
///
/// Keep this function minimal: it initializes the Tauri Builder and registers
/// the mobile plugin set. Business features (pairing flow, relay client, etc.)
/// are added by the P2-P9 sub-tasks.
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_barcode_scanner::init())
        .plugin(tauri_plugin_biometric::init())
        .plugin(tauri_plugin_haptics::init())
        .plugin(
            tauri_plugin_stronghold::Builder::new(|password| {
                // Placeholder key derivation. The P6 credential-management
                // task replaces this with a biometric-unlocked passphrase
                // flow backed by the device keychain.
                sha2::Sha256::digest(password.as_bytes()).to_vec()
            })
            .build(),
        );

    builder
        .setup(|app| {
            use tauri::Manager;
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("app_data_dir unavailable: {e}"))?;
            let app_version = env!("CARGO_PKG_VERSION");

            let database =
                tauri::async_runtime::block_on(db::init_database(&app_data_dir, app_version))
                    .map_err(|e| e.to_string())?;

            let broadcaster = Arc::new(WebEventBroadcaster::new());
            let emitter = EventEmitter::WebOnly(broadcaster.clone());
            let pet_state = pet_state_mapper::new_pet_state_handle();

            let pairing_coordinator = default_pairing_coordinator(&app_data_dir);

            let state = Arc::new(AppState {
                db: db::AppDatabase {
                    conn: database.conn.clone(),
                },
                connection_manager: default_connection_manager(),
                event_broadcaster: broadcaster,
                emitter,
                data_dir: app_data_dir,
                web_server_state: WebServerState::new(),
                chat_channel_manager: default_chat_channel_manager(),
                pet_state,
                pairing_coordinator,
            });

            app.manage(database);
            app.manage(state);

            // Forward `codeg://` deep links received by the OS / `tauri-plugin-deep-link`
            // into a webview-side `"deep-link"` event. The TS side's
            // `subscribeDeepLinks()` hook listens for this and routes the
            // user into the matching page (`/m/session` or `/m/pair`).
            let app_handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    let url_str = url.to_string();
                    if let Err(e) = app_handle.emit("deep-link", &url_str) {
                        eprintln!(
                            "[mobile] failed to forward deep link {url_str} to webview: {e}"
                        );
                    }
                }
            });

            // Mobile does not use the desktop window setup, so create the root
            // webview explicitly instead of launching an empty iOS scene.
            if app.get_webview_window("main").is_none() {
                tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::App("index.html".into()),
                )
                .devtools(true)
                .build()
                .map_err(|e| format!("failed to create mobile webview: {e}"))?;
            }

            // Follow-up sub-tasks will:
            //   * start the in-process Axum server bound to 127.0.0.1
            //   * wire the relay outbound client
            //   * persist / load paired-devices list

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri mobile application");
}
