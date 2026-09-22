import assert from "node:assert/strict"
import { test } from "node:test"
import { createOpencodeClient } from "@opencode-ai/sdk"
import renameSelf from "../plugins/rename-self.ts"

const seedling = "\u{1f331}"
const fox = "\u{1f98a}"
const graphemes = (text) => [...new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(text)]

// Exercise the actual SDK transport as well as the exported plugin hooks.
async function fixture(initial = [], history = {}) {
  const records = new Map(initial.map((item) => [item.id, {
    projectID: "project", directory: "/test", version: "1", time: { created: 1, updated: 1 }, ...item,
  }]))
  if (!records.size) records.set("main", {
    id: "main", projectID: "project", directory: "/test", title: "Original", version: "1",
    time: { created: 1, updated: 1 },
  })
  const requests = []
  const writes = []
  const failures = new Set()
  const missing = new Set()
  const transport = { beforeResponse: undefined, onWrite: undefined }
  const client = createOpencodeClient({
    baseUrl: "http://plugin.test",
    fetch: async (request) => {
      const url = new URL(request.url)
      const key = `${request.method} ${url.pathname}`
      requests.push(key)
      assert.equal(url.searchParams.get("directory"), "/test")
      await transport.beforeResponse?.({ request, url, key, records })
      if (failures.has(key)) return Response.json({ name: "UnknownError", data: { message: "test failure" } }, { status: 500 })
      if (missing.has(key)) return new Response(null, { status: 204 })
      if (url.pathname === "/session") {
        assert.equal(url.searchParams.get("roots"), "true")
        assert.equal(url.searchParams.get("limit"), "51")
        return Response.json([...records.values()].filter((record) => !record.parentID)
          .sort((a, b) => b.time.updated - a.time.updated).slice(0, 51))
      }
      const [, , id, resource] = url.pathname.split("/")
      if (resource === "message") return Response.json(history[id] ?? [])
      if (request.method === "PATCH") {
        const { title } = await request.json()
        await transport.onWrite?.({ id, title, records })
        writes.push({ id, title })
        records.set(id, { ...records.get(id), title, time: { created: 1, updated: Date.now() } })
      }
      if (!records.has(id)) return Response.json({ name: "NotFoundError", data: { message: "missing" } }, { status: 404 })
      return Response.json(records.get(id))
    },
  })
  const plugin = await renameSelf({ client, directory: "/test" })
  const context = (id = "main") => ({ sessionID: id, messageID: "assistant", agent: "build", abort: new AbortController().signal })
  return {
    plugin, records, history, requests, writes, failures, missing, transport,
    async event(event) {
      return plugin.event({ event })
    },
    async turn(id, sessionID = "main", parts = [{ type: "text", text: "User input" }]) {
      const message = { id, sessionID, role: "user", time: { created: 1 } }
      await plugin["chat.message"]({ sessionID }, { message, parts })
      const messages = history[sessionID] ??= []
      if (!messages.some(({ info }) => info.id === id)) messages.push({ info: message, parts })
    },
    async prompt(sessionID = "main", messages = history[sessionID] ?? []) {
      const cloned = structuredClone(messages)
      await plugin["experimental.chat.messages.transform"]({}, { messages: cloned })
      return cloned
    },
    async rename(args, id = "main") {
      const definition = plugin.tool.rename_session
      // Use the real schema rather than bypassing argument validation.
      const parsed = Object.fromEntries(Object.entries(definition.args).map(([key, schema]) => [key, schema.parse(args[key])]))
      return definition.execute(parsed, context(id))
    },
  }
}

const defaultTitle = "New session - 2026-09-20T00:00:00.000Z"
const sessionEvent = (type, info) => ({ type, properties: { info } })

const reminderParts = (messages) => messages.flatMap(({ parts }) => parts)
  .filter((part) => part.type === "text" && part.synthetic && part.text.includes('source="rename-self"'))

