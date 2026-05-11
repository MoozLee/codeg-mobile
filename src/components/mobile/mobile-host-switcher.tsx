"use client"

import { useCallback } from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"
import { ChevronDown, Plus } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  useMobileHostsActions,
  useMobileHostsStore,
} from "@/stores/mobile-hosts-store"

export function MobileHostSwitcher() {
  const t = useTranslations("mobile")
  const router = useRouter()
  const { hosts, activeHostId } = useMobileHostsStore()
  const { setActiveHost } = useMobileHostsActions()
  const active = hosts.find((h) => h.id === activeHostId) ?? null

  const handleAddHost = useCallback(() => {
    router.push("/m/pair")
  }, [router])

  const label = active?.nickname ?? t("host.none")

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="max-w-[180px] truncate"
        >
          <span className="truncate">{label}</span>
          <ChevronDown className="size-3.5 opacity-70" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-56">
        <DropdownMenuLabel>{t("host.switchTitle")}</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {hosts.length === 0 ? (
          <DropdownMenuItem disabled>{t("host.noneListed")}</DropdownMenuItem>
        ) : (
          hosts.map((h) => (
            <DropdownMenuItem
              key={h.id}
              onClick={() => setActiveHost(h.id)}
              className={h.id === activeHostId ? "font-medium" : undefined}
            >
              <span className="truncate">{h.nickname}</span>
            </DropdownMenuItem>
          ))
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={handleAddHost}>
          <Plus className="mr-1 size-3.5" />
          {t("host.add")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
