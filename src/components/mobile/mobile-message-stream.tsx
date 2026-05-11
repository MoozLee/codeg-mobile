"use client"

import { useEffect, useRef } from "react"
import { useTranslations } from "next-intl"

import { cn } from "@/lib/utils"
import type { MessageEnvelope } from "@/lib/mobile-transport"
import { MobileToolCall } from "./mobile-tool-call"
import { MobileDiffView } from "./mobile-diff-view"

interface MobileMessageStreamProps {
  messages: MessageEnvelope[]
  streamingText?: string | null
  generating?: boolean
}

/**
 * Chat-like vertical message stream. Each turn renders as a bubble whose
 * alignment and colour track the role. Tool calls and diffs are
 * delegated to `MobileToolCall` / `MobileDiffView`. We auto-scroll to
 * the bottom when messages arrive so the phone behaves like a messenger
 * app; if the user has scrolled up we keep their position.
 *
 * Uses a native overflow-y-auto div rather than the OverlayScrollbars
 * ScrollArea primitive because we need a direct `onScroll` binding to
 * track viewport position; ScrollArea does not expose a typed DOM
 * scroll prop.
 */
export function MobileMessageStream({
  messages,
  streamingText,
  generating,
}: MobileMessageStreamProps) {
  const t = useTranslations("mobile")
  const scrollerRef = useRef<HTMLDivElement | null>(null)
  const stuckToBottomRef = useRef<boolean>(true)

  useEffect(() => {
    const el = scrollerRef.current
    if (!el) return
    if (stuckToBottomRef.current) {
      el.scrollTop = el.scrollHeight
    }
  }, [messages, streamingText])

  return (
    <div
      ref={scrollerRef}
      className="min-h-0 flex-1 overflow-y-auto"
      onScroll={(e) => {
        const el = e.currentTarget
        const remaining = el.scrollHeight - el.scrollTop - el.clientHeight
        stuckToBottomRef.current = remaining < 80
      }}
    >
      <div className="flex flex-col gap-3 p-3">
        {messages.length === 0 && !streamingText && (
          <div className="py-10 text-center text-sm text-muted-foreground">
            {t("messageStream.empty")}
          </div>
        )}
        {messages.map((msg) => (
          <MessageBubble key={msg.id} msg={msg} />
        ))}
        {streamingText && (
          <div className="flex justify-start">
            <div className="max-w-[85%] rounded-2xl rounded-bl-sm bg-muted px-3 py-2 text-sm leading-relaxed whitespace-pre-wrap break-words">
              {streamingText}
              {generating && (
                <span className="ml-1 inline-block size-1.5 animate-pulse rounded-full bg-foreground/70" />
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

function MessageBubble({ msg }: { msg: MessageEnvelope }) {
  const t = useTranslations("mobile")
  if (msg.role === "tool") {
    // Tool messages get the collapsible card so long tool output never
    // dominates the view on a phone.
    return (
      <MobileToolCall
        title={msg.text.split("\n")[0] || t("toolCall.untitled")}
        payload={msg.text}
      />
    )
  }
  if (msg.role === "system") {
    return (
      <div className="text-center text-[11px] uppercase tracking-wide text-muted-foreground">
        {msg.text}
      </div>
    )
  }
  const isDiff = looksLikeUnifiedDiff(msg.text)
  if (isDiff) {
    return <MobileDiffView diff={msg.text} />
  }
  const isUser = msg.role === "user"
  return (
    <div className={cn("flex", isUser ? "justify-end" : "justify-start")}>
      <div
        className={cn(
          "max-w-[85%] rounded-2xl px-3 py-2 text-sm leading-relaxed whitespace-pre-wrap break-words",
          isUser
            ? "rounded-br-sm bg-primary text-primary-foreground"
            : "rounded-bl-sm bg-muted text-foreground"
        )}
      >
        {msg.text}
      </div>
    </div>
  )
}

function looksLikeUnifiedDiff(text: string): boolean {
  if (!text) return false
  const head = text.split("\n", 5).join("\n")
  return /(^|\n)(?:---\s|\+\+\+\s|@@\s)/.test(head)
}
