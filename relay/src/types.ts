export type Role = "daemon" | "client"

export interface Env {
  RELAY: DurableObjectNamespace
}

/** Parsed `/session/<id>/<role>` URL. */
export interface ParsedRoute {
  sessionId: string
  role: Role
}

export const CLOSE_CODE_PEER_GONE = 1001
export const CLOSE_REASON_PEER_GONE = "peer gone"

/**
 * Drop a buffered message if more than this many accumulate before the
 * peer connects. Keeps memory bounded even if one side connects much
 * later than the other.
 */
export const MAX_PENDING_MESSAGES = 16

/**
 * Tear down the Durable Object if no message flows for this long. The
 * timer is reset on every forwarded frame. 15 minutes matches the PRD.
 */
export const IDLE_TIMEOUT_MS = 15 * 60 * 1000
