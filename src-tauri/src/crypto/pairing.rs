//! Pairing URL helpers.
//!
//! A pairing URL looks like:
//!
//! ```text
//! codeg-pair://<relay-origin>/<session-id>#<base64url(daemon_pub_key)>
//! ```
//!
//! The daemon's Curve25519 public key lives in the URL fragment so it is
//! never sent to the relay server (HTTP clients strip fragments before
//! emitting the request). `<relay-origin>` is the relay host (and optional
//! port); `<session-id>` is the server-assigned session slot.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64_URL;
use base64::Engine;
use serde::{Deserialize, Serialize};

use super::error::CryptoError;

/// Scheme prefix for pairing URLs.
pub const PAIRING_SCHEME: &str = "codeg-pair://";

/// Parsed pairing URL payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingPayload {
    /// Relay origin, e.g. `codeg-relay.example.workers.dev`.
    pub relay_origin: String,
    /// Relay session id. Opaque to the crypto layer.
    pub session_id: String,
    /// Daemon's 32-byte long-term public key, hex-encoded.
    pub server_pub_hex: String,
}

/// Build a pairing URL given the relay host, session id, and daemon public
/// key bytes. Panics on zero-length inputs to catch programmer error.
pub fn build_pairing_url(
    relay_origin: &str,
    session_id: &str,
    pub_key: &[u8; 32],
) -> String {
    let fragment = B64_URL.encode(pub_key);
    format!(
        "{PAIRING_SCHEME}{origin}/{session}#{fragment}",
        origin = relay_origin,
        session = session_id,
    )
}

/// Parse a pairing URL back into its parts. Rejects any URL that does not
/// carry all three of relay-origin, session-id, and base64url public key in
/// the fragment.
pub fn parse_pairing_url(url: &str) -> Result<PairingPayload, CryptoError> {
    let rest = url
        .strip_prefix(PAIRING_SCHEME)
        .ok_or_else(|| CryptoError::InvalidPairingUrl(format!("missing scheme: {url}")))?;

    let (path_part, fragment) = rest
        .split_once('#')
        .ok_or_else(|| CryptoError::InvalidPairingUrl("missing fragment".into()))?;

    let (relay_origin, session_id) = path_part.split_once('/').ok_or_else(|| {
        CryptoError::InvalidPairingUrl("missing /<session-id> segment".into())
    })?;

    if relay_origin.is_empty() {
        return Err(CryptoError::InvalidPairingUrl("empty relay origin".into()));
    }
    if session_id.is_empty() {
        return Err(CryptoError::InvalidPairingUrl("empty session id".into()));
    }

    let pub_bytes = B64_URL
        .decode(fragment.as_bytes())
        .map_err(|e| CryptoError::InvalidPairingUrl(format!("fragment base64url: {e}")))?;
    if pub_bytes.len() != 32 {
        return Err(CryptoError::InvalidPairingUrl(format!(
            "fragment must decode to 32 bytes, got {}",
            pub_bytes.len()
        )));
    }

    Ok(PairingPayload {
        relay_origin: relay_origin.to_string(),
        session_id: session_id.to_string(),
        server_pub_hex: hex::encode(pub_bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_parse_roundtrip() {
        let key = [1u8; 32];
        let url = build_pairing_url("relay.example.dev", "sess-abc", &key);
        assert!(url.starts_with(PAIRING_SCHEME));
        let parsed = parse_pairing_url(&url).unwrap();
        assert_eq!(parsed.relay_origin, "relay.example.dev");
        assert_eq!(parsed.session_id, "sess-abc");
        assert_eq!(parsed.server_pub_hex, hex::encode(key));
    }

    #[test]
    fn rejects_wrong_scheme() {
        let err =
            parse_pairing_url("https://relay.example.dev/sess#abc").unwrap_err();
        assert!(matches!(err, CryptoError::InvalidPairingUrl(_)));
    }

    #[test]
    fn rejects_missing_fragment() {
        let err =
            parse_pairing_url("codeg-pair://relay.example.dev/sess").unwrap_err();
        assert!(matches!(err, CryptoError::InvalidPairingUrl(_)));
    }

    #[test]
    fn rejects_wrong_fragment_length() {
        let err = parse_pairing_url(
            "codeg-pair://relay.example.dev/sess#AA",
        )
        .unwrap_err();
        assert!(matches!(err, CryptoError::InvalidPairingUrl(_)));
    }
}
