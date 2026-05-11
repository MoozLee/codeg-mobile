"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import {
  Check,
  Copy,
  RefreshCw,
  Smartphone,
  Trash2,
  PencilLine,
} from "lucide-react"
import QRCode from "qrcode"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import {
  mobileGeneratePairingOffer,
  mobileGetRelayOrigin,
  mobileListPairedDevices,
  mobileRenamePairedDevice,
  mobileRevokePairedDevice,
  mobileSetRelayOrigin,
} from "@/lib/api"
import { copyTextToClipboard } from "@/lib/utils"
import type { PairedDevice, PairingOffer } from "@/lib/types"

// IMPLEMENT_PLACEHOLDER

function formatRelativeTime(unixSeconds: number): string {
  const nowSec = Math.floor(Date.now() / 1000)
  const delta = nowSec - unixSeconds
  if (delta < 60) return `${delta}s`
  if (delta < 3600) return `${Math.floor(delta / 60)}m`
  if (delta < 86400) return `${Math.floor(delta / 3600)}h`
  return `${Math.floor(delta / 86400)}d`
}

function shortPubKey(hex: string): string {
  return hex.length <= 16 ? hex : `${hex.slice(0, 8)}…${hex.slice(-4)}`
}

export function MobileDevicesSettings() {
  const t = useTranslations("MobileDevicesSettings")
  const [devices, setDevices] = useState<PairedDevice[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState("")
  const [offer, setOffer] = useState<PairingOffer | null>(null)
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null)
  const [dialogOpen, setDialogOpen] = useState(false)
  const [relayOrigin, setRelayOrigin] = useState<string>("")
  const [relayInput, setRelayInput] = useState<string>("")
  const [renameTarget, setRenameTarget] = useState<PairedDevice | null>(null)
  const [renameValue, setRenameValue] = useState("")
  const [revokeTarget, setRevokeTarget] = useState<PairedDevice | null>(null)
  const [copied, setCopied] = useState(false)
  const relayBoundaryRef = useRef<string>("")

  const refreshDevices = useCallback(async () => {
    try {
      const rows = await mobileListPairedDevices()
      setDevices(rows)
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      setError(msg)
    }
  }, [])

  const refreshRelayOrigin = useCallback(async () => {
    try {
      const origin = await mobileGetRelayOrigin()
      setRelayOrigin(origin)
      setRelayInput(origin)
      relayBoundaryRef.current = origin
    } catch {
      // Non-fatal; user may not have configured a relay yet.
    }
  }, [])

  useEffect(() => {
    refreshDevices()
    refreshRelayOrigin()
  }, [refreshDevices, refreshRelayOrigin])

  async function handlePairNewDevice() {
    setError("")
    setLoading(true)
    try {
      const fresh = await mobileGeneratePairingOffer()
      setOffer(fresh)
      const dataUrl = await QRCode.toDataURL(fresh.pairing_url, {
        margin: 1,
        width: 320,
      })
      setQrDataUrl(dataUrl)
      setDialogOpen(true)
      // Poll for new devices so the list updates once the phone completes
      // the handshake. A lightweight 1s poll is plenty for manual pairing.
      const poll = setInterval(() => {
        refreshDevices()
      }, 1500)
      setTimeout(() => clearInterval(poll), 10 * 60 * 1000)
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      setError(msg)
    } finally {
      setLoading(false)
    }
  }

  async function handleCopyPairingUrl() {
    if (!offer) return
    const ok = await copyTextToClipboard(offer.pairing_url)
    if (!ok) return
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }

  async function handleSaveRelay() {
    const trimmed = relayInput.trim()
    if (trimmed === relayBoundaryRef.current) return
    try {
      const applied = await mobileSetRelayOrigin(trimmed)
      setRelayOrigin(applied)
      setRelayInput(applied)
      relayBoundaryRef.current = applied
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      setError(msg)
    }
  }

  async function handleConfirmRename() {
    if (!renameTarget) return
    const trimmed = renameValue.trim()
    if (!trimmed) return
    try {
      await mobileRenamePairedDevice(renameTarget.device_id, trimmed)
      setRenameTarget(null)
      refreshDevices()
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      setError(msg)
    }
  }

  async function handleConfirmRevoke() {
    if (!revokeTarget) return
    try {
      await mobileRevokePairedDevice(revokeTarget.device_id)
      setRevokeTarget(null)
      refreshDevices()
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      setError(msg)
    }
  }

  return (
    <ScrollArea className="h-full">
      <div className="space-y-6 p-3 md:p-4">
        <div>
          <h3 className="text-lg font-medium">{t("sectionTitle")}</h3>
          <p className="text-sm text-muted-foreground">
            {t("sectionDescription")}
          </p>
        </div>

        {error && (
          <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </div>
        )}

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-base">
              <Smartphone className="h-4 w-4" />
              {t("newDevice.title")}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 px-6">
            <p className="text-sm text-muted-foreground">
              {t("newDevice.description")}
            </p>
            <div>
              <Button
                type="button"
                onClick={handlePairNewDevice}
                disabled={loading}
              >
                {loading ? t("newDevice.preparing") : t("newDevice.action")}
              </Button>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">{t("relay.title")}</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 px-6">
            <p className="text-sm text-muted-foreground">
              {t("relay.description")}
            </p>
            <div className="flex items-center gap-2">
              <Input
                value={relayInput}
                onChange={(e) => setRelayInput(e.target.value)}
                placeholder="wss://codeg-relay.<account>.workers.dev"
                className="flex-1"
                spellCheck={false}
              />
              <Button
                type="button"
                variant="secondary"
                onClick={handleSaveRelay}
                disabled={relayInput.trim() === relayBoundaryRef.current}
              >
                {t("relay.save")}
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              {t("relay.current", { origin: relayOrigin || "—" })}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">{t("devices.title")}</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 px-6">
            {devices.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                {t("devices.empty")}
              </p>
            ) : (
              devices.map((device) => (
                <DeviceRow
                  key={device.device_id}
                  device={device}
                  onRename={() => {
                    setRenameTarget(device)
                    setRenameValue(device.nickname)
                  }}
                  onRevoke={() => setRevokeTarget(device)}
                />
              ))
            )}
            <div className="pt-1">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => refreshDevices()}
              >
                <RefreshCw className="mr-1 h-3.5 w-3.5" />
                {t("devices.refresh")}
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("newDevice.dialogTitle")}</DialogTitle>
            <DialogDescription>
              {t("newDevice.dialogDescription")}
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col items-center gap-4">
            {qrDataUrl && (
              // Data URL QR code; next/image Optimizer is not useful here.
              // eslint-disable-next-line @next/next/no-img-element
              <img
                src={qrDataUrl}
                alt={t("newDevice.qrAlt")}
                className="rounded-md border bg-background p-2"
                width={320}
                height={320}
              />
            )}
            {offer && (
              <div className="w-full space-y-1">
                <div className="text-xs font-medium text-muted-foreground">
                  {t("newDevice.urlLabel")}
                </div>
                <div className="flex items-center gap-2 rounded-md border bg-muted/40 px-3 py-2">
                  <code className="min-w-0 flex-1 truncate font-mono text-xs">
                    {offer.pairing_url}
                  </code>
                  <button
                    type="button"
                    onClick={handleCopyPairingUrl}
                    className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                    title={t("newDevice.copy")}
                  >
                    {copied ? (
                      <Check className="h-3.5 w-3.5 text-green-500" />
                    ) : (
                      <Copy className="h-3.5 w-3.5" />
                    )}
                  </button>
                </div>
              </div>
            )}
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="secondary"
              onClick={() => setDialogOpen(false)}
            >
              {t("newDevice.close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={renameTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRenameTarget(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("rename.title")}</DialogTitle>
            <DialogDescription>{t("rename.description")}</DialogDescription>
          </DialogHeader>
          <Input
            value={renameValue}
            onChange={(e) => setRenameValue(e.target.value)}
            placeholder={t("rename.placeholder")}
            autoFocus
          />
          <DialogFooter>
            <Button
              type="button"
              variant="secondary"
              onClick={() => setRenameTarget(null)}
            >
              {t("rename.cancel")}
            </Button>
            <Button
              type="button"
              onClick={handleConfirmRename}
              disabled={renameValue.trim().length === 0}
            >
              {t("rename.save")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog
        open={revokeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRevokeTarget(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("revoke.title")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("revoke.description", {
                nickname: revokeTarget?.nickname ?? "",
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("revoke.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={(e) => {
                e.preventDefault()
                handleConfirmRevoke()
              }}
            >
              {t("revoke.confirm")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </ScrollArea>
  )
}

interface DeviceRowProps {
  device: PairedDevice
  onRename: () => void
  onRevoke: () => void
}

function DeviceRow({ device, onRename, onRevoke }: DeviceRowProps) {
  const t = useTranslations("MobileDevicesSettings")
  return (
    <div className="flex items-center gap-3 rounded-md border bg-background px-3 py-2">
      <Smartphone className="h-4 w-4 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-medium">
            {device.nickname}
          </span>
          {device.revoked && (
            <span className="rounded-full bg-destructive/10 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-destructive">
              {t("devices.revokedBadge")}
            </span>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <span className="font-mono">
            {shortPubKey(device.client_pub_hex)}
          </span>
          <span>·</span>
          <span>
            {t("devices.lastActive", {
              ago: formatRelativeTime(device.last_active_at),
            })}
          </span>
        </div>
      </div>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        onClick={onRename}
        title={t("devices.rename")}
      >
        <PencilLine className="h-3.5 w-3.5" />
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        onClick={onRevoke}
        title={t("devices.revoke")}
        disabled={device.revoked}
      >
        <Trash2 className="h-3.5 w-3.5" />
      </Button>
    </div>
  )
}
