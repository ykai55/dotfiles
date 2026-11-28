import { afterEach, expect, spyOn, test } from "bun:test"
import plugin from "../plugins/gpt-imagegen/index.ts"

let request
afterEach(() => request?.mockRestore())

test("chat attachments use the newest user image with the requested name", async () => {
  let tool
  const old = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10, 1]).toString("base64")
  const latest = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10, 2]).toString("base64")
  await plugin.setup({
    tool: { transform: async fn => fn({ add: value => { tool = value } }) },
    command: { transform: async fn => fn({ add() {} }) },
    session: {
      get: async () => ({ location: { directory: "/workspace" } }),
      context: async ({ sessionID }) => {
        expect(sessionID).toBe("ses_attachment")
        return [
          { type: "user", files: [{ name: "image-1.png", mime: "image/png", data: old }] },
          { type: "user", files: [{ name: "image-1.png", mime: "image/png", data: latest }] },
          { type: "user", text: "Edit that image" },
        ]
      },
    },
    integration: { connection: { active: async () => ({}), resolve: async () => ({ type: "oauth", methodID: "chatgpt-browser", access: "test-token" }) } },
  })
  request = spyOn(globalThis, "fetch").mockImplementation(async (_url, options) => {
    const body = JSON.parse(options.body)
    expect(body.input[0].content[1].image_url).toBe(`data:image/png;base64,${latest}`)
    return new Response(JSON.stringify({ error: { message: "test stops before generation" } }), { status: 400 })
  })
  await expect(tool.execute({ prompt: "Replace SU7 with YU7", attachments: ["image-1.png"] }, {
    sessionID: "ses_attachment", signal: new AbortController().signal,
  })).rejects.toThrow("test stops before generation")
  expect(request).toHaveBeenCalledTimes(1)
})

test("missing attachments fail before auth or network, rather than generating without an input image", async () => {
  let tool
  await plugin.setup({
    tool: { transform: async fn => fn({ add: value => { tool = value } }) },
    command: { transform: async fn => fn({ add() {} }) },
    session: {
      get: async () => ({ location: { directory: "/workspace" } }),
      context: async () => [],
    },
  })
  await expect(tool.execute({ prompt: "Edit image", attachments: ["missing.png"] }, {
    sessionID: "ses_attachment", signal: new AbortController().signal,
  })).rejects.toThrow("Attachment not found: missing.png")
})

const chat = { type: "credential", id: "chat", label: "Main", method: "oauth" }
const unrelated = { ...chat, id: "other", label: "Other" }
const imageLogin = { ...chat, id: "image", label: "Main (Codex images)" }

async function withConnections(connections, resolved) {
  let tool
  await plugin.setup({
    tool: { transform: async fn => fn({ add: v => { tool = v } }) },
    command: { transform: async fn => fn({ add() {} }) },
    session: { get: async () => ({ location: { directory: "/workspace" } }) },
    integration: {
      get: async () => ({ data: { connections } }),
      connection: {
        active: async () => chat,
        resolve: async c => {
          resolved.push(c.id)
          return { type: "oauth", methodID: c === chat ? "chatgpt-token-sharing" : "chatgpt-browser",
            access: `${c.id}-token`, metadata: c === imageLogin ? { accountID: "test-workspace" } : {} }
        },
      },
    },
  })
  return tool.execute({ prompt: "test" }, { sessionID: "ses_image", signal: new AbortController().signal })
}

test("token sharing uses only its explicitly paired Codex login, including its workspace header", async () => {
  const resolved = []
  request = spyOn(globalThis, "fetch").mockImplementation(async (url, options) => {
    expect(url).toBe("https://chatgpt.com/backend-api/codex/responses")
    expect(options.headers.Authorization).toBe("Bearer image-token")
    expect(options.headers["ChatGPT-Account-Id"]).toBe("test-workspace")
    return new Response(JSON.stringify({ detail: "stop after auth" }), { status: 400 })
  })
  await expect(withConnections([chat, unrelated, imageLogin], resolved)).rejects.toThrow("stop after auth")
  expect(resolved).toEqual(["chat", "image"])
})

test("a missing or ambiguous pairing never sends the prompt or falls back to another account", async () => {
  request = spyOn(globalThis, "fetch").mockImplementation(() => { throw new Error("unexpected upstream request") })
  for (const entries of [[chat, unrelated], [chat, imageLogin, { ...imageLogin, id: "duplicate" }]]) {
    const resolved = []
    await expect(withConnections(entries, resolved)).rejects.toThrow("Main (Codex images)")
    expect(resolved).toEqual(["chat"])
  }
  expect(request).not.toHaveBeenCalled()
})
