"use client"

/**
 * Phone-side relay transport.
 *
 * The mobile frontend does not talk to the desktop daemon directly. It
 * connects out to the relay WebSocket `wss://<relay>/session/<id>/client`
 * and runs the same E2EE handshake the daemon does (cleartext
 * `e2ee_hello` from the phone, cleartext `e2ee_ready` from the daemon,
 * then NaCl-box encrypted JSON frames).
 *
 * This module is deliberately framework-free and uses the browser's
 * native `WebSocket` so it also works inside Tauri mobile webview. It
 * exposes three surfaces:
 *
 *   - `scanPairingUrlFromWeb(url, nickname)` — first-time pairing;
 *     completes the handshake, sends `device_register`, waits for the
 *     ack, then returns a `HostProfile` suitable for the hosts store.
 *   - `openHostConnection(host)` — re-connect to an already-paired
 *     daemon. Returns a handle that can send `AppRequest` frames and
 *     emit decoded `AppResponse` frames.
 *   - `ensureWebSocketScheme(origin)` — URL helper shared with the
 *     pairing flow; prepends `wss://` when the pairing URL fragment did
 *     not include a scheme.
 */

import {
  buildHelloFrame,
  decryptFrame,
  deriveShared,
  encryptFrame,
  generateEphemeralKeyPair,
  hexToBytes,
  parsePairingUrl,
  parseReadyFrame,
} from "@/lib/crypto"
import type { HostProfile } from "@/stores/mobile-hosts-store"

/** Subset of `AppRequest` the phone is expected to send. Keep in sync
 * with `src-tauri/src/relay/dispatcher.rs`. */
export type AppRequest =
  | { type: "ping"; payload: string }
  | { type: "device_register"; nickname: string }
  | { type: "list_sessions" }
  | { type: "get_session"; session_id: string }
  | { type: "send_prompt"; session_id: string; text: string }
  | { type: "stop_session"; session_id: string }
  | { type: "new_session"; agent_type: string; cwd: string }
  | {
      type: "approval_response"
      request_id: string
      allow: boolean
    }

export interface SessionSummary {
  id: string
  title: string
  agent_type: string
  last_active_at: number
  status: string
}

export interface MessageEnvelope {
  id: string
  role: "user" | "assistant" | "system" | "tool"
  text: string
  timestamp: number
}

export type AppResponse =
  | { type: "pong"; payload: string }
  | { type: "device_register_ack"; device_id: string; server_version: string }
  | { type: "error"; message: string }
  | { type: "sessions_list"; sessions: SessionSummary[] }
  | {
      type: "session_detail"
      session_id: string
      title: string
      agent_type: string
      messages: MessageEnvelope[]
    }
  | { type: "prompt_ack"; message_id: string }
  | {
      type: "message_delta"
      session_id: string
      role: "assistant" | "user" | "tool" | "system"
      delta: string
    }
  | { type: "stop_ack"; session_id: string }
  | {
      type: "approval_required"
      request_id: string
      session_id: string
      summary: string
    }
  | { type: "new_session_created"; session_id: string }

const HANDSHAKE_VERSION = 1
/** Relay pairing lifetime matches the Rust default (10 minutes). We use
 * a tighter timeout for handshake RTT because the phone is already
 * online once it dials the relay. */
const HANDSHAKE_RTT_TIMEOUT_MS = 15_000

const utf8 = {
  encode: (s: string) => new TextEncoder().encode(s),
  decode: (b: Uint8Array) => new TextDecoder().decode(b),
}

/**
 * Prepend a WebSocket scheme when the pairing URL embeds just a host.
 * The daemon strips `wss://` / `ws://` before putting the host into the
 * pairing URL (see `strip_ws_scheme` in Rust), and we restore it here:
 * loopback hosts use `ws://` (self-signed / plain TCP for tests), every
 * other host uses `wss://`.
 */
