use sea_orm::entity::prelude::*;

/// Row in `paired_devices`.
///
/// `device_id` is the stable identifier (uuid) assigned at pairing time and
/// surfaced to the UI; `client_pub_hex` is the phone's 32-byte Curve25519
/// public key hex-encoded; `relay_session_id` is the opaque session slot on
/// the relay this device is connected through. `revoked` flips to `1` when
/// the user removes the device from the desktop UI.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "paired_devices")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub device_id: String,
    pub nickname: String,
    #[sea_orm(unique)]
    pub client_pub_hex: String,
    pub relay_session_id: String,
    pub paired_at: i64,
    pub last_active_at: i64,
    pub revoked: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
