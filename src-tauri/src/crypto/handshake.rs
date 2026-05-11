//! Handshake frame serialization.
//!
//! Two JSON messages flow in cleartext over the relay before encryption
//! kicks in:
//!
//! - `e2ee_hello` (phone -> daemon): `{ v: 1, client_pub_hex, session_id }`
//! - `e2ee_ready` (daemon -> phone): `{ v: 1, server_pub_hex }`
//!
//! The version field is frozen at `1` for this task. Future versions are
//! welcome to bump it but the wire name (`v`) stays.

use serde::{Deserialize, Serialize};

use super::error::CryptoError;

/// Current handshake protocol version.
pub const HANDSHAKE_VERSION: u8 = 1;

/// Client -> daemon pairing hello.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloFrame {
    pub v: u8,
    pub client_pub_hex: String,
    pub session_id: String,
}

impl HelloFrame {
    pub fn new(client_pub: &[u8; 32], session_id: impl Into<String>) -> Self {
        Self {
            v: HANDSHAKE_VERSION,
            client_pub_hex: hex::encode(client_pub),
            session_id: session_id.into(),
        }
    }

    pub fn encode(&self) -> Result<String, CryptoError> {
        serde_json::to_string(self).map_err(CryptoError::from)
    }

    pub fn decode(raw: &str) -> Result<Self, CryptoError> {
        let frame: Self = serde_json::from_str(raw)?;
        if frame.v != HANDSHAKE_VERSION {
            return Err(CryptoError::InvalidHandshakeFrame(format!(
                "unsupported hello version: {}",
                frame.v
            )));
        }
        Ok(frame)
    }
}

/// Daemon -> client ready response. `server_pub_hex` is the daemon's
/// long-term public key, echoed back so the phone can verify it matches the
/// one it scanned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadyFrame {
    pub v: u8,
    pub server_pub_hex: String,
}

impl ReadyFrame {
    pub fn new(server_pub: &[u8; 32]) -> Self {
        Self {
            v: HANDSHAKE_VERSION,
            server_pub_hex: hex::encode(server_pub),
        }
    }

    pub fn encode(&self) -> Result<String, CryptoError> {
        serde_json::to_string(self).map_err(CryptoError::from)
    }

    pub fn decode(raw: &str) -> Result<Self, CryptoError> {
        let frame: Self = serde_json::from_str(raw)?;
        if frame.v != HANDSHAKE_VERSION {
            return Err(CryptoError::InvalidHandshakeFrame(format!(
                "unsupported ready version: {}",
                frame.v
            )));
        }
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_frame_roundtrip() {
        let key = [3u8; 32];
        let hello = HelloFrame::new(&key, "sess-1");
        let encoded = hello.encode().unwrap();
        let decoded = HelloFrame::decode(&encoded).unwrap();
        assert_eq!(hello, decoded);
    }

    #[test]
    fn ready_frame_roundtrip() {
        let key = [7u8; 32];
        let ready = ReadyFrame::new(&key);
        let encoded = ready.encode().unwrap();
        let decoded = ReadyFrame::decode(&encoded).unwrap();
        assert_eq!(ready, decoded);
    }

    #[test]
    fn hello_rejects_bad_version() {
        let raw = r#"{"v":9,"client_pub_hex":"00","session_id":"s"}"#;
        let err = HelloFrame::decode(raw).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidHandshakeFrame(_)));
    }

    #[test]
    fn ready_rejects_bad_version() {
        let raw = r#"{"v":9,"server_pub_hex":"00"}"#;
        let err = ReadyFrame::decode(raw).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidHandshakeFrame(_)));
    }
}
