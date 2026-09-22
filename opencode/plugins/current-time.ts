import type { Hooks, Plugin } from "@opencode-ai/plugin"

const INTERVAL_MS = 60_000
const REMINDER_MARKER = '<system-reminder source="current-time">'
const dateTime = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hour12: false,
  timeZoneName: "longOffset",
})

type ChatHistory = Parameters<NonNullable<Hooks["experimental.chat.messages.transform"]>>[1]["messages"]
type PendingReminder = {
  callID: string
}

export default (async () => {
  const lastReminderAt = new Map<string, number>()
  const pending = new Map<string, PendingReminder>()

  return {
    async "tool.execute.after"({ sessionID, callID }) {
      const now = Date.now()
      const last = lastReminderAt.get(sessionID)
      if (last !== undefined && now - last <= INTERVAL_MS) return
      // A later completed tool wins until the next model request consumes the reminder.
      pending.set(sessionID, { callID })
    },

    async "experimental.chat.messages.transform"(_input, output) {
      for (const [sessionID, reminder] of pending) {
        const message = [...output.messages].reverse().find((item) =>
          item.info.role === "assistant" &&
          item.info.sessionID === sessionID &&
          item.parts.some((part) => part.type === "tool" && part.callID === reminder.callID &&
            (part.state.status === "completed" || part.state.status === "error")),
        )
        if (!message) continue
        if (message.parts.some((part) => part.type === "text" && part.text.includes(REMINDER_MARKER))) {
          pending.delete(sessionID)
          lastReminderAt.set(sessionID, Date.now())
          continue
        }
        message.parts.push({
          id: `current-time-${reminder.callID}`,
          messageID: message.info.id,
          sessionID,
          type: "text",
          synthetic: true,
          text: [
            REMINDER_MARKER,
            `Current local time: ${dateTime.format(Date.now())}.`,
            "Use this only to keep time-sensitive reasoning current; do not mention it unless relevant.",
            "</system-reminder>",
          ].join("\n"),
        } satisfies ChatHistory[number]["parts"][number])
        pending.delete(sessionID)
        lastReminderAt.set(sessionID, Date.now())
      }
    },

    async event({ event }) {
      if (event.type !== "session.deleted") return
      lastReminderAt.delete(event.properties.info.id)
      pending.delete(event.properties.info.id)
    },
  }
}) satisfies Plugin
