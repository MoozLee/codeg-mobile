import type { Env, ParsedRoute, Role } from "./types"

export { RelaySession } from "./session"

const ROLES = new Set<Role>(["daemon", "client"])

/**
 * Parse `/session/<sessionId>/<role>`. Returns `null` on any shape
 * mismatch so the caller can respond with a 400.
 */
function parseRoute(pathname: string): ParsedRoute | null {
  // Expect exactly 4 segments after splitting on "/": ["", "session", id, role]
  const parts = pathname.split("/")
  if (parts.length !== 4) return null
  if (parts[0] !== "" || parts[1] !== "session") return null
  const sessionId = parts[2]
  const role = parts[3]
  if (!sessionId) return null
  if (!ROLES.has(role as Role)) return null
  return { sessionId, role: role as Role }
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url)

    if (url.pathname === "/health") {
      return new Response("ok", { status: 200 })
    }

    const route = parseRoute(url.pathname)
    if (!route) {
      return new Response("not found", { status: 404 })
    }

    if (request.headers.get("Upgrade") !== "websocket") {
      return new Response("expected websocket upgrade", { status: 400 })
    }

    // Route every client of the same `sessionId` to the same Durable Object
    // instance so the daemon and phone end up colocated and can forward
    // messages to each other through in-memory state.
    const id = env.RELAY.idFromName(route.sessionId)
    const stub = env.RELAY.get(id)

    // Preserve the original URL so the Durable Object can inspect the role
    // segment. Other headers (Upgrade, Sec-WebSocket-Key, etc.) flow through
    // unchanged for the Worker runtime to complete the handshake.
    return stub.fetch(request)
  },
} satisfies ExportedHandler<Env>
