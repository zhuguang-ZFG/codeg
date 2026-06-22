"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { ClipboardList, Loader2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { MessageResponse } from "@/components/ai-elements/message"
import type { PendingPlanState, PlanAnswer } from "@/lib/types"

interface PlanApprovalCardProps {
  plan: PendingPlanState
  onAnswer: (planId: string, answer: PlanAnswer) => void | Promise<void>
}

export function PlanApprovalCard({ plan, onAnswer }: PlanApprovalCardProps) {
  const t = useTranslations("Folder.chat.planApproval")
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState(false)

  const submit = async (accepted: boolean) => {
    setSubmitting(true)
    setError(false)
    try {
      await onAnswer(plan.plan_id, { accepted, cancelled: false })
    } catch {
      setError(true)
      setSubmitting(false)
    }
  }

  return (
    <div className="mx-4 mb-3 rounded-xl border border-border/70 bg-card/95 p-3 shadow-sm">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 space-y-1">
          <div className="flex items-center gap-1.5 text-sm font-medium">
            <ClipboardList className="h-4 w-4 shrink-0 text-sky-500" />
            <span className="truncate">
              {plan.name?.trim() || t("titleFallback")}
            </span>
          </div>
          {plan.overview?.trim() && (
            <p className="text-xs text-muted-foreground">{plan.overview}</p>
          )}
        </div>
        <Badge variant="outline" className="shrink-0 text-[10px]">
          {t("badge")}
        </Badge>
      </div>

      <div className="mt-3 max-h-[min(36vh,18rem)] space-y-3 overflow-y-auto pr-1">
        <MessageResponse>{plan.plan}</MessageResponse>
        {plan.todos.length > 0 && (
          <ul className="space-y-1 text-xs text-muted-foreground">
            {plan.todos.map((todo) => (
              <li key={todo.id} className="flex items-start gap-2">
                <span className="mt-0.5 shrink-0 uppercase">{todo.status}</span>
                <span>{todo.content}</span>
              </li>
            ))}
          </ul>
        )}
      </div>

      {error && (
        <p className="mt-2 text-xs text-destructive">{t("submitError")}</p>
      )}

      <div className="mt-3 flex flex-wrap justify-end gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={submitting}
          onClick={() => submit(false)}
        >
          {submitting ? <Loader2 className="h-4 w-4 animate-spin" /> : t("reject")}
        </Button>
        <Button
          type="button"
          size="sm"
          disabled={submitting}
          onClick={() => submit(true)}
        >
          {submitting ? <Loader2 className="h-4 w-4 animate-spin" /> : t("accept")}
        </Button>
      </div>
    </div>
  )
}
