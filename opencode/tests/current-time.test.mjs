import assert from "node:assert/strict"
import { test } from "node:test"
import currentTime from "../plugins/current-time.ts"

async function fixture() {
  const plugin = await currentTime({})
  const messages = []
  const addTool = (sessionID, callID, status = "completed") => {
    const message = {
      info: { id: `assistant-${callID}`, sessionID, role: "assistant", time: { created: 1 } },
      parts: [{
        id: `part-${callID}`, messageID: `assistant-${callID}`, sessionID, type: "tool", tool: "bash", callID,
        state: status === "completed"
          ? { status, input: {}, output: "done", title: "bash", metadata: {}, time: { start: 1, end: 2 } }
          : { status, input: {}, error: "failed", time: { start: 1, end: 2 } },
      }],
    }
    messages.push(message)
    return message
  }
  return {
    plugin,
    messages,
    addTool,
    async finish(sessionID, callID) {
      await plugin["tool.execute.after"]({ tool: "bash", sessionID, callID, args: {} }, {
        title: "bash", output: "done", metadata: {},
      })
    },
    async transform() {
      const cloned = structuredClone(messages)
      await plugin["experimental.chat.messages.transform"]({}, { messages: cloned })
      return cloned
    },
  }
}

const reminders = (messages) => messages.flatMap(({ parts }) => parts)
  .filter((part) => part.type === "text" && part.synthetic && part.text.includes('source="current-time"'))

test("injects one request-local time reminder after the completed tool", async () => {
  const f = await fixture()
  const message = f.addTool("main", "call-1")
  await f.finish("main", "call-1")
  const transformed = await f.transform()
  const injected = reminders(transformed)
  assert.equal(injected.length, 1)
  assert.equal(transformed[0].parts.at(-1), injected[0])
  assert.equal(injected[0].messageID, message.info.id)
  assert.match(injected[0].text, /Current local time:/)
  assert.equal(reminders(f.messages).length, 0)
  assert.equal(reminders(await f.transform()).length, 0)
})

test("starts the one-minute interval when a reminder is actually inserted", async (t) => {
  let wall = Date.UTC(2026, 8, 21, 6, 32, 18)
  t.mock.method(Date, "now", () => wall)
  const f = await fixture()

  f.addTool("main", "call-1")
  await f.finish("main", "call-1")
  wall += 120_000
  const first = reminders(await f.transform())
  assert.equal(first.length, 1)
  assert.match(first[0].text, /2026/)

  f.addTool("main", "call-2")
  wall += 59_999
  await f.finish("main", "call-2")
  assert.equal(reminders(await f.transform()).length, 0)

  f.addTool("main", "call-3")
  wall += 2
  await f.finish("main", "call-3")
  assert.equal(reminders(await f.transform()).length, 1)
})

test("tracks sessions independently and accepts failed tool results", async (t) => {
  let wall = 10
  t.mock.method(Date, "now", () => wall)
  const f = await fixture()
  f.addTool("one", "one-1")
  f.addTool("two", "two-1", "error")
  await f.finish("one", "one-1")
  await f.finish("two", "two-1")
  const first = reminders(await f.transform())
  assert.deepEqual(new Set(first.map((part) => part.sessionID)), new Set(["one", "two"]))

  wall += 1_000
  f.addTool("one", "one-2")
  f.addTool("three", "three-1")
  await f.finish("one", "one-2")
  await f.finish("three", "three-1")
  const second = reminders(await f.transform())
  assert.deepEqual(second.map((part) => part.sessionID), ["three"])
})

test("waits for the matching completed tool and uses the latest completion", async () => {
  const f = await fixture()
  const old = f.addTool("main", "call-old")
  const latest = f.addTool("main", "call-latest", "running")
  await f.finish("main", "call-old")
  await f.finish("main", "call-latest")
  assert.equal(reminders(await f.transform()).length, 0)
  latest.parts[0].state = { status: "completed", input: {}, output: "done", title: "bash", metadata: {}, time: { start: 1, end: 2 } }
  const transformed = await f.transform()
  assert.equal(reminders(transformed).length, 1)
  assert.equal(reminders(transformed)[0].messageID, latest.info.id)
  assert.equal(reminders(transformed)[0].messageID === old.info.id, false)
})

test("deletion clears throttle and pending state", async (t) => {
  let wall = 100
  t.mock.method(Date, "now", () => wall)
  const f = await fixture()
  f.addTool("main", "call-1")
  await f.finish("main", "call-1")
  assert.equal(reminders(await f.transform()).length, 1)

  f.addTool("main", "call-2")
  await f.finish("main", "call-2")
  await f.plugin.event({ event: { type: "session.deleted", properties: { info: { id: "main" } } } })
  assert.equal(reminders(await f.transform()).length, 0)

  f.addTool("main", "call-3")
  await f.finish("main", "call-3")
  assert.equal(reminders(await f.transform()).length, 1)
})
