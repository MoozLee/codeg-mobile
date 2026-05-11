//! Pairing coordinator.
//!
//! The desktop Settings → "Mobile Devices" page calls
//! [`PairingCoordinator::create_offer`] to mint a fresh pairing URL. That
//! also spawns a background task which:
//!
//! 1. Connects outbound to `wss://<relay>/session/<id>/daemon`.
//! 2. Runs the crypto handshake (cleartext `e2ee_hello` -> `e2ee_ready`).
//! 3. Waits for the first encrypted `device_register` frame from the phone.
//! 4. Persists a `paired_devices` row with the phone's pub key + nickname.
//! 5. Sends back `device_register_ack` with the newly-assigned `device_id`.
//! 6. Closes the socket and removes itself from the active map.
//!
//! Expiry (default 10 minutes) and explicit cancellation are wired through
//! a `tokio::sync::watch<bool>` shutdown, matching the pattern used by
//! [`super::client::RelayClient`].

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex};
use tokio_tungstenite::tungstenite;

use crate::crypto::frame::{decrypt_frame, derive_shared, encrypt_frame};
use crate::crypto::handshake::{HelloFrame, ReadyFrame};
use crate::crypto::keypair::LongTermKeyPair;
use crate::crypto::pairing::build_pairing_url;

use super::dispatcher::{AppRequest, AppResponse};
use super::session_store;

/// Default lifetime of a pairing offer before the background task gives up.
pub const DEFAULT_PAIRING_TTL_SECS: u64 = 10 * 60;

/// Default relay origin. Replaced by user-configured value via AppConfig.
/// `wss://codeg-relay.workers.dev` is a placeholder; operators are expected
/// to deploy their own Worker (see `relay/README.md`).
pub const DEFAULT_RELAY_ORIGIN: &str = "wss://codeg-relay.workers.dev";

