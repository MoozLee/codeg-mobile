"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { Send, StopCircle } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"

interface MobileComposerProps {
  disabled?: boolean
  /** Whether the agent is currently generating; toggles Send→Stop. */
  generating?: boolean
  placeholder?: string
  onSend: (text: string) => void
  onStop: () => void
}

/**
 * Fixed-bottom composer with a growing textarea. Uses the shared
 * Textarea primitive (field-sizing-content) so the input expands up to
 * ~6 lines before overflowing. Enter inserts a newline; users tap Send
 * to submit — the classic phone affordance.
 */
export function MobileComposer({
  disabled = false,
  generating = false,
  placeholder,
  onSend,
  onStop,
}: MobileComposerProps) {
  const t = useTranslations("mobile")
  const [value, setValue] = useState("")
  const ref = useRef<HTMLTextAreaElement | null>(null)

  useEffect(() => {
    if (!generating) {
      ref.current?.focus()
    }
  }, [generating])

  const handleSend = () => {
    const text = value.trim()
    if (!text || disabled) return
    onSend(text)
    setValue("")
  }

  return (
    <div className="border-t bg-background/95 px-3 py-2 pb-[calc(env(safe-area-inset-bottom)+0.5rem)]">
      <div className="flex items-end gap-2">
        <Textarea
          ref={ref}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder={placeholder ?? t("composer.placeholder")}
          rows={2}
          disabled={disabled || generating}
          className="max-h-40 min-h-10 resize-none"
        />
        {generating ? (
          <Button
            type="button"
            variant="destructive"
            size="icon"
            onClick={onStop}
            title={t("composer.stop")}
            aria-label={t("composer.stop")}
          >
            <StopCircle className="size-4" />
          </Button>
        ) : (
          <Button
            type="button"
            size="icon"
            onClick={handleSend}
            disabled={disabled || value.trim().length === 0}
            title={t("composer.send")}
            aria-label={t("composer.send")}
          >
            <Send className="size-4" />
          </Button>
        )}
      </div>
    </div>
  )
}
