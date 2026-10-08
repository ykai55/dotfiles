import type { V2Event } from "@opencode/client"
import type { Plugin } from "@opencode/plugin"
import { serviceClient } from "../../lib/service-client"
import {
  NotificationComposer,
  type CompactionNotice, type DoneNotice, type PermissionNotice,
  type ProgressNotice, type QuestionNotice, type SessionNotice,
} from "./composer"

export type NotifySender = {
  ensureSession(session: SessionNotice): Promise<void>
  syncSessionTitle?(session: SessionNotice): Promise<void>
  sendDone(done: DoneNotice): Promise<void>
  updateProgress?(notice: ProgressNotice): Promise<void>
  sendCompaction?(notice: CompactionNotice): Promise<void>
  sendPermission?(notice: PermissionNotice): Promise<void>
  clearPermission?(requestID: string, sessionID?: string): Promise<void>
  sendQuestion?(notice: QuestionNotice): Promise<void>
  clearQuestion?(requestID: string, sessionID?: string): Promise<void>
  errorLabel: string
}

export type NotifyChannel = {
  event(event: V2Event): Promise<void>
  dispose(): Promise<void>
}

export function createDispatcher(input: {
  ctx: Plugin.Context
  composer: NotificationComposer
  sender: NotifySender
  signal: AbortSignal
  notifyDone: boolean
  notifyPermission: boolean
  notifyQuestion: boolean
  permissionNotifyDelay: number
  contextLimit(model: unknown): Promise<number | undefined>
  activeContextTokens?: (sessionID: string) => Promise<number | undefined>
}) {
  const pending = new Map<string, { sessionID: string; timer: ReturnType<typeof setTimeout> }>()
  const notified = new Set<string>()
  const toolNames = new Map<string, string>()
  const sending = new Set<Promise<void>>()
  const handling = new Set<Promise<void>>()

  function clearPermission(requestID: string) {
    const entry = pending.get(requestID)
    if (entry) clearTimeout(entry.timer)
    pending.delete(requestID)
  }

  async function handle(event: V2Event) {
      if (input.signal.aborted) return
      const data = event.data
      const sessionID = event.type === "form.created" ? event.data.form.sessionID : "sessionID" in data ? data.sessionID : undefined
      if (typeof sessionID !== "string") return
      try {
        if (event.type === "session.deleted") {
          for (const [id, value] of pending) if (value.sessionID === sessionID) clearPermission(id)
          input.composer.delete(sessionID)
          return
        }
        // Public events span locations. Check the session before any outward side effect.
        if (event.location && event.location.directory !== input.ctx.location.directory) return
        const session = await input.ctx.session.get({ sessionID })
        if (input.signal.aborted || session.parentID || session.location.directory !== input.ctx.location.directory) return
        input.composer.sessionInfo(sessionID, session)

        switch (event.type) {
          case "session.renamed":
            await input.sender.syncSessionTitle?.(input.composer.session(sessionID))
            break
          case "session.inbox.delivered": {
            const client = await serviceClient(input.ctx.options)
            const message = await client.session.message.get({ sessionID, messageID: event.data.inboxID }, { signal: input.signal })
            if (message.type !== "user") break
            const notice = input.composer.chatMessage(sessionID, [{ id: message.id, text: message.text }])
            if (notice) await input.sender.ensureSession(notice)
            break
          }
          case "session.execution.started":
            input.composer.status(sessionID, "busy")
            break
          case "session.step.started":
            input.composer.status(sessionID, "busy")
            input.composer.stepStarted(sessionID, await input.contextLimit(event.data.model))
            break
          case "session.step.ended":
            input.composer.stepEnded(sessionID, event.data)
            break
          case "session.compaction.started":
            input.composer.compactionStarted(sessionID)
            break
          case "session.compaction.ended": {
            const notice = input.composer.compactionEnded(sessionID, await input.activeContextTokens?.(sessionID))
            if (notice) await input.sender.sendCompaction?.(notice)
            break
          }
          case "session.text.delta":
          case "session.text.ended": {
            const partID = `${event.data.assistantMessageID}:${event.data.ordinal}`
            const notice = event.type === "session.text.delta"
              ? input.composer.partDelta(sessionID, partID, event.data.delta)
              : input.composer.partUpdated(sessionID, { type: "text", id: partID, text: event.data.text })
            if (notice && input.notifyDone) await input.sender.updateProgress?.(notice)
            break
          }
          case "session.tool.input.started":
            toolNames.set(`${sessionID}:${event.data.id}`, event.data.name)
            break
          case "session.tool.called": {
            const notice = input.composer.partUpdated(sessionID, {
              type: "tool", id: event.data.id, callID: event.data.id,
              tool: toolNames.get(`${sessionID}:${event.data.id}`) ?? "tool",
              state: { input: event.data.input },
            })
            toolNames.delete(`${sessionID}:${event.data.id}`)
            if (notice && input.notifyDone) await input.sender.updateProgress?.(notice)
            break
          }
          case "session.status": {
            if (event.data.status.type === "idle") {
              for (const [id, value] of pending) if (value.sessionID === sessionID) clearPermission(id)
            }
            const notice = input.composer.status(sessionID, event.data.status.type)
            if (notice && input.notifyDone) await input.sender.sendDone(notice)
            break
          }
          case "permission.asked": {
            if (!input.notifyPermission || !input.sender.sendPermission || notified.has(event.data.id)) break
            const request = event.data
            notified.add(request.id)
            pending.set(request.id, { sessionID, timer: setTimeout(() => {
              pending.delete(request.id)
              if (input.signal.aborted) return
              const send = input.sender.sendPermission!({
                sessionID, requestID: request.id, permission: request.action, patterns: request.resources.join(", "),
              }).catch((error) => {
                if (!input.signal.aborted) console.warn(input.sender.errorLabel, error)
              }).finally(() => sending.delete(send))
              sending.add(send)
            }, input.permissionNotifyDelay) })
            break
          }
          case "permission.replied":
            clearPermission(event.data.requestID)
            notified.delete(event.data.requestID)
            await input.sender.clearPermission?.(event.data.requestID, sessionID)
            break
          case "form.created": {
            const form = event.data.form
            if (!input.notifyQuestion || notified.has(form.id)) break
            notified.add(form.id)
            const fields = form.fields.filter((field) => field.type !== "external" && !field.hidden)
            const question = fields.map((field) => [
              `${field.key}: ${field.title ?? ""}`, field.description,
              "options" in field ? field.options?.map((option) => `${option.label} (${option.value})`).join(" / ") : undefined,
            ].filter(Boolean).join("\n")).join("\n\n")
            const instruction = fields.length === 1 && fields[0].type === "string" ? "Reply with your answer."
              : fields.length === 1 && fields[0].type === "multiselect" ? 'Reply with a choice or a JSON array, e.g. ["a", "b"].'
                : 'Reply with a JSON object keyed by field names, e.g. {"field": "answer"}.'
            await input.sender.sendQuestion?.({ sessionID, requestID: form.id, header: form.title, question: `${question}\n\n${instruction}` })
            break
          }
          case "form.replied":
          case "form.cancelled":
            notified.delete(event.data.id)
            await input.sender.clearQuestion?.(event.data.id, sessionID)
            break
        }
      } catch (error) {
        if (!input.signal.aborted) console.warn(input.sender.errorLabel, error)
      }
  }

  return {
    async dispose() {
      for (const id of pending.keys()) clearPermission(id)
      await Promise.allSettled(handling)
      for (const id of pending.keys()) clearPermission(id)
      await Promise.allSettled(sending)
      notified.clear()
      toolNames.clear()
    },
    event(event: V2Event) {
      const operation = handle(event).finally(() => handling.delete(operation))
      handling.add(operation)
      return operation
    },
  }
}
