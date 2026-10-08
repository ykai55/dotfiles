import { afterEach, expect, test } from "bun:test"
import renameSelf from "../plugins/rename-self/index.ts"
import { fixture } from "./fixture.mjs"

let f
afterEach(async () => { await f?.close() })
const reminders = (event) => event.messages.flatMap((m) => m.content).filter((p) => p.text?.includes('source="rename-self"'))
async function setup() { f = fixture(); f.cleanup = await renameSelf.setup(f.ctx) }

test("every third delivered user turn gets a request-local reminder, including after restart", async () => {
  await setup()
  f.turn(); f.turn(); f.turn("", "ses_main", [{ uri: "file:///test/image.png" }])
  const context = await f.context()
  expect(reminders(context)).toHaveLength(1)
  expect(reminders(await f.context())).toHaveLength(1)
  expect(JSON.stringify(f.history.get("ses_main"))).not.toContain("system-reminder")
  expect(context.system).toEqual([])
  await f.cleanup()
  f.cleanup = await renameSelf.setup(f.ctx)
  expect(reminders(await f.context())).toHaveLength(1)
  f.turn("A fourth turn")
  expect(reminders(await f.context())).toHaveLength(0)
})

test("history pagination counts only real user input and never child sessions", async () => {
  await setup()
  f.turn(); f.turn(); const latest = f.turn()
  f.route = (request, url) => {
    if (!url.pathname.endsWith("/message")) return
    expect(url.searchParams.get("type")).toBe("user")
    expect(url.searchParams.has("order")).toBe(!url.searchParams.has("cursor"))
    return Response.json(url.searchParams.has("cursor")
      ? { data: [latest, { id: "empty", type: "user", text: "", time: { created: 0 } }], cursor: {} }
      : { data: f.history.get("ses_main").slice(0, 2), cursor: { next: "page2" } })
  }
  expect(reminders(await f.context())).toHaveLength(1)
  f.records.get("ses_main").parentID = "ses_parent"
  expect(reminders(await f.context())).toHaveLength(0)
})

test("native initial titles receive one emoji; existing custom titles are not backfilled", async () => {
  await setup()
  await f.emit("session.renamed", { sessionID: "ses_main", title: "Existing" })
  expect(f.writes).toHaveLength(0)
  f.records.get("ses_main").title = undefined
  await f.emit("session.created", { sessionID: "ses_main", location: { directory: "/test" } })
  f.records.get("ses_main").title = "Native title"
  await f.emit("session.renamed", { sessionID: "ses_main", title: "Native title" })
  expect(f.records.get("ses_main").title).toMatch(/^\p{Extended_Pictographic}.* Native title$/u)
  await f.emit("session.renamed", { sessionID: "ses_main", title: f.records.get("ses_main").title })
  expect(f.writes).toHaveLength(1)
})

test("structured fork events get a fresh prefix and stale title events cannot overwrite edits", async () => {
  await setup()
  Object.assign(f.records.get("ses_main"), { title: "🌱 Source (fork)", fork: { sessionID: "ses_source" } })
  await f.emit("session.forked", { sessionID: "ses_main", parentID: "ses_source" })
  expect(f.records.get("ses_main").title).toEndWith("Source (fork)")
  expect(f.records.get("ses_main").title).not.toStartWith("🌱")
  f.records.get("ses_main").title = undefined
  await f.emit("session.created", { sessionID: "ses_main", location: { directory: "/test" } })
  f.records.get("ses_main").title = "Manual edit"
  await f.emit("session.renamed", { sessionID: "ses_main", title: "Stale title" })
  expect(f.records.get("ses_main").title).toBe("Manual edit")
})

test("rename preserves emoji, enforces grapheme limits and suppresses subsequent reminders", async () => {
  await setup()
  f.turn(); f.turn(); f.turn()
  f.records.get("ses_main").title = "🌱 Existing"
  await expect(f.rename({ name: "x".repeat(29) })).rejects.toThrow("shorten")
  await expect(f.rename({ name: "🌱 Supplied" })).rejects.toThrow("omit")
  await expect(f.rename({ name: "\u202eBad" })).rejects.toThrow("control")
  const result = await f.rename({ name: "e\u0301".repeat(28), reason: "manual" })
  expect(result.content).toContain("🌱")
  expect(reminders(await f.context())).toHaveLength(0)
  await f.rename({ name: "e\u0301".repeat(28) })
  expect(f.writes).toHaveLength(1)
})

test("initial and topic-shift renames add emoji and exclude recent root prefixes", async () => {
  await setup()
  f.records.set("ses_other", { ...f.records.get("ses_main"), id: "ses_other", title: "🌱 Other" })
  f.records.get("ses_main").title = undefined
  await f.rename({ name: "Initial", emoji: false })
  expect(f.records.get("ses_main").title).not.toStartWith("🌱")
  expect(f.records.get("ses_main").title).toEndWith(" Initial")
  f.records.get("ses_main").title = "Plain"
  await f.rename({ name: "New direction", reason: "topic_shift" })
  expect(f.records.get("ses_main").title).not.toBe("New direction")
})

test("failed rename does not poison the queue", async () => {
  await setup()
  f.route = (request) => request.method === "PATCH" ? Response.json({ message: "failure" }, { status: 500 }) : undefined
  await expect(f.rename({ name: "First" })).rejects.toBeDefined()
  f.route = undefined
  await f.rename({ name: "Second" })
  expect(f.records.get("ses_main").title).toBe("Second")
})
