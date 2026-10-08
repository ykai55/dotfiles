import { afterEach, expect, spyOn, test } from "bun:test"
import { createDispatcher } from "../plugins/chat-notify/dispatcher.ts"
import { NotificationComposer } from "../plugins/chat-notify/composer.ts"
import telegram from "../plugins/chat-notify/telegram.ts"
import lark from "../plugins/chat-notify/lark.ts"
import { answerForm } from "../lib/form-reply.ts"
import { WSClient } from "@larksuiteoapi/node-sdk"
import { fixture } from "./fixture.mjs"

let f
let channel
let clock
let websocketStart
let websocketClose
afterEach(async () => {
  try { await channel?.dispose() } finally {
    channel = undefined; await f?.close(); clock?.mockRestore(); clock = undefined
    websocketStart?.mockRestore(); websocketClose?.mockRestore()
  }
})
const event = (type, data, directory = "/test") => ({ type, data, location: { directory } })
async function waitFor(predicate) {
  for (let i = 0; i < 500; i++) {
    if (predicate()) return
    await Bun.sleep(2)
  }
  throw new Error("Timed out waiting for test operation")
}

test("V2 text, tools, compaction, forms and permissions produce notifications only for root sessions in this location", async () => {
  f = fixture()
  const notices = { done: [], progress: [], permission: [], question: [], compaction: [] }
  const controller = new AbortController()
  const dispatcher = createDispatcher({
    ctx: f.ctx, signal: controller.signal, composer: new NotificationComposer({ directory: "/test", maxOutputChars: 3000 }),
    notifyDone: true, notifyPermission: true, notifyQuestion: true, permissionNotifyDelay: 5,
    contextLimit: async () => 200000, activeContextTokens: async () => 400,
    sender: {
      errorLabel: "test", async ensureSession() {},
      async sendDone(value) { notices.done.push(value) }, async updateProgress(value) { notices.progress.push(value) },
      async sendPermission(value) { notices.permission.push(value) }, async sendQuestion(value) { notices.question.push(value) },
      async sendCompaction(value) { notices.compaction.push(value) },
    },
  })
  channel = { ...dispatcher, async dispose() { controller.abort(); await dispatcher.dispose() } }
  const send = (type, data) => dispatcher.event(event(type, { sessionID: "ses_main", ...data }))
  await send("session.step.started", { model: { providerID: "test", id: "test" } })
  await send("session.text.delta", { assistantMessageID: "msg_a", ordinal: 0, delta: "Thinking" })
  await send("session.tool.input.started", { id: "call_1", name: "patch" })
  await send("session.tool.called", { id: "call_1", input: { patchText: "*** Begin Patch\n*** Update File: demo.ts\n*** End Patch" } })
  await send("session.text.delta", { assistantMessageID: "msg_b", ordinal: 0, delta: "Do" })
  await send("session.text.ended", { assistantMessageID: "msg_b", ordinal: 0, text: "Done" })
  await send("session.step.ended", { tokens: { input: 1000, output: 50, reasoning: 0, cache: { read: 0, write: 0 } }, files: ["demo.ts"] })
  await send("session.compaction.started", {})
  await send("session.compaction.ended", {})
  expect(notices.compaction[0]).toMatchObject({ beforeTokens: 1050, afterTokens: 400 })
  await send("permission.asked", { id: "per_cancel", action: "shell", resources: ["git push"] })
  await send("permission.replied", { requestID: "per_cancel" })
  await send("permission.asked", { id: "per_live", action: "shell", resources: ["git push"] })
  await waitFor(() => notices.permission.length === 1)
  expect(notices.permission[0]).toMatchObject({ requestID: "per_live", permission: "shell", patterns: "git push" })
  await send("form.created", { form: { id: "frm_1", sessionID: "ses_main", title: "Choose", fields: [{ key: "choice", type: "string", title: "Branch", options: [{ label: "Main", value: "main" }] }] } })
  expect(notices.question[0].question).toContain("Main (main)")
  await send("session.status", { status: { type: "idle" } })
  await send("session.status", { status: { type: "idle" } })
  expect(notices.done).toHaveLength(1)
  expect(notices.done[0]).toMatchObject({ output: "Done", tools: 1, changed: 1 })
  f.records.get("ses_main").parentID = "ses_parent"
  await send("form.created", { form: { id: "frm_child", sessionID: "ses_main", title: "Child", fields: [] } })
  await dispatcher.event(event("form.created", { form: { id: "frm_other", sessionID: "ses_main", fields: [] } }, "/other"))
  expect(notices.question).toHaveLength(1)
})

