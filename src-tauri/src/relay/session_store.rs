//! Persistence for paired mobile devices.
//!
//! Each phone that finishes the pairing handshake earns a row in
//! `paired_devices`. The row stores the phone's Curve25519 public key so
//! the daemon can verify incoming ciphertext originated from a device the
//! user actually approved, plus a relay session id for the current
//! connection.
//!
//! `revoked` flips to `1` when the user removes the device from the
//! desktop "Mobile Devices" page. The client-side socket should be closed
//! immediately by the caller; subsequent reconnect attempts find the row
//! revoked and are rejected.

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};

use crate::db::entities::paired_device;
use crate::db::error::DbError;

/// Cross-layer struct returned to API callers. Matches the Rust entity
/// one-for-one but keeps the module free from SeaORM types for ease of
/// serialization across the web bridge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PairedDeviceInfo {
    pub device_id: String,
    pub nickname: String,
    pub client_pub_hex: String,
    pub relay_session_id: String,
    pub paired_at: i64,
    pub last_active_at: i64,
    pub revoked: bool,
}

impl From<paired_device::Model> for PairedDeviceInfo {
    fn from(m: paired_device::Model) -> Self {
        Self {
            device_id: m.device_id,
            nickname: m.nickname,
            client_pub_hex: m.client_pub_hex,
            relay_session_id: m.relay_session_id,
            paired_at: m.paired_at,
            last_active_at: m.last_active_at,
            revoked: m.revoked != 0,
        }
    }
}

/// Input to [`create_paired_device`]. Kept as a struct so future fields
/// (icon, platform, app version) slot in without rewriting call sites.
#[derive(Debug, Clone)]
pub struct NewPairedDevice<'a> {
    pub device_id: &'a str,
    pub nickname: &'a str,
    pub client_pub_hex: &'a str,
    pub relay_session_id: &'a str,
}

/// Persist a freshly paired device. `paired_at` and `last_active_at` are
/// set to the current wall clock in unix seconds.
pub async fn create_paired_device(
    conn: &DatabaseConnection,
    input: NewPairedDevice<'_>,
) -> Result<PairedDeviceInfo, DbError> {
    let now = Utc::now().timestamp();
    let active = paired_device::ActiveModel {
        device_id: Set(input.device_id.to_string()),
        nickname: Set(input.nickname.to_string()),
        client_pub_hex: Set(input.client_pub_hex.to_string()),
        relay_session_id: Set(input.relay_session_id.to_string()),
        paired_at: Set(now),
        last_active_at: Set(now),
        revoked: Set(0),
    };
    let model = active.insert(conn).await?;
    Ok(model.into())
}

