import { Plugin } from "@opencode/plugin"

const INTERVAL_MS = 60_000
const TURN_GAP_MS = 5 * 60_000

const dateTime = new Intl.DateTimeFormat(undefined, {
  year: "numeric", month: "2-digit", day: "2-digit",
  hour: "2-digit", minute: "2-digit", second: "2-digit",
  hour12: false, timeZoneName: "longOffset",
})
const clock = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit", minute: "2-digit", second: "2-digit",
  hour12: false, timeZoneName: "longOffset",
})

type Outcome = "succeeded" | "failed" | "interrupted"
type InterruptionReason = "user" | "shutdown" | "superseded" | "inactivity"
type Compaction = {
  reason: "auto" | "manual"
  status: "completed" | "failed"
  error?: string
}
type PreviousTurn = {
  completedAt: number
  gapMs: number
  outcome: Outcome
  reason?: InterruptionReason
  error?: string
  compaction?: Compaction
}
type TurnState = {
  userMessageID: string
  startedAt: number
  previous?: PreviousTurn
}

function formatDuration(ms: number) {
  const total = Math.max(0, Math.floor(ms / 1000))
  const hours = Math.floor(total / 3600)
  const minutes = Math.floor((total % 3600) / 60)
  const seconds = total % 60
  if (hours) return `${hours}h ${minutes}m`
  if (minutes) return `${minutes}m ${seconds}s`
  return `${seconds}s`
}

function formatError(error: { type: string; message: string; status?: number }) {
  const message = error.message.length > 160 ? `${error.message.slice(0, 157)}...` : error.message
  return `${error.type}${error.status ? ` ${error.status}` : ""}: ${message}`
}

function reasonText(reason: InterruptionReason) {
  switch (reason) {
    case "user": return "user interrupted"
    case "shutdown": return "service shutdown"
    case "superseded": return "superseded by a newer request"
    case "inactivity": return "inactivity timeout"
  }
}

function turnGapText(turn: TurnState) {
  const previous = turn.previous
  if (!previous) return ""
  const reason = previous.reason ? ` (${reasonText(previous.reason)})` : ""
  const error = previous.error ? ` (${previous.error})` : ""
  const lines = [
    '<system-reminder source="turn-gap">',
    `Previous turn: ${previous.outcome}${reason}${error}; ended at ${clock.format(previous.completedAt)}.`,
    `The user resumed at ${clock.format(turn.startedAt)} after ${formatDuration(previous.gapMs)}.`,
  ]
  if (previous.compaction) {
    if (previous.compaction.status === "completed") {
      lines.push(`Context ${previous.compaction.reason} compaction completed before this message.`)
    } else {
      const detail = previous.compaction.error ? `: ${previous.compaction.error}` : ""
      lines.push(`Context ${previous.compaction.reason} compaction failed before this message${detail}.`)
    }
  }
  lines.push("</system-reminder>")
  return lines.join("\n")
}

