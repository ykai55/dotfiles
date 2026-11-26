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
  expect(reminders(context)[0].text).toContain("仅在不准确时改名")
  expect(reminders(context)[0].text).toContain("不要宣布")
  expect(reminders(context)[0].text).not.toContain("2-3 real user turns")
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
  f.records.get("ses_main").title = "Existing"
  await f.emit("session.created", { sessionID: "ses_main", location: { directory: "/test" } })
  expect(f.writes).toHaveLength(0)
  f.records.set("ses_new", { ...f.records.get("ses_main"), id: "ses_new", title: undefined })
  await f.emit("session.created", { sessionID: "ses_new", location: { directory: "/test" } })
  f.records.get("ses_new").title = "Native title"
  await f.title("ses_new")
  await f.emit("session.renamed", { sessionID: "ses_new", title: "Native title" })
  expect(f.records.get("ses_new").title).toMatch(/^\p{Extended_Pictographic}.* Native title$/u)
  expect(f.records.get("ses_main").title).toBe("Existing")
  await f.emit("session.renamed", { sessionID: "ses_new", title: f.records.get("ses_new").title })
  expect(f.writes).toHaveLength(1)
})

test("an external rename owns the title: no prefix, no reminders, and automatic renames are rejected", async () => {
  await setup()
  f.records.get("ses_main").title = undefined
  await f.emit("session.created", { sessionID: "ses_main", location: { directory: "/test" } })
  f.records.get("ses_main").title = "My title"
  await f.emit("session.renamed", { sessionID: "ses_main", title: "My title" })
  expect(f.writes).toHaveLength(0)
  f.turn(); f.turn(); f.turn()
  expect(reminders(await f.context())).toHaveLength(0)
  await expect(f.rename({ name: "Automatic" })).rejects.toThrow("user set this session title")
  await f.rename({ name: "Explicit", reason: "manual" })
  expect(f.records.get("ses_main").title).toBe("Explicit")
})

test("a manual tool rename records ownership", async () => {
  await setup()
  await f.rename({ name: "Chosen", reason: "manual" })
  expect(f.records.get("ses_main").title).toBe("Chosen")
  await f.cleanup()
  f.cleanup = await renameSelf.setup(f.ctx)
  f.turn(); f.turn(); f.turn()
  expect(reminders(await f.context())).toHaveLength(0)
  await expect(f.rename({ name: "Automatic" })).rejects.toThrow("user set this session title")
  await f.rename({ name: "Explicit", reason: "manual" })
  expect(f.records.get("ses_main").title).toBe("Explicit")
})

test("a requested model-chosen rename replaces and unlocks a user-owned title", async () => {
  await setup()
  await f.rename({ name: "Exact user title", reason: "manual" })
  await f.rename({ name: "Model suggestion", reason: "requested" })
  expect(f.records.get("ses_main").title).toBe("Model suggestion")
  await f.cleanup()
  f.cleanup = await renameSelf.setup(f.ctx)
  await f.rename({ name: "Automatic" })
  expect(f.records.get("ses_main").title).toBe("Automatic")
})

test("temporary renames preserve the existing ownership state", async () => {
  await setup()
  await f.rename({ name: "Watch: Original", reason: "temporary" })
  await f.rename({ name: "Original", reason: "temporary" })
  await f.rename({ name: "Automatic" })
  await f.rename({ name: "Exact user title", reason: "manual" })
  await f.rename({ name: "Watch: Exact user title", reason: "temporary" })
  await f.rename({ name: "Exact user title", reason: "temporary" })
  await expect(f.rename({ name: "Blocked" })).rejects.toThrow("user set this session title")
})

test("a rename during native title generation is external; a completed native title is prefixed", async () => {
  await setup()
  f.records.set("ses_generating", { ...f.records.get("ses_main"), id: "ses_generating", title: undefined })
  f.records.get("ses_main").title = undefined
  await f.emit("session.created", { sessionID: "ses_generating", location: { directory: "/test" } })
  await f.emit("session.created", { sessionID: "ses_main", location: { directory: "/test" } })
  await f.title("ses_generating", false)
  f.records.get("ses_generating").title = "Quick title"
  await f.emit("session.renamed", { sessionID: "ses_generating", title: "Quick title" })
  expect(f.records.get("ses_generating").title).toBe("Quick title")
  f.records.get("ses_main").title = "Native title"
  await f.title("ses_main")
  await f.emit("session.renamed", { sessionID: "ses_main", title: "Native title" })
  expect(f.records.get("ses_main").title).toMatch(/^\p{Extended_Pictographic}.* Native title$/u)
  expect(f.writes).toHaveLength(1)
})

test("an unfinished title response does not make a later manual rename look native", async () => {
  await setup()
  f.records.get("ses_main").title = undefined
  await f.emit("session.created", { sessionID: "ses_main", location: { directory: "/test" } })
  await f.title("ses_main", false)
  f.records.get("ses_main").title = "Manual during generation"
  await f.emit("session.renamed", { sessionID: "ses_main", title: "Manual during generation" })
  expect(f.records.get("ses_main").title).toBe("Manual during generation")
  expect(f.writes).toHaveLength(0)
  await expect(f.rename({ name: "Automatic" })).rejects.toThrow("user set this session title")
})

