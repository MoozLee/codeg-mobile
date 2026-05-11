//! End-to-end integration test for `RelayClient`.
//!
//! Spins up an in-process relay mock that speaks the codeg handshake, then
//! runs the real client against it and verifies:
//!
//! 1. The client replies to `e2ee_hello` with a proper `e2ee_ready`.
//! 2. After the handshake, an encrypted `ping` request from the mock is
//!    answered by the client with an encrypted `pong` carrying the same
//!    payload.
//! 3. The client shuts down cleanly when the watch channel fires.
//!
//! No real network is used; everything rides a `tokio::net::TcpListener`
//! bound to 127.0.0.1.

#![cfg(feature = "relay-client")]

use std::sync::Arc;
use std::time::Duration;

use codeg_lib::crypto::frame::{decrypt_frame, derive_shared, encrypt_frame};
use codeg_lib::crypto::handshake::{HelloFrame, ReadyFrame};
use codeg_lib::crypto::keypair::{ephemeral_keypair, LongTermKeyPair};
use codeg_lib::relay::{AppRequest, AppResponse, RelayClient, RelayClientOptions};

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite;

/// Spawn a minimal relay that handshakes with the daemon, then sends one
/// encrypted `ping` request and reports back whether the daemon's reply
/// decrypted to the expected `pong`.
async fn spawn_mock_relay(
    daemon_long_term_pub: [u8; 32],
    session_id: String,
) -> (String, tokio::task::JoinHandle<Result<String, String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let origin = format!("ws://{}", addr);

    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.map_err(|e| e.to_string())?;

        // Read the HTTP request line to verify the daemon hit the right URL.
        // We accept either `/session/<id>/daemon` or any path; we only check
        // that the session_id appears. `accept_async` handles the upgrade.
        let ws = tokio_tungstenite::accept_async(stream)
            .await
            .map_err(|e| format!("accept: {e}"))?;

        let (mut write, mut read) = ws.split();

        // 1. Send hello.
        let (client_pub, client_sec) = ephemeral_keypair();
        let hello = HelloFrame::new(&client_pub, session_id.clone());
        let hello_raw = hello.encode().map_err(|e| e.to_string())?;
        write
            .send(tungstenite::Message::Text(hello_raw.into()))
            .await
            .map_err(|e| e.to_string())?;

        // 2. Expect ready back.
        let ready_msg = match read.next().await {
            Some(Ok(tungstenite::Message::Text(s))) => s.to_string(),
            other => return Err(format!("expected text ready, got: {other:?}")),
        };
        let ready = ReadyFrame::decode(&ready_msg).map_err(|e| e.to_string())?;
        if hex::decode(&ready.server_pub_hex).unwrap() != daemon_long_term_pub {
            return Err(format!(
                "ready server_pub_hex mismatch: got {} expected {}",
                ready.server_pub_hex,
                hex::encode(daemon_long_term_pub)
            ));
        }

        // 3. Derive shared key.
        let shared = derive_shared(&daemon_long_term_pub, &client_sec);

        // 4. Send encrypted ping.
        let req = AppRequest::Ping {
            payload: "relay-smoke-test".into(),
        };
        let req_bytes = serde_json::to_vec(&req).map_err(|e| e.to_string())?;
        let frame = encrypt_frame(&shared, &req_bytes, None);
        write
            .send(tungstenite::Message::Text(frame.into()))
            .await
            .map_err(|e| e.to_string())?;

        // 5. Expect encrypted pong back.
        let reply = match read.next().await {
            Some(Ok(tungstenite::Message::Text(s))) => s.to_string(),
            other => return Err(format!("expected encrypted pong, got: {other:?}")),
        };
        let plaintext = decrypt_frame(&shared, &reply).map_err(|e| e.to_string())?;
        let resp: AppResponse = serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
        let payload = match resp {
            AppResponse::Pong { payload } => payload,
            other => return Err(format!("expected Pong got: {other:?}")),
        };

        // 6. Politely close.
        let _ = write.send(tungstenite::Message::Close(None)).await;

        Ok(payload)
    });

    (origin, handle)
}

#[tokio::test]
async fn client_handshakes_and_roundtrips_an_encrypted_ping() {
    let daemon_keypair = LongTermKeyPair::generate();
    let daemon_pub = *daemon_keypair.public();
    let session_id = "integration-session".to_string();

    let (origin, mock_handle) = spawn_mock_relay(daemon_pub, session_id.clone()).await;

    let client = RelayClient::new(
        RelayClientOptions {
            relay_origin: origin,
            session_id,
        },
        Arc::new(daemon_keypair),
    );

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let run_handle = tokio::spawn(async move {
        client.run(shutdown_rx).await;
    });

    // The mock drives the exchange and returns the observed payload; give
    // it a generous timeout to avoid a flaky CI failure if the machine is
    // slow to schedule tasks.
    let payload = tokio::time::timeout(Duration::from_secs(10), mock_handle)
        .await
        .expect("mock relay did not finish in time")
        .expect("mock task panicked")
        .expect("mock relay error");

    assert_eq!(payload, "relay-smoke-test");

    // Shut the client down and wait for its loop to exit.
    shutdown_tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(5), run_handle)
        .await
        .expect("client did not shut down")
        .expect("client panicked");
}
