//! Mobile pairing HTTP handlers.
//!
//! These endpoints drive the Settings → "Mobile Devices" UI in web mode.
//! They are only compiled when `relay-client` is on, matching the command
//! module's gating; the standalone `codeg-server` build still pre-dates
//! the mobile stack and therefore skips these routes entirely.

use std::sync::Arc;

use axum::{extract::Extension, Json};
use serde::Deserialize;

use crate::app_error::AppCommandError;
use crate::app_state::AppState;
use crate::commands::mobile_pairing as cmd;
use crate::relay::PairingOffer;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceIdParams {
    pub device_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameParams {
    pub device_id: String,
    pub nickname: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayOriginParams {
    pub origin: String,
}

pub async fn generate_pairing_offer(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<PairingOffer>, AppCommandError> {
    let offer =
        cmd::generate_pairing_offer_core(&state.pairing_coordinator, &state.db).await?;
    Ok(Json(offer))
}

pub async fn list_paired_devices(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Vec<cmd::PairedDeviceDto>>, AppCommandError> {
    let devices = cmd::list_paired_devices_core(&state.db).await?;
    Ok(Json(devices))
}

pub async fn revoke_paired_device(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<DeviceIdParams>,
) -> Result<Json<()>, AppCommandError> {
    cmd::revoke_paired_device_core(&state.db, params.device_id).await?;
    Ok(Json(()))
}

pub async fn rename_paired_device(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<RenameParams>,
) -> Result<Json<()>, AppCommandError> {
    cmd::rename_paired_device_core(&state.db, params.device_id, params.nickname).await?;
    Ok(Json(()))
}

pub async fn get_mobile_relay_origin(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<String>, AppCommandError> {
    let origin = cmd::get_mobile_relay_origin_core(&state.db.conn).await?;
    Ok(Json(origin))
}

pub async fn set_mobile_relay_origin(
    Extension(state): Extension<Arc<AppState>>,
    Json(params): Json<RelayOriginParams>,
) -> Result<Json<String>, AppCommandError> {
    let origin = cmd::set_mobile_relay_origin_core(&state.db.conn, params.origin).await?;
    Ok(Json(origin))
}