test("form replies preserve option values, multi-select and multi-field answers", async () => {
  f = fixture()
  const form = { id: "frm_test", sessionID: "ses_main", state: { status: "pending" }, fields: [{ key: "choice", type: "string", options: [{ label: "Main", value: "main" }] }] }
  f.forms.set(form.id, form)
  await answerForm("ses_main", form.id, "Main")
  expect(form.state.answer).toEqual({ choice: "main" })
  form.fields[0].type = "multiselect"
  await answerForm("ses_main", form.id, '["Main", "other"]')
  expect(form.state.answer).toEqual({ choice: ["main", "other"] })
  form.fields.push({ key: "count", type: "number" })
  await expect(answerForm("ses_main", form.id, "plain text")).rejects.toBeDefined()
  await answerForm("ses_main", form.id, '{"choice": ["main"], "count": 2}')
  expect(form.state.answer).toEqual({ choice: ["main"], count: 2 })
})

test("Telegram forwards replies and permission buttons through V2 and aborts its poll on unload", async () => {
  f = fixture()
  const outgoing = []
  let poll
  let aborted = false
  f.route = (request, url, body) => {
    if (url.hostname !== "api.telegram.org") return
    const method = url.pathname.split("/").at(-1)
    if (method === "getUpdates") {
      if (!body.timeout) return Response.json({ ok: true, result: [] })
      return new Promise((resolve, reject) => {
        poll = (updates) => { poll = undefined; resolve(Response.json({ ok: true, result: updates })) }
        request.signal.addEventListener("abort", () => { aborted = true; reject(request.signal.reason) }, { once: true })
      })
    }
    outgoing.push({ method, body })
    return Response.json({ ok: true, result: { message_id: 10, message_thread_id: 42 } })
  }
  channel = await telegram(f.ctx, { token: "test-token", chatID: "1", statePath: ":memory:", forumTopics: true, permissionNotifyDelay: 1 })
  const message = f.turn("Please work")
  await channel.event(event("session.inbox.delivered", { sessionID: "ses_main", inboxID: message.id }))
  expect(outgoing.some((item) => item.method === "sendRichMessage")).toBe(true)
  await waitFor(() => !!poll)
  poll([{ update_id: 1, message: { message_id: 21, chat: { id: 1 }, message_thread_id: 42, text: "Follow up" } }])
  await waitFor(() => f.prompts.length === 1 && !!poll)
  expect(f.prompts[0]).toMatchObject({ sessionID: "ses_main", text: "Follow up", delivery: "steer" })
  await channel.event(event("permission.asked", { sessionID: "ses_main", id: "per_1", action: "shell", resources: ["git push"] }))
  await waitFor(() => outgoing.some((item) => JSON.stringify(item.body).includes("op:perm:once:per_1")))
  poll([{ update_id: 2, callback_query: { id: "cb_1", data: "op:perm:once:per_1", message: { chat: { id: 1 }, message_id: 10 } } }])
  await waitFor(() => f.permissions.length === 1 && !!poll)
  expect(f.permissions[0]).toMatchObject({ sessionID: "ses_main", requestID: "per_1", decision: "once" })
  f.forms.set("frm_1", { id: "frm_1", sessionID: "ses_main", title: "Question", fields: [{ key: "answer", type: "string" }], state: { status: "pending" } })
  await channel.event(event("form.created", { form: f.forms.get("frm_1") }))
  poll([{ update_id: 3, message: { message_id: 22, chat: { id: 1 }, message_thread_id: 42, text: "Accepted" } }])
  await waitFor(() => f.forms.get("frm_1").state.status === "answered" && !!poll)
  expect(f.forms.get("frm_1").state.answer).toEqual({ answer: "Accepted" })
  await channel.dispose()
  channel = undefined
  expect(aborted).toBe(true)
})

