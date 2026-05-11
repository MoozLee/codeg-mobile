"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { AlertCircle, Loader2, RefreshCw } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { cn } from "@/lib/utils"
import {
  openHostConnection,
  type AppResponse,
  type HostConnection,
  type SessionSummary,
} from "@/lib/mobile-transport"
import {
  useMobileHostsActions,
  useMobileHostsStore,
} from "@/stores/mobile-hosts-store"

interface MobileSessionListProps {
  onOpenSession: (sessionId: string) => void
}

export function MobileSessionList({ onOpenSession }: MobileSessionListProps) {
  const t = useTranslations("mobile")
  const router = useRouter()
  const { activeHostId } = useMobileHostsStore()
  const { getActiveHost } = useMobileHostsActions()
  const [sessions, setSessions] = useState<SessionSummary[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [search, setSearch] = useState("")
  const connRef = useRef<HostConnection | null>(null)

  const host = getActiveHost()

  const loadSessions = useCallback(async () => {
    if (!host) return
    setLoading(true)
    setError(null)
    try {
      const conn = await openHostConnection(host)
      connRef.current = conn
      const unsub = conn.onMessage((msg: AppResponse) => {
        if (msg.type === "sessions_list") {
          setSessions(msg.sessions)
          setLoading(false)
          unsub()
          conn.close()
          connRef.current = null
        } else if (msg.type === "error") {
          setError(msg.message)
          setLoading(false)
          unsub()
          conn.close()
          connRef.current = null
        }
      })
      conn.send({ type: "list_sessions" })
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setLoading(false)
    }
  }, [host])

  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- initial fetch on mount / host change
    loadSessions()
    return () => {
      connRef.current?.close()
      connRef.current = null
    }
  }, [loadSessions])

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    if (!q) return sessions
    return sessions.filter((s) => s.title.toLowerCase().includes(q))
  }, [sessions, search])

  if (!activeHostId || !host) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        <AlertCircle className="size-8 text-muted-foreground" aria-hidden />
        <p className="text-sm text-muted-foreground">{t("sessions.noHost")}</p>
        <Button onClick={() => router.push("/m/pair")} size="sm">
          {t("sessions.pairAction")}
        </Button>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b p-3">
        <Input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={t("sessions.searchPlaceholder")}
          className="h-8"
        />
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          onClick={loadSessions}
          disabled={loading}
          title={t("sessions.refresh")}
        >
          {loading ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <RefreshCw className="size-4" />
          )}
        </Button>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="divide-y">
          {error && (
            <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
              {error}
            </div>
          )}
          {!loading && filtered.length === 0 && !error ? (
            <div className="p-6 text-center text-sm text-muted-foreground">
              {t("sessions.empty")}
            </div>
          ) : (
            filtered.map((s) => (
              <button
                key={s.id}
                type="button"
                onClick={() => onOpenSession(s.id)}
                className={cn(
                  "flex w-full flex-col items-start gap-0.5 px-4 py-3 text-left",
                  "transition-colors hover:bg-muted active:bg-muted"
                )}
              >
                <div className="flex w-full items-center gap-2">
                  <span className="truncate text-sm font-medium">
                    {s.title || t("sessions.untitled")}
                  </span>
                  <span className="ml-auto shrink-0 text-[11px] uppercase tracking-wide text-muted-foreground">
                    {s.agent_type}
                  </span>
                </div>
                <div className="text-[11px] text-muted-foreground">
                  {t("sessions.lastActive", {
                    ago: formatAgo(s.last_active_at),
                  })}
                </div>
              </button>
            ))
          )}
        </div>
      </ScrollArea>
    </div>
  )
}

function formatAgo(unix: number): string {
  if (!unix) return "-"
  const now = Math.floor(Date.now() / 1000)
  const delta = Math.max(0, now - unix)
  if (delta < 60) return `${delta}s`
  if (delta < 3600) return `${Math.floor(delta / 60)}m`
  if (delta < 86400) return `${Math.floor(delta / 3600)}h`
  return `${Math.floor(delta / 86400)}d`
}
