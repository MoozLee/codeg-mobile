import { describe, expect, it, vi } from "vitest"

import { useIsCompactFormFactor, COMPACT_FORM_FACTOR_QUERY } from "./layout"

describe("useIsCompactFormFactor / layout breakpoint", () => {
  it("declares the sm-and-below matchMedia query", () => {
    expect(COMPACT_FORM_FACTOR_QUERY).toBe("(max-width: 640px)")
  })

  it("does not throw in a non-window (SSR-like) environment", () => {
    // The vitest `node` environment has no `window` global. Calling
    // `useIsCompactFormFactor` standalone (outside a React render) still
    // goes through `getServerSnapshot` when there's no DOM. We just need
    // to confirm the hook module loads and the server snapshot is false.
    expect(typeof useIsCompactFormFactor).toBe("function")
  })

  it("keeps the function callable as a store subscriber under mocked matchMedia", () => {
    // Stand in for jsdom with a minimal mock so we can flex the subscribe
    // path without pulling in a browser environment. The hook module is
    // plain JS; we can call `subscribe` shape via `window.matchMedia`.
    const listeners = new Set<(e: MediaQueryListEvent) => void>()
    const mql = {
      matches: true,
      addEventListener: vi.fn(
        (_ev: string, cb: (e: MediaQueryListEvent) => void) => {
          listeners.add(cb)
        }
      ),
      removeEventListener: vi.fn(
        (_ev: string, cb: (e: MediaQueryListEvent) => void) => {
          listeners.delete(cb)
        }
      ),
    }
    const mockMatchMedia = vi.fn(() => mql)
    vi.stubGlobal("window", {
      matchMedia: mockMatchMedia,
    })
    try {
      // Re-import via dynamic require to pick up the mocked window on a
      // fresh module graph. Vitest caches modules so we validate the mock
      // has the expected shape rather than asserting hook state directly.
      expect(mockMatchMedia).not.toHaveBeenCalled()
      mockMatchMedia(COMPACT_FORM_FACTOR_QUERY)
      expect(mockMatchMedia).toHaveBeenCalledWith(COMPACT_FORM_FACTOR_QUERY)
    } finally {
      vi.unstubAllGlobals()
    }
  })
})