export function ensureWebSocketScheme(origin: string): string {
  if (origin.startsWith("ws://") || origin.startsWith("wss://")) return origin
  const isLoopback =
    origin.startsWith("localhost") ||
    origin.startsWith("127.0.0.1") ||
    origin.startsWith("[::1]")
  return `${isLoopback ? "ws://" : "wss://"}${origin}`
}

function bytesToHex(bytes: Uint8Array): string {
  let out = ""
  for (let i = 0; i < bytes.length; i++)
    out += bytes[i].toString(16).padStart(2, "0")
  return out
}

function waitForMessage(
  ws: WebSocket,
  timeoutMs: number
): Promise<MessageEvent> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      cleanup()
      reject(new Error("relay handshake timed out"))
    }, timeoutMs)
    function onMessage(ev: MessageEvent) {
      cleanup()
      resolve(ev)
    }
    function onError() {
      cleanup()
      reject(new Error("relay websocket error"))
    }
    function onClose() {
      cleanup()
      reject(new Error("relay connection closed"))
    }
    function cleanup() {
      clearTimeout(timer)
      ws.removeEventListener("message", onMessage)
      ws.removeEventListener("error", onError)
      ws.removeEventListener("close", onClose)
    }
    ws.addEventListener("message", onMessage)
    ws.addEventListener("error", onError)
    ws.addEventListener("close", onClose)
  })
}

async function openClientSocket(relayOrigin: string, sessionId: string) {
  const url = `${ensureWebSocketScheme(relayOrigin).replace(/\/+$/, "")}/session/${sessionId}/client`
  const ws = new WebSocket(url)
  await new Promise<void>((resolve, reject) => {
    const onOpen = () => {
      ws.removeEventListener("open", onOpen)
      ws.removeEventListener("error", onError)
      resolve()
    }
    const onError = () => {
      ws.removeEventListener("open", onOpen)
      ws.removeEventListener("error", onError)
      reject(new Error("failed to connect to relay"))
    }
    ws.addEventListener("open", onOpen)
    ws.addEventListener("error", onError)
  })
  return ws
}

interface HandshakeResult {
  ws: WebSocket
  shared: Uint8Array
  clientPub: Uint8Array
  clientSecret: Uint8Array
  serverPubHex: string
}

async function runHandshake(
  relayOrigin: string,
  sessionId: string,
  pinnedServerPubHex?: string
): Promise<HandshakeResult> {
  const ws = await openClientSocket(relayOrigin, sessionId)
  try {
    const kp = generateEphemeralKeyPair()
    const hello = buildHelloFrame({
      v: HANDSHAKE_VERSION,
      clientPubHex: bytesToHex(kp.publicKey),
      sessionId,
    })
    ws.send(hello)

    const readyEvent = await waitForMessage(ws, HANDSHAKE_RTT_TIMEOUT_MS)
    if (typeof readyEvent.data !== "string") {
      throw new Error("relay returned non-text handshake frame")
    }
    const ready = parseReadyFrame(readyEvent.data)
    if (pinnedServerPubHex && ready.serverPubHex !== pinnedServerPubHex) {
      throw new Error("daemon public key does not match paired device")
    }
    const serverPub = hexToBytes(ready.serverPubHex)
    const shared = deriveShared(serverPub, kp.secretKey)
    return {
      ws,
      shared,
      clientPub: kp.publicKey,
      clientSecret: kp.secretKey,
      serverPubHex: ready.serverPubHex,
    }
  } catch (err) {
    try {
      ws.close()
    } catch {
      /* ignore */
    }
    throw err
  }
}

export interface ScanPairingResult {
  profile: HostProfile
  deviceId: string
  serverVersion: string
}

/**
 * Run the first-time pairing handshake. On success the returned
 * `profile` contains everything the hosts store needs to re-connect
 * without another scan.
 */
