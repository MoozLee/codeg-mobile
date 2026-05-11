"use client"

import { useRef, useState } from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { MobileShell } from "@/components/mobile/mobile-shell"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import {
  openHostConnection,
  type AppResponse,
  type HostConnection,
} from "@/lib/mobile-transport"
import {
  useMobileHostsActions,
  useMobileHostsStore,
} from "@/stores/mobile-hosts-store"
import { ALL_AGENT_TYPES, AGENT_LABELS } from "@/lib/types"

export default function MobileNewSessionPage() {
  const t = useTranslations("mobile")
  const router = useRouter()
  const { activeHostId } = useMobileHostsStore()
  const { getActiveHost } = useMobileHostsActions()
  const host = getActiveHost()

  const [agent, setAgent] = useState<string>(ALL_AGENT_TYPES[0])
  const [cwd, setCwd] = useState<string>("")
  const [prompt, setPrompt] = useState<string>("")
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const connRef = useRef<HostConnection | null>(null)

  const handleCreate = async () => {
    if (!host || !activeHostId) {
      setError(t("newSession.noHost"))
      return
    }
    const trimmedCwd = cwd.trim()
    const trimmedPrompt = prompt.trim()
    if (!trimmedCwd || !trimmedPrompt) return
    setError(null)
    setSubmitting(true)
    try {
      const conn = await openHostConnection(host)
      connRef.current = conn
      const sessionId = await new Promise<string>((resolve, reject) => {
        const timer = setTimeout(() => {
          reject(new Error(t("newSession.timeout")))
        }, 30_000)
        const unsub = conn.onMessage((msg: AppResponse) => {
          if (msg.type === "new_session_created") {
            clearTimeout(timer)
            unsub()
            resolve(msg.session_id)
          } else if (msg.type === "error") {
            clearTimeout(timer)
            unsub()
            reject(new Error(msg.message))
          }
        })
        conn.send({ type: "new_session", agent_type: agent, cwd: trimmedCwd })
      })
      // Send first prompt
      conn.send({
        type: "send_prompt",
        session_id: sessionId,
        text: trimmedPrompt,
      })
      conn.close()
      connRef.current = null
      router.push(
        `/m/session?host=${encodeURIComponent(activeHostId)}&session=${encodeURIComponent(
          sessionId
        )}`
      )
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      connRef.current?.close()
      connRef.current = null
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <MobileShell title={t("newSession.title")}>
      <div className="flex h-full flex-col">
        <div className="flex-1 space-y-4 overflow-auto p-4">
          {!host && (
            <div className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-xs text-amber-700 dark:text-amber-400">
              {t("newSession.noHost")}
            </div>
          )}
          {error && (
            <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-xs text-destructive">
              {error}
            </div>
          )}
          <div className="space-y-2">
            <Label htmlFor="agent">{t("newSession.agent")}</Label>
            <Select value={agent} onValueChange={setAgent}>
              <SelectTrigger id="agent">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ALL_AGENT_TYPES.map((a) => (
                  <SelectItem key={a} value={a}>
                    {AGENT_LABELS[a]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label htmlFor="cwd">{t("newSession.cwd")}</Label>
            <Input
              id="cwd"
              value={cwd}
              onChange={(e) => setCwd(e.target.value)}
              placeholder={t("newSession.cwdPlaceholder")}
            />
            <p className="text-[11px] text-muted-foreground">
              {t("newSession.cwdHint")}
            </p>
          </div>
          <div className="space-y-2">
            <Label htmlFor="prompt">{t("newSession.firstPrompt")}</Label>
            <Textarea
              id="prompt"
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={4}
              placeholder={t("newSession.firstPromptPlaceholder")}
            />
          </div>
        </div>
        <div className="border-t p-3 pb-[calc(env(safe-area-inset-bottom)+0.5rem)]">
          <Button
            type="button"
            className="w-full"
            onClick={handleCreate}
            disabled={
              submitting ||
              !host ||
              cwd.trim().length === 0 ||
              prompt.trim().length === 0
            }
          >
            {submitting ? t("newSession.creating") : t("newSession.create")}
          </Button>
        </div>
      </div>
    </MobileShell>
  )
}
