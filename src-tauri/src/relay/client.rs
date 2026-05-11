//! Outbound daemon -> relay WebSocket client.
//!
//! The daemon opens `wss://<relay-origin>/session/<session-id>/daemon`
//! and speaks the codeg handshake:
//!
//! 1. Wait for `e2ee_hello` (cleartext JSON) from the phone.
//! 2. Derive the NaCl shared key from the phone's ephemeral pubkey and
//!    the daemon's long-term secret.
//! 3. Reply with `e2ee_ready` (cleartext JSON) containing the daemon's
//!    long-term public key.
//! 4. From then on, every frame is ciphertext: base64-encoded
//!    `[24-byte nonce || ciphertext]` produced by [`crate::crypto::frame`].
//!
//! If the connection drops, [`RelayClient::run`] reconnects with
//! exponential backoff capped at 30 seconds. Shutdown is driven by a
//! [`tokio::sync::watch`] channel so the caller can deterministically
//! tear the client down from any task.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, watch, Mutex};
use tokio_tungstenite::tungstenite;

use crate::app_state::AppState;
use crate::crypto::frame::{decrypt_frame, derive_shared, encrypt_frame};
use crate::crypto::handshake::{HelloFrame, ReadyFrame};
use crate::crypto::keypair::LongTermKeyPair;

use super::dispatcher::{
    acp_event_to_app_response, dispatch_app_request, dispatch_app_request_with_state, AppRequest,
    AppResponse,
};

/// Maximum reconnect backoff. 30s matches the PRD; Paseo uses the same
/// ceiling and in practice anything larger makes recovery after a WiFi
/// flip feel sluggish on a phone.
const MAX_BACKOFF_SECS: u64 = 30;

