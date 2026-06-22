import { render } from "@testing-library/react"
import { NextIntlClientProvider } from "next-intl"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { CursorNotificationToastBridge } from "./cursor-task-toast-bridge"
import enMessages from "@/i18n/messages/en.json"
import type { EventEnvelope } from "@/lib/types"

let eventHandler: ((envelope: EventEnvelope) => void) | null = null

vi.mock("@/contexts/acp-connections-context", () => ({
  useAcpEvent: (handler: (envelope: EventEnvelope) => void) => {
    eventHandler = handler
  },
}))

const mockOpenPath = vi.fn()
vi.mock("@/lib/platform", () => ({
  isDesktop: () => true,
  openPath: (...args: unknown[]) => mockOpenPath(...args),
}))

vi.mock("sonner", () => ({
  toast: {
    info: vi.fn(),
    success: vi.fn(),
  },
}))

import { toast } from "sonner"

function renderBridge() {
  return render(
    <NextIntlClientProvider locale="en" messages={enMessages}>
      <CursorNotificationToastBridge />
    </NextIntlClientProvider>
  )
}

describe("CursorNotificationToastBridge", () => {
  beforeEach(() => {
    eventHandler = null
    vi.mocked(toast.info).mockReset()
    vi.mocked(toast.success).mockReset()
    mockOpenPath.mockReset()
  })

  it("shows an info toast for cursor_task events", () => {
    renderBridge()
    expect(eventHandler).toBeTruthy()

    eventHandler!({
      seq: 1,
      connection_id: "c1",
      type: "cursor_task",
      description: "Explore codebase",
      subagent_type: "explore",
      duration_ms: 1500,
      agent_id: null,
    })

    expect(toast.info).toHaveBeenCalledWith(
      "Explore codebase",
      expect.objectContaining({
        description: expect.stringContaining("explore"),
      })
    )
  })

  it("shows a success toast for cursor_generate_image events", () => {
    renderBridge()
    eventHandler!({
      seq: 1,
      connection_id: "c1",
      type: "cursor_generate_image",
      description: "App icon mockup",
      file_path: "/tmp/icon.png",
      reference_image_paths: ["/tmp/ref.png"],
    })

    expect(toast.success).toHaveBeenCalledWith(
      "App icon mockup",
      expect.objectContaining({
        description: expect.stringContaining("/tmp/icon.png"),
        action: expect.objectContaining({ label: "Open file" }),
      })
    )
    const action = vi.mocked(toast.success).mock.calls[0][1]?.action
    action?.onClick()
    expect(mockOpenPath).toHaveBeenCalledWith("/tmp/icon.png")
  })

  it("ignores unrelated events", () => {
    renderBridge()
    eventHandler!({
      seq: 1,
      connection_id: "c1",
      type: "turn_complete",
      session_id: "s1",
      stop_reason: "end_turn",
      agent_type: "cursor",
    })
    expect(toast.info).not.toHaveBeenCalled()
    expect(toast.success).not.toHaveBeenCalled()
  })
})
