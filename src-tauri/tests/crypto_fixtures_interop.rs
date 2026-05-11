//! Cross-language interop fixtures. Exercises the exact byte patterns the
//! TS vitest suite in `src/lib/crypto/` also validates, so the two
//! implementations are guaranteed to agree on encoding.

use std::fs;
use std::path::PathBuf;

use codeg_lib::crypto::frame::{decrypt_frame, derive_shared, encrypt_frame};
use hex::FromHex;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Vector {
    client_secret_hex: String,
    client_pub_hex: String,
    server_secret_hex: String,
    server_pub_hex: String,
    nonce_hex: String,
    plaintext_utf8: String,
    expected_frame_base64: String,
}

fn load_vectors() -> Vec<Vector> {
    let path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("crypto_vectors.json");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    serde_json::from_str(&raw).expect("crypto_vectors.json parses")
}

fn decode_32(h: &str) -> [u8; 32] {
    <[u8; 32]>::from_hex(h).expect("32-byte hex")
}

fn decode_24(h: &str) -> [u8; 24] {
    <[u8; 24]>::from_hex(h).expect("24-byte hex")
}

#[test]
fn public_keys_match_secrets_via_x25519() {
    for v in load_vectors() {
        let client_secret = decode_32(&v.client_secret_hex);
        let server_secret = decode_32(&v.server_secret_hex);
        let client_pub_from_sec = x25519_dalek::PublicKey::from(
            &x25519_dalek::StaticSecret::from(client_secret),
        )
        .to_bytes();
        let server_pub_from_sec = x25519_dalek::PublicKey::from(
            &x25519_dalek::StaticSecret::from(server_secret),
        )
        .to_bytes();
        assert_eq!(
            hex::encode(client_pub_from_sec),
            v.client_pub_hex,
            "client public key must be derivable from client secret"
        );
        assert_eq!(
            hex::encode(server_pub_from_sec),
            v.server_pub_hex,
            "server public key must be derivable from server secret"
        );
    }
}

#[test]
fn daemon_can_decrypt_client_produced_frame() {
    for v in load_vectors() {
        let server_secret = decode_32(&v.server_secret_hex);
        let client_pub = decode_32(&v.client_pub_hex);
        let shared = derive_shared(&client_pub, &server_secret);
        let decoded = decrypt_frame(&shared, &v.expected_frame_base64)
            .expect("fixture decrypts on the server side");
        assert_eq!(
            std::str::from_utf8(&decoded).expect("plaintext is utf8"),
            v.plaintext_utf8
        );
    }
}

#[test]
fn client_encrypt_with_fixed_nonce_reproduces_expected_frame() {
    for v in load_vectors() {
        let client_secret = decode_32(&v.client_secret_hex);
        let server_pub = decode_32(&v.server_pub_hex);
        let nonce = decode_24(&v.nonce_hex);

        let shared = derive_shared(&server_pub, &client_secret);
        let frame =
            encrypt_frame(&shared, v.plaintext_utf8.as_bytes(), Some(nonce));
        assert_eq!(
            frame, v.expected_frame_base64,
            "deterministic encrypt must match the fixture"
        );
    }
}

#[test]
fn derive_shared_is_symmetric_for_every_vector() {
    for v in load_vectors() {
        let client_secret = decode_32(&v.client_secret_hex);
        let server_secret = decode_32(&v.server_secret_hex);
        let client_pub = decode_32(&v.client_pub_hex);
        let server_pub = decode_32(&v.server_pub_hex);

        let shared_client = derive_shared(&server_pub, &client_secret);
        let shared_server = derive_shared(&client_pub, &server_secret);
        assert_eq!(shared_client, shared_server);
    }
}
