"use client"

import { useTranslations } from "next-intl"

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

export interface MobileApprovalPayload {
  request_id: string
  session_id: string
  summary: string
}

interface MobileApprovalDialogProps {
  payload: MobileApprovalPayload | null
  onRespond: (requestId: string, allow: boolean) => void
}

/**
 * Single-button approve/deny dialog for agent permission requests.
 * Re-uses the shared AlertDialog primitive so focus management and
 * overlay behaviour are consistent with desktop approvals.
 */
export function MobileApprovalDialog({
  payload,
  onRespond,
}: MobileApprovalDialogProps) {
  const t = useTranslations("mobile")
  return (
    <AlertDialog
      open={payload !== null}
      onOpenChange={(open) => {
        if (!open && payload) onRespond(payload.request_id, false)
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("approval.title")}</AlertDialogTitle>
          <AlertDialogDescription>
            {payload?.summary ?? ""}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel
            onClick={() => payload && onRespond(payload.request_id, false)}
          >
            {t("approval.deny")}
          </AlertDialogCancel>
          <AlertDialogAction
            onClick={() => payload && onRespond(payload.request_id, true)}
          >
            {t("approval.allow")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
