//! Mobile-side pairing skeleton.
//!
//! Given a scanned `codeg-pair://...` URL, this module:
//!
//! 1. Parses the URL into relay origin / session id / daemon pubkey.
//! 2. Generates an ephemeral Curve25519 keypair.
//! 3. Opens `wss://<relay>/session/<id>/client`.
//! 4. Sends the cleartext `e2ee_hello` frame.
//! 5. Waits for `e2ee_ready` from the daemon, verifying the pubkey matches.
//! 6. Derives the shared key and sends `device_register { nickname }`
//!    as an encrypted frame.
//! 7. Waits for the encrypted `device_register_ack` response.
//! 8. Returns a [`PairingOutcome`] that the mobile app can persist
//!    (actual persistence arrives in the device-credentials sub-task).
//!
//! This module is compiled regardless of `mobile-runtime` / `tauri-runtime`
//! feature flags because the Rust test harness exercises it against an
//! in-process mock daemon. The `scan_pairing_url` entry point intentionally
//! has no Tauri dependencies so it can be re-bound to a Tauri command in
//! P5 without churn.

#![cfg(feature = "relay-client")]

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite;

use crate::crypto::frame::{decrypt_frame, derive_shared, encrypt_frame};
use crate::crypto::handshake::{HelloFrame, ReadyFrame};
use crate::crypto::keypair::ephemeral_keypair;
use crate::crypto::pairing::parse_pairing_url;
use crate::relay::dispatcher::{AppRequest, AppResponse};

/// How long to wait in total for the full pairing handshake.
pub const PAIRING_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, thiserror::Error)]
pub enum MobilePairingError {
    #[error("invalid pairing url: {0}")]
    InvalidPairingUrl(String),