/// Buffer size for the outbound app-message channel. 64 is comfortably
/// above any burst we expect from a single session (tool-call streaming
/// included) while still bounding memory if the peer disappears.
const OUTBOUND_CHANNEL_CAPACITY: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum RelayClientError {
    #[error("websocket: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("crypto: {0}")]
    Crypto(#[from] crate::crypto::CryptoError),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("handshake: {0}")]
    Handshake(String),

    #[error("relay session id mismatch: expected {expected}, got {got}")]
    SessionMismatch { expected: String, got: String },

    #[error("connection closed before handshake completed")]
    ClosedDuringHandshake,
}

/// Configuration for building a [`RelayClient`].
#[derive(Debug, Clone)]
pub struct RelayClientOptions {
    /// Base origin of the relay, for example `wss://codeg-relay.<acct>.workers.dev`.
    /// No trailing slash; the client appends `/session/<id>/daemon`.
    pub relay_origin: String,
    /// Relay session identifier generated at pairing time.
    pub session_id: String,
}

/// Handle used by the rest of the codebase to push app-layer messages
/// out over the relay. Dropping all senders terminates the outbound
/// loop cleanly once the current message flushes.
#[derive(Debug, Clone)]
pub struct RelayClientSender(mpsc::Sender<AppResponse>);

impl RelayClientSender {
    pub async fn send(&self, msg: AppResponse) -> Result<(), RelayClientError> {
        self.0
            .send(msg)
            .await
            .map_err(|_| RelayClientError::Handshake("relay outbound channel closed".into()))
    }
}

/// Long-running outbound relay client. The primary entry point is
/// [`RelayClient::run`], which loops over connect attempts until the
/// shutdown channel fires.
pub struct RelayClient {
    options: RelayClientOptions,
    keypair: Arc<LongTermKeyPair>,
    outbound: Mutex<Option<RelayClientSender>>,
    /// Optional `AppState` handle. When set, incoming requests are routed
    /// through [`dispatch_app_request_with_state`] and relevant daemon
    /// events (content delta / permission request / etc.) are forwarded
    /// to the phone as `message_delta` / `approval_required` frames.
    /// Left `None` in unit/integration tests and the pairing-only code
    /// path, which want the pure-function dispatcher.
    app_state: Option<Arc<AppState>>,
}

impl RelayClient {
    pub fn new(options: RelayClientOptions, keypair: Arc<LongTermKeyPair>) -> Self {
        Self {
            options,
            keypair,
            outbound: Mutex::new(None),
            app_state: None,
        }
    }

    /// Attach an `AppState` so inbound app-layer requests get routed
    /// through the state-aware dispatcher and daemon events get mirrored
    /// to the phone as push frames.
    pub fn with_app_state(mut self, state: Arc<AppState>) -> Self {
        self.app_state = Some(state);
        self
    }

    /// Most recent outbound sender, if a connection is live. Returns
    /// `None` between reconnect attempts.
    pub async fn sender(&self) -> Option<RelayClientSender> {
        self.outbound.lock().await.clone()
    }

    /// Run until `shutdown` fires. Exponential backoff between reconnect
    /// attempts caps at [`MAX_BACKOFF_SECS`].
    pub async fn run(&self, mut shutdown: watch::Receiver<bool>) {
        let mut backoff_secs: u64 = 1;

        loop {
            if *shutdown.borrow() {
                return;
            }

            // `connect_and_serve` needs its own mutable borrow of a
            // receiver to react to shutdown mid-session; clone so the
            // outer loop keeps its view for the backoff sleep.
            let mut inner_shutdown = shutdown.clone();
            let result = tokio::select! {
                r = self.connect_and_serve(&mut inner_shutdown) => r,
                _ = shutdown.changed() => {
                    return;
                }
            };

            match result {
                Ok(_) => {
                    // Clean shutdown (shutdown signal, or peer closed).
                    // Reset backoff so the next connect is immediate.
                    backoff_secs = 1;
                }
                Err(err) => {
                    eprintln!("[Relay] session ended: {err}");
                }
            }

            // Clear the sender while we sleep so callers see "not connected".
            *self.outbound.lock().await = None;

            let delay = Duration::from_secs(backoff_secs);
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = shutdown.changed() => return,
            }
            backoff_secs = (backoff_secs.saturating_mul(2)).min(MAX_BACKOFF_SECS);
        }
    }

    async fn connect_and_serve(
        &self,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<(), RelayClientError> {
        let url = format!(
            "{origin}/session/{session}/daemon",
            origin = self.options.relay_origin.trim_end_matches('/'),
            session = self.options.session_id,
        );
        eprintln!("[Relay] connecting to {url}");

        let (ws_stream, _resp) = tokio_tungstenite::connect_async(&url).await?;
        let (mut write, mut read) = ws_stream.split();

        // --- Handshake ---

        // Step 1: receive hello (cleartext JSON).
        let hello_msg = match read.next().await {
            Some(Ok(tungstenite::Message::Text(s))) => s,
            Some(Ok(tungstenite::Message::Close(_))) | None => {
                return Err(RelayClientError::ClosedDuringHandshake);
            }
            Some(Ok(other)) => {
                return Err(RelayClientError::Handshake(format!(
                    "expected text hello, got: {other:?}"
                )));
            }
            Some(Err(e)) => return Err(RelayClientError::WebSocket(e)),
        };
        let hello = HelloFrame::decode(&hello_msg)?;
        if hello.session_id != self.options.session_id {
            return Err(RelayClientError::SessionMismatch {
                expected: self.options.session_id.clone(),
                got: hello.session_id,
            });
        }

        // Step 2: derive shared key.
        let client_pub = parse_32_hex(&hello.client_pub_hex)
            .map_err(|e| RelayClientError::Handshake(format!("client pub: {e}")))?;
        let shared = derive_shared(&client_pub, self.keypair.secret());

        // Step 3: send ready (cleartext JSON).
        let ready = ReadyFrame::new(self.keypair.public());
        let ready_raw = ready.encode()?;
        write
            .send(tungstenite::Message::Text(ready_raw.into()))
            .await?;

        eprintln!(
            "[Relay] handshake ok, session_id={}",
            self.options.session_id
        );

        // --- Encrypted pump ---

        let (out_tx, mut out_rx) = mpsc::channel::<AppResponse>(OUTBOUND_CHANNEL_CAPACITY);
        *self.outbound.lock().await = Some(RelayClientSender(out_tx.clone()));

        // Optional: forward daemon-side events to the phone as push frames.
        // We subscribe to the shared `event_broadcaster`, translate the
        // ACP payload into our narrow wire format, and push it back onto
        // the outbound channel so it rides the same encrypted pipe as
        // responses. When `app_state` is None we skip this entirely —
        // that path is used by pairing-only connections and tests.
        let event_bridge = if let Some(state) = self.app_state.as_ref() {
            let mut rx = state.event_broadcaster.subscribe();
            let forward_tx = out_tx.clone();
            let handle = tokio::spawn(async move {
                while let Ok(evt) = rx.recv().await {
                    // `emit_with_state` writes ACP events on the
                    // `acp://event` channel; other channels (pet state,
                    // terminal output, etc.) aren't relevant for the
                    // phone wire, so drop them here.
                    if evt.channel != "acp://event" {
                        continue;
                    }
                    // Expect the same envelope shape `emit_with_state`
                    // produces: `{ seq, connection_id, type, ...fields }`.
                    // We only care about a few types; deserialize lazily.
                    let Ok(env) = serde_json::from_value::<
                        crate::acp::EventEnvelope,
                    >(evt.payload.as_ref().clone()) else {
                        continue;
                    };
                    let session_id = env.connection_id.clone();
                    let Some(mut resp) = acp_event_to_app_response(&env.payload) else {
                        continue;
                    };
                    // Patch in the session_id field that
                    // `acp_event_to_app_response` leaves blank (it has no
                    // access to the envelope context).
                    match &mut resp {
                        AppResponse::MessageDelta { session_id: s, .. } => {
                            *s = session_id;
                        }
                        AppResponse::ApprovalRequired {
                            session_id: s, ..
                        } => {
                            *s = session_id;
                        }
                        _ => {}
                    }
                    if forward_tx.send(resp).await.is_err() {
                        // Outbound channel closed (peer gone); stop
                        // pumping events.
                        return;
                    }
                }
            });
            Some(handle)
        } else {
            None
        };

        let result = loop {
            tokio::select! {
                // Outbound: encrypt the queued response and forward.
                maybe_msg = out_rx.recv() => {
                    let Some(msg) = maybe_msg else {
                        let _ = write.send(tungstenite::Message::Close(None)).await;
                        break Ok(());
                    };
                    let plaintext = match serde_json::to_vec(&msg) {
                        Ok(v) => v,
                        Err(e) => break Err(RelayClientError::Json(e)),
                    };
                    let frame = encrypt_frame(&shared, &plaintext, None);
                    if let Err(e) = write.send(tungstenite::Message::Text(frame.into())).await {
                        break Err(RelayClientError::WebSocket(e));
                    }
                }
                // Inbound: decrypt, dispatch, and echo a response.
                incoming = read.next() => {
                    match incoming {
                        Some(Ok(tungstenite::Message::Text(frame))) => {
                            let plaintext = match decrypt_frame(&shared, &frame) {
                                Ok(v) => v,
                                Err(e) => break Err(RelayClientError::Crypto(e)),
                            };
                            let req: AppRequest = match serde_json::from_slice(&plaintext) {
                                Ok(r) => r,
                                Err(e) => break Err(RelayClientError::Json(e)),
                            };
                            let resp = if let Some(state) = self.app_state.as_ref() {
                                dispatch_app_request_with_state(req, state).await
                            } else {
                                dispatch_app_request(req)
                            };
                            let resp_bytes = match serde_json::to_vec(&resp) {
                                Ok(v) => v,
                                Err(e) => break Err(RelayClientError::Json(e)),
                            };
                            let out_frame = encrypt_frame(&shared, &resp_bytes, None);
                            if let Err(e) = write
                                .send(tungstenite::Message::Text(out_frame.into()))
                                .await
                            {
                                break Err(RelayClientError::WebSocket(e));
                            }
                        }
                        Some(Ok(tungstenite::Message::Ping(data))) => {
                            if let Err(e) =
                                write.send(tungstenite::Message::Pong(data)).await
                            {
                                break Err(RelayClientError::WebSocket(e));
                            }
                        }
                        Some(Ok(tungstenite::Message::Pong(_))) => {}
                        Some(Ok(tungstenite::Message::Binary(_))) => {
                            // All application traffic rides text frames (base64
                            // ciphertext). Binary is reserved for future use.
                            break Err(RelayClientError::Handshake(
                                "unexpected binary frame after handshake".into(),
                            ));
                        }
                        Some(Ok(tungstenite::Message::Frame(_))) => {}
                        Some(Ok(tungstenite::Message::Close(_))) | None => {
                            break Ok(());
                        }
                        Some(Err(e)) => {
                            break Err(RelayClientError::WebSocket(e));
                        }
                    }
                }
                _ = shutdown.changed() => {
                    let _ = write.send(tungstenite::Message::Close(None)).await;
                    break Ok(());
                }
            }
        };

        if let Some(h) = event_bridge {
            h.abort();
        }

        result
    }
}

fn parse_32_hex(raw: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(raw).map_err(|e| format!("hex: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!("expected 32 bytes, got {}", bytes.len()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_32_hex_accepts_valid() {
        let raw = hex::encode([3u8; 32]);
        let parsed = parse_32_hex(&raw).unwrap();
        assert_eq!(parsed, [3u8; 32]);
    }

    #[test]
    fn parse_32_hex_rejects_wrong_length() {
        let raw = hex::encode([1u8; 10]);
        assert!(parse_32_hex(&raw).is_err());
    }
}
