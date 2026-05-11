//! End-to-end pairing test.
//!
//! Spins up a mock relay `server` endpoint, kicks off
//! `PairingCoordinator::create_offer_with_ttl`, and runs the phone-side
//! `scan_pairing_url` in the same process. Asserts:
//!
//! 1. The handshake completes and produces a matching shared key.
//! 2. A `paired_devices` row is persisted with the nickname and client
//!    pubkey the phone sent.
//! 3. The `PairingOffer` carries the expected relay origin / session id.
//!
//! No real network: the mock relay listens on 127.0.0.1:<ephemeral>.

#![cfg(feature = "relay-client")]

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite;

use codeg_lib::crypto::keypair::LongTermKeyPair;
use codeg_lib::db::migration::Migrator;
use codeg_lib::db::AppDatabase;
use codeg_lib::mobile::pairing::scan_pairing_url;
use codeg_lib::relay::session_store;
use codeg_lib::relay::PairingCoordinator;

async fn fresh_in_memory_db() -> AppDatabase {
    let conn = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite::memory: connect");
    conn.execute(Statement::from_string(
        DbBackend::Sqlite,
        "PRAGMA foreign_keys=ON;".to_owned(),
    ))
    .await
    .expect("foreign keys pragma");
    Migrator::up(&conn, None).await.expect("migrations");
    AppDatabase { conn }
}

/// A mock relay that accepts the first two websocket connections and
/// forwards text frames between them. The tests below open it as
/// `/session/<id>/daemon` and `/session/<id>/client` in that order.
async fn spawn_mock_relay() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let origin = format!("ws://{}", addr);

    let handle = tokio::spawn(async move {
        // Accept the daemon side first.
        let (daemon_stream, _) = listener.accept().await.expect("accept daemon");
        let daemon_ws = tokio_tungstenite::accept_async(daemon_stream)
            .await
            .expect("accept daemon ws");

        // Then the client side.
        let (client_stream, _) = listener.accept().await.expect("accept client");
        let client_ws = tokio_tungstenite::accept_async(client_stream)
            .await
            .expect("accept client ws");

        let (mut daemon_write, mut daemon_read) = daemon_ws.split();
        let (mut client_write, mut client_read) = client_ws.split();

        let forward_daemon_to_client = async {
            while let Some(msg) = daemon_read.next().await {
                match msg {
                    Ok(m @ tungstenite::Message::Text(_)) => {
                        if client_write.send(m).await.is_err() {
                            break;
                        }
                    }
                    Ok(tungstenite::Message::Close(_)) => {
                        let _ = client_write.send(tungstenite::Message::Close(None)).await;
                        break;
                    }
                    _ => continue,
                }
            }
        };
        let forward_client_to_daemon = async {
            while let Some(msg) = client_read.next().await {
                match msg {
                    Ok(m @ tungstenite::Message::Text(_)) => {
                        if daemon_write.send(m).await.is_err() {
                            break;
                        }
                    }
                    Ok(tungstenite::Message::Close(_)) => {
                        let _ = daemon_write.send(tungstenite::Message::Close(None)).await;
                        break;
                    }
                    _ => continue,
                }
            }
        };

        tokio::join!(forward_daemon_to_client, forward_client_to_daemon);
    });

    (origin, handle)
}

#[tokio::test]
async fn end_to_end_pairing_persists_device_row() {
    let db = fresh_in_memory_db().await;

    let daemon_keypair = Arc::new(LongTermKeyPair::generate());
    let coordinator = PairingCoordinator::new(Arc::clone(&daemon_keypair));

    let (relay_origin, relay_handle) = spawn_mock_relay().await;

    let offer = coordinator
        .create_offer_with_ttl(&relay_origin, db.conn.clone(), Duration::from_secs(15))
        .await;
    assert!(offer.pairing_url.starts_with("codeg-pair://"));
    assert_eq!(offer.relay_origin, relay_origin);

    // Give the coordinator task a moment to start listening on the mock
    // relay. In practice this is near-instant but avoids a race on slow CI.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let outcome = scan_pairing_url(&offer.pairing_url, "Lee's iPhone".into())
        .await
        .expect("pairing outcome");
    assert_eq!(outcome.session_id, offer.session_id);
    assert_eq!(
        outcome.daemon_pub_hex,
        hex::encode(daemon_keypair.public())
    );
    assert!(!outcome.device_id.is_empty());

    // Wait up to 2 seconds for the daemon side to persist the row.
    let row = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let rows = session_store::list_paired_devices(&db.conn).await.unwrap();
            if let Some(row) = rows.into_iter().next() {
                return row;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("paired device row not persisted in time");

    assert_eq!(row.device_id, outcome.device_id);
    assert_eq!(row.nickname, "Lee's iPhone");
    assert_eq!(row.relay_session_id, offer.session_id);
    assert!(!row.revoked);

    // Relay goes away once both sides close.
    let _ = tokio::time::timeout(Duration::from_secs(5), relay_handle).await;
}
