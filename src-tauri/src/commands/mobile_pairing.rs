//! Desktop-side mobile pairing commands.
//!
//! The pairing coordinator lives on `AppState` (relay-client gated). These
//! commands are only meaningful when that feature is on, so the whole
//! module is gated the same way. `codeg-server` currently builds without
//! `relay-client` and skips the module entirely.

#[cfg(feature = "tauri-runtime")]
use std::sync::Arc;

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
#[cfg(feature = "tauri-runtime")]
use tauri::State;

use crate::app_error::AppCommandError;
use crate::db::service::app_metadata_service;
use crate::db::AppDatabase;
use crate::relay::session_store;
use crate::relay::{PairingCoordinator, PairingOffer, DEFAULT_RELAY_ORIGIN};

/// App-metadata key for the user-chosen relay origin. Defaults to
/// [`DEFAULT_RELAY_ORIGIN`] when unset.
pub const MOBILE_RELAY_ORIGIN_KEY: &str = "mobile_relay_origin";

/// UI-facing clone of [`session_store::PairedDeviceInfo`]. Duplicated so the
/// TS side can consume a camel-friendly public type if needed later without
/// breaking the persistence struct.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PairedDeviceDto {
    pub device_id: String,
    pub nickname: String,
    pub client_pub_hex: String,
    pub relay_session_id: String,
    pub paired_at: i64,
    pub last_active_at: i64,
    pub revoked: bool,
}

impl From<session_store::PairedDeviceInfo> for PairedDeviceDto {
    fn from(v: session_store::PairedDeviceInfo) -> Self {
        Self {
            device_id: v.device_id,
            nickname: v.nickname,
            client_pub_hex: v.client_pub_hex,
            relay_session_id: v.relay_session_id,
            paired_at: v.paired_at,
            last_active_at: v.last_active_at,
            revoked: v.revoked,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared core functions (used by Tauri commands and web handlers)
// ---------------------------------------------------------------------------

/// Read the persisted relay origin, falling back to the default if nothing
/// has been configured yet. A blank string also falls back.
pub async fn get_mobile_relay_origin_core(
    conn: &DatabaseConnection,
) -> Result<String, AppCommandError> {
    let raw = app_metadata_service::get_value(conn, MOBILE_RELAY_ORIGIN_KEY)
        .await
        .map_err(AppCommandError::from)?;
    let value = raw.map(|v| v.trim().to_string()).filter(|s| !s.is_empty());
    Ok(value.unwrap_or_else(|| DEFAULT_RELAY_ORIGIN.to_string()))
}

/// Overwrite the persisted relay origin. An empty string resets to default.
pub async fn set_mobile_relay_origin_core(
    conn: &DatabaseConnection,
    origin: String,
) -> Result<String, AppCommandError> {
    let trimmed = origin.trim().to_string();
    if trimmed.is_empty() {
        app_metadata_service::upsert_value(conn, MOBILE_RELAY_ORIGIN_KEY, "")
            .await
            .map_err(AppCommandError::from)?;
        return Ok(DEFAULT_RELAY_ORIGIN.to_string());
    }
    app_metadata_service::upsert_value(conn, MOBILE_RELAY_ORIGIN_KEY, &trimmed)
        .await
        .map_err(AppCommandError::from)?;
    Ok(trimmed)
}

pub async fn generate_pairing_offer_core(
    coordinator: &PairingCoordinator,
    db: &AppDatabase,
) -> Result<PairingOffer, AppCommandError> {
    let relay_origin = get_mobile_relay_origin_core(&db.conn).await?;
    let offer = coordinator
        .create_offer(&relay_origin, db.conn.clone())
        .await;
    Ok(offer)
}

pub async fn list_paired_devices_core(
    db: &AppDatabase,
) -> Result<Vec<PairedDeviceDto>, AppCommandError> {
    let rows = session_store::list_paired_devices(&db.conn)
        .await
        .map_err(AppCommandError::from)?;
    Ok(rows.into_iter().map(PairedDeviceDto::from).collect())
}

pub async fn revoke_paired_device_core(
    db: &AppDatabase,
    device_id: String,
) -> Result<(), AppCommandError> {
    if device_id.trim().is_empty() {
        return Err(AppCommandError::invalid_input("device_id cannot be empty"));
    }
    session_store::revoke_paired_device(&db.conn, &device_id)
        .await
        .map_err(AppCommandError::from)?;
    Ok(())
}

pub async fn rename_paired_device_core(
    db: &AppDatabase,
    device_id: String,
    nickname: String,
) -> Result<(), AppCommandError> {
    let device_id = device_id.trim().to_string();
    let nickname = nickname.trim().to_string();
    if device_id.is_empty() {
        return Err(AppCommandError::invalid_input("device_id cannot be empty"));
    }
    if nickname.is_empty() {
        return Err(AppCommandError::invalid_input("nickname cannot be empty"));
    }
    session_store::rename_paired_device(&db.conn, &device_id, &nickname)
        .await
        .map_err(AppCommandError::from)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri command wrappers (desktop / mobile-runtime only)
// ---------------------------------------------------------------------------

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mobile_generate_pairing_offer(
    coordinator: State<'_, Arc<PairingCoordinator>>,
    db: State<'_, AppDatabase>,
) -> Result<PairingOffer, AppCommandError> {
    generate_pairing_offer_core(&coordinator, &db).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mobile_list_paired_devices(
    db: State<'_, AppDatabase>,
) -> Result<Vec<PairedDeviceDto>, AppCommandError> {
    list_paired_devices_core(&db).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mobile_revoke_paired_device(
    db: State<'_, AppDatabase>,
    device_id: String,
) -> Result<(), AppCommandError> {
    revoke_paired_device_core(&db, device_id).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mobile_rename_paired_device(
    db: State<'_, AppDatabase>,
    device_id: String,
    nickname: String,
) -> Result<(), AppCommandError> {
    rename_paired_device_core(&db, device_id, nickname).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mobile_get_relay_origin(
    db: State<'_, AppDatabase>,
) -> Result<String, AppCommandError> {
    get_mobile_relay_origin_core(&db.conn).await
}

#[cfg(feature = "tauri-runtime")]
#[tauri::command]
pub async fn mobile_set_relay_origin(
    db: State<'_, AppDatabase>,
    origin: String,
) -> Result<String, AppCommandError> {
    set_mobile_relay_origin_core(&db.conn, origin).await
}
