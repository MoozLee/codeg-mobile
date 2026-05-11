import { describe, expect, it } from "vitest"

import {
  buildHelloFrame,
  buildPairingUrl,
  buildReadyFrame,
  decryptFrame,
  deriveShared,
  encryptFrame,
  generateEphemeralKeyPair,
  hexToBytes,
  parseHelloFrame,
  parsePairingUrl,
  parseReadyFrame,
} from "./index"
import { FIXTURE_VECTORS } from "./fixtures"

const utf8 = new TextEncoder()

describe("keypair", () => {
  it("generates 32-byte public + secret", () => {
    const kp = generateEphemeralKeyPair()
    expect(kp.publicKey.length).toBe(32)
    expect(kp.secretKey.length).toBe(32)
  })

  it("generates distinct keypairs on each call", () => {
    const a = generateEphemeralKeyPair()
    const b = generateEphemeralKeyPair()
    expect(a.secretKey).not.toEqual(b.secretKey)
    expect(a.publicKey).not.toEqual(b.publicKey)
  })
})

describe("deriveShared", () => {
  it("is symmetric: ECDH gives the same key from either side", () => {
    const alice = generateEphemeralKeyPair()
    const bob = generateEphemeralKeyPair()
    const sharedFromAlice = deriveShared(bob.publicKey, alice.secretKey)
    const sharedFromBob = deriveShared(alice.publicKey, bob.secretKey)
    expect(sharedFromAlice).toEqual(sharedFromBob)
    expect(sharedFromAlice.length).toBe(32)
  })
})

describe("encrypt/decrypt frame", () => {
  it("round-trips plaintext", () => {
    const alice = generateEphemeralKeyPair()
    const bob = generateEphemeralKeyPair()
    const sharedA = deriveShared(bob.publicKey, alice.secretKey)
    const sharedB = deriveShared(alice.publicKey, bob.secretKey)

    const plaintext = utf8.encode("hello codeg")
    const frame = encryptFrame(sharedA, plaintext)
    const decoded = decryptFrame(sharedB, frame)
    expect(new TextDecoder().decode(decoded)).toBe("hello codeg")
  })

  it("is deterministic when nonce is fixed", () => {
    const shared = new Uint8Array(32).fill(7)
    const nonce = new Uint8Array(24).fill(9)
    const a = encryptFrame(shared, utf8.encode("abc"), nonce)
    const b = encryptFrame(shared, utf8.encode("abc"), nonce)
    expect(a).toBe(b)
  })

  it("throws on tampered ciphertext", () => {
    const alice = generateEphemeralKeyPair()
    const bob = generateEphemeralKeyPair()
    const sharedA = deriveShared(bob.publicKey, alice.secretKey)
    const sharedB = deriveShared(alice.publicKey, bob.secretKey)

    const frame = encryptFrame(sharedA, utf8.encode("payload"))
    const lastChar = frame.charAt(frame.length - 1)
    const tampered = frame.slice(0, -1) + (lastChar === "A" ? "B" : "A")

    expect(() => decryptFrame(sharedB, tampered)).toThrow(/decryption failed/i)
  })

  it("throws on truncated frames", () => {
    const shared = new Uint8Array(32)
    expect(() => decryptFrame(shared, "AAAA")).toThrow(/too short/i)
  })

  it("rejects wrong-length shared keys", () => {
    const bad = new Uint8Array(16)
    expect(() => encryptFrame(bad, utf8.encode("x"))).toThrow(
      /shared must be 32 bytes/
    )
  })
})