const userHistory = (count, id = "main") => Array.from({ length: count }, (_, index) => ({
  info: { role: "user", id: `past-${index}`, sessionID: id },
  parts: [{ type: "text", text: "A real turn" }],
}))

test("fork creation replaces the inherited emoji and preserves its title suffix", async () => {
  const f = await fixture([
    { id: "parent", title: `${seedling} Parent topic`, time: { updated: 10 } },
    { id: "fork", title: `${seedling} Parent topic (fork #1)`, time: { updated: 11 } },
  ])
  await f.event(sessionEvent("session.created", f.records.get("fork")))
  const title = f.records.get("fork").title
  const prefix = graphemes(title)[0].segment
  assert.notEqual(prefix, seedling)
  assert.equal(title, `${prefix} Parent topic (fork #1)`)
  assert.equal(f.writes.length, 1)

  await f.event(sessionEvent("session.updated", f.records.get("fork")))
  assert.equal(f.writes.length, 1)
})

test("fork detection also assigns an emoji when the source title had none", async () => {
  const f = await fixture([{ id: "fork", title: "Plain topic (fork #2)" }])
  await f.event(sessionEvent("session.created", f.records.get("fork")))
  assert.match(f.records.get("fork").title, / Plain topic \(fork #2\)$/)
  assert.equal(f.writes.length, 1)
})

test("fork-shaped updates, malformed suffixes and child sessions are not rewritten", async () => {
  const f = await fixture([
    { id: "update", title: `${seedling} Topic (fork #1)` },
    { id: "malformed", title: `${seedling} Topic (fork #x)` },
    { id: "child", title: `${seedling} Topic (fork #1)`, parentID: "update" },
  ])
  await f.event(sessionEvent("session.updated", f.records.get("update")))
  await f.event(sessionEvent("session.created", f.records.get("malformed")))
  await f.event(sessionEvent("session.created", f.records.get("child")))
  assert.equal(f.writes.length, 0)
})

test("stale and duplicate fork events never overwrite a newer title", async () => {
  const f = await fixture([{ id: "fork", title: `${seedling} Topic (fork #1)` }])
  const stale = { ...f.records.get("fork") }
  f.records.set("fork", { ...stale, title: "Newer title" })
  await f.event(sessionEvent("session.created", stale))
  assert.equal(f.records.get("fork").title, "Newer title")
  assert.equal(f.writes.length, 0)

  f.records.set("fork", stale)
  await f.event(sessionEvent("session.created", stale))
  assert.equal(f.records.get("fork").title, stale.title)
  assert.equal(f.writes.length, 0)
})

test("fork update failures warn without repeated writes", async (t) => {
  const warning = t.mock.method(console, "warn", () => {})
  const f = await fixture([{ id: "fork", title: `${seedling} Topic (fork #1)` }])
  f.failures.add("PATCH /session/fork")
  await f.event(sessionEvent("session.created", f.records.get("fork")))
  assert.equal(f.records.get("fork").title, `${seedling} Topic (fork #1)`)
  assert.equal(warning.mock.callCount(), 1)
  await f.event(sessionEvent("session.created", f.records.get("fork")))
  assert.equal(warning.mock.callCount(), 1)
})

test("first native title receives one stable emoji without altering or truncating its text", async () => {
  const longTitle = "Native generated title ".repeat(4).trim()
  const f = await fixture([{ id: "main", title: defaultTitle }])
  await f.event(sessionEvent("session.created", f.records.get("main")))
  f.records.set("main", { ...f.records.get("main"), title: longTitle })
  await f.event(sessionEvent("session.updated", f.records.get("main")))
  const title = f.records.get("main").title
  const prefix = graphemes(title)[0].segment
  assert.equal(title, `${prefix} ${longTitle}`)
  assert.ok(graphemes(title).length > 30)
  assert.equal(f.writes.length, 1)

  await f.event(sessionEvent("session.updated", f.records.get("main")))
  assert.equal(f.writes.length, 1)
  await f.rename({ name: "Later title", reason: "refinement" })
  assert.equal(f.records.get("main").title, `${prefix} Later title`)
})

test("a placeholder seen through the first chat hook also enables initial emoji assignment", async () => {
  const f = await fixture([{ id: "main", title: defaultTitle }])
  await f.turn("first")
  f.records.set("main", { ...f.records.get("main"), title: "Native title" })
  await f.event(sessionEvent("session.updated", f.records.get("main")))
  assert.match(f.records.get("main").title, /^\P{L}.* Native title$/u)
  assert.equal(f.writes.length, 1)
})

test("initial emoji allocation avoids recent roots and ignores child sessions", async () => {
  const f = await fixture([
    { id: "main", title: defaultTitle },
    { id: "recent", title: `${seedling} Recent`, time: { updated: 10 } },
    { id: "child", title: `${fox} Child`, parentID: "main", time: { updated: 20 } },
  ])
  await f.event(sessionEvent("session.created", f.records.get("main")))
  f.records.set("main", { ...f.records.get("main"), title: "Native title" })
  await f.event(sessionEvent("session.updated", f.records.get("main")))
  const prefix = graphemes(f.records.get("main").title)[0].segment
  assert.notEqual(prefix, seedling)
  assert.equal(f.requests.filter((request) => request === "GET /session").length, 1)
})

test("custom, child and already-prefixed first titles are left unchanged", async () => {
  const f = await fixture([
    { id: "custom", title: "Custom title" },
    { id: "child", title: defaultTitle, parentID: "custom" },
    { id: "prefixed", title: defaultTitle },
  ])
  await f.event(sessionEvent("session.created", f.records.get("custom")))
  await f.event(sessionEvent("session.created", f.records.get("child")))
  await f.event(sessionEvent("session.created", f.records.get("prefixed")))
  f.records.set("prefixed", { ...f.records.get("prefixed"), title: `${seedling} Native title` })
  await f.event(sessionEvent("session.updated", f.records.get("prefixed")))
  assert.equal(f.writes.length, 0)
})

test("failed first-title assignment warns, retries at idle, and does not poison later renames", async (t) => {
  const warning = t.mock.method(console, "warn", () => {})
  const f = await fixture([{ id: "main", title: defaultTitle }])
  await f.event(sessionEvent("session.created", f.records.get("main")))
  f.records.set("main", { ...f.records.get("main"), title: "Native title" })
  f.failures.add("PATCH /session/main")
  await f.event(sessionEvent("session.updated", f.records.get("main")))
  assert.equal(f.records.get("main").title, "Native title")
  assert.equal(warning.mock.callCount(), 1)
  f.failures.clear()
  await f.event({ type: "session.idle", properties: { sessionID: "main" } })
  assert.match(f.records.get("main").title, / Native title$/)
  assert.equal(f.writes.length, 1)
  await f.rename({ name: "Recovered" })
  assert.match(f.records.get("main").title, / Recovered$/)
})

test("initial-title recheck never overwrites a newer title", async () => {
  const f = await fixture([{ id: "main", title: defaultTitle }])
  await f.event(sessionEvent("session.created", f.records.get("main")))
  f.records.set("main", { ...f.records.get("main"), title: "First native title" })
  let reads = 0
  f.transport.beforeResponse = async ({ key, records }) => {
    if (key !== "GET /session/main" || ++reads !== 2) return
    records.set("main", { ...records.get("main"), title: "Newer title" })
  }
  await f.event(sessionEvent("session.updated", { ...f.records.get("main"), title: "First native title" }))
  assert.equal(f.records.get("main").title, "Newer title")
  assert.equal(f.writes.length, 0)
})

test("reminds on every third turn by adding one ephemeral synthetic part to the latest user message", async () => {
  const f = await fixture()
  for (let turn = 1; turn <= 6; turn++) {
    await f.turn(`turn-${turn}`)
    const before = JSON.stringify(f.history)
    const output = await f.prompt()
    assert.equal(reminderParts(output).length, turn % 3 === 0 ? 1 : 0)
    assert.equal(JSON.stringify(f.history), before)
    assert.equal(reminderParts(await f.prompt("main", output)).length, reminderParts(output).length)
  }
  assert.equal(f.writes.length, 0)
  assert.match(reminderParts(await f.prompt())[0].text, /Current title.*Original/)
})

test("duplicate delivery and concurrent hooks count each user message once", async () => {
  const f = await fixture()
  await Promise.all([f.turn("one"), f.turn("one"), f.turn("two")])
  assert.equal(reminderParts(await f.prompt()).length, 0)
  await f.turn("three")
  await f.turn("three")
  assert.equal(reminderParts(await f.prompt()).length, 1)
})

test("synthetic, ignored, empty and compaction messages do not count; files do", async () => {
  const f = await fixture()
  await f.turn("one")
  await f.turn("synthetic", "main", [{ type: "text", text: "Continue", synthetic: true }])
  await f.turn("ignored", "main", [{ type: "text", text: "hidden", ignored: true }])
  await f.turn("empty", "main", [{ type: "text", text: "  " }])
  await f.turn("compact", "main", [{ type: "compaction", auto: true }])
  await f.turn("file", "main", [{ type: "file", url: "data:text/plain,a", mime: "text/plain" }])
  assert.equal(reminderParts(await f.prompt()).length, 0)
  await f.turn("three")
  assert.equal(reminderParts(await f.prompt()).length, 1)
  await f.turn("resume", "main", [{ type: "text", text: "Continue", synthetic: true }])
  assert.equal(reminderParts(await f.prompt()).length, 1)
})

test("restores counting from persisted history across plugin restarts", async () => {
  const history = { main: userHistory(2) }
  history.main.push({ info: { role: "assistant", id: "tool-step" }, parts: [{ type: "text", text: "answer" }] })
  history.main.push({ info: { role: "user", id: "summary" }, parts: [{ type: "text", text: "resume", synthetic: true }] })
  const f = await fixture([], history)
  await f.turn("third")
  assert.equal(reminderParts(await f.prompt()).length, 1)
  const restarted = await fixture([...f.records.values()], history)
  await restarted.turn("fourth")
  assert.equal(reminderParts(await restarted.prompt()).length, 0)
})

test("sessions count independently and child sessions do not receive reminders", async () => {
  const f = await fixture([
    { id: "main", title: "Main" }, { id: "other", title: "Other" },
    { id: "child", title: "Child", parentID: "main" },
  ])
  for (let i = 1; i <= 3; i++) {
    await f.turn(`main-${i}`)
    await f.turn(`child-${i}`, "child")
  }
  await f.turn("other-one", "other")
  assert.equal(reminderParts(await f.prompt()).length, 1)
  assert.equal(reminderParts(await f.prompt("other")).length, 0)
  assert.equal(reminderParts(await f.prompt("child")).length, 0)
  assert.equal(reminderParts(await f.prompt("unknown")).length, 0)
  assert.equal(typeof f.plugin["experimental.chat.system.transform"], "undefined")
})

test("idle clears the reminder and session updates cannot create one", async () => {
  const f = await fixture()
  for (let i = 1; i <= 3; i++) await f.turn(String(i))
  await f.plugin.event({ event: { type: "session.updated", properties: { info: { id: "main", title: "Changed" } } } })
  assert.match(reminderParts(await f.prompt())[0].text, /Changed/)
  await f.plugin.event({ event: { type: "session.idle", properties: { sessionID: "main" } } })
  assert.equal(reminderParts(await f.prompt()).length, 0)
  await f.plugin.event({ event: { type: "session.updated", properties: { info: { id: "main", title: "Changed again" } } } })
  assert.equal(reminderParts(await f.prompt()).length, 0)
})

test("removal reloads history, and deletion clears cached session data", async () => {
  const f = await fixture()
  await f.turn("one")
  await f.turn("two")
  f.history.main.pop()
  await f.plugin.event({ event: { type: "message.removed", properties: { sessionID: "main", messageID: "two" } } })
  await f.turn("replacement")
  assert.equal(reminderParts(await f.prompt()).length, 0)
  await f.turn("third")
  assert.equal(reminderParts(await f.prompt()).length, 1)
  await f.plugin.event({ event: { type: "session.deleted", properties: { info: { id: "main" } } } })
  assert.equal(reminderParts(await f.prompt()).length, 0)
})

test("first plugin rename from the default placeholder always assigns an emoji", async () => {
  const f = await fixture([{ id: "main", title: defaultTitle }])
  await f.rename({ name: "dotfiles project structure", reason: "refinement", emoji: false })
  const title = f.records.get("main").title
  const prefix = graphemes(title)[0].segment
  assert.equal(title, `${prefix} dotfiles project structure`)
  assert.equal(f.requests.filter((request) => request === "GET /session").length, 1)
})

test("plain manual/refinement renames remain compatible and identical titles are no-ops", async () => {
  const f = await fixture()
  await f.rename({ name: "  Better\n title  " })
  assert.equal(f.records.get("main").title, "Better title")
  await f.rename({ name: "Better title", reason: "manual" })
  assert.equal(f.writes.length, 1)
  assert.ok(!f.requests.includes("GET /session"))
})

test("topic shifts force a prefix even with emoji=false, avoiding recent emojis", async () => {
  const f = await fixture([
    { id: "main", title: "Original" },
    { id: "recent", title: `${seedling} Recent`, time: { updated: 10 } },
    { id: "recent-2", title: `${fox} Recent`, time: { updated: 9 } },
  ])
  await f.rename({ name: "New direction", reason: "topic_shift", emoji: false })
  const title = f.records.get("main").title
  const prefix = graphemes(title)[0].segment
  assert.notEqual(prefix, seedling)
  assert.notEqual(prefix, fox)
  assert.equal(title, `${prefix} New direction`)
  await f.rename({ name: "Refined direction", emoji: false })
  assert.equal(f.records.get("main").title, `${prefix} Refined direction`)
})

test("optional emoji assignment survives a new plugin instance", async () => {
  const f = await fixture()
  await f.rename({ name: "Optional", emoji: true })
  const prefix = graphemes(f.records.get("main").title)[0].segment
  const restarted = await fixture([...f.records.values()])
  await restarted.rename({ name: "Renamed", reason: "manual" })
  assert.equal(restarted.records.get("main").title, `${prefix} Renamed`)
})

test("existing compound emoji, flag and keycap prefixes remain stable", async () => {
  for (const prefix of ["\u{1f469}\u200d\u{1f4bb}", "\u{1f1e8}\u{1f1f3}", "1\ufe0f\u20e3", "\u2600\ufe0f"]) {
    const f = await fixture([{ id: "main", title: `${prefix} Existing` }])
    await f.rename({ name: "New", reason: "topic_shift" })
    assert.equal(f.records.get("main").title, `${prefix} New`)
    assert.ok(!f.requests.includes("GET /session"))
  }
})

test("30-character limit counts graphemes, including prefix and separator", async () => {
  const f = await fixture()
  const chinese = "\u4e2d"
  await f.rename({ name: chinese.repeat(30) })
  await assert.rejects(f.rename({ name: chinese.repeat(31) }), /shorten/)
  await assert.rejects(f.rename({ name: "x".repeat(29), reason: "topic_shift" }), /at most 28/)
  await f.rename({ name: "e\u0301".repeat(28), reason: "topic_shift" })
  assert.equal(graphemes(f.records.get("main").title).length, 30)
  await assert.rejects(f.rename({ name: "x".repeat(29), emoji: false }), /shorten/)
  assert.equal(f.writes.length, 2)
})

test("rejects blank titles, control characters, agent-selected prefixes and invalid reasons", async () => {
  const f = await fixture()
  for (const name of [" \n ", "unsafe\u0000", "unsafe\u202e", `${seedling} Agent picked`]) {
    await assert.rejects(f.rename({ name }))
  }
  await assert.rejects(f.rename({ name: "Title", reason: "unsupported" }))
  assert.equal(f.writes.length, 0)
})

test("successful rename clears reminders and failures do not poison the queue", async () => {
  const f = await fixture()
  for (let i = 1; i <= 3; i++) await f.turn(String(i))
  f.failures.add("PATCH /session/main")
  await assert.rejects(f.rename({ name: "Failed" }), /update failed/)
  assert.equal(reminderParts(await f.prompt()).length, 1)
  f.failures.clear()
  await f.rename({ name: "Recovered" })
  assert.equal(reminderParts(await f.prompt()).length, 0)
  assert.equal(f.records.get("main").title, "Recovered")
})

test("read/list/missing update responses never report success or write without checks", async () => {
  const f = await fixture()
  f.failures.add("GET /session/main")
  await assert.rejects(f.rename({ name: "No read" }), /could not read/)
  f.failures.clear()
  f.failures.add("GET /session")
  await assert.rejects(f.rename({ name: "No list", reason: "topic_shift" }), /recent session/)
  f.failures.clear()
  f.missing.add("PATCH /session/main")
  await assert.rejects(f.rename({ name: "No response" }), /unexpected title/)
  assert.equal(f.writes.length, 0)
})

test("reminder API failures log a warning without blocking user messages", async (t) => {
  const warning = t.mock.method(console, "warn", () => {})
  const f = await fixture()
  f.failures.add("GET /session/main/message")
  await f.turn("first")
  assert.equal(reminderParts(await f.prompt()).length, 0)
  assert.equal(warning.mock.callCount(), 1)
  f.failures.clear()
  await f.turn("second")
  f.failures.add("GET /session/main")
  await f.turn("third")
  assert.equal(reminderParts(await f.prompt()).length, 0)
  assert.equal(warning.mock.callCount(), 2)
})

test("concurrent allocations in one instance receive distinct prefixes", async () => {
  const f = await fixture([{ id: "main", title: "Main" }, { id: "other", title: "Other" }])
  await Promise.all([
    f.rename({ name: "First", reason: "topic_shift" }),
    f.rename({ name: "Second", reason: "topic_shift" }, "other"),
  ])
  assert.notEqual(graphemes(f.records.get("main").title)[0].segment, graphemes(f.records.get("other").title)[0].segment)
})

test("expanded emoji pool contains 96 distinct prefixes", async () => {
  const source = await import("node:fs/promises").then((fs) => fs.readFile(new URL("../plugins/rename-self.ts", import.meta.url), "utf8"))
  const literals = source.split("const EMOJIS = [", 2)[1].split("]", 1)[0].match(/"[^"]+"/g)
  assert.equal(literals.length, 96)
  assert.equal(new Set(literals).size, 96)
})

test("only the latest 50 non-child sessions participate in emoji exclusion", async () => {
  const f = await fixture()
  for (let i = 0; i < 51; i++) {
    f.records.set(`recent-${i}`, {
      id: `recent-${i}`,
      title: `${seedling} Recent ${i}`,
      time: { updated: i + 1 },
    })
  }
  await f.rename({ name: "Allocated", emoji: true })
  const allocated = graphemes(f.records.get("main").title)[0].segment
  assert.notEqual(allocated, seedling)
  assert.equal(f.requests.filter((request) => request === "GET /session").length, 1)
})
