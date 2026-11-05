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
      // Insert only after the owning turn's complete tool block. A user message
      // between parallel tool results is invalid for strict providers.
      const owner = event.messages.findLastIndex((message) =>
        message.role === "assistant" &&
        message.content.some((part) => part.type === "tool-call" && part.id === id),
      )
      if (owner < 0) return
      const calls = event.messages[owner].content
        .filter((part) => part.type === "tool-call")
        .map((part) => part.id)
      let end = owner + 1
      const results = new Map<string, boolean>()
      while (end < event.messages.length && event.messages[end].role === "tool") {
        for (const part of event.messages[end].content) {
          if (part.type !== "tool-result") continue
          results.set(part.id, part.result.type !== "text" || part.result.value !== "Tool result missing")
        }
        end++
      }
      // A parallel run may still be settling (for example a synthetic missing
      // result). Wait for the next context instead of splitting the block.
      if (calls.some((call) => results.get(call) !== true)) return
      // Keep tool-call/result pairs intact and leave the persisted transcript untouched.
      event.messages.splice(end, 0, {
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
