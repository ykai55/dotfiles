import { afterEach, expect, spyOn, test } from "bun:test"
import currentTime from "../plugins/current-time/index.ts"
import { fixture } from "./fixture.mjs"

let f
let clock
afterEach(async () => { await f?.close(); clock?.mockRestore() })

const toolTurn = (id, result = { type: "text", value: "done" }) => [
  {
    role: "assistant",
    content: [
      { type: "text", text: "calling" },
      { type: "tool-call", id, name: "shell", input: {} },
    ],
  },
  {
    role: "tool",
    content: [{ type: "tool-result", id, name: "shell", result }],
  },
]
const userMessage = (id, text = "go") => ({ id, role: "user", content: [{ type: "text", text }] })
const reminders = (event) => event.messages.flatMap((message) => message.content).filter((part) => part.text?.includes('source="current-time"'))
const turnGaps = (event) => event.messages.flatMap((message) => message.content).filter((part) => part.text?.includes('source="turn-gap"'))

test("time reminder follows a matching result without persisting or changing the system prompt", async () => {
  f = fixture()
  f.cleanup = await currentTime.setup(f.ctx)
  const original = toolTurn("call_1")
  await f.hooks.get("execute.after")({ sessionID: "ses_main", id: "call_1", status: "completed" })
  const event = await f.context("ses_main", original)
  expect(reminders(event)).toHaveLength(1)
  expect(event.messages[0]).toEqual(original[0])
  expect(event.messages[1]).toEqual(original[1])
  expect(event.messages[2].role).toBe("user")
  expect(original).toHaveLength(2)
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
  expect(reminders(await f.context("ses_main", toolTurn("unrelated")))).toHaveLength(0)
  await finish("new")
  expect(reminders(await f.context("ses_main", toolTurn("old")))).toHaveLength(0)
  expect(reminders(await f.context("ses_main", toolTurn("new")))).toHaveLength(1)
  now += 60000
  await finish("next")
  expect(reminders(await f.context("ses_main", toolTurn("next")))).toHaveLength(0)
  await finish("child", "ses_child")
  expect(reminders(await f.context("ses_child", toolTurn("child")))).toHaveLength(1)
  now += 1
  await finish("next")
  expect(reminders(await f.context("ses_main", toolTurn("next")))).toHaveLength(1)
  await f.emit("session.deleted", { sessionID: "ses_main" })
  await finish("reset")
  expect(reminders(await f.context("ses_main", toolTurn("reset")))).toHaveLength(1)
})

test("failed tool results are eligible and unloading stops the subscription", async () => {
  f = fixture()
  f.cleanup = await currentTime.setup(f.ctx)
  await f.hooks.get("execute.after")({ sessionID: "ses_main", id: "failed", status: "error" })
  expect(reminders(await f.context("ses_main", toolTurn("failed", { type: "error", value: "failed" })))).toHaveLength(1)
  await f.cleanup()
  expect(f.subscriptions.size).toBe(0)
})

test("a long pause appends the previous turn state to the user message", async () => {
  const now = 1_800_000_000_000
  clock = spyOn(Date, "now").mockImplementation(() => now)
  f = fixture()
  f.cleanup = await currentTime.setup(f.ctx)
  f.history.set("ses_main", [
    { id: "msg_prev", type: "assistant", time: { created: now - 700000, completed: now - 600000 } },
    { id: "msg_idle", type: "idle", time: { created: now - 600000 }, outcome: "succeeded" },
    { id: "msg_user", type: "user", time: { created: now }, text: "back" },
  ])
  await f.hooks.get("prompt")({ sessionID: "ses_main", messageID: "msg_user" })
  const event = await f.context("ses_main")
  const text = event.messages[0].content.map((part) => part.text ?? "").join("\n")
  expect(turnGaps(event)).toHaveLength(1)
  expect(text).toContain("Previous turn: succeeded")
  expect(text).toContain("10m 0s")
})

