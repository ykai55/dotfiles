import assert from "node:assert/strict"
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const executable = Bun.which("opencode")
assert(executable, "opencode must be on PATH")
const directory = await mkdtemp(join(tmpdir(), "opencode-v2-smoke-"))
console.log("Smoke directory:", directory)
for (const [key, value] of Object.entries({
  HOME: join(directory, "home"), XDG_CONFIG_HOME: join(directory, "config"),
  XDG_DATA_HOME: join(directory, "data"), XDG_STATE_HOME: join(directory, "state"),
  XDG_CACHE_HOME: join(directory, "cache"),
})) {
  process.env[key] = value
  await mkdir(value, { recursive: true })
}
delete process.env.OPENAI_BASE_URL
delete process.env.OPENAI_API_KEY
// Isolate from explicit connections and inline configuration inherited by the harness.
for (const key of Object.keys(process.env)) if (key.startsWith("OPENCODE_")) delete process.env[key]
process.chdir(directory)
const { OpenCode } = await import("@opencode/client")
const { Service } = await import("@opencode/client/service")
const calls = []
const model = Bun.serve({
  hostname: "127.0.0.1", port: 0,
  async fetch(request) {
    const body = await request.json()
    calls.push(body)
    const tools = body.tools ?? []
    const rename = tools.find((tool) => tool.function?.name.includes("rename_session"))
    const called = body.messages?.some((message) => message.role === "tool")
    const exercise = body.messages?.some((message) => JSON.stringify(message.content).includes("exercise-tools"))
    const toolCalls = rename && exercise && !called ? [{ index: 0, id: "call_smoke", type: "function", function: { name: rename.function.name, arguments: JSON.stringify({ name: "Smoke checked", emoji: true }) } }] : undefined
    const message = toolCalls ? { role: "assistant", content: null, tool_calls: toolCalls } : { role: "assistant", content: "Smoke response" }
    const finish = toolCalls ? "tool_calls" : "stop"
    if (!body.stream) return Response.json({ id: "chatcmpl-smoke", object: "chat.completion", created: 1, model: "smoke", choices: [{ index: 0, message, finish_reason: finish }], usage: { prompt_tokens: 20, completion_tokens: 5, total_tokens: 25 } })
    return new Response([
      `data: ${JSON.stringify({ id: "chatcmpl-smoke", object: "chat.completion.chunk", created: 1, model: "smoke", choices: [{ index: 0, delta: message, finish_reason: null }] })}\n\n`,
      `data: ${JSON.stringify({ id: "chatcmpl-smoke", object: "chat.completion.chunk", created: 1, model: "smoke", choices: [{ index: 0, delta: {}, finish_reason: finish }], usage: { prompt_tokens: 20, completion_tokens: 5, total_tokens: 25 } })}\n\n`,
      "data: [DONE]\n\n",
    ].join(""), { headers: { "content-type": "text/event-stream" } })
  },
})
let started = false
let passed = false
let explicitProcess
try {
  await mkdir(join(directory, "providers"))
  await writeFile(join(directory, "providers", "smoke.json"), JSON.stringify({ provider: { smoke: {
    npm: "@ai-sdk/openai-compatible", options: { baseURL: `http://127.0.0.1:${model.port}/v1`, apiKey: "test" }, models: { smoke: { name: "Smoke" } },
  } } }))
  await writeFile(join(directory, "opencode.json"), JSON.stringify({
    model: "smoke/smoke", snapshots: false,
    agents: { title: { model: "smoke/smoke" } },
    plugins: [
      { package: join(root, "provider-loader"), options: { providersDirectory: join(directory, "providers"), envFile: join(directory, "missing.env") } },
      join(root, "plugins/current-time"), join(root, "plugins/rename-self"),
      { package: join(root, "plugins/chat-notify"), options: { telegram: false, lark: false } },
    ],
  }))
  const reservation = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response() })
  const port = reservation.port
  reservation.stop(true)
  let endpoint
  if (process.argv.includes("--explicit")) {
    process.env.OPENCODE_SERVER_PASSWORD = "smoke-password"
    explicitProcess = Bun.spawn([executable, "serve", "--hostname", "127.0.0.1", "--port", String(port)], {
      env: process.env,
      stdout: Bun.file(join(directory, "server.stdout")), stderr: Bun.file(join(directory, "server.stderr")),
    })
    endpoint = { url: `http://127.0.0.1:${port}`, auth: { type: "basic", username: "opencode", password: "smoke-password" } }
    let ready = false
    for (let attempt = 0; attempt < 100; attempt++) {
      try {
        const response = await fetch(`${endpoint.url}/api/info`, { headers: Service.headers(endpoint) })
        if (response.ok) { ready = true; break }
      } catch {}
      await Bun.sleep(100)
    }
    assert(ready, "Explicit server must start")
  } else {
    const configured = Bun.spawnSync([executable, "service", "set", "port", String(port)], { stdout: "pipe", stderr: "pipe" })
    assert.equal(configured.exitCode, 0, configured.stderr.toString())
    endpoint = await Service.ensure({ command: [executable, "serve", "--service"], version: "2.0.16" })
    started = true
  }
  const client = OpenCode.make({ baseUrl: endpoint.url, headers: Service.headers(endpoint) })
  let local = []
  for (let attempt = 0; attempt < 100; attempt++) {
    const plugins = await client.plugin.list({ location: { directory } })
    local = plugins.data.filter((plugin) => plugin.source.type !== "builtin")
    if (local.length === 4) break
    await Bun.sleep(100)
  }
  console.log("Local plugins:", JSON.stringify(local.map(({ id, state }) => ({ id, state }))))
  assert.equal(local.length, 4)
  assert(local.every((plugin) => plugin.state.status === "active"), "All local plugins must load")
  const session = await client.session.create({ location: { directory } })
  for (const text of ["First turn", "Second turn", "exercise-tools"]) {
    await client.session.prompt({ sessionID: session.id, text })
    await client.session.wait({ sessionID: session.id }, { signal: AbortSignal.timeout(30000) })
    const state = await client.session.get({ sessionID: session.id })
    assert.equal(state.outcome, "succeeded", `Prompt failed: ${text}`)
  }
  const final = await client.session.get({ sessionID: session.id })
  console.log("Final title:", final.title)
  assert(final.title?.endsWith(" Smoke checked"), "The plugin tool must rename the session")
  assert(calls.some((body) => JSON.stringify(body.messages).includes('source=\\"current-time\\"')), "A tool continuation must contain the current-time reminder")
  assert(calls.some((body) => JSON.stringify(body.messages).includes('source=\\"rename-self\\"')), "The third turn must contain the title reminder")
  const history = await client.session.context({ sessionID: session.id })
  assert(!JSON.stringify(history).includes("system-reminder source="), "Reminders must not persist")
  await client.location.reload()
  const reloaded = await client.plugin.list({ location: { directory } })
  assert(reloaded.data.filter((plugin) => plugin.source.type !== "builtin").every((plugin) => plugin.state.status === "active"))
  console.log(`V2 smoke passed: 4 active plugins, 3 user turns, tool execution, request-local reminders and reload (${calls.length} model requests).`)
  passed = true
} catch (error) {
  console.error("Smoke artifacts:", directory)
  await writeFile(join(directory, "model-requests.json"), JSON.stringify(calls, null, 2))
  console.error("Model requests:", calls.length)
  throw error
} finally {
  if (started) await Service.stop()
  if (explicitProcess) { explicitProcess.kill(); await explicitProcess.exited }
  model.stop(true)
  if (passed) await rm(directory, { recursive: true, force: true })
}
