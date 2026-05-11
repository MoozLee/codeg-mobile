//! Error type used across the crypto module.
//!
//! Intentionally narrow: the crypto layer is the bottom of the stack so we
//! only surface failures that the caller can reasonably distinguish (bad
//! input, crypto failure, IO on the daemon key file).

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("base64 decode: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("hex decode: {0}")]
    Hex(#[from] hex::FromHexError),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("invalid nonce length: expected 24, got {actual}")]
    InvalidNonceLength { actual: usize },

    #[error("frame too short: need at least 24 bytes for the nonce, got {actual}")]
    FrameTooShort { actual: usize },

    #[error("decryption failed (bad key, tampered ciphertext, or wrong nonce)")]
    DecryptionFailed,

    #[error("invalid pairing url: {0}")]
    InvalidPairingUrl(String),

    #[error("invalid handshake frame: {0}")]
    InvalidHandshakeFrame(String),
}