test("Lark sends completion cards and forwards thread replies through V2", async () => {
  f = fixture()
  let now = 100000
  clock = spyOn(Date, "now").mockImplementation(() => now)
  const outgoing = []
  let delivered = false
  f.route = (request, url, body) => {
    if (url.hostname !== "open.feishu.cn") return
    if (url.pathname.endsWith("tenant_access_token/internal")) return Response.json({ code: 0, tenant_access_token: "test", expire: 3600 })
    if (url.pathname.endsWith("batch_get_id")) return Response.json({ code: 0, data: { user_list: [{ email: "test@example.invalid", user_id: "ou_test" }] } })
    if (request.method === "GET") {
      const items = !delivered && url.pathname.endsWith("/messages") ? [{ message_id: "incoming", root_id: "root", thread_id: "thread", msg_type: "text", body: { content: JSON.stringify({ text: "Lark reply" }) }, sender: { sender_type: "user" } }] : []
      if (items.length) delivered = true
      return Response.json({ code: 0, data: { items } })
    }
    outgoing.push({ url: url.pathname, body })
    return Response.json({ code: 0, data: { message_id: "root", thread_id: "thread", card_id: "card" } })
  }
  channel = await lark(f.ctx, { appID: "test", appSecret: "test", chatID: "test", mentionEmail: "test@example.invalid", statePath: ":memory:", cardActions: false, streamOutput: false, pollInterval: 5 })
  const message = f.turn("Work")
  await channel.event(event("session.inbox.delivered", { sessionID: "ses_main", inboxID: message.id }))
  await channel.event(event("session.step.started", { sessionID: "ses_main", model: { providerID: "test", id: "test" } }))
  await channel.event(event("session.text.ended", { sessionID: "ses_main", assistantMessageID: "msg_a", ordinal: 0, text: "Completed" }))
  await channel.event(event("session.status", { sessionID: "ses_main", status: { type: "idle" } }))
  expect(outgoing.some((item) => JSON.stringify(item.body).includes("Completed"))).toBe(true)
  now += 2000
  await waitFor(() => f.prompts.length === 1)
  expect(f.prompts[0]).toMatchObject({ sessionID: "ses_main", text: "Lark reply", delivery: "steer" })
  await channel.dispose()
  channel = undefined
})

test("Lark shares a WebSocket and closes it after the last instance unloads", async () => {
  f = fixture()
  websocketStart = spyOn(WSClient.prototype, "start").mockResolvedValue(undefined)
  websocketClose = spyOn(WSClient.prototype, "close").mockImplementation(() => {})
  const options = { appID: "shared-test", appSecret: "test", chatID: "test", statePath: ":memory:", cardActions: true, pollInterval: 5000 }
  const first = await lark(f.ctx, options)
  const second = await lark(f.ctx, options)
  let firstClosed = false
  channel = { async dispose() { if (!firstClosed) await first.dispose(); await second.dispose() } }
  await waitFor(() => websocketStart.mock.calls.length === 1)
  await first.dispose()
  firstClosed = true
  expect(websocketClose).not.toHaveBeenCalled()
  await second.dispose()
  channel = undefined
  expect(websocketClose).toHaveBeenCalled()
})

test("unloading cancels delayed permissions and does not send afterwards", async () => {
  f = fixture()
  const sent = []
  const controller = new AbortController()
  const dispatcher = createDispatcher({
    ctx: f.ctx, signal: controller.signal, composer: new NotificationComposer({ directory: "/test", maxOutputChars: 3000 }),
    notifyDone: true, notifyPermission: true, notifyQuestion: true, permissionNotifyDelay: 10,
    contextLimit: async () => undefined,
    sender: { errorLabel: "test", async ensureSession() {}, async sendDone() {}, async sendPermission(value) { sent.push(value) } },
  })
  channel = { ...dispatcher, async dispose() { controller.abort(); await dispatcher.dispose() } }
  await dispatcher.event(event("permission.asked", { sessionID: "ses_main", id: "per_1", action: "shell", resources: ["git push"] }))
  await channel.dispose()
  await Bun.sleep(20)
  expect(sent).toHaveLength(0)
})
