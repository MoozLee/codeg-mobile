"use client"

import { useSyncExternalStore } from "react"

/**
 * Persistent profile for one paired daemon. Mirrors what the pairing
 * handshake produces on the phone side: the session the daemon is
 * listening on, the derived shared secret (hex), and the daemon's public
 * key. These are enough to reopen an E2EE relay connection without
 * running the pairing flow again.
 *
 * `sharedSecretHex` is the crypto_box precomputed shared key
 * (`nacl.box.before`). It is 32 bytes hex-encoded (64 chars).
 * `serverPubHex` is the daemon's long-term public key as hex.
 * `clientSecretHex` / `clientPubHex` are the phone's ephemeral keypair,
 * retained so the handshake can be re-run if the relay session resets.
 *
 * MVP stores this in `localStorage`. P8 swaps it out for Stronghold +
 * biometric unlock. The shape is intentionally minimal so the P8
 * migration can add fields without invalidating existing rows.
 */
export interface HostProfile {
  id: string
  nickname: string
  relayOrigin: string
  sessionId: string
  sharedSecretHex: string
  serverPubHex: string
  clientPubHex: string
  clientSecretHex: string
  addedAt: number
}

export interface MobileHostsState {
  hosts: HostProfile[]
  activeHostId: string | null
}

const STORAGE_KEY = "codeg:mobile-hosts"

let state: MobileHostsState = { hosts: [], activeHostId: null }
let hydrated = false
const listeners = new Set<() => void>()

function loadFromStorage(): MobileHostsState {
  if (typeof window === "undefined") {
    return { hosts: [], activeHostId: null }
  }
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY)
    if (!raw) return { hosts: [], activeHostId: null }
    const parsed: unknown = JSON.parse(raw)
    if (!parsed || typeof parsed !== "object") {
      return { hosts: [], activeHostId: null }
    }
    const obj = parsed as Record<string, unknown>
    const rawHosts = Array.isArray(obj.hosts) ? obj.hosts : []
    const hosts: HostProfile[] = []
    for (const item of rawHosts) {
      const h = asHostProfile(item)
      if (h) hosts.push(h)
    }
    const activeHostId =
      typeof obj.activeHostId === "string" &&
      hosts.some((h) => h.id === obj.activeHostId)
        ? obj.activeHostId
        : (hosts[0]?.id ?? null)
    return { hosts, activeHostId }
  } catch {
    return { hosts: [], activeHostId: null }
  }
}

function asHostProfile(v: unknown): HostProfile | null {
  if (!v || typeof v !== "object") return null
  const o = v as Record<string, unknown>
  const required: (keyof HostProfile)[] = [
    "id",
    "nickname",
    "relayOrigin",
    "sessionId",
    "sharedSecretHex",
    "serverPubHex",
    "clientPubHex",
    "clientSecretHex",
  ]
  for (const k of required) {
    if (typeof o[k] !== "string") return null
  }
  return {
    id: String(o.id),
    nickname: String(o.nickname),
    relayOrigin: String(o.relayOrigin),
    sessionId: String(o.sessionId),
    sharedSecretHex: String(o.sharedSecretHex),
    serverPubHex: String(o.serverPubHex),
    clientPubHex: String(o.clientPubHex),
    clientSecretHex: String(o.clientSecretHex),
    addedAt: typeof o.addedAt === "number" ? o.addedAt : Date.now(),
  }
}

function persist(next: MobileHostsState) {
  if (typeof window === "undefined") return
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next))
  } catch {
    // Storage may be unavailable (quota / private mode); the in-memory
    // copy still works for the current session.
  }
}

function emit() {
  for (const cb of listeners) cb()
}

function ensureHydrated() {
  if (hydrated) return
  hydrated = true
  if (typeof window === "undefined") return
  const loaded = loadFromStorage()
  state = loaded
}

function setState(next: MobileHostsState) {
  state = next
  persist(next)
  emit()
}

function subscribe(cb: () => void): () => void {
  ensureHydrated()
  listeners.add(cb)
  return () => {
    listeners.delete(cb)
  }
}

function getSnapshot(): MobileHostsState {
  ensureHydrated()
  return state
}

function getServerSnapshot(): MobileHostsState {
  return { hosts: [], activeHostId: null }
}

export interface MobileHostsActions {
  addHost: (profile: Omit<HostProfile, "addedAt">) => HostProfile
  removeHost: (id: string) => void
  setActiveHost: (id: string | null) => void
  renameHost: (id: string, nickname: string) => void
  /** Replace the shared secret / session id after a re-pair. */
  updateHostConnection: (
    id: string,
    patch: Pick<HostProfile, "sessionId" | "sharedSecretHex">
  ) => void
  getActiveHost: () => HostProfile | null
  getHosts: () => HostProfile[]
}

const actions: MobileHostsActions = {
  addHost(profile) {
    ensureHydrated()
    const full: HostProfile = { ...profile, addedAt: Date.now() }
    const existingIdx = state.hosts.findIndex((h) => h.id === full.id)
    const nextHosts =
      existingIdx >= 0
        ? state.hosts.map((h) => (h.id === full.id ? full : h))
        : [...state.hosts, full]
    const activeHostId = state.activeHostId ?? full.id
    setState({ hosts: nextHosts, activeHostId })
    return full
  },
  removeHost(id) {
    ensureHydrated()
    const nextHosts = state.hosts.filter((h) => h.id !== id)
    const activeHostId =
      state.activeHostId === id
        ? (nextHosts[0]?.id ?? null)
        : state.activeHostId
    setState({ hosts: nextHosts, activeHostId })
  },
  setActiveHost(id) {
    ensureHydrated()
    if (id !== null && !state.hosts.some((h) => h.id === id)) return
    setState({ ...state, activeHostId: id })
  },
  renameHost(id, nickname) {
    ensureHydrated()
    const trimmed = nickname.trim()
    if (!trimmed) return
    const hosts = state.hosts.map((h) =>
      h.id === id ? { ...h, nickname: trimmed } : h
    )
    setState({ ...state, hosts })
  },
  updateHostConnection(id, patch) {
    ensureHydrated()
    const hosts = state.hosts.map((h) => (h.id === id ? { ...h, ...patch } : h))
    setState({ ...state, hosts })
  },
  getActiveHost() {
    ensureHydrated()
    return (
      state.hosts.find((h) => h.id === state.activeHostId) ??
      state.hosts[0] ??
      null
    )
  },
  getHosts() {
    ensureHydrated()
    return state.hosts
  },
}

export function useMobileHostsStore(): MobileHostsState {
  return useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot)
}

export function useMobileHostsActions(): MobileHostsActions {
  return actions
}

/** Access actions without subscribing to state changes. */
export function getMobileHostsActions(): MobileHostsActions {
  return actions
}

/** Test-only: clear in-memory + storage state. */
export function __resetMobileHostsStoreForTests(): void {
  hydrated = false
  state = { hosts: [], activeHostId: null }
  if (typeof window !== "undefined") {
    try {
      window.localStorage.removeItem(STORAGE_KEY)
    } catch {
      // ignore
    }
  }
  emit()
}
