import { OpenCode } from "@opencode/client"
import { Service } from "@opencode/client/service"
import { spyOn } from "bun:test"

// Exercise the released HTTP client as well as the plugin registrations.
export function fixture() {
  const records = new Map([["ses_main", {
    id: "ses_main", title: "Original", location: { directory: "/test" },
    projectID: "project", time: { created: 1, updated: 1 },
  }]])
  const history = new Map([["ses_main", []]])
  const forms = new Map()
  const writes = []
  const requests = []
  const prompts = []
  const permissions = []
  const hooks = new Map()
  const tools = new Map()
  const subscriptions = new Set()
  const f = { records, history, forms, writes, requests, prompts, permissions, hooks, tools, subscriptions }
  const transport = async (url, init) => {
    const request = url instanceof Request ? url : new Request(url, init)
    const parsed = new URL(request.url)
    const path = parsed.pathname
    const body = request.method === "GET" ? undefined : await request.clone().json().catch(() => undefined)
    requests.push({ method: request.method, path, query: parsed.searchParams, body })
    const override = await f.route?.(request, parsed, body)
    if (override) return override
    if (parsed.hostname !== "plugin.test") throw new Error(`Unexpected external request: ${parsed.hostname}`)
    if (path === "/api/session") {
      return Response.json({ data: [...records.values()].filter((s) => !s.parentID && s.location.directory === "/test"), cursor: {} })
    }
    if (path === "/api/model") return Response.json({ location: { directory: "/test" }, data: [{ id: "test", providerID: "test", limit: { context: 200000 } }] })
    const [, , , id, resource, child, action] = path.split("/")
    if (resource === "message") {
      if (child) return Response.json({ data: history.get(id)?.find((m) => m.id === child) })
      const type = parsed.searchParams.get("type")
      return Response.json({ data: (history.get(id) ?? []).filter((m) => !type || m.type === type), cursor: {} })
    }
    if (resource === "context") return Response.json({ data: history.get(id) ?? [] })
    if (resource === "prompt") {
      prompts.push({ sessionID: id, ...body })
      return Response.json({ data: { id: "msg_prompt", ...body } })
    }
    if (resource === "permission") {
      permissions.push({ sessionID: id, requestID: child, ...body })
      return new Response(null, { status: 204 })
    }
    if (resource === "form") {
      if (!child) return Response.json({ data: [...forms.values()].filter((form) => form.sessionID === id && form.state.status === "pending") })
      const form = forms.get(child)
      if (request.method === "GET") return Response.json({ data: form })
      form.state = action === "reply" ? { status: "answered", answer: body.answer } : { status: "cancelled" }
      return new Response(null, { status: 204 })
    }
    if (request.method === "PATCH") {
      writes.push({ sessionID: id, ...body })
      Object.assign(records.get(id), body)
      return new Response(null, { status: 204 })
    }
    if (records.has(id)) return Response.json({ data: records.get(id) })
    return Response.json({ message: "missing" }, { status: 404 })
  }
  const originalFetch = globalThis.fetch
  globalThis.fetch = transport
  const discover = spyOn(Service, "discover").mockResolvedValue({ url: "http://plugin.test", auth: undefined })
  const client = OpenCode.make({ baseUrl: "http://plugin.test", fetch: transport })
  f.ctx = {
    app: { version: "2.0.16" }, options: {},
    location: { directory: "/test", project: { id: "project", directory: "/test", canonical: "/test" } },
    session: { ...client.session, async hook(name, callback) { hooks.set(name, callback) } },
    permission: client.permission,
    model: client.model,
    tool: {
      async hook(name, callback) { hooks.set(name, callback) },
      async transform(callback) { callback({ add(tool) { tools.set(tool.name, tool) } }) },
    },
    event: {
      async *subscribe({ signal }) {
        const queue = []
        let wake
        const subscription = { queue, notify() { wake?.() } }
        subscriptions.add(subscription)
        const abort = () => wake?.()
        signal.addEventListener("abort", abort)
        try {
          while (!signal.aborted) {
            if (!queue.length) await new Promise((resolve) => { wake = resolve })
            if (signal.aborted) break
            const { event, resolve } = queue.shift()
            try { yield event } finally { resolve() }
          }
        } finally {
          signal.removeEventListener("abort", abort)
          subscriptions.delete(subscription)
          for (const entry of queue) entry.resolve()
        }
      },
    },
  }
  f.emit = (type, data, location = { directory: "/test" }) => Promise.all([...subscriptions].map((subscription) => new Promise((resolve) => {
    subscription.queue.push({ event: { type, data, location }, resolve })
    subscription.notify()
  })))
  f.turn = (text = "User input", sessionID = "ses_main", files) => {
    const messages = history.get(sessionID) ?? []
    const message = { id: `msg_${messages.length}`, type: "user", text, files, time: { created: messages.length + 1 } }
    messages.push(message)
    history.set(sessionID, messages)
    return message
  }
  f.context = async (sessionID = "ses_main", messages) => {
    const event = { sessionID, messages: structuredClone(messages ?? (history.get(sessionID) ?? [])
      .filter((m) => m.type === "user").map((m) => ({ id: m.id, role: "user", content: [{ type: "text", text: m.text }] }))), system: [], options: {}, tools: {} }
    await hooks.get("context")?.(event)
    return event
  }
  f.rename = (args, sessionID = "ses_main") => tools.get("rename_session").execute(args, { sessionID, signal: new AbortController().signal })
  f.close = async () => {
    try { await f.cleanup?.() } finally {
      discover.mockRestore()
      globalThis.fetch = originalFetch
    }
  }
  return f
}
