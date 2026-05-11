"use client"

import { useEffect } from "react"
import { useRouter, usePathname } from "next/navigation"
import { useTranslations } from "next-intl"
import { MessageSquare, PlusCircle, Smartphone } from "lucide-react"

import { cn } from "@/lib/utils"
import { subscribeDeepLinks, type DeepLink } from "@/lib/deep-link"
import { useMobileHostsActions } from "@/stores/mobile-hosts-store"
import { MobileHostSwitcher } from "./mobile-host-switcher"

interface MobileShellProps {
  children: React.ReactNode
  /** Title rendered in the top bar. */
  title?: string
  /** Optional right-side action button slot (e.g. "Settings"). */
  titleAction?: React.ReactNode
}

/**
 * Mobile single-panel shell. Renders a title bar (host switcher + page
 * title), the active page, and a fixed bottom nav that mirrors the
 * MobilePanelView finite state machine from Paseo: sessions / new /
 * pair.
 */
export function MobileShell({
  children,
  title,
  titleAction,
}: MobileShellProps) {
  const t = useTranslations("mobile")
  const pathname = usePathname()
  const pageTitle = title ?? t("shell.title")
  useDeepLinkRouter()

  return (
    <div className="flex h-dvh flex-col bg-background text-foreground">
      <header className="flex items-center gap-2 border-b bg-background/95 px-3 py-2 backdrop-blur supports-[backdrop-filter]:bg-background/60">
        <MobileHostSwitcher />
        <div className="min-w-0 flex-1 text-center text-sm font-semibold">
          <span className="block truncate">{pageTitle}</span>
        </div>
        <div className="flex items-center gap-1">{titleAction}</div>
      </header>
      <main className="min-h-0 flex-1 overflow-hidden">{children}</main>
      <MobileBottomNav currentPath={pathname ?? ""} />
    </div>
  )
}

function MobileBottomNav({ currentPath }: { currentPath: string }) {
  const t = useTranslations("mobile")
  const router = useRouter()
  const items: Array<{
    key: "sessions" | "new" | "pair"
    label: string
    icon: React.ComponentType<{ className?: string }>
    href: string
  }> = [
    {
      key: "sessions",
      label: t("nav.sessions"),
      icon: MessageSquare,
      href: "/m/sessions",
    },
    {
      key: "new",
      label: t("nav.new"),
      icon: PlusCircle,
      href: "/m/new-session",
    },
    {
      key: "pair",
      label: t("nav.pair"),
      icon: Smartphone,
      href: "/m/pair",
    },
  ]
  return (
    <nav
      role="navigation"
      aria-label={t("nav.ariaLabel")}
      className="grid grid-cols-3 border-t bg-background/95 pb-[env(safe-area-inset-bottom)]"
    >
      {items.map((item) => {
        const active = currentPath.startsWith(item.href)
        const Icon = item.icon
        return (
          <button
            key={item.key}
            type="button"
            onClick={() => router.push(item.href)}
            className={cn(
              "flex flex-col items-center gap-0.5 px-2 py-2 text-[11px] font-medium transition-colors",
              active
                ? "text-primary"
                : "text-muted-foreground hover:text-foreground"
            )}
            aria-current={active ? "page" : undefined}
          >
            <Icon className="size-5" aria-hidden />
            <span>{item.label}</span>
          </button>
        )
      })}
    </nav>
  )
}

/**
 * Subscribe to OS-delivered `codeg://` deep links and route accordingly.
 *
 * - `codeg://session?host=<id>&session=<id>` jumps into the session viewer,
 *   falling back to the currently active host when `host` is omitted.
 * - `codeg://pair?url=<pairing_url>` pre-fills the pairing page.
 */
function useDeepLinkRouter() {
  const router = useRouter()
  const { getActiveHost } = useMobileHostsActions()

  useEffect(() => {
    let disposer: (() => void) | null = null
    let cancelled = false

    const handle = (link: DeepLink) => {
      if (link.kind === "session") {
        const host = link.host ?? getActiveHost()?.id
        const qs = new URLSearchParams()
        if (host) qs.set("host", host)
        qs.set("session", link.session)
        router.replace(`/m/session?${qs.toString()}`)
      } else if (link.kind === "pair") {
        const qs = new URLSearchParams({ url: link.pairingUrl })
        router.replace(`/m/pair?${qs.toString()}`)
      }
    }

    subscribeDeepLinks(handle)
      .then((unsubscribe) => {
        if (cancelled) {
          unsubscribe()
        } else {
          disposer = unsubscribe
        }
      })
      .catch((err: unknown) => {
        console.warn("[mobile] deep-link subscription failed:", err)
      })

    return () => {
      cancelled = true
      if (disposer) disposer()
    }
  }, [router, getActiveHost])
}
