/**
 * End-to-end encryption protocol, TypeScript side.
 *
 * Mirrors the Rust implementation under `src-tauri/src/crypto/`. All wire
 * formats (frame layout, handshake JSON, pairing URL) are shared so the
 * phone frontend and the daemon can interoperate through the relay.
 *
 * Byte-level compatibility is verified by `src/lib/crypto/index.test.ts`
 * which loads the same fixture file as the Rust integration test in
 * `src-tauri/tests/crypto_fixtures_interop.rs`.
 */

import nacl from "tweetnacl"
import naclUtil from "tweetnacl-util"

import type {
  E2eeHello,
  E2eeReady,
  EphemeralKeyPair,
  PairingPayload,
} from "./types"

export const PAIRING_SCHEME = "codeg-pair://"
export const HANDSHAKE_VERSION = 1
export const NONCE_SIZE = 24
export const KEY_SIZE = 32
export const SHARED_KEY_SIZE = 32

/** Generate a random X25519 keypair suitable for a single relay session. */
export function generateEphemeralKeyPair(): EphemeralKeyPair {
  const kp = nacl.box.keyPair()
  return { publicKey: kp.publicKey, secretKey: kp.secretKey }
}

/**
 * Derive the NaCl `crypto_box_beforenm` precomputed key. Byte-for-byte
 * equivalent to `derive_shared()` on the Rust side (X25519 DH followed by
 * HSalsa20 with a zero 16-byte nonce).
 */
export function deriveShared(
  theirPub: Uint8Array,
  mySecret: Uint8Array
): Uint8Array {
  assertLength(theirPub, KEY_SIZE, "theirPub")
  assertLength(mySecret, KEY_SIZE, "mySecret")
  return nacl.box.before(theirPub, mySecret)
}

/**
 * Encrypt `plaintext` and return a base64 frame `[24-byte nonce || ciphertext]`.
 * Pass `nonceOverride` only from tests; at runtime use a fresh random nonce.
 */
export function encryptFrame(
  shared: Uint8Array,
  plaintext: Uint8Array,
  nonceOverride?: Uint8Array
): string {
  assertLength(shared, SHARED_KEY_SIZE, "shared")
  const nonce = nonceOverride ?? nacl.randomBytes(NONCE_SIZE)
  assertLength(nonce, NONCE_SIZE, "nonce")
  const ciphertext = nacl.box.after(plaintext, nonce, shared)
  const frame = new Uint8Array(NONCE_SIZE + ciphertext.length)
  frame.set(nonce, 0)
  frame.set(ciphertext, NONCE_SIZE)
  return naclUtil.encodeBase64(frame)
}

/**
 * Decode and decrypt a base64 frame produced by `encryptFrame`. Throws on
 * wrong key, tampered ciphertext, wrong nonce, or frames shorter than
 * `NONCE_SIZE`.
 */
export function decryptFrame(
  shared: Uint8Array,
  frameBase64: string
): Uint8Array {
  assertLength(shared, SHARED_KEY_SIZE, "shared")
  const raw = naclUtil.decodeBase64(frameBase64)
  if (raw.length < NONCE_SIZE) {
    throw new Error(
      `crypto frame too short: need ${NONCE_SIZE} bytes for nonce, got ${raw.length}`
    )
  }
  const nonce = raw.subarray(0, NONCE_SIZE)
  const ciphertext = raw.subarray(NONCE_SIZE)
  const plaintext = nacl.box.open.after(ciphertext, nonce, shared)
  if (plaintext === null) {
    throw new Error("crypto frame decryption failed")
  }
  return plaintext
}

/** Build a pairing URL for the given relay origin, session id, and daemon public key. */
export function buildPairingUrl(
  relayOrigin: string,
  sessionId: string,
  pubKey: Uint8Array
): string {
  assertLength(pubKey, KEY_SIZE, "pubKey")
  if (relayOrigin.length === 0) {
    throw new Error("buildPairingUrl: relayOrigin must not be empty")
  }
  if (sessionId.length === 0) {
    throw new Error("buildPairingUrl: sessionId must not be empty")
  }
  const fragment = base64UrlNoPadEncode(pubKey)
  return `${PAIRING_SCHEME}${relayOrigin}/${sessionId}#${fragment}`
}

/** Parse a pairing URL back into its components. Throws on malformed input. */
export function parsePairingUrl(url: string): PairingPayload {
  if (!url.startsWith(PAIRING_SCHEME)) {
    throw new Error(`parsePairingUrl: missing scheme ${PAIRING_SCHEME}: ${url}`)
  }
  const rest = url.slice(PAIRING_SCHEME.length)
  const hashIdx = rest.indexOf("#")
  if (hashIdx === -1) {
    throw new Error("parsePairingUrl: missing fragment")
  }
  const pathPart = rest.slice(0, hashIdx)
  const fragment = rest.slice(hashIdx + 1)

  const slashIdx = pathPart.indexOf("/")
  if (slashIdx === -1) {
    throw new Error("parsePairingUrl: missing /<session-id> segment")
  }
  const relayOrigin = pathPart.slice(0, slashIdx)
  const sessionId = pathPart.slice(slashIdx + 1)
  if (!relayOrigin) {
    throw new Error("parsePairingUrl: empty relay origin")
  }
  if (!sessionId) {
    throw new Error("parsePairingUrl: empty session id")
  }

  const pubBytes = base64UrlNoPadDecode(fragment)
  if (pubBytes.length !== KEY_SIZE) {
    throw new Error(
      `parsePairingUrl: fragment must decode to ${KEY_SIZE} bytes, got ${pubBytes.length}`
    )
  }

  return {
    relayOrigin,
    sessionId,
    serverPubHex: bytesToHex(pubBytes),
  }
}