describe("pairing URL", () => {
  it("round-trips origin, session, and pub key", () => {
    const pubKey = new Uint8Array(32).fill(1)
    const url = buildPairingUrl("relay.example.dev", "sess-abc", pubKey)
    expect(url.startsWith("codeg-pair://")).toBe(true)
    const parsed = parsePairingUrl(url)
    expect(parsed.relayOrigin).toBe("relay.example.dev")
    expect(parsed.sessionId).toBe("sess-abc")
    expect(parsed.serverPubHex).toBe("01".repeat(32))
  })

  it("rejects wrong scheme", () => {
    expect(() => parsePairingUrl("https://relay.example.dev/sess#abc")).toThrow(
      /scheme/
    )
  })

  it("rejects missing fragment", () => {
    expect(() =>
      parsePairingUrl("codeg-pair://relay.example.dev/sess")
    ).toThrow(/fragment/)
  })

  it("rejects wrong-length fragment", () => {
    expect(() =>
      parsePairingUrl("codeg-pair://relay.example.dev/sess#AA")
    ).toThrow(/32 bytes/)
  })

  it("rejects empty origin or session", () => {
    expect(() => buildPairingUrl("", "sess", new Uint8Array(32))).toThrow(
      /relayOrigin/
    )
    expect(() =>
      buildPairingUrl("relay.example.dev", "", new Uint8Array(32))
    ).toThrow(/sessionId/)
  })
})

describe("handshake frames", () => {
  it("hello frame round-trips", () => {
    const encoded = buildHelloFrame({
      v: 1,
      clientPubHex: "ab".repeat(32),
      sessionId: "s1",
    })
    const parsed = parseHelloFrame(encoded)
    expect(parsed).toEqual({
      v: 1,
      clientPubHex: "ab".repeat(32),
      sessionId: "s1",
    })
  })

  it("ready frame round-trips", () => {
    const encoded = buildReadyFrame({
      v: 1,
      serverPubHex: "cd".repeat(32),
    })
    const parsed = parseReadyFrame(encoded)
    expect(parsed).toEqual({ v: 1, serverPubHex: "cd".repeat(32) })
  })

  it("hello rejects wrong version", () => {
    expect(() =>
      parseHelloFrame(
        JSON.stringify({
          v: 2,
          client_pub_hex: "00",
          session_id: "s",
        })
      )
    ).toThrow(/version/)
  })

  it("ready rejects wrong version", () => {
    expect(() =>
      parseReadyFrame(JSON.stringify({ v: 2, server_pub_hex: "00" }))
    ).toThrow(/version/)
  })
})

describe("shared fixtures (interop with Rust)", () => {
  it("each fixture has well-formed hex", () => {
    for (const v of FIXTURE_VECTORS) {
      expect(v.client_secret_hex).toHaveLength(64)
      expect(v.server_secret_hex).toHaveLength(64)
      expect(v.client_pub_hex).toHaveLength(64)
      expect(v.server_pub_hex).toHaveLength(64)
      expect(v.nonce_hex).toHaveLength(48)
    }
  })

  it("client-side encrypt reproduces the expected frame", () => {
    for (const v of FIXTURE_VECTORS) {
      const clientSecret = hexToBytes(v.client_secret_hex)
      const serverPub = hexToBytes(v.server_pub_hex)
      const nonce = hexToBytes(v.nonce_hex)

      const shared = deriveShared(serverPub, clientSecret)
      const frame = encryptFrame(shared, utf8.encode(v.plaintext_utf8), nonce)
      expect(frame).toBe(v.expected_frame_base64)
    }
  })

  it("server-side decrypt recovers the plaintext", () => {
    for (const v of FIXTURE_VECTORS) {
      const serverSecret = hexToBytes(v.server_secret_hex)
      const clientPub = hexToBytes(v.client_pub_hex)
      const shared = deriveShared(clientPub, serverSecret)
      const decoded = decryptFrame(shared, v.expected_frame_base64)
      expect(new TextDecoder().decode(decoded)).toBe(v.plaintext_utf8)
    }
  })

  it("derive_shared is symmetric for each fixture", () => {
    for (const v of FIXTURE_VECTORS) {
      const clientSecret = hexToBytes(v.client_secret_hex)
      const serverSecret = hexToBytes(v.server_secret_hex)
      const clientPub = hexToBytes(v.client_pub_hex)
      const serverPub = hexToBytes(v.server_pub_hex)

      const fromClient = deriveShared(serverPub, clientSecret)
      const fromServer = deriveShared(clientPub, serverSecret)
      expect(fromClient).toEqual(fromServer)
    }
  })
})