/// Return every row sorted by `last_active_at DESC` (most recently used
/// device first). Revoked rows are included so the UI can show a history.
pub async fn list_paired_devices(
    conn: &DatabaseConnection,
) -> Result<Vec<PairedDeviceInfo>, DbError> {
    let rows = paired_device::Entity::find()
        .order_by_desc(paired_device::Column::LastActiveAt)
        .all(conn)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Look up a single device by its public key (hex). Returns `Ok(None)` if
/// the row does not exist — the crypto layer uses this to short-circuit
/// incoming connections that no longer match any stored pairing.
pub async fn load_paired_device(
    conn: &DatabaseConnection,
    client_pub_hex: &str,
) -> Result<Option<PairedDeviceInfo>, DbError> {
    let row = paired_device::Entity::find()
        .filter(paired_device::Column::ClientPubHex.eq(client_pub_hex))
        .one(conn)
        .await?;
    Ok(row.map(Into::into))
}

/// Mark a device as revoked. Further incoming traffic is expected to be
/// dropped by the caller; any live WebSocket for this device should be
/// closed separately.
pub async fn revoke_paired_device(
    conn: &DatabaseConnection,
    device_id: &str,
) -> Result<(), DbError> {
    let Some(row) = paired_device::Entity::find_by_id(device_id.to_string())
        .one(conn)
        .await?
    else {
        return Ok(());
    };
    let mut active: paired_device::ActiveModel = row.into();
    active.revoked = Set(1);
    active.update(conn).await?;
    Ok(())
}

/// Bump `last_active_at` to "now". Called whenever a device produces
/// traffic, so the UI can sort recently used devices to the top.
pub async fn touch_paired_device(
    conn: &DatabaseConnection,
    device_id: &str,
) -> Result<(), DbError> {
    let Some(row) = paired_device::Entity::find_by_id(device_id.to_string())
        .one(conn)
        .await?
    else {
        return Ok(());
    };
    let mut active: paired_device::ActiveModel = row.into();
    active.last_active_at = Set(Utc::now().timestamp());
    active.update(conn).await?;
    Ok(())
}

/// Rename a paired device. No-op if the row does not exist.
pub async fn rename_paired_device(
    conn: &DatabaseConnection,
    device_id: &str,
    nickname: &str,
) -> Result<(), DbError> {
    let Some(row) = paired_device::Entity::find_by_id(device_id.to_string())
        .one(conn)
        .await?
    else {
        return Ok(());
    };
    let mut active: paired_device::ActiveModel = row.into();
    active.nickname = Set(nickname.to_string());
    active.update(conn).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::fresh_in_memory_db;

    #[tokio::test]
    async fn create_and_list_roundtrip() {
        let db = fresh_in_memory_db().await;
        let info = create_paired_device(
            &db.conn,
            NewPairedDevice {
                device_id: "dev-1",
                nickname: "iPhone",
                client_pub_hex: "aa",
                relay_session_id: "sess-1",
            },
        )
        .await
        .unwrap();
        assert_eq!(info.device_id, "dev-1");
        assert_eq!(info.nickname, "iPhone");
        assert!(!info.revoked);

        let rows = list_paired_devices(&db.conn).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].device_id, "dev-1");
    }

    #[tokio::test]
    async fn load_by_pub_key() {
        let db = fresh_in_memory_db().await;
        create_paired_device(
            &db.conn,
            NewPairedDevice {
                device_id: "dev-2",
                nickname: "iPad",
                client_pub_hex: "bb",
                relay_session_id: "sess-2",
            },
        )
        .await
        .unwrap();
        let hit = load_paired_device(&db.conn, "bb").await.unwrap();
        assert!(hit.is_some());
        let miss = load_paired_device(&db.conn, "cc").await.unwrap();
        assert!(miss.is_none());
    }

    #[tokio::test]
    async fn revoke_flips_flag() {
        let db = fresh_in_memory_db().await;
        create_paired_device(
            &db.conn,
            NewPairedDevice {
                device_id: "dev-3",
                nickname: "Pixel",
                client_pub_hex: "cc",
                relay_session_id: "sess-3",
            },
        )
        .await
        .unwrap();
        revoke_paired_device(&db.conn, "dev-3").await.unwrap();
        let row = load_paired_device(&db.conn, "cc").await.unwrap().unwrap();
        assert!(row.revoked);
    }

    #[tokio::test]
    async fn touch_updates_last_active() {
        let db = fresh_in_memory_db().await;
        create_paired_device(
            &db.conn,
            NewPairedDevice {
                device_id: "dev-4",
                nickname: "iPhone",
                client_pub_hex: "dd",
                relay_session_id: "sess-4",
            },
        )
        .await
        .unwrap();
        // Small sleep so the timestamp tick is observable; 1 second matches
        // chrono's unix seconds granularity.
        let before = load_paired_device(&db.conn, "dd").await.unwrap().unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        touch_paired_device(&db.conn, "dev-4").await.unwrap();
        let after = load_paired_device(&db.conn, "dd").await.unwrap().unwrap();
        assert!(after.last_active_at >= before.last_active_at);
    }
}
