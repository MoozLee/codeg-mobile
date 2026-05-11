//! Outbound relay transport: WebSocket client + encryption bridge.
//!
//! The daemon connects out to a Cloudflare Worker relay as the `daemon`
//! role of a session. Phones connect in as the `client` role; the relay
//! forwards opaque text frames between them. Every application-layer
//! message is sealed by [`crate::crypto`] before it hits the wire, so the
//! relay itself only ever sees ciphertext.
//!
//! Layout:
//!
//! - [`client`] — long-running outbound WebSocket connection, handshake,
//!   exponential backoff reconnect.
//! - [`dispatcher`] — routes decrypted application messages through the
//!   in-process handler table. P3 has `ping/pong`; P4 adds the pairing
//!   variants (`device_register` / `device_register_ack`). Later stages
//!   add session / prompt / approval variants.
//! - [`pairing`] — short-lived pairing coordinator that spawns a relay
//!   client per new pairing session and writes the resulting
//!   `paired_devices` row.
//! - [`session_store`] — persistence for paired phones (device id,
//!   public key, relay session).

#![cfg(feature = "relay-client")]

pub mod client;
pub mod dispatcher;
pub mod pairing;
pub mod session_store;

pub use client::{RelayClient, RelayClientError, RelayClientOptions};
pub use dispatcher::{
    acp_event_to_app_response, dispatch_app_request, dispatch_app_request_with_state, AppRequest,
    AppResponse, MobileMessage, MobileSessionSummary,
};
pub use pairing::{
    PairingCoordinator, PairingError, PairingOffer, DEFAULT_PAIRING_TTL_SECS,
    DEFAULT_RELAY_ORIGIN,
};
pub use session_store::{
    create_paired_device, list_paired_devices, load_paired_device, rename_paired_device,
    revoke_paired_device, touch_paired_device, PairedDeviceInfo,
};