export default Plugin.define({
  id: "current-time",
  async setup(ctx) {
    const lastReminderAt = new Map<string, number>()
    const pending = new Map<string, string>()
    const pendingTurn = new Map<string, string>()
    const turns = new Map<string, TurnState>()
    const interruptions = new Map<string, { reason: InterruptionReason }>()
    const failures = new Map<string, { error: string }>()
    const compactions = new Map<string, { reason: "auto" | "manual"; status: "running" | "completed" | "failed"; error?: string; at: number }>()

    const clearSession = (sessionID: string) => {
      lastReminderAt.delete(sessionID)
      pending.delete(sessionID)
      pendingTurn.delete(sessionID)
      turns.delete(sessionID)
      interruptions.delete(sessionID)
      failures.delete(sessionID)
      compactions.delete(sessionID)
    }

    await ctx.tool.hook("execute.after", ({ sessionID, id }) => {
      const last = lastReminderAt.get(sessionID)
      if (last !== undefined && Date.now() - last <= INTERVAL_MS) return
      pending.set(sessionID, id)
    })

    await ctx.session.hook("prompt", ({ sessionID, messageID }) => {
      pendingTurn.set(sessionID, messageID)
    })

    await ctx.session.hook("context", async (event) => {
      const messageID = pendingTurn.get(event.sessionID)
      if (messageID) {
        const index = event.messages.findLastIndex((message) => message.id === messageID && message.role === "user")
        if (index >= 0) {
          try {
            const history = await ctx.session.context({ sessionID: event.sessionID })
            const userIndex = history.findLastIndex((message) => message.id === messageID)
            if (userIndex >= 0) {
              const user = history[userIndex]
              if (user.type === "user" || user.type === "synthetic") {
                const turn: TurnState = { userMessageID: messageID, startedAt: user.time.created }
                turns.set(event.sessionID, turn)
                pendingTurn.delete(event.sessionID)

                const boundary = history[userIndex - 1]
                if (boundary?.type === "idle") {
                  const previous = history.slice(0, userIndex - 1).findLast((message) => message.type === "assistant")
                  const completedAt = previous?.type === "assistant" && previous.time.completed !== undefined
                    ? previous.time.completed
                    : boundary.time.created
                  const gapMs = Math.max(0, user.time.created - completedAt)
                  const interruption = interruptions.get(event.sessionID)
                  const failure = failures.get(event.sessionID)
                  const compaction = compactions.get(event.sessionID)
                  let activeCompaction: Compaction | undefined
                  if (compaction && compaction.status !== "running" && compaction.at <= user.time.created) {
                    activeCompaction = { reason: compaction.reason, status: compaction.status, error: compaction.error }
                  }
                  turn.previous = {
                    completedAt,
                    gapMs,
                    outcome: boundary.outcome,
                    reason: boundary.outcome === "interrupted" ? interruption?.reason : undefined,
                    error: boundary.outcome === "failed" ? failure?.error : undefined,
                    compaction: activeCompaction,
                  }
                  interruptions.delete(event.sessionID)
                  failures.delete(event.sessionID)
                  compactions.delete(event.sessionID)

                  if (gapMs >= TURN_GAP_MS || turn.previous.outcome !== "succeeded" || turn.previous.compaction) {
                    event.messages[index] = {
                      ...event.messages[index],
                      content: [
                        ...event.messages[index].content,
                        { type: "text", text: turnGapText(turn) },
                      ],
                    }
                  }
                } else {
                  // A steered message joins the running turn; it has no completed
                  // previous turn to measure.
                  pendingTurn.delete(event.sessionID)
                }
              }
            }
          } catch (error) {
            pendingTurn.delete(event.sessionID)
            console.warn("[current-time] Turn gap skipped:", error)
          }
        }
      }

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
      const lastUser = event.messages.findLast((message) => message.role === "user" && message.id !== undefined)
      const candidate = turns.get(event.sessionID)
      const turn = lastUser?.id && candidate?.userMessageID === lastUser.id ? candidate : undefined
      const now = Date.now()
      const lines = [
        '<system-reminder source="current-time">',
        `Current local time: ${dateTime.format(now)}.`,
      ]
      if (turn) {
        lines.push(`This turn started at ${clock.format(turn.startedAt)}; ${formatDuration(now - turn.startedAt)} elapsed.`)
      }
      lines.push("Use this only to keep time-sensitive reasoning current; do not mention it unless relevant.")
      lines.push("</system-reminder>")
      // Keep tool-call/result pairs intact and leave the persisted transcript untouched.
      event.messages.splice(end, 0, {
        role: "user",
        content: [{ type: "text", text: lines.join("\n") }],
      })
      pending.delete(event.sessionID)
      lastReminderAt.set(event.sessionID, now)
    })

    const controller = new AbortController()
    const events = (async () => {
      for await (const event of ctx.event.subscribe({ signal: controller.signal })) {
        switch (event.type) {
          case "session.deleted":
            clearSession(event.data.sessionID)
            break
          case "session.execution.succeeded":
            interruptions.delete(event.data.sessionID)
            failures.delete(event.data.sessionID)
            break
          case "session.execution.failed":
            failures.set(event.data.sessionID, { error: formatError(event.data.error) })
            interruptions.delete(event.data.sessionID)
            break
          case "session.execution.interrupted":
            interruptions.set(event.data.sessionID, { reason: event.data.reason })
            failures.delete(event.data.sessionID)
            break
          case "session.compaction.started":
            compactions.set(event.data.sessionID, { reason: event.data.reason, status: "running", at: event.created })
            break
          case "session.compaction.ended":
            compactions.set(event.data.sessionID, { reason: event.data.reason, status: "completed", at: event.created })
            break
          case "session.compaction.failed":
            compactions.set(event.data.sessionID, {
              reason: event.data.reason,
              status: "failed",
              error: formatError(event.data.error),
              at: event.created,
            })
            break
        }
      }
    })().catch((error) => {
      if (!controller.signal.aborted) console.warn("[current-time] Event subscription failed:", error)
    })
    return async () => {
      controller.abort()
      await events
      lastReminderAt.clear()
      pending.clear()
      pendingTurn.clear()
      turns.clear()
      interruptions.clear()
      failures.clear()
      compactions.clear()
    }
  },
})
