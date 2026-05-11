"use client"

import { useRouter } from "next/navigation"

import { MobileShell } from "@/components/mobile/mobile-shell"
import { MobileSessionList } from "@/components/mobile/mobile-session-list"
import { useMobileHostsStore } from "@/stores/mobile-hosts-store"

export default function MobileSessionsPage() {
  const router = useRouter()
  const { activeHostId } = useMobileHostsStore()

  return (
    <MobileShell>
      <MobileSessionList
        onOpenSession={(sessionId) => {
          if (!activeHostId) return
          router.push(
            `/m/session?host=${encodeURIComponent(
              activeHostId
            )}&session=${encodeURIComponent(sessionId)}`
          )
        }}
      />
    </MobileShell>
  )
}