test("a short successful pause leaves the user message unchanged", async () => {
  const now = 1_800_000_000_000
  clock = spyOn(Date, "now").mockImplementation(() => now)
  f = fixture()
  f.cleanup = await currentTime.setup(f.ctx)
  f.history.set("ses_main", [
    { id: "msg_prev", type: "assistant", time: { created: now - 70000, completed: now - 60000 } },
    { id: "msg_idle", type: "idle", time: { created: now - 60000 }, outcome: "succeeded" },
    { id: "msg_user", type: "user", time: { created: now }, text: "back" },
  ])
  await f.hooks.get("prompt")({ sessionID: "ses_main", messageID: "msg_user" })
  const event = await f.context("ses_main")
  expect(turnGaps(event)).toHaveLength(0)
  expect(event.messages[0].content).toHaveLength(1)
})

test("an interrupted turn reports the event reason even after a short pause", async () => {
  const now = 1_800_000_000_000
  clock = spyOn(Date, "now").mockImplementation(() => now)
  f = fixture()
  f.cleanup = await currentTime.setup(f.ctx)
  await f.emit("session.execution.interrupted", { sessionID: "ses_main", reason: "user" })
  f.history.set("ses_main", [
    { id: "msg_prev", type: "assistant", time: { created: now - 20000, completed: now - 10000 } },
    { id: "msg_idle", type: "idle", time: { created: now - 10000 }, outcome: "interrupted" },
    { id: "msg_user", type: "user", time: { created: now }, text: "retry" },
  ])
  await f.hooks.get("prompt")({ sessionID: "ses_main", messageID: "msg_user" })
  const event = await f.context("ses_main")
  const text = event.messages[0].content.map((part) => part.text ?? "").join("\n")
  expect(text).toContain("Previous turn: interrupted (user interrupted)")
  expect(text).toContain("10s")
})

test("tool reminders include elapsed time for the current turn", async () => {
  const now = 1_800_000_000_000
  clock = spyOn(Date, "now").mockImplementation(() => now)
  f = fixture()
  f.cleanup = await currentTime.setup(f.ctx)
  f.history.set("ses_main", [
    { id: "msg_prev", type: "assistant", time: { created: now - 500000, completed: now - 420000 } },
    { id: "msg_idle", type: "idle", time: { created: now - 420000 }, outcome: "succeeded" },
    { id: "msg_user", type: "user", time: { created: now - 120000 }, text: "go" },
  ])
  await f.hooks.get("prompt")({ sessionID: "ses_main", messageID: "msg_user" })
  expect(turnGaps(await f.context("ses_main"))).toHaveLength(1)
  await f.hooks.get("execute.after")({ sessionID: "ses_main", id: "call_1", status: "completed" })
  const event = await f.context("ses_main", [userMessage("msg_user"), ...toolTurn("call_1")])
  expect(reminders(event)).toHaveLength(1)
  expect(reminders(event)[0].text).toContain("This turn started at")
  expect(reminders(event)[0].text).toContain("2m 0s elapsed")
})

test("a compaction after the previous turn is mentioned", async () => {
  const now = 1_800_000_000_000
  clock = spyOn(Date, "now").mockImplementation(() => now)
  f = fixture()
  f.cleanup = await currentTime.setup(f.ctx)
  await f.emit("session.compaction.ended", { sessionID: "ses_main", reason: "auto" })
  f.history.set("ses_main", [
    { id: "msg_prev", type: "assistant", time: { created: now - 700000, completed: now - 600000 } },
    { id: "msg_idle", type: "idle", time: { created: now - 600000 }, outcome: "succeeded" },
    { id: "msg_user", type: "user", time: { created: now }, text: "back" },
  ])
  await f.hooks.get("prompt")({ sessionID: "ses_main", messageID: "msg_user" })
  const event = await f.context("ses_main")
  const text = event.messages[0].content.map((part) => part.text ?? "").join("\n")
  expect(text).toContain("Context auto compaction completed before this message.")
})