#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error("websocket: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("crypto: {0}")]
    Crypto(#[from] crate::crypto::CryptoError),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("database: {0}")]
    Database(#[from] crate::db::error::DbError),

    #[error("handshake: {0}")]
    Handshake(String),

    #[error("expected device_register, got: {0}")]
    UnexpectedRequest(&'static str),

    #[error("expired before the phone completed pairing")]
    Expired,

    #[error("pairing session was cancelled")]
    Cancelled,
}

/// Offer returned to the UI. `pairing_url` is what the QR code encodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingOffer {
    pub session_id: String,
    pub pairing_url: String,
    pub relay_origin: String,
    /// Unix seconds when the daemon gives up waiting for the phone.
    pub expires_at: i64,
}

/// Shared state for the pairing subsystem. Lives on `AppState` and is
/// initialized in `AppState::new` alongside the long-term daemon keypair.
pub struct PairingCoordinator {
    keypair: Arc<LongTermKeyPair>,
    active: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
}

impl PairingCoordinator {
    pub fn new(keypair: Arc<LongTermKeyPair>) -> Self {
        Self {
            keypair,
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Access the long-term daemon keypair. Tests and the mobile-side
    /// `scan_pairing_url` reuse this to verify round-trips.
    pub fn keypair(&self) -> &Arc<LongTermKeyPair> {
        &self.keypair
    }

    /// Generate a pairing offer and spawn a background task that waits for
    /// the phone to connect. The returned [`PairingOffer`] is the piece the
    /// UI renders as a QR code.
    pub async fn create_offer(
        &self,
        relay_origin: &str,
        db_conn: DatabaseConnection,
    ) -> PairingOffer {
        self.create_offer_with_ttl(
            relay_origin,
            db_conn,
            Duration::from_secs(DEFAULT_PAIRING_TTL_SECS),
        )
        .await
    }

    /// Test-friendly variant of [`Self::create_offer`] that lets callers
    /// override the session TTL.
    pub async fn create_offer_with_ttl(
        &self,
        relay_origin: &str,
        db_conn: DatabaseConnection,
        ttl: Duration,
    ) -> PairingOffer {
        let session_id = uuid::Uuid::new_v4().to_string();
        // The pairing URL embeds the host only (PRD:
        // `codeg-pair://<relay-host>/<session-id>#<pubkey>`), without the
        // scheme. The phone later reconstructs `wss://` (or `ws://` for
        // loopback) before opening the WebSocket.
        let host_for_pairing = strip_ws_scheme(relay_origin);
        let pairing_url =
            build_pairing_url(host_for_pairing, &session_id, self.keypair.public());
        let expires_at = chrono::Utc::now().timestamp() + ttl.as_secs() as i64;

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        {
            let mut active = self.active.lock().await;
            active.insert(session_id.clone(), shutdown_tx);
        }

        let keypair = Arc::clone(&self.keypair);
        let relay_origin_owned = relay_origin.to_string();
        let session_id_for_task = session_id.clone();
        let active_clone = Arc::clone(&self.active);

        tokio::spawn(async move {
            let result = run_pairing_attempt(
                &relay_origin_owned,
                &session_id_for_task,
                keypair,
                db_conn,
                shutdown_rx,
                ttl,
            )
            .await;
            match &result {
                Ok(device_id) => {
                    eprintln!(
                        "[Pairing] session {} completed: device_id={}",
                        session_id_for_task, device_id
                    );
                }
                Err(err) => {
                    eprintln!(
                        "[Pairing] session {} ended: {err}",
                        session_id_for_task
                    );
                }
            }
            // Always drop ourselves from the active map so later calls to
            // `cancel()` do not race against a dead sender.
            let mut active = active_clone.lock().await;
            active.remove(&session_id_for_task);
        });

        PairingOffer {
            session_id,
            pairing_url,
            relay_origin: relay_origin.to_string(),
            expires_at,
        }
    }

    /// Cancel an in-flight pairing session. No-op if the session is already
    /// gone (e.g. timed out or completed).
    pub async fn cancel(&self, session_id: &str) {
        let sender = {
            let mut active = self.active.lock().await;
            active.remove(session_id)
        };
        if let Some(tx) = sender {
            let _ = tx.send(true);
        }
    }

    /// Count of in-flight pairing sessions. Exposed for tests and debug UI.
    pub async fn active_count(&self) -> usize {
        self.active.lock().await.len()
    }
}

/// Run a single pairing attempt. Returns the newly assigned `device_id`
/// on success.
async fn run_pairing_attempt(
    relay_origin: &str,
    session_id: &str,
    keypair: Arc<LongTermKeyPair>,
    db_conn: DatabaseConnection,
    mut shutdown: watch::Receiver<bool>,
    ttl: Duration,
) -> Result<String, PairingError> {
    let url = format!(
        "{origin}/session/{session}/daemon",
        origin = relay_origin.trim_end_matches('/'),
        session = session_id,
    );
    eprintln!("[Pairing] waiting for phone on {url}");

    let expiry = tokio::time::sleep(ttl);
    tokio::pin!(expiry);

    let connect_fut = tokio_tungstenite::connect_async(&url);
    let (ws_stream, _) = tokio::select! {
        r = connect_fut => r?,
        _ = &mut expiry => return Err(PairingError::Expired),
        _ = shutdown.changed() => return Err(PairingError::Cancelled),
    };
    let (mut write, mut read) = ws_stream.split();

    // 1. Receive cleartext hello.
    let hello_text = loop {
        tokio::select! {
            next = read.next() => match next {
                Some(Ok(tungstenite::Message::Text(s))) => break s.to_string(),
                Some(Ok(tungstenite::Message::Ping(data))) => {
                    let _ = write.send(tungstenite::Message::Pong(data)).await;
                    continue;
                }
                Some(Ok(tungstenite::Message::Pong(_))) => continue,
                Some(Ok(tungstenite::Message::Close(_))) | None => {
                    return Err(PairingError::Handshake(
                        "closed before hello".into(),
                    ));
                }
                Some(Ok(other)) => {
                    return Err(PairingError::Handshake(format!(
                        "expected text hello, got: {other:?}"
                    )));
                }
                Some(Err(e)) => return Err(PairingError::WebSocket(e)),
            },
            _ = &mut expiry => return Err(PairingError::Expired),
            _ = shutdown.changed() => return Err(PairingError::Cancelled),
        }
    };
    let hello = HelloFrame::decode(&hello_text)?;
    if hello.session_id != session_id {
        return Err(PairingError::Handshake(format!(
            "session id mismatch: expected {session_id} got {}",
            hello.session_id
        )));
    }

    // 2. Derive shared key.
    let client_pub_bytes = decode_32_hex(&hello.client_pub_hex)
        .map_err(|e| PairingError::Handshake(format!("client pub: {e}")))?;
    let shared = derive_shared(&client_pub_bytes, keypair.secret());

    // 3. Send cleartext ready (carries the long-term daemon pubkey).
    let ready = ReadyFrame::new(keypair.public());
    let ready_raw = ready.encode()?;
    write
        .send(tungstenite::Message::Text(ready_raw.into()))
        .await?;

    // 4. Wait for the first encrypted frame → expect `device_register`.
    let register_frame = loop {
        tokio::select! {
            next = read.next() => match next {
                Some(Ok(tungstenite::Message::Text(s))) => break s.to_string(),
                Some(Ok(tungstenite::Message::Ping(data))) => {
                    let _ = write.send(tungstenite::Message::Pong(data)).await;
                    continue;
                }
                Some(Ok(tungstenite::Message::Pong(_))) => continue,
                Some(Ok(tungstenite::Message::Close(_))) | None => {
                    return Err(PairingError::Handshake(
                        "closed before device_register".into(),
                    ));
                }
                Some(Ok(other)) => {
                    return Err(PairingError::Handshake(format!(
                        "expected text frame, got: {other:?}"
                    )));
                }
                Some(Err(e)) => return Err(PairingError::WebSocket(e)),
            },
            _ = &mut expiry => return Err(PairingError::Expired),
            _ = shutdown.changed() => return Err(PairingError::Cancelled),
        }
    };
    let register_plain = decrypt_frame(&shared, &register_frame)?;
    let req: AppRequest = serde_json::from_slice(&register_plain)?;
    let nickname = match req {
        AppRequest::DeviceRegister { nickname } => nickname,
        AppRequest::Ping { .. } => return Err(PairingError::UnexpectedRequest("ping")),
        // Any other request variant is illegal during pairing: the phone
        // is expected to send `device_register` first. Classifying them
        // all as "not device_register" keeps the error surface narrow.
        _ => return Err(PairingError::UnexpectedRequest("non_register")),
    };

    // 5. Persist the paired device row.
    let device_id = uuid::Uuid::new_v4().to_string();
    let client_pub_hex = hello.client_pub_hex.clone();
    session_store::create_paired_device(
        &db_conn,
        session_store::NewPairedDevice {
            device_id: &device_id,
            nickname: &nickname,
            client_pub_hex: &client_pub_hex,
            relay_session_id: session_id,
        },
    )
    .await?;

    // 6. Reply with ack.
    let ack = AppResponse::DeviceRegisterAck {
        device_id: device_id.clone(),
        server_version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let ack_bytes = serde_json::to_vec(&ack)?;
    let ack_frame = encrypt_frame(&shared, &ack_bytes, None);
    write
        .send(tungstenite::Message::Text(ack_frame.into()))
        .await?;

    // 7. Close politely. Dropping the write half is enough but we send a
    //    normal-closure frame first so mocks can detect EOS deterministically.
    let _ = write.send(tungstenite::Message::Close(None)).await;

    Ok(device_id)
}

fn decode_32_hex(raw: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(raw).map_err(|e| format!("hex: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!("expected 32 bytes, got {}", bytes.len()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Strip an optional `ws://` / `wss://` prefix from a relay origin so the
/// result can be embedded in a pairing URL without double-encoding the
/// scheme. The phone adds the scheme back based on the host: loopback and
/// plain IPs in tests use `ws://`, everything else uses `wss://`.
fn strip_ws_scheme(origin: &str) -> &str {
    origin
        .strip_prefix("wss://")
        .or_else(|| origin.strip_prefix("ws://"))
        .unwrap_or(origin)
}
