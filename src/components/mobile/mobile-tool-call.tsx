"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { ChevronDown, ChevronRight, Wrench } from "lucide-react"

import { cn } from "@/lib/utils"

interface MobileToolCallProps {
  title: string
  payload?: string | null
  status?: "pending" | "in_progress" | "completed" | "failed"
}

/**
 * Windsw-friendly tool call card. Collapsed by default to keep the
 * message stream scannable on a phone; tapping expands the raw payload.
 */
export function MobileToolCall({
  title,
  payload,
  status,
}: MobileToolCallProps) {
  const t = useTranslations("mobile")
  const [open, setOpen] = useState(false)
  return (
    <div className="rounded-md border bg-card/40">
      <button
        type="button"
        onClick={() => setOpen((prev) => !prev)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-xs"
      >
        {open ? (
          <ChevronDown className="size-3.5 text-muted-foreground" />
        ) : (
          <ChevronRight className="size-3.5 text-muted-foreground" />
        )}
        <Wrench className="size-3.5 text-muted-foreground" aria-hidden />
        <span className="flex-1 truncate font-medium">{title}</span>
        {status && (
          <span
            className={cn(
              "shrink-0 rounded-full px-2 py-0.5 text-[10px] uppercase tracking-wide",
              status === "completed" && "bg-emerald-500/10 text-emerald-500",
              status === "failed" && "bg-destructive/10 text-destructive",
              (status === "pending" || status === "in_progress") &&
                "bg-amber-500/10 text-amber-600"
            )}
          >
            {t(`toolCall.status.${status}`)}
          </span>
        )}
      </button>
      {open && (
        <div className="max-h-60 overflow-auto border-t px-3 py-2">
          <pre className="overflow-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed">
            {payload ?? t("toolCall.emptyPayload")}
          </pre>
        </div>
      )}
    </div>
  )
}
