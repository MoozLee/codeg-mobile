"use client"

import { useSyncExternalStore } from "react"

/**
 * Tailwind `sm` breakpoint. Screens at or below this width render the
 * mobile single-panel shell; wider screens keep the desktop layout.
 */
export const COMPACT_FORM_FACTOR_QUERY = "(max-width: 640px)"

/**
 * Returns `true` when the viewport is narrower than Tailwind's `sm`
 * breakpoint (641px). SSR snapshot is `false` so the desktop layout is
 * rendered on the server, matching static export expectations.
 */
export function useIsCompactFormFactor(): boolean {
  return useSyncExternalStore(
    subscribeMatchMedia,
    getCompactSnapshot,
    getServerSnapshot
  )
}

function subscribeMatchMedia(callback: () => void): () => void {
  if (typeof window === "undefined") {
    return () => {}
  }
  const mql = window.matchMedia(COMPACT_FORM_FACTOR_QUERY)
  mql.addEventListener("change", callback)
  return () => mql.removeEventListener("change", callback)
}

function getCompactSnapshot(): boolean {
  if (typeof window === "undefined") return false
  return window.matchMedia(COMPACT_FORM_FACTOR_QUERY).matches
}

function getServerSnapshot(): boolean {
  return false
}