test("renames of child or foreign sessions are ignored", async () => {
  await setup()
  f.records.set("ses_child", { ...f.records.get("ses_main"), id: "ses_child", parentID: "ses_main", title: "Child title" })
  f.records.set("ses_other", { ...f.records.get("ses_main"), id: "ses_other", title: "Other title", location: { directory: "/other" } })
  await f.emit("session.renamed", { sessionID: "ses_child", title: "Child renamed" })
  await f.emit("session.renamed", { sessionID: "ses_other", title: "Other renamed" })
  expect(f.writes).toHaveLength(0)
  expect([...f.storage.keys()]).toHaveLength(0)
})

test("the plugin's own title update is not mistaken for a user rename", async () => {
  await setup()
  f.records.get("ses_main").title = "🌱 Existing"
  await f.rename({ name: "Managed", reason: "topic_shift" })
  const title = f.records.get("ses_main").title
  await f.emit("session.renamed", { sessionID: "ses_main", title })
  await f.rename({ name: "Next", reason: "topic_shift" })
  expect(f.records.get("ses_main").title).toContain("Next")
})

test("title ownership survives a plugin reload", async () => {
  await setup()
  f.records.get("ses_main").title = "My title"
  await f.emit("session.renamed", { sessionID: "ses_main", title: "My title" })
  await f.cleanup()
  f.cleanup = await renameSelf.setup(f.ctx)
  f.turn(); f.turn(); f.turn()
  expect(reminders(await f.context())).toHaveLength(0)
  await expect(f.rename({ name: "Automatic" })).rejects.toThrow("user set this session title")
  await f.emit("session.deleted", { sessionID: "ses_main" })
  expect([...f.storage.keys()]).toHaveLength(0)
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
  await f.rename({ name: "e\u0301".repeat(28), reason: "manual" })
  expect(f.writes).toHaveLength(1)
})

test("initial and topic-shift renames add emoji and exclude recent root prefixes across projects", async () => {
  await setup()
  f.records.set("ses_other", { ...f.records.get("ses_main"), id: "ses_other", title: "🌱 Other", location: { directory: "/other" }, projectID: "other" })
  f.route = (request, url) => {
    if (url.pathname !== "/api/session") return
    expect(url.searchParams.has("directory")).toBe(false)
    return Response.json({ data: [...f.records.values()], cursor: {} })
  }
  f.records.get("ses_main").title = undefined
  await f.rename({ name: "Initial", emoji: false })
  expect(f.records.get("ses_main").title).not.toStartWith("🌱")
  expect(f.records.get("ses_main").title).toEndWith(" Initial")
  f.records.get("ses_main").title = "Plain"
  await f.rename({ name: "New direction", reason: "topic_shift" })
  expect(f.records.get("ses_main").title).not.toBe("New direction")
})

test("renames from outside the tool keep the emoji the plugin assigned", async () => {
  await setup()
  f.records.get("ses_main").title = undefined
  await f.emit("session.created", { sessionID: "ses_main", location: { directory: "/test" } })
  f.records.get("ses_main").title = "Native title"
  await f.title("ses_main")
  await f.emit("session.renamed", { sessionID: "ses_main", title: "Native title" })
  const emoji = f.records.get("ses_main").title.split(" ")[0]
  expect(emoji).toMatch(/^\p{Extended_Pictographic}/u)

  // OpenChamber's work.name replaces the whole title, as the rename dialog does.
  f.records.get("ses_main").title = "Task name"
  await f.emit("session.renamed", { sessionID: "ses_main", title: "Task name" })
  expect(f.records.get("ses_main").title).toBe(`${emoji} Task name`)

  f.records.get("ses_main").title = "🌊 Chosen elsewhere"
  await f.emit("session.renamed", { sessionID: "ses_main", title: "🌊 Chosen elsewhere" })
  f.records.get("ses_main").title = "Task name again"
  await f.emit("session.renamed", { sessionID: "ses_main", title: "Task name again" })
  expect(f.records.get("ses_main").title).toBe("🌊 Task name again")
})

test("external renames keep the emoji across a plugin restart", async () => {
  await setup()
  f.records.get("ses_main").title = undefined
  await f.emit("session.created", { sessionID: "ses_main", location: { directory: "/test" } })
  f.records.get("ses_main").title = "Native title"
  await f.title("ses_main")
  await f.emit("session.renamed", { sessionID: "ses_main", title: "Native title" })
  const prefixed = f.records.get("ses_main").title
  await f.cleanup()
  f.cleanup = await renameSelf.setup(f.ctx)
  f.records.get("ses_main").title = "Task name"
  await f.emit("session.renamed", { sessionID: "ses_main", title: "Task name" })
  expect(f.records.get("ses_main").title).toBe(prefixed.replace("Native title", "Task name"))
})

test("failed rename does not poison the queue", async () => {
  await setup()
  f.route = (request) => request.method === "PATCH" ? Response.json({ message: "failure" }, { status: 500 }) : undefined
  await expect(f.rename({ name: "First" })).rejects.toBeDefined()
  f.route = undefined
  await f.rename({ name: "Second" })
  expect(f.records.get("ses_main").title).toBe("Second")
})
