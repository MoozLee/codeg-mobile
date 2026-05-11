import { describe, expect, it } from "vitest"

import {
  DEEP_LINK_SCHEME,
  buildPairDeepLink,
  buildSessionDeepLink,
  parseDeepLink,
} from "./deep-link"

describe("parseDeepLink", () => {
  it("parses session deep link with host and session", () => {
    const link = parseDeepLink("codeg://session?host=abc&session=42")
    expect(link).toEqual({ kind: "session", host: "abc", session: "42" })
  })

  it("parses session deep link without host (optional)", () => {
    const link = parseDeepLink("codeg://session?session=42")
    expect(link).toEqual({ kind: "session", session: "42" })
  })

  it("decodes percent-encoded session id values", () => {
    const link = parseDeepLink("codeg://session?host=my%20daemon&session=s%2F1")
    expect(link).toEqual({
      kind: "session",
      host: "my daemon",
      session: "s/1",
    })
  })

  it("parses pair deep link with URL-encoded pairing URL", () => {
    const pairingUrl = "codeg-pair://relay.example/abc#deadbeef"
    const link = parseDeepLink(
      `codeg://pair?url=${encodeURIComponent(pairingUrl)}`
    )
    expect(link).toEqual({ kind: "pair", pairingUrl })
  })

  it("accepts a trailing slash on the path segment", () => {
    const link = parseDeepLink("codeg://session/?session=42")
    expect(link).toEqual({ kind: "session", session: "42" })
  })

  it("returns null for an unknown scheme", () => {
    expect(parseDeepLink("https://codeg.app/session?session=42")).toBeNull()
    expect(parseDeepLink("paseo://session?session=42")).toBeNull()
  })

  it("returns null for an unknown path", () => {
    expect(parseDeepLink("codeg://unknown?session=42")).toBeNull()
  })

  it("returns null when session path is missing the session field", () => {
    expect(parseDeepLink("codeg://session?host=abc")).toBeNull()
  })

  it("returns null when pair path is missing the url field", () => {
    expect(parseDeepLink("codeg://pair?other=1")).toBeNull()
  })

  it("returns null for non-string inputs", () => {
    expect(parseDeepLink(undefined as unknown as string)).toBeNull()
    expect(parseDeepLink(null as unknown as string)).toBeNull()
  })

  it("returns null for plain garbage input", () => {
    expect(parseDeepLink("not a url")).toBeNull()
    expect(parseDeepLink("")).toBeNull()
  })

  it("exposes the scheme constant used by builders", () => {
    expect(DEEP_LINK_SCHEME).toBe("codeg://")
  })
})

describe("deep-link builders", () => {
  it("buildSessionDeepLink round-trips through parseDeepLink", () => {
    const url = buildSessionDeepLink({ host: "laptop", session: "s/42" })
    expect(url.startsWith(DEEP_LINK_SCHEME)).toBe(true)
    const parsed = parseDeepLink(url)
    expect(parsed).toEqual({ kind: "session", host: "laptop", session: "s/42" })
  })

  it("buildSessionDeepLink omits host when absent", () => {
    const url = buildSessionDeepLink({ session: "42" })
    expect(url).toContain("session=42")
    expect(url).not.toContain("host=")
    const parsed = parseDeepLink(url)
    expect(parsed).toEqual({ kind: "session", session: "42" })
  })

  it("buildPairDeepLink round-trips through parseDeepLink", () => {
    const pairingUrl = "codeg-pair://relay.example/abc#deadbeef"
    const url = buildPairDeepLink(pairingUrl)
    const parsed = parseDeepLink(url)
    expect(parsed).toEqual({ kind: "pair", pairingUrl })
  })
})
