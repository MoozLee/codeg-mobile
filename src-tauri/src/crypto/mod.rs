//! End-to-end encryption protocol for codeg mobile <-> daemon transport.
//!
//! Implements a NaCl-compatible `crypto_box` flow (Curve25519 X25519 key
//! agreement + XSalsa20-Poly1305 authenticated encryption). The protocol
//! format is deliberately identical to libsodium / TweetNaCl so the TS and
//! Rust sides can interoperate over the same WebSocket relay.
//!
//! ## Frame format
//!
//! A single encrypted frame is the concatenation `[24-byte nonce || ciphertext]`
//! encoded as base64. `ciphertext` includes the trailing 16-byte Poly1305 tag.
//!
//! ## Shared key derivation
//!
//! `derive_shared(their_pub, my_secret)` performs X25519 Diffie-Hellman and
//! then applies `HSalsa20` with an all-zero 16-byte nonce to produce the
//! 32-byte precomputed key — this is the same construction NaCl calls
//! `crypto_box_beforenm`.
//!
//! ## Handshake
//!
//! 1. Phone → Daemon: `e2ee_hello { v: 1, client_pub_hex, session_id }`
//! 2. Daemon → Phone: `e2ee_ready { v: 1, server_pub_hex }`
//!
//! After the handshake, every application-layer message is wrapped by
//! [`frame::encrypt_frame`] using the shared key.

pub mod error;
pub mod frame;
pub mod handshake;
pub mod keypair;
pub mod pairing;

#[cfg(test)]
mod gen_fixtures;

pub use error::CryptoError;