    #[error("websocket: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("crypto: {0}")]
    Crypto(#[from] crate::crypto::CryptoError),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("handshake: {0}")]
    Handshake(String),

    #[error("daemon public key mismatch: expected {expected}, got {got}")]
    PubKeyMismatch { expected: String, got: String },

    #[error("unexpected response variant: {0}")]
    UnexpectedResponse(&'static str),

    #[error("timed out waiting for daemon")]
    Timeout,
}

/// Outcome of a successful pairing round-trip. The mobile app persists
/// this into its Stronghold-backed credential store (see the follow-up
/// credential-management task).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingOutcome {
    pub device_id: String,
    pub daemon_pub_hex: String,
    pub shared_hex: String,
    pub server_version: String,
    pub relay_origin: String,
    pub session_id: String,
}

/// Entry point called by the scan screen. Consumes the scanned URL and a
/// user-chosen nickname and returns the outcome or an error.
pub async fn scan_pairing_url(
    url: &str,
    nickname: String,
) -> Result<PairingOutcome, MobilePairingError> {
    tokio::time::timeout(
        Duration::from_secs(PAIRING_TIMEOUT_SECS),
        scan_pairing_url_inner(url, nickname),
    )
    .await
    .map_err(|_| MobilePairingError::Timeout)?
}

async fn scan_pairing_url_inner(
    url: &str,
    nickname: String,
) -> Result<PairingOutcome, MobilePairingError> {
    let payload = parse_pairing_url(url)
        .map_err(|e| MobilePairingError::InvalidPairingUrl(e.to_string()))?;

    let (client_pub, client_sec) = ephemeral_keypair();

    let relay_origin_with_scheme = ensure_ws_scheme(&payload.relay_origin);
    let ws_url = format!(
        "{origin}/session/{session}/client",
        origin = relay_origin_with_scheme.trim_end_matches('/'),
        session = payload.session_id,
    );
    let (ws_stream, _resp) = tokio_tungstenite::connect_async(&ws_url).await?;
    let (mut write, mut read) = ws_stream.split();

    // 1. Send hello.
    let hello = HelloFrame::new(&client_pub, payload.session_id.clone());
    let hello_raw = hello.encode()?;
    write
        .send(tungstenite::Message::Text(hello_raw.into()))
        .await?;

    // 2. Wait for ready (cleartext).
    let ready_text = loop {
        match read.next().await {
            Some(Ok(tungstenite::Message::Text(s))) => break s.to_string(),
            Some(Ok(tungstenite::Message::Ping(data))) => {
                let _ = write.send(tungstenite::Message::Pong(data)).await;
                continue;
            }
            Some(Ok(tungstenite::Message::Pong(_))) => continue,
            Some(Ok(other)) => {
                return Err(MobilePairingError::Handshake(format!(
                    "expected text ready, got: {other:?}"
                )));
            }
            Some(Err(e)) => return Err(MobilePairingError::WebSocket(e)),
            None => {
                return Err(MobilePairingError::Handshake(
                    "closed before ready".into(),
                ));
            }
        }
    };
    let ready = ReadyFrame::decode(&ready_text)?;
    if !eq_hex_ignore_case(&ready.server_pub_hex, &payload.server_pub_hex) {
        return Err(MobilePairingError::PubKeyMismatch {
            expected: payload.server_pub_hex.clone(),
            got: ready.server_pub_hex.clone(),
        });
    }

    // 3. Derive shared key.
    let daemon_pub_bytes = decode_32_hex(&payload.server_pub_hex)
        .map_err(|e| MobilePairingError::Handshake(format!("server pub: {e}")))?;
    let shared = derive_shared(&daemon_pub_bytes, &client_sec);

    // 4. Encrypted device_register.
    let req = AppRequest::DeviceRegister {
        nickname: nickname.clone(),
    };
    let plaintext = serde_json::to_vec(&req)?;
    let frame = encrypt_frame(&shared, &plaintext, None);
    write
        .send(tungstenite::Message::Text(frame.into()))
        .await?;

    // 5. Wait for encrypted device_register_ack.
    let ack_text = loop {
        match read.next().await {
            Some(Ok(tungstenite::Message::Text(s))) => break s.to_string(),
            Some(Ok(tungstenite::Message::Ping(data))) => {
                let _ = write.send(tungstenite::Message::Pong(data)).await;
                continue;
            }
            Some(Ok(tungstenite::Message::Pong(_))) => continue,
            Some(Ok(other)) => {
                return Err(MobilePairingError::Handshake(format!(
                    "expected text ack, got: {other:?}"
                )));
            }
            Some(Err(e)) => return Err(MobilePairingError::WebSocket(e)),
            None => {
                return Err(MobilePairingError::Handshake(
                    "closed before ack".into(),
                ));
            }
        }
    };
    let ack_bytes = decrypt_frame(&shared, &ack_text)?;
    let response: AppResponse = serde_json::from_slice(&ack_bytes)?;
    let (device_id, server_version) = match response {
        AppResponse::DeviceRegisterAck {
            device_id,
            server_version,
        } => (device_id, server_version),
        AppResponse::Pong { .. } => {
            return Err(MobilePairingError::UnexpectedResponse("pong"));
        }
        AppResponse::Error { message } => {
            return Err(MobilePairingError::Handshake(message));
        }
        // Any other variant (SessionsList / SessionDetail / etc.) should
        // never appear during pairing. We treat it as an unexpected
        // response rather than a decode error so the caller still gets a
        // typed `MobilePairingError`.
        _ => {
            return Err(MobilePairingError::UnexpectedResponse("non_ack"));
        }
    };

    // 6. Close politely.
    let _ = write.send(tungstenite::Message::Close(None)).await;

    Ok(PairingOutcome {
        device_id,
        daemon_pub_hex: payload.server_pub_hex,
        shared_hex: hex::encode(shared),
        server_version,
        relay_origin: payload.relay_origin,
        session_id: payload.session_id,
    })
}

fn eq_hex_ignore_case(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
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

/// The pairing URL carries only the relay host (no `ws://` / `wss://`
/// scheme). Reconstruct the scheme for the WebSocket connect: loopback
/// and plain-IP tests use `ws://`, everything else `wss://`.
fn ensure_ws_scheme(origin: &str) -> String {
    if origin.starts_with("ws://") || origin.starts_with("wss://") {
        return origin.to_string();
    }
    // Test-friendly shortcut: if the host looks like a loopback / IP with
    // port, use the plaintext scheme so the unit test's TcpListener works.
    let host_only = origin.split('/').next().unwrap_or(origin);
    let bare_host = host_only.split(':').next().unwrap_or(host_only);
    let is_local = bare_host == "localhost"
        || bare_host == "127.0.0.1"
        || bare_host == "[::1]"
        || bare_host.parse::<std::net::IpAddr>().is_ok();
    if is_local {
        format!("ws://{origin}")
    } else {
        format!("wss://{origin}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keypair::LongTermKeyPair;
    use crate::crypto::pairing::build_pairing_url;
    use tokio::net::TcpListener;

    /// Spins up a mock daemon side of the relay that speaks the handshake,
    /// accepts the `device_register`, and returns a canned ack.
    async fn spawn_mock_daemon(
        keypair: LongTermKeyPair,
        session_id: String,
    ) -> (String, tokio::task::JoinHandle<Result<String, String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let relay_origin = format!("ws://{}", addr);

        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let ws = tokio_tungstenite::accept_async(stream)
                .await
                .map_err(|e| format!("accept: {e}"))?;
            let (mut write, mut read) = ws.split();

            // Expect hello.
            let hello_text = match read.next().await {
                Some(Ok(tungstenite::Message::Text(s))) => s.to_string(),
                other => return Err(format!("expected hello, got: {other:?}")),
            };
            let hello = HelloFrame::decode(&hello_text).map_err(|e| e.to_string())?;
            if hello.session_id != session_id {
                return Err("session mismatch".into());
            }

            // Send ready.
            let ready = ReadyFrame::new(keypair.public());
            write
                .send(tungstenite::Message::Text(
                    ready.encode().map_err(|e| e.to_string())?.into(),
                ))
                .await
                .map_err(|e| e.to_string())?;

            // Derive shared.
            let client_pub = decode_32_hex(&hello.client_pub_hex).unwrap();
            let shared = derive_shared(&client_pub, keypair.secret());

            // Decrypt register.
            let register_frame = match read.next().await {
                Some(Ok(tungstenite::Message::Text(s))) => s.to_string(),
                other => return Err(format!("expected encrypted register, got: {other:?}")),
            };
            let plain = decrypt_frame(&shared, &register_frame).map_err(|e| e.to_string())?;
            let req: AppRequest = serde_json::from_slice(&plain).map_err(|e| e.to_string())?;
            let nickname = match req {
                AppRequest::DeviceRegister { nickname } => nickname,
                other => return Err(format!("expected device_register, got: {other:?}")),
            };

            // Reply with ack.
            let ack = AppResponse::DeviceRegisterAck {
                device_id: "mock-device".into(),
                server_version: "0.0.0-test".into(),
            };
            let ack_bytes = serde_json::to_vec(&ack).map_err(|e| e.to_string())?;
            let ack_frame = encrypt_frame(&shared, &ack_bytes, None);
            write
                .send(tungstenite::Message::Text(ack_frame.into()))
                .await
                .map_err(|e| e.to_string())?;

            let _ = write.send(tungstenite::Message::Close(None)).await;
            Ok(nickname)
        });

        (relay_origin, handle)
    }

    #[tokio::test]
    async fn scan_pairing_url_round_trips_against_mock() {
        let keypair = LongTermKeyPair::generate();
        let daemon_pub = *keypair.public();
        let session_id = "mobile-pair-session".to_string();
        let (relay_origin, mock_handle) = spawn_mock_daemon(keypair, session_id.clone()).await;

        // Strip the `ws://` scheme so the pairing URL format matches what
        // `PairingCoordinator::create_offer` produces (host-only). The
        // phone reconstructs the scheme via `ensure_ws_scheme`.
        let host_only = relay_origin.trim_start_matches("ws://");
        let pairing_url = build_pairing_url(host_only, &session_id, &daemon_pub);

        let outcome = scan_pairing_url(&pairing_url, "Lee's iPhone".into())
            .await
            .expect("pairing outcome");
        assert_eq!(outcome.device_id, "mock-device");
        assert_eq!(outcome.server_version, "0.0.0-test");
        assert_eq!(outcome.daemon_pub_hex, hex::encode(daemon_pub));
        assert_eq!(outcome.session_id, session_id);

        // The mock task returns the nickname it saw, so we can double-check.
        let nickname = mock_handle.await.unwrap().unwrap();
        assert_eq!(nickname, "Lee's iPhone");
    }
}
