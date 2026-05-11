"use client"

import { Suspense, useCallback, useEffect, useRef, useState } from "react"
import { useSearchParams, useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { MobileShell } from "@/components/mobile/mobile-shell"
import { MobileMessageStream } from "@/components/mobile/mobile-message-stream"
import { MobileComposer } from "@/components/mobile/mobile-composer"
import {
  MobileApprovalDialog,
  type MobileApprovalPayload,
} from "@/components/mobile/mobile-approval-dialog"
import { Button } from "@/components/ui/button"
import {
  openHostConnection,
  type AppResponse,
  type HostConnection,
  type MessageEnvelope,
} from "@/lib/mobile-transport"
import {
  useMobileHostsActions,
  useMobileHostsStore,
} from "@/stores/mobile-hosts-store"

function MobileSessionInner() {
  const t = useTranslations("mobile")
  const params = useSearchParams()
  const router = useRouter()
  const hostParam = params?.get("host") ?? null
  const sessionId = params?.get("session") ?? null
  const { hosts } = useMobileHostsStore()
  const { setActiveHost } = useMobileHostsActions()

  const [title, setTitle] = useState<string>("")
  const [messages, setMessages] = useState<MessageEnvelope[]>([])
  const [streaming, setStreaming] = useState<string>("")
  const [generating, setGenerating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [approval, setApproval] = useState<MobileApprovalPayload | null>(null)
  const connRef = useRef<HostConnection | null>(null)

  const host = hosts.find((h) => h.id === hostParam) ?? null

  // Sync the active host so the top bar matches the session's daemon.
  useEffect(() => {
    if (hostParam && hosts.some((h) => h.id === hostParam)) {
      setActiveHost(hostParam)
    }
  }, [hostParam, hosts, setActiveHost])

  useEffect(() => {
    if (!host || !sessionId) return
    let cancelled = false
    /* eslint-disable react-hooks/set-state-in-effect -- reset state on session change, then stream async results */
    setError(null)
    setMessages([])
    setStreaming("")
    setGenerating(false)

    openHostConnection(host)
      .then((conn) => {
        if (cancelled) {
          conn.close()
          return
        }
        connRef.current = conn
        conn.onMessage((msg: AppResponse) => {
          switch (msg.type) {
            case "session_detail":
              if (msg.session_id === sessionId) {
                setTitle(msg.title)
                setMessages(msg.messages)
              }
              break
            case "message_delta":
              if (msg.session_id !== sessionId) return
              if (msg.role === "assistant") {
                setStreaming((prev) => prev + msg.delta)
                setGenerating(true)
              } else {
                // user / tool / system: append as fresh bubble
                setMessages((prev) => [
                  ...prev,
                  {
                    id: `${Date.now()}-${Math.random()}`,
                    role: msg.role,
                    text: msg.delta,
                    timestamp: Date.now() / 1000,
                  },
                ])
              }
              break
            case "prompt_ack":
              setGenerating(true)
              break
            case "stop_ack":
              setGenerating(false)
              break
            case "approval_required":
              if (msg.session_id === sessionId) {
                setApproval({
                  request_id: msg.request_id,
                  session_id: msg.session_id,
                  summary: msg.summary,
                })
              }
              break
            case "error":
              setError(msg.message)
              break
            default:
              break
          }
        })
        conn.send({ type: "get_session", session_id: sessionId })
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setError(err instanceof Error ? err.message : String(err))
      })

    return () => {
      cancelled = true
      connRef.current?.close()
      connRef.current = null
    }
    /* eslint-enable react-hooks/set-state-in-effect */
  }, [host, sessionId])

  const sendPrompt = useCallback(
    (text: string) => {
      if (!connRef.current || !sessionId) return
      // Append optimistically so the user sees their message right away.
      setMessages((prev) => [
        ...prev,
        {
          id: `optimistic-${Date.now()}`,
          role: "user",
          text,
          timestamp: Date.now() / 1000,
        },
      ])
      setStreaming("")
      setGenerating(true)
      connRef.current.send({ type: "send_prompt", session_id: sessionId, text })
    },
    [sessionId]
  )

  const stopSession = useCallback(() => {
    if (!connRef.current || !sessionId) return
    connRef.current.send({ type: "stop_session", session_id: sessionId })
  }, [sessionId])

  const respondApproval = useCallback((requestId: string, allow: boolean) => {
    if (!connRef.current) return
    connRef.current.send({
      type: "approval_response",
      request_id: requestId,
      allow,
    })
    setApproval(null)
  }, [])

  // When an assistant finishes streaming (no more deltas for a moment),
  // promote the buffered streaming text into a stable message so future
  // deltas start a new bubble.
  useEffect(() => {
    if (!generating) return
    if (!streaming) return
    const timer = setTimeout(() => {
      setMessages((prev) => [
        ...prev,
        {
          id: `assistant-${Date.now()}`,
          role: "assistant",
          text: streaming,
          timestamp: Date.now() / 1000,
        },
      ])
      setStreaming("")
      setGenerating(false)
    }, 1500)
    return () => clearTimeout(timer)
  }, [streaming, generating])

  if (!host || !sessionId) {
    return (
      <MobileShell title={t("session.title")}>
        <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
          <p className="text-sm text-muted-foreground">
            {t("session.missing")}
          </p>
          <Button size="sm" onClick={() => router.push("/m/sessions")}>
            {t("session.back")}
          </Button>
        </div>
      </MobileShell>
    )
  }

  return (
    <MobileShell title={title || t("session.title")}>
      <div className="flex h-full flex-col">
        {error && (
          <div className="border-b border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        )}
        <MobileMessageStream
          messages={messages}
          streamingText={streaming || null}
          generating={generating}
        />
        <MobileComposer
          generating={generating}
          onSend={sendPrompt}
          onStop={stopSession}
        />
      </div>
      <MobileApprovalDialog payload={approval} onRespond={respondApproval} />
    </MobileShell>
  )
}

export default function MobileSessionPage() {
  return (
    <Suspense fallback={null}>
      <MobileSessionInner />
    </Suspense>
  )
}
