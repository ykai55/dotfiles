import { afterEach, expect, spyOn, test } from "bun:test"
import currentTime from "../plugins/current-time/index.ts"
import { fixture } from "./fixture.mjs"

let f
let clock
afterEach(async () => { await f?.close(); clock?.mockRestore() })
const messages = (id) => [{ role: "tool", content: [{ type: "tool-result", id, name: "shell", result: { type: "text", value: "done" } }] }]
const reminders = (event) => event.messages.flatMap((message) => message.content).filter((part) => part.text?.includes('source="current-time"'))

test("time reminder follows a matching result without persisting or changing the system prompt", async () => {
  f = fixture()
  f.cleanup = await currentTime.setup(f.ctx)
  const original = messages("call_1")
  await f.hooks.get("execute.after")({ sessionID: "ses_main", id: "call_1", status: "completed" })
  const event = await f.context("ses_main", original)
  expect(reminders(event)).toHaveLength(1)
  expect(event.messages[0]).toEqual(original[0])
  expect(event.messages[1].role).toBe("user")
  expect(original).toHaveLength(1)
  expect(event.system).toEqual([])
  expect(reminders(await f.context("ses_main", original))).toHaveLength(0)
})

test("time throttling starts on consumption, is per session, and resets on deletion", async () => {
  let now = 100000
  clock = spyOn(Date, "now").mockImplementation(() => now)
  f = fixture()
  f.cleanup = await currentTime.setup(f.ctx)
  const finish = (id, sessionID = "ses_main") => f.hooks.get("execute.after")({ sessionID, id })
  await finish("old")
  now += 120000
  expect(reminders(await f.context("ses_main", messages("unrelated")))).toHaveLength(0)
  await finish("new")
  expect(reminders(await f.context("ses_main", messages("old")))).toHaveLength(0)
  expect(reminders(await f.context("ses_main", messages("new")))).toHaveLength(1)
  now += 60000
  await finish("next")
  expect(reminders(await f.context("ses_main", messages("next")))).toHaveLength(0)
  await finish("child", "ses_child")
  expect(reminders(await f.context("ses_child", messages("child")))).toHaveLength(1)
  now += 1
  await finish("next")
  expect(reminders(await f.context("ses_main", messages("next")))).toHaveLength(1)
  await f.emit("session.deleted", { sessionID: "ses_main" })
  await finish("reset")
  expect(reminders(await f.context("ses_main", messages("reset")))).toHaveLength(1)
})

test("failed tool results are eligible and unloading stops the subscription", async () => {
  f = fixture()
  f.cleanup = await currentTime.setup(f.ctx)
  await f.hooks.get("execute.after")({ sessionID: "ses_main", id: "failed", status: "error" })
  const input = messages("failed")
  input[0].content[0].result = { type: "error", value: "failed" }
  expect(reminders(await f.context("ses_main", input))).toHaveLength(1)
  await f.cleanup()
  expect(f.subscriptions.size).toBe(0)
})
