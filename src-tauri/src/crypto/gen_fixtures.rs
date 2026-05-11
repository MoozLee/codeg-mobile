//! Generates interoperability fixtures for the crypto protocol.
//!
//! This test is `#[ignore]` by default. Run it with
//! `cargo test --lib crypto::gen_fixtures -- --ignored --nocapture`
//! to print a JSON payload with 3 deterministic encryption vectors. Copy
//! the output into `src-tauri/tests/fixtures/crypto_vectors.json` so the
//! Rust integration test and the TS vitest suite can share the same
//! ground truth.

#![cfg(test)]

use super::frame::{derive_shared, encrypt_frame};

fn fixture_case(
    client_secret_hex: &str,
    server_secret_hex: &str,
    nonce_hex: &str,
    plaintext: &str,
) -> String {
    use hex::FromHex;
    let client_secret: [u8; 32] = <[u8; 32]>::from_hex(client_secret_hex).unwrap();
    let server_secret: [u8; 32] = <[u8; 32]>::from_hex(server_secret_hex).unwrap();
    let nonce: [u8; 24] = <[u8; 24]>::from_hex(nonce_hex).unwrap();

    // Derive public keys through x25519-dalek the same way runtime code does.
    let client_pub = x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(
        client_secret,
    ))
    .to_bytes();
    let server_pub = x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(
        server_secret,
    ))
    .to_bytes();

    // Client-side: encrypt using client_secret + server_pub.
    let shared = derive_shared(&server_pub, &client_secret);
    let frame = encrypt_frame(&shared, plaintext.as_bytes(), Some(nonce));

    format!(
        r#"  {{
    "client_secret_hex": "{client_secret_hex}",
    "client_pub_hex": "{client_pub_hex}",
    "server_secret_hex": "{server_secret_hex}",
    "server_pub_hex": "{server_pub_hex}",
    "nonce_hex": "{nonce_hex}",
    "plaintext_utf8": {plaintext_json},
    "expected_frame_base64": "{frame}"
  }}"#,
        client_pub_hex = hex::encode(client_pub),
        server_pub_hex = hex::encode(server_pub),
        plaintext_json = serde_json::to_string(plaintext).unwrap(),
    )
}

#[test]
#[ignore]
fn print_fixtures() {
    let cases = [
        fixture_case(
            "0101010101010101010101010101010101010101010101010101010101010101",
            "0202020202020202020202020202020202020202020202020202020202020202",
            "030303030303030303030303030303030303030303030303",
            "hello codeg",
        ),
        fixture_case(
            "1111111111111111111111111111111111111111111111111111111111111111",
            "2222222222222222222222222222222222222222222222222222222222222222",
            "333333333333333333333333333333333333333333333333",
            "{\"type\":\"ping\",\"ts\":1700000000}",
        ),
        fixture_case(
            "deadbeefcafebabefeedface0123456789abcdef0123456789abcdef01234567",
            "0fedcba9876543210fedcba9876543210fedcba9876543210fedcba987654321",
            "cafefacefeedbabe0123456789abcdef0123456789abcdef",
            "mobile -> daemon handshake fixture vector",
        ),
    ];
    println!("[");
    for (i, c) in cases.iter().enumerate() {
        print!("{}", c);
        if i + 1 < cases.len() {
            println!(",");
        } else {
            println!();
        }
    }
    println!("]");
}
