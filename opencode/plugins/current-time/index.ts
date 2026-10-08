import { Plugin } from "@opencode/plugin"

const INTERVAL_MS = 60_000
const dateTime = new Intl.DateTimeFormat(undefined, {
  year: "numeric", month: "2-digit", day: "2-digit",
  hour: "2-digit", minute: "2-digit", second: "2-digit",
  hour12: false, timeZoneName: "longOffset",
})

export default Plugin.define({
  id: "current-time",
  async setup(ctx) {
    const lastReminderAt = new Map<string, number>()
    const pending = new Map<string, string>()
    await ctx.tool.hook("execute.after", ({ sessionID, id }) => {
      const last = lastReminderAt.get(sessionID)
      if (last !== undefined && Date.now() - last <= INTERVAL_MS) return
      pending.set(sessionID, id)
    })
    await ctx.session.hook("context", (event) => {
      const id = pending.get(event.sessionID)
      if (!id) return
      const index = event.messages.findLastIndex((message) =>
        message.role === "tool" && message.content.some((part) => part.type === "tool-result" && part.id === id),
      )
      if (index < 0) return
      // Keep tool-call/result pairs intact and leave the persisted transcript untouched.
      event.messages.splice(index + 1, 0, {
        role: "user",
        content: [{ type: "text", text: [
          '<system-reminder source="current-time">',
          `Current local time: ${dateTime.format(Date.now())}.`,
          "Use this only to keep time-sensitive reasoning current; do not mention it unless relevant.",
          "</system-reminder>",
        ].join("\n") }],
      })
      pending.delete(event.sessionID)
      lastReminderAt.set(event.sessionID, Date.now())
    })
    const controller = new AbortController()
    const events = (async () => {
      for await (const event of ctx.event.subscribe({ signal: controller.signal })) {
        if (event.type !== "session.deleted") continue
        lastReminderAt.delete(event.data.sessionID)
        pending.delete(event.data.sessionID)
      }
    })().catch((error) => {
      if (!controller.signal.aborted) console.warn("[current-time] Event subscription failed:", error)
    })
    return async () => {
      controller.abort()
      await events
      lastReminderAt.clear()
      pending.clear()
    }
  },
})