/** Build the JSON-encoded `e2ee_hello` payload sent by the phone. */
export function buildHelloFrame(payload: E2eeHello): string {
  if (payload.v !== HANDSHAKE_VERSION) {
    throw new Error(`buildHelloFrame: unsupported version ${payload.v}`)
  }
  return JSON.stringify({
    v: payload.v,
    client_pub_hex: payload.clientPubHex,
    session_id: payload.sessionId,
  })
}

/** Parse an `e2ee_hello` JSON payload. Throws on malformed or wrong-version input. */
export function parseHelloFrame(json: string): E2eeHello {
  const raw = safeParseJson(json)
  if (!raw || typeof raw !== "object") {
    throw new Error("parseHelloFrame: not a json object")
  }
  const obj = raw as Record<string, unknown>
  const v = obj.v
  if (v !== HANDSHAKE_VERSION) {
    throw new Error(`parseHelloFrame: unsupported version ${String(v)}`)
  }
  const clientPubHex = obj.client_pub_hex
  const sessionId = obj.session_id
  if (typeof clientPubHex !== "string" || typeof sessionId !== "string") {
    throw new Error("parseHelloFrame: missing or wrong-type fields")
  }
  return {
    v: HANDSHAKE_VERSION,
    clientPubHex,
    sessionId,
  }
}

/** Build the JSON-encoded `e2ee_ready` payload sent by the daemon. */
export function buildReadyFrame(payload: E2eeReady): string {
  if (payload.v !== HANDSHAKE_VERSION) {
    throw new Error(`buildReadyFrame: unsupported version ${payload.v}`)
  }
  return JSON.stringify({
    v: payload.v,
    server_pub_hex: payload.serverPubHex,
  })
}

/** Parse an `e2ee_ready` JSON payload. Throws on malformed or wrong-version input. */
export function parseReadyFrame(json: string): E2eeReady {
  const raw = safeParseJson(json)
  if (!raw || typeof raw !== "object") {
    throw new Error("parseReadyFrame: not a json object")
  }
  const obj = raw as Record<string, unknown>
  const v = obj.v
  if (v !== HANDSHAKE_VERSION) {
    throw new Error(`parseReadyFrame: unsupported version ${String(v)}`)
  }
  const serverPubHex = obj.server_pub_hex
  if (typeof serverPubHex !== "string") {
    throw new Error("parseReadyFrame: missing or wrong-type server_pub_hex")
  }
  return {
    v: HANDSHAKE_VERSION,
    serverPubHex,
  }
}

// ---------------------------------------------------------------------------
// Internal helpers

function assertLength(
  bytes: Uint8Array,
  expected: number,
  label: string
): void {
  if (bytes.length !== expected) {
    throw new Error(`${label} must be ${expected} bytes, got ${bytes.length}`)
  }
}

function safeParseJson(json: string): unknown {
  try {
    return JSON.parse(json)
  } catch (err) {
    throw new Error(
      `invalid json handshake frame: ${
        err instanceof Error ? err.message : String(err)
      }`
    )
  }
}

function bytesToHex(bytes: Uint8Array): string {
  let out = ""
  for (let i = 0; i < bytes.length; i++) {
    out += bytes[i].toString(16).padStart(2, "0")
  }
  return out
}

/** Hex string to `Uint8Array`. Public so fixture loaders can share the helper. */
export function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) {
    throw new Error(`hexToBytes: odd-length hex string (${hex.length})`)
  }
  const out = new Uint8Array(hex.length / 2)
  for (let i = 0; i < out.length; i++) {
    const byte = parseInt(hex.slice(i * 2, i * 2 + 2), 16)
    if (Number.isNaN(byte)) {
      throw new Error(`hexToBytes: invalid hex at offset ${i * 2}`)
    }
    out[i] = byte
  }
  return out
}

function base64UrlNoPadEncode(bytes: Uint8Array): string {
  return naclUtil
    .encodeBase64(bytes)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "")
}

function base64UrlNoPadDecode(input: string): Uint8Array {
  const normalized = input.replace(/-/g, "+").replace(/_/g, "/")
  const paddingNeeded = (4 - (normalized.length % 4)) % 4
  const padded = normalized + "=".repeat(paddingNeeded)
  return naclUtil.decodeBase64(padded)
}

export type {
  E2eeHello,
  E2eeReady,
  EphemeralKeyPair,
  PairingPayload,
} from "./types"
