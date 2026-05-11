"use client"

import { useEffect } from "react"
import { useRouter } from "next/navigation"
import { isDesktop } from "@/lib/platform"
import { COMPACT_FORM_FACTOR_QUERY } from "@/lib/layout"

export default function Page() {
  const router = useRouter()
  useEffect(() => {
    // On a compact form factor (phone-sized viewport in web/Tauri Mobile)
    // drop straight into the mobile shell. Desktop flow is unchanged.
    if (
      typeof window !== "undefined" &&
      !isDesktop() &&
      window.matchMedia(COMPACT_FORM_FACTOR_QUERY).matches
    ) {
      router.replace("/m/sessions")
      return
    }
    if (isDesktop()) {
      router.replace("/workspace")
      return
    }
    // Web mode: validate token before entering app
    const token = localStorage.getItem("codeg_token")
    if (!token) {
      router.replace("/login")
      return
    }
    // Verify token is still valid
    fetch("/api/health", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${token}`,
      },
      body: "{}",
    })
      .then((res) => {
        if (res.ok) {
          router.replace("/workspace")
        } else {
          localStorage.removeItem("codeg_token")
          router.replace("/login")
        }
      })
      .catch(() => {
        // Server unreachable
        localStorage.removeItem("codeg_token")
        router.replace("/login")
      })
  }, [router])
  return null
}
