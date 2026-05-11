/**
 * Crypto protocol types shared between the pairing UI, relay client, and
 * test suite.
 *
 * Field naming mirrors the Rust side (`src-tauri/src/crypto/`) so the same
 * JSON payloads round-trip without a translation layer.
 */

export interface PairingPayload {
  /** Relay origin, e.g. `codeg-relay.example.workers.dev`. */
  relayOrigin: string
  /** Relay-assigned session identifier. Opaque to the crypto layer. */
  sessionId: string
  /**
   * Daemon's 32-byte long-term X25519 public key, hex-encoded. Stored as
   * hex (not raw bytes) because the pairing URL carries it that way.
   */
  serverPubHex: string
}

export interface E2eeHello {
  v: 1
  /** Client's ephemeral X25519 public key, hex-encoded. */
  clientPubHex: string
  /** Relay session id the daemon will use to route messages. */
  sessionId: string
}

export interface E2eeReady {
  v: 1
  /** Daemon's long-term X25519 public key, hex-encoded. */
  serverPubHex: string
}

export interface EphemeralKeyPair {
  publicKey: Uint8Array
  secretKey: Uint8Array
}