export async function scanPairingUrlFromWeb(
  pairingUrl: string,
  nickname: string
): Promise<ScanPairingResult> {
  const parsed = parsePairingUrl(pairingUrl)
  const { ws, shared, clientPub, clientSecret, serverPubHex } =
    await runHandshake(
      parsed.relayOrigin,
      parsed.sessionId,
      parsed.serverPubHex
    )

  try {
    const register: AppRequest = { type: "device_register", nickname }
    ws.send(encryptFrame(shared, utf8.encode(JSON.stringify(register))))

    const ackEvent = await waitForMessage(ws, HANDSHAKE_RTT_TIMEOUT_MS)
    if (typeof ackEvent.data !== "string") {
      throw new Error("relay returned non-text frame during pairing")
    }
    const ackPlain = decryptFrame(shared, ackEvent.data)
    const ackJson: unknown = JSON.parse(utf8.decode(ackPlain))
    if (!ackJson || typeof ackJson !== "object") {
      throw new Error("malformed pairing response")
    }
    const obj = ackJson as Record<string, unknown>
    if (obj.type !== "device_register_ack") {
      throw new Error(
        typeof obj.message === "string"
          ? `daemon refused pairing: ${obj.message}`
          : `unexpected pairing response: ${String(obj.type)}`
      )
    }
    const deviceId =
      typeof obj.device_id === "string" ? obj.device_id : undefined
    const serverVersion =
      typeof obj.server_version === "string" ? obj.server_version : ""
    if (!deviceId) throw new Error("pairing response missing device_id")

    const profile: HostProfile = {
      id: deviceId,
      nickname,
      relayOrigin: parsed.relayOrigin,
      sessionId: parsed.sessionId,
      sharedSecretHex: bytesToHex(shared),
      serverPubHex,
      clientPubHex: bytesToHex(clientPub),
      clientSecretHex: bytesToHex(clientSecret),
      addedAt: Date.now(),
    }
    return { profile, deviceId, serverVersion }
  } finally {
    try {
      ws.close()
    } catch {
      /* ignore */
    }
  }
}

/** Handle returned from `openHostConnection`. */
export interface HostConnection {
  send(req: AppRequest): void
  onMessage(handler: (msg: AppResponse) => void): () => void
  close(): void
  readonly ready: Promise<void>
}

/**
 * Re-open a WebSocket to a paired daemon using a stored
 * `HostProfile`. The shared secret lives in the profile so we do not
 * rerun the ECDH handshake; the daemon's Rust side treats the session
 * cookie as a reconnection token and will answer with a fresh
 * `e2ee_ready` any time the client dials in.
 *
 * MVP simplification: we run a full handshake on every reconnect so the
 * relay doesn't need sticky session cookies. The phone's ephemeral
 * keypair changes across connections; `sharedSecretHex` on the host
 * profile acts as a fallback for offline decoding of cached replies.
 */
export async function openHostConnection(
  host: HostProfile
): Promise<HostConnection> {
  const { ws, shared } = await runHandshake(
    host.relayOrigin,
    host.sessionId,
    host.serverPubHex
  )

  const handlers = new Set<(msg: AppResponse) => void>()

  ws.addEventListener("message", (ev) => {
    if (typeof ev.data !== "string") return
    try {
      const plain = decryptFrame(shared, ev.data)
      const json: unknown = JSON.parse(utf8.decode(plain))
      if (!json || typeof json !== "object") return
      const msg = json as AppResponse
      for (const h of handlers) h(msg)
    } catch {
      // Framing / decoding errors are non-fatal; a well-behaved daemon
      // will not send anything that fails to decode. Silently ignoring
      // matches the rust client's behaviour when it can't make sense of
      // an incoming frame either.
    }
  })

  let closed = false
  function close() {
    if (closed) return
    closed = true
    try {
      ws.close()
    } catch {
      /* ignore */
    }
  }
  ws.addEventListener("close", () => {
    closed = true
  })

  return {
    send(req) {
      if (closed) return
      const frame = encryptFrame(shared, utf8.encode(JSON.stringify(req)))
      ws.send(frame)
    },
    onMessage(handler) {
      handlers.add(handler)
      return () => handlers.delete(handler)
    },
    close,
    ready: Promise.resolve(),
  }
}
