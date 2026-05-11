"use client"

import { useState } from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { AlertCircle, CheckCircle2, Smartphone } from "lucide-react"

import { MobileShell } from "@/components/mobile/mobile-shell"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { scanPairingUrlFromWeb } from "@/lib/mobile-transport"
import { useMobileHostsActions } from "@/stores/mobile-hosts-store"

export default function MobilePairPage() {
  const t = useTranslations("mobile")
  const router = useRouter()
  const { addHost, setActiveHost } = useMobileHostsActions()

  const [pairingUrl, setPairingUrl] = useState("")
  const [nickname, setNickname] = useState("")
  const [status, setStatus] = useState<"idle" | "pairing" | "done" | "error">(
    "idle"
  )
  const [error, setError] = useState<string | null>(null)

  const handlePair = async () => {
    setStatus("pairing")
    setError(null)
    try {
      const result = await scanPairingUrlFromWeb(
        pairingUrl.trim(),
        nickname.trim() || t("pair.defaultNickname")
      )
      addHost(result.profile)
      setActiveHost(result.profile.id)
      setStatus("done")
      setTimeout(() => router.push("/m/sessions"), 500)
    } catch (err) {
      setStatus("error")
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  return (
    <MobileShell title={t("pair.title")}>
      <div className="flex h-full flex-col">
        <div className="flex-1 space-y-4 overflow-auto p-4">
          <div className="flex items-start gap-2 rounded-md border bg-muted/40 p-3">
            <Smartphone
              className="mt-0.5 size-4 text-muted-foreground"
              aria-hidden
            />
            <div className="space-y-1 text-xs text-muted-foreground">
              <p>{t("pair.description1")}</p>
              <p>{t("pair.description2")}</p>
            </div>
          </div>

          {error && (
            <div className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-xs text-destructive">
              <AlertCircle className="mt-0.5 size-3.5" aria-hidden />
              <span>{error}</span>
            </div>
          )}

          {status === "done" && (
            <div className="flex items-start gap-2 rounded-md border border-emerald-500/40 bg-emerald-500/10 p-3 text-xs text-emerald-700 dark:text-emerald-400">
              <CheckCircle2 className="mt-0.5 size-3.5" aria-hidden />
              <span>{t("pair.success")}</span>
            </div>
          )}

          <div className="space-y-2">
            <Label htmlFor="pair-url">{t("pair.urlLabel")}</Label>
            <Input
              id="pair-url"
              type="text"
              inputMode="url"
              autoComplete="off"
              autoCorrect="off"
              spellCheck={false}
              value={pairingUrl}
              onChange={(e) => setPairingUrl(e.target.value)}
              placeholder="codeg-pair://..."
            />
            <p className="text-[11px] text-muted-foreground">
              {t("pair.urlHint")}
            </p>
          </div>

          <div className="space-y-2">
            <Label htmlFor="pair-nickname">{t("pair.nicknameLabel")}</Label>
            <Input
              id="pair-nickname"
              type="text"
              value={nickname}
              onChange={(e) => setNickname(e.target.value)}
              placeholder={t("pair.nicknamePlaceholder")}
            />
          </div>
        </div>

        <div className="border-t p-3 pb-[calc(env(safe-area-inset-bottom)+0.5rem)]">
          <Button
            type="button"
            className="w-full"
            disabled={status === "pairing" || pairingUrl.trim().length === 0}
            onClick={handlePair}
          >
            {status === "pairing" ? t("pair.pairing") : t("pair.action")}
          </Button>
        </div>
      </div>
    </MobileShell>
  )
}
