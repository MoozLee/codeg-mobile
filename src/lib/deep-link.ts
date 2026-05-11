"use client"

/**
 * `codeg://` deep-link parser and Tauri/web subscription helper.
 *
 * Two link shapes are supported:
 *
 *   - `codeg://session?host=<host-id>&session=<session-id>` — jumps to a
 *     specific session on a paired daemon. `host` is optional; when absent,
 *     the mobile shell falls back to the currently active host.
 *   - `codeg://pair?url=<url-encoded-pairing-url>` — pre-fills the pairing
 *     page with a ready-to-accept pairing URL so the user can confirm the
 *     nickname and connect without scanning a QR code.
 *
 * On Tauri mobile the Rust shell registers the URL scheme and emits a
 * `"deep-link"` app event carrying the raw URL string. [`subscribeDeepLinks`]
 * wires that event into a typed handler; it also listens for `hashchange`
 * so web-only flows (chat channel links opened in a browser) still work.
 */

export interface SessionDeepLink {
  kind: "session"
  host?: string
  session: string
}

export interface PairDeepLink {
  kind: "pair"
  pairingUrl: string
}

export type DeepLink = SessionDeepLink | PairDeepLink

/** Canonical scheme the iOS shell registers with via `CFBundleURLTypes`. */
export const DEEP_LINK_SCHEME = "codeg://"

/** Parse a `codeg://...` URL into a typed deep link.
 *
 * Returns `null` when the URL is not a valid codeg deep link so callers can
 * silently ignore stray URL events (e.g. the webview receiving a generic
 * `about:blank` or a malformed OS-delivered URL).
 */
export function parseDeepLink(url: string): DeepLink | null {
  if (typeof url !== "string") return null
  if (!url.startsWith(DEEP_LINK_SCHEME)) return null

  const rest = url.slice(DEEP_LINK_SCHEME.length)
  // Split path from query ignoring any trailing fragment.
  const hashIdx = rest.indexOf("#")
  const withoutHash = hashIdx >= 0 ? rest.slice(0, hashIdx) : rest
  const qIdx = withoutHash.indexOf("?")
  const path = qIdx >= 0 ? withoutHash.slice(0, qIdx) : withoutHash
  const queryString = qIdx >= 0 ? withoutHash.slice(qIdx + 1) : ""

  // Strip a trailing slash on the path so `codeg://session/` also parses.
  const normalizedPath = path.replace(/\/+$/, "")

  const params = parseQuery(queryString)

  switch (normalizedPath) {
    case "session": {
      const session = params.get("session")
      if (!session) return null
      const host = params.get("host") ?? undefined
      return { kind: "session", session, host }
    }
    case "pair": {
      const pairingUrl = params.get("url")
      if (!pairingUrl) return null
      return { kind: "pair", pairingUrl }
    }
    default:
      return null
  }
}

function parseQuery(query: string): Map<string, string> {
  const out = new Map<string, string>()
  if (!query) return out
  for (const piece of query.split("&")) {
    if (!piece) continue
    const eq = piece.indexOf("=")
    const rawKey = eq >= 0 ? piece.slice(0, eq) : piece
    const rawVal = eq >= 0 ? piece.slice(eq + 1) : ""
    try {
      out.set(decodeURIComponent(rawKey), decodeURIComponent(rawVal))
    } catch {
      // Malformed encoding — skip the pair rather than throwing.
    }
  }
  return out
}

/** Build a `codeg://session` URL with URI-encoded values. */
export function buildSessionDeepLink(params: {
  session: string
  host?: string
}): string {
  const qs = new URLSearchParams()
  if (params.host) qs.set("host", params.host)
  qs.set("session", params.session)
  return `${DEEP_LINK_SCHEME}session?${qs.toString()}`
}

/** Build a `codeg://pair` URL carrying a pairing URL. */
export function buildPairDeepLink(pairingUrl: string): string {
  const qs = new URLSearchParams({ url: pairingUrl })
  return `${DEEP_LINK_SCHEME}pair?${qs.toString()}`
}

/** Subscribe to deep-link events. Returns an async unsubscribe function.
 *
 * Priority order for sources:
 * 1. Tauri `@tauri-apps/api/event#listen("deep-link", ...)` when running
 *    inside the mobile shell (the Rust side forwards the plugin event).
 * 2. The initial `window.location` hash / search for web fallbacks when the
 *    page was opened via a rendered `codeg://` link inside the webview.
 */
export async function subscribeDeepLinks(
  handler: (link: DeepLink) => void
): Promise<() => void> {
  const disposers: Array<() => void | Promise<void>> = []

  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    try {
      const { listen } = await import("@tauri-apps/api/event")
      const unlisten = await listen<string>("deep-link", (event) => {
        if (typeof event.payload !== "string") return
        const link = parseDeepLink(event.payload)
        if (link) handler(link)
      })
      disposers.push(unlisten)
    } catch (err) {
      // Tauri event module is not available in dev fallbacks — swallow so
      // the web-side fallback below still runs.
      console.warn("[deep-link] failed to subscribe via Tauri event:", err)
    }
  }

  // Fire once for any pending `codeg://...` URL sitting in the hash or
  // search string so the mobile shell can act on a cold-start link.
  if (typeof window !== "undefined") {
    const pending = readPendingDeepLinkFromLocation(window.location)
    if (pending) handler(pending)
    const onHash = () => {
      const next = readPendingDeepLinkFromLocation(window.location)
      if (next) handler(next)
    }
    window.addEventListener("hashchange", onHash)
    disposers.push(() => window.removeEventListener("hashchange", onHash))
  }

  return () => {
    for (const d of disposers) {
      try {
        void d()
      } catch {
        /* ignore cleanup errors */
      }
    }
  }
}

function readPendingDeepLinkFromLocation(location: Location): DeepLink | null {
  // Look for `?open=codeg%3A%2F%2F...` or a hash containing the scheme.
  const search = new URLSearchParams(location.search)
  const openParam = search.get("open")
  if (openParam) {
    const link = parseDeepLink(openParam)
    if (link) return link
  }
  const hash = location.hash.startsWith("#") ? location.hash.slice(1) : ""
  if (hash.startsWith(DEEP_LINK_SCHEME)) {
    return parseDeepLink(hash)
  }
  return null
}
