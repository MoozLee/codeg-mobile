"use client"

import { useEffect } from "react"
import { useRouter } from "next/navigation"
import { isDesktop } from "@/lib/platform"
import { COMPACT_FORM_FACTOR_QUERY } from "@/lib/layout"

export default function Page() {
  const router = useRouter()
  useEffect(() => {
    // On a compact form factor, drop straight into the mobile shell.
    // Tauri mobile reports `isDesktop()` as true because it is still a Tauri
    // webview, so the viewport check must win before the desktop branch.
    if (
      typeof window !== "undefined" &&
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
