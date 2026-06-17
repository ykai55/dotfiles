import type { Hooks, PluginInput } from "@opencode-ai/plugin"
import {
  NotificationComposer,
  patterns,
  prop,
  textOption,
  type CompactionNotice,
  type DoneNotice,
  type PermissionNotice,
  type QuestionNotice,
  type SessionNotice,
} from "./composer"

export type NotifySender = {
  ensureSession(session: SessionNotice): Promise<void>
  syncSessionTitle?(session: SessionNotice): Promise<void>
  sendDone(done: DoneNotice): Promise<void>
  sendCompaction?(notice: CompactionNotice): Promise<void>
  sendPermission?(notice: PermissionNotice): Promise<void>
  clearPermission?(requestID: string): Promise<void>
  sendQuestion?(notice: QuestionNotice): Promise<void>
  errorLabel: string
}

export function createDispatcher(input: {
  plugin: PluginInput
  composer: NotificationComposer
  sender: NotifySender
  notifyDone: boolean
  notifyPermission: boolean
  notifyQuestion: boolean
  permissionNotifyDelay: number
  contextLimit(model: unknown): Promise<number | undefined>
  activeContextTokens?: (sessionID: string) => Promise<number | undefined>
}): Hooks {
  const notifiedRequests = new Set<string>()
  const pendingPermissionTimers = new Map<string, { sessionID: string; timer: ReturnType<typeof setTimeout> }>()

  function clearPermissionTimer(requestID: string) {
    const pending = pendingPermissionTimers.get(requestID)
    if (!pending) return
    clearTimeout(pending.timer)
    pendingPermissionTimers.delete(requestID)
  }

  function clearSessionPermissionTimers(sessionID: string) {
    for (const [requestID, pending] of pendingPermissionTimers) {
      if (pending.sessionID !== sessionID) continue
      clearTimeout(pending.timer)
      pendingPermissionTimers.delete(requestID)
    }
  }

  return {
    async "chat.message"(messageInput, output) {
      const session = input.composer.chatMessage(
        messageInput.sessionID,
        output.parts,
        await input.contextLimit(messageInput.model),
      )
      if (session) await input.sender.ensureSession(session)
    },

    async event(eventInput) {
      const event = eventInput.event
      const eventType = prop(event, "type")
      const properties = prop(event, "properties")
      const sessionID = prop(properties, "sessionID")

      try {
        if (eventType === "session.created" || eventType === "session.updated") {
          const info = prop(properties, "info")
          if (typeof sessionID !== "string") return
          const title = input.composer.sessionInfo(sessionID, info, await input.contextLimit(prop(info, "model")))
          if (title) await input.sender.syncSessionTitle?.(input.composer.session(sessionID))
          return
        }

        if (eventType === "session.next.step.started") {
          if (typeof sessionID !== "string") return
          input.composer.stepStarted(sessionID, await input.contextLimit(prop(properties, "model")))
          return
        }

        if (eventType === "session.next.step.ended") {
          if (typeof sessionID !== "string") return
          input.composer.stepEnded(sessionID, properties)
          return
        }

        if (eventType === "session.next.compaction.started") {
          if (typeof sessionID !== "string") return
          input.composer.compactionStarted(sessionID)
          return
        }

        if (eventType === "session.next.compaction.ended") {
          if (typeof sessionID !== "string" || !input.sender.sendCompaction) return
          const notice = input.composer.compactionEnded(sessionID, await input.activeContextTokens?.(sessionID))
          if (notice) await input.sender.sendCompaction(notice)
          return
        }

        if (eventType === "message.part.delta") {
          const partID = prop(properties, "partID")
          const field = prop(properties, "field")
          const delta = prop(properties, "delta")
          if (
            typeof sessionID !== "string" ||
            typeof partID !== "string" ||
            field !== "text" ||
            typeof delta !== "string"
          )
            return
          input.composer.partDelta(sessionID, partID, delta)
          return
        }

        if (eventType === "message.part.updated") {
          if (typeof sessionID !== "string") return
          input.composer.partUpdated(sessionID, prop(properties, "part"))
          return
        }

        if (eventType === "session.status") {
          const statusType = prop(prop(properties, "status"), "type")
          if (typeof sessionID !== "string") return
          if (statusType !== "busy" && statusType !== "retry") clearSessionPermissionTimers(sessionID)
          const done = input.composer.status(sessionID, statusType)
          if (done && input.notifyDone) await input.sender.sendDone(done)
          return
        }

        if (eventType === "permission.replied") {
          const requestID = prop(properties, "requestID")
          if (typeof requestID !== "string") return
          clearPermissionTimer(requestID)
          await input.sender.clearPermission?.(requestID)
          return
        }

        if (eventType === "permission.asked" && input.notifyPermission && input.sender.sendPermission) {
          const requestID = prop(properties, "id")
          if (typeof sessionID !== "string" || typeof requestID !== "string" || input.composer.isMuted(sessionID)) return
          const key = `permission:${sessionID}:${prop(properties, "permission")}:${patterns(prop(properties, "patterns"))}`
          if (notifiedRequests.has(key)) return
          notifiedRequests.add(key)
          pendingPermissionTimers.set(requestID, {
            sessionID,
            timer: setTimeout(() => {
              pendingPermissionTimers.delete(requestID)
              void input.sender.sendPermission?.({
                sessionID,
                requestID,
                permission: textOption(prop(properties, "permission"), "unknown") ?? "unknown",
                patterns: patterns(prop(properties, "patterns")) || "unknown",
              })
            }, input.permissionNotifyDelay),
          })
          return
        }

        if (eventType === "question.asked" && input.notifyQuestion && input.sender.sendQuestion) {
          const requestID = prop(properties, "id")
          if (typeof sessionID !== "string" || typeof requestID !== "string" || input.composer.isMuted(sessionID)) return
          const key = `question:${requestID}`
          if (notifiedRequests.has(key)) return
          notifiedRequests.add(key)
          await input.sender.sendQuestion({ sessionID, requestID })
        }
      } catch (error) {
        console.warn(`${input.sender.errorLabel}:`, error instanceof Error ? error.message : error)
      }
    },
  }
}
