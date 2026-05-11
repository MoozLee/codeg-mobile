import {
  CLOSE_CODE_PEER_GONE,
  CLOSE_REASON_PEER_GONE,
  IDLE_TIMEOUT_MS,
  MAX_PENDING_MESSAGES,
  type Role,
} from "./types"

/**
 * Pair of WebSockets routed by `sessionId`. The object is the dumbest
 * possible forwarder: whatever one side sends is passed verbatim to the
 * other side. It does not parse the payload and never sees plaintext
 * because every application frame is end-to-end encrypted by the phone
 * and the daemon.
 */
export class RelaySession {
  private daemonWs: WebSocket | null = null
  private clientWs: WebSocket | null = null

  // Buffer messages that arrive before the peer connects. Bounded so a
  // stale session can't grow without limit.
  private pendingForDaemon: Array<string | ArrayBuffer> = []
  private pendingForClient: Array<string | ArrayBuffer> = []

  private idleTimerId: number | null = null

  // The Durable Object state / env parameters are unused today: the
  // forwarder is purely in-memory so there is no persistence or binding
  // to read. Kept in the signature so the runtime can instantiate the
  // class the same way any other Durable Object expects.
  constructor(_state: DurableObjectState, _env: unknown) {}

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url)
    const parts = url.pathname.split("/")
    const role = parts[3] as Role | undefined
    if (role !== "daemon" && role !== "client") {
      return new Response("bad role", { status: 400 })
    }

    if (request.headers.get("Upgrade") !== "websocket") {
      return new Response("expected websocket upgrade", { status: 400 })
    }

    const pair = new WebSocketPair()
    const server = pair[0]
    const client = pair[1]

    server.accept()
    this.attach(role, server)

    this.resetIdleTimer()

    return new Response(null, {
      status: 101,
      webSocket: client,
    })
  }

  private attach(role: Role, ws: WebSocket): void {
    // If the same role reconnects, evict the previous socket so the new
    // one wins. This keeps the session usable across network hiccups
    // without a formal re-registration protocol.
    if (role === "daemon") {
      if (this.daemonWs) {
        this.safeClose(this.daemonWs, 1000, "replaced")
      }
      this.daemonWs = ws
    } else {
      if (this.clientWs) {
        this.safeClose(this.clientWs, 1000, "replaced")
      }
      this.clientWs = ws
    }

    // Flush anything buffered for this role.
    const pending = role === "daemon" ? this.pendingForDaemon : this.pendingForClient
    if (pending.length > 0) {
      for (const msg of pending) {
        this.safeSend(ws, msg)
      }
      if (role === "daemon") {
        this.pendingForDaemon = []
      } else {
        this.pendingForClient = []
      }
    }

    ws.addEventListener("message", (event) => {
      this.onMessage(role, event.data as string | ArrayBuffer)
    })

    const onClose = () => {
      this.onPeerGone(role)
    }
    ws.addEventListener("close", onClose)
    ws.addEventListener("error", onClose)
  }

  private onMessage(from: Role, data: string | ArrayBuffer): void {
    this.resetIdleTimer()
    const peerRole: Role = from === "daemon" ? "client" : "daemon"
    const peer = peerRole === "daemon" ? this.daemonWs : this.clientWs
    if (peer) {
      this.safeSend(peer, data)
      return
    }
    // No peer yet: buffer up to the cap, then start dropping the oldest
    // messages. This matches "best effort before pairing completes".
    const buffer =
      peerRole === "daemon" ? this.pendingForDaemon : this.pendingForClient
    buffer.push(data)
    while (buffer.length > MAX_PENDING_MESSAGES) {
      buffer.shift()
    }
  }

  private onPeerGone(role: Role): void {
    if (role === "daemon") {
      this.daemonWs = null
      // Clear anything we would have forwarded into daemon — that traffic
      // has nowhere to go anymore.
      this.pendingForDaemon = []
      if (this.clientWs) {
        this.safeClose(this.clientWs, CLOSE_CODE_PEER_GONE, CLOSE_REASON_PEER_GONE)
        this.clientWs = null
      }
    } else {
      this.clientWs = null
      this.pendingForClient = []
      if (this.daemonWs) {
        this.safeClose(this.daemonWs, CLOSE_CODE_PEER_GONE, CLOSE_REASON_PEER_GONE)
        this.daemonWs = null
      }
    }
    if (!this.daemonWs && !this.clientWs) {
      this.clearIdleTimer()
    }
  }

  private resetIdleTimer(): void {
    this.clearIdleTimer()
    // setTimeout is available in Workers. Cast because DOM-lib typings
    // type it as number; Workers return an identifier we can clearTimeout.
    this.idleTimerId = setTimeout(() => {
      this.timeout()
    }, IDLE_TIMEOUT_MS) as unknown as number
  }

  private clearIdleTimer(): void {
    if (this.idleTimerId !== null) {
      clearTimeout(this.idleTimerId)
      this.idleTimerId = null
    }
  }

  private timeout(): void {
    if (this.daemonWs) {
      this.safeClose(this.daemonWs, 1000, "idle timeout")
      this.daemonWs = null
    }
    if (this.clientWs) {
      this.safeClose(this.clientWs, 1000, "idle timeout")
      this.clientWs = null
    }
    this.pendingForDaemon = []
    this.pendingForClient = []
  }

  private safeSend(ws: WebSocket, data: string | ArrayBuffer): void {
    try {
      ws.send(data)
    } catch {
      // Peer went away between our last read and this write. The close
      // listener will fire and drive the teardown.
    }
  }

  private safeClose(ws: WebSocket, code: number, reason: string): void {
    try {
      ws.close(code, reason)
    } catch {
      // Already closed.
    }
  }
}
