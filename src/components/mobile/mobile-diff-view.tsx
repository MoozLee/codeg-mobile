"use client"

import { useMemo } from "react"

import { cn } from "@/lib/utils"

interface MobileDiffViewProps {
  /** Raw unified-diff-style text. Lines beginning with `+` / `-` / ` ` are
   * coloured; everything else is rendered neutrally. */
  diff: string
}

/**
 * Minimal unified-diff renderer tuned for narrow screens. We deliberately
 * avoid the heavyweight desktop diff component here: on a phone it
 * cannot side-by-side and pulls in Monaco assets. Line counts and hunk
 * headers are rendered but without sticky file headers.
 */
export function MobileDiffView({ diff }: MobileDiffViewProps) {
  const lines = useMemo(() => diff.split(/\r?\n/), [diff])
  return (
    <pre className="overflow-auto rounded-md border bg-card/40 p-2 font-mono text-[11px] leading-5">
      {lines.map((line, idx) => {
        let className = "whitespace-pre-wrap break-words"
        if (line.startsWith("+++") || line.startsWith("---")) {
          className = cn(className, "text-muted-foreground font-semibold")
        } else if (line.startsWith("@@")) {
          className = cn(className, "text-sky-600 font-semibold")
        } else if (line.startsWith("+")) {
          className = cn(className, "text-emerald-600")
        } else if (line.startsWith("-")) {
          className = cn(className, "text-destructive")
        }
        return (
          <div key={idx} className={className}>
            {line || " "}
          </div>
        )
      })}
    </pre>
  )
}
