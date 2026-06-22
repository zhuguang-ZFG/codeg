"use client"

import { useCallback } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { useAcpEvent } from "@/contexts/acp-connections-context"
import type { EventEnvelope } from "@/lib/types"
import { isDesktop, openPath } from "@/lib/platform"

function formatDurationMs(ms: number): string {
  const secs = ms / 1000
  return secs >= 10 ? `${Math.round(secs)}s` : `${secs.toFixed(1)}s`
}

/**
 * Surfaces Cursor CLI ephemeral notifications (`cursor/task`,
 * `cursor/generate_image`) as toasts. Must be rendered inside
 * `AcpConnectionsProvider`.
 */
export function CursorNotificationToastBridge() {
  const tTask = useTranslations("Folder.chat.cursorTask")
  const tImage = useTranslations("Folder.chat.cursorGenerateImage")

  useAcpEvent(
    useCallback(
      (envelope: EventEnvelope) => {
        if (envelope.type === "cursor_task") {
          const title =
            envelope.description.trim() || tTask("fallbackDescription")
          const subagent = envelope.subagent_type.trim() || "unspecified"
          const durationMs = envelope.duration_ms ?? null
          toast.info(title, {
            description:
              durationMs != null
                ? tTask("toastDescriptionWithDuration", {
                    subagent,
                    duration: formatDurationMs(durationMs),
                  })
                : tTask("toastDescription", { subagent }),
          })
          return
        }

        if (envelope.type === "cursor_generate_image") {
          const title =
            envelope.description.trim() || tImage("fallbackDescription")
          const path = envelope.file_path?.trim()
          const refCount = envelope.reference_image_paths?.length ?? 0
          const parts: string[] = []
          if (path) {
            parts.push(tImage("toastDescriptionWithPath", { path }))
          } else {
            parts.push(tImage("toastDescription"))
          }
          if (refCount > 0) {
            parts.push(tImage("references", { count: refCount }))
          }
          toast.success(title, {
            description: parts.join(" · "),
            ...(path && isDesktop()
              ? {
                  action: {
                    label: tImage("openFile"),
                    onClick: () => {
                      void openPath(path)
                    },
                  },
                }
              : {}),
          })
        }
      },
      [tTask, tImage]
    )
  )

  return null
}

/** @deprecated Use `CursorNotificationToastBridge`. */
export const CursorTaskToastBridge = CursorNotificationToastBridge
