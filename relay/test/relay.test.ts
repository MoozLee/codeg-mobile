import { SELF } from "cloudflare:test"
import { describe, expect, it } from "vitest"

function openWs(sessionId: string, role: "daemon" | "client"): Promise<WebSocket> {
  return new Promise((resolve, reject) => {
    const req = new Request(
      `https://relay.example/session/${sessionId}/${role}`,
      { headers: { Upgrade: "websocket" } }
    )
    SELF.fetch(req)
      .then((res) => {
        const ws = res.webSocket
        if (!ws) {
          reject(new Error(`upgrade failed: ${res.status}`))
          return
        }
        ws.accept()
        resolve(ws)
      })
      .catch(reject)
  })
}

function nextMessage(ws: WebSocket): Promise<string> {
  return new Promise((resolve, reject) => {
    const onMsg = (event: MessageEvent) => {
      ws.removeEventListener("message", onMsg)
      ws.removeEventListener("close", onClose)
      resolve(
        typeof event.data === "string" ? event.data : new TextDecoder().decode(event.data as ArrayBuffer)
      )
    }
    const onClose = () => {
      ws.removeEventListener("message", onMsg)
      ws.removeEventListener("close", onClose)
      reject(new Error("closed before message"))
    }
    ws.addEventListener("message", onMsg)
    ws.addEventListener("close", onClose)
  })
}

function nextClose(ws: WebSocket): Promise<{ code: number; reason: string }> {
  return new Promise((resolve) => {
    const onClose = (event: CloseEvent) => {
      ws.removeEventListener("close", onClose)
      resolve({ code: event.code, reason: event.reason })
    }
    ws.addEventListener("close", onClose)
  })
}

describe("relay forwarding", () => {
  it("returns 404 for unknown paths", async () => {
    const res = await SELF.fetch("https://relay.example/nope")
    expect(res.status).toBe(404)
  })

  it("returns 400 when upgrade header is missing", async () => {
    const res = await SELF.fetch("https://relay.example/session/s1/daemon")
    expect(res.status).toBe(400)
  })

  it("rejects an unknown role", async () => {
    const res = await SELF.fetch(
      "https://relay.example/session/s1/eavesdropper",
      { headers: { Upgrade: "websocket" } }
    )
    expect(res.status).toBe(404)
  })

  it("forwards messages in both directions", async () => {
    const sessionId = "forward-session"
    const daemon = await openWs(sessionId, "daemon")
    const client = await openWs(sessionId, "client")

    daemon.send("hello from daemon")
    expect(await nextMessage(client)).toBe("hello from daemon")

    client.send("hello from client")
    expect(await nextMessage(daemon)).toBe("hello from client")

    daemon.close(1000, "done")
    client.close(1000, "done")
  })

  it("buffers early messages until the peer connects", async () => {
    const sessionId = "buffered-session"
    const daemon = await openWs(sessionId, "daemon")

    daemon.send("one")
    daemon.send("two")

    const client = await openWs(sessionId, "client")
    expect(await nextMessage(client)).toBe("one")
    expect(await nextMessage(client)).toBe("two")

    daemon.close(1000, "done")
    client.close(1000, "done")
  })

  it("closes the peer with code 1001 when one side drops", async () => {
    const sessionId = "peer-gone-session"
    const daemon = await openWs(sessionId, "daemon")
    const client = await openWs(sessionId, "client")

    const closed = nextClose(client)
    daemon.close(1000, "bye")

    const ev = await closed
    expect(ev.code).toBe(1001)
    expect(ev.reason).toBe("peer gone")
  })
})
