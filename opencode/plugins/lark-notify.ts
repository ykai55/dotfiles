import { Database } from "bun:sqlite"
import { mkdirSync } from "node:fs"
import { dirname } from "node:path"
import { fileURLToPath } from "node:url"
import type { Plugin } from "@opencode-ai/plugin"

const readConfigFile = async (filePath: string) => {
  const file = Bun.file(filePath)
  if (!(await file.exists())) return {}
  return Object.fromEntries(
    (await file.text())
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line && !line.startsWith("#"))
      .map((line) => {
        const index = line.indexOf("=")
        if (index === -1) return
        return [
          line.slice(0, index).trim(),
          line
            .slice(index + 1)
            .trim()
            .replace(/^['"]|['"]$/g, ""),
        ]
      })
      .filter((entry): entry is [string, string] => Array.isArray(entry)),
  )
}

const readPluginConfig = () => readConfigFile(`${dirname(fileURLToPath(import.meta.url))}/lark-notify.conf`)

const conf = (values: Record<string, string>, key: string) => values[key]?.trim()

const textOption = (value: unknown, fallback?: string) => {
  if (typeof value !== "string") return fallback
  const trimmed = value.trim()
  if (!trimmed) return fallback
  return trimmed
}

const numberOption = (value: unknown) => {
  if (typeof value === "number" && Number.isFinite(value)) return value
  if (typeof value !== "string") return
  const parsed = Number(value)
  if (Number.isFinite(parsed)) return parsed
}

const boolOption = (value: unknown, fallback: boolean) => {
  if (typeof value === "boolean") return value
  return fallback
}

const boolText = (value: unknown, fallback: boolean) => {
  if (value === "1" || value === "true") return true
  if (value === "0" || value === "false") return false
  return fallback
}

const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object"

const prop = (value: unknown, key: string) => {
  if (!record(value)) return
  return value[key]
}

const shortID = (value: unknown) => (typeof value === "string" ? value.slice(0, 8) : "unknown")

const patterns = (value: unknown) => {
  if (!Array.isArray(value)) return ""
  return value.filter((item): item is string => typeof item === "string").join(", ")
}

const truncate = (value: string, limit: number) => {
  if (value.length <= limit) return value
  return `[truncated first ${value.length - limit} chars]\n\n${value.slice(-limit).trimStart()}`
}

const truncateEnd = (value: string, limit: number) => {
  if (value.length <= limit) return value
  return `${value.slice(0, limit).trimEnd()}...`
}

const compactNumber = (value: number) => {
  if (value >= 1_000_000) return `${Number((value / 1_000_000).toFixed(value >= 10_000_000 ? 0 : 1))}M`
  if (value >= 1_000) return `${Number((value / 1_000).toFixed(value >= 100_000 ? 0 : 1))}K`
  return String(Math.round(value))
}

type SessionStats = {
  muted: boolean
  introSent: boolean
  rootMessageID: string | undefined
  rootMessagePromise: Promise<string | undefined> | undefined
  threadID: string | undefined
  sessionTitle: string | undefined
  userInput: string
  userPartIDs: Set<string>
  ignoredPartIDs: Set<string>
  textByPart: Map<string, string>
  toolCalls: Set<string>
  readFiles: Set<string>
  writtenFiles: Set<string>
  changedFiles: Set<string>
  contextTokens: number | undefined
  contextLimit: number | undefined
}

type SessionRow = {
  muted: number | null
  intro_sent: number | null
  root_message_id: string | null
  thread_id: string | null
  session_title: string | null
}

type SessionLookupRow = {
  session_id: string
  directory: string | null
  muted: number | null
}

const tokenCount = (tokens: unknown) => {
  if (!record(tokens)) return
  const cache = prop(tokens, "cache")
  return (
    (numberOption(prop(tokens, "input")) ?? 0) +
    (numberOption(prop(tokens, "output")) ?? 0) +
    (numberOption(prop(tokens, "reasoning")) ?? 0) +
    (numberOption(prop(cache, "read")) ?? 0) +
    (numberOption(prop(cache, "write")) ?? 0)
  )
}

const contextLimitFrom = (value: unknown) =>
  numberOption(prop(prop(value, "limit"), "context")) ??
  numberOption(prop(value, "context")) ??
  numberOption(prop(value, "contextLimit"))

const modelRef = (value: unknown) => {
  const providerID = textOption(prop(value, "providerID"))
  const modelID = textOption(prop(value, "modelID") ?? prop(value, "id"))
  if (!providerID || !modelID) return
  return { providerID, modelID }
}

const toolInput = (part: unknown) => prop(prop(part, "state"), "input")

const stringList = (value: unknown) => {
  if (!Array.isArray(value)) return []
  return value.filter((item): item is string => typeof item === "string" && item.trim().length > 0)
}

const inputPaths = (input: unknown) =>
  [
    prop(input, "filePath"),
    prop(input, "filepath"),
    prop(input, "path"),
    prop(input, "cwd"),
    ...stringList(prop(input, "files")),
    ...stringList(prop(input, "paths")),
  ].filter((item): item is string => typeof item === "string" && item.trim().length > 0)

const patchPaths = (input: unknown) => {
  const patch = prop(input, "patch")
  if (typeof patch !== "string") return []
  return Array.from(patch.matchAll(/^\*\*\* (?:Add File|Update File|Delete File): (.+)$/gm), (match) => match[1])
}

const larkText = (text: string) => JSON.stringify({ text })

const larkMessageText = (message: unknown) => {
  const content = prop(prop(message, "body"), "content")
  if (typeof content !== "string") return
  try {
    const parsed = JSON.parse(content)
    return textOption(prop(parsed, "text"))
  } catch {
    return textOption(content)
  }
}

export default (async (input, options) => {
  const config = await readPluginConfig()
  const appID = textOption(options?.appID, conf(config, "LARK_APP_ID"))
  const appSecret = textOption(options?.appSecret, conf(config, "LARK_APP_SECRET"))
  const chatID = textOption(options?.chatID, conf(config, "LARK_CHAT_ID"))
  const notifyDone = boolOption(options?.notifyDone, true)
  const notifyPermission = boolOption(options?.notifyPermission, true)
  const notifyQuestion = boolOption(options?.notifyQuestion, true)
  const maxOutputChars = numberOption(options?.maxOutputChars ?? conf(config, "LARK_MAX_OUTPUT_CHARS")) ?? 3000
  const permissionNotifyDelay =
    numberOption(options?.permissionNotifyDelay ?? conf(config, "LARK_PERMISSION_NOTIFY_DELAY")) ?? 5000
  const pollInterval = numberOption(options?.pollInterval ?? conf(config, "LARK_POLL_INTERVAL_MS")) ?? 5000
  const dbPath =
    textOption(options?.statePath, conf(config, "LARK_STATE_DB")) ??
    `${process.env.HOME ?? input.directory}/.config/opencode/lark-notify.sqlite`

  mkdirSync(dirname(dbPath), { recursive: true })
  const db = new Database(dbPath, { create: true })
  db.run("PRAGMA journal_mode = WAL")
  db.run("PRAGMA busy_timeout = 5000")
  db.run(`
    CREATE TABLE IF NOT EXISTS session_state (
      project_id TEXT NOT NULL,
      session_id TEXT NOT NULL,
      directory TEXT,
      muted INTEGER NOT NULL DEFAULT 0,
      intro_sent INTEGER NOT NULL DEFAULT 0,
      root_message_id TEXT,
      thread_id TEXT,
      session_title TEXT,
      updated_at INTEGER NOT NULL,
      PRIMARY KEY (project_id, session_id)
    )
  `)
  db.run(`
    CREATE TABLE IF NOT EXISTS sent_message (
      message_id TEXT PRIMARY KEY,
      updated_at INTEGER NOT NULL
    )
  `)
  db.run(`
    CREATE TABLE IF NOT EXISTS lark_poll_lock (
      id TEXT PRIMARY KEY,
      owner TEXT NOT NULL,
      expires_at INTEGER NOT NULL
    )
  `)
  const selectSession = db.query(`
    SELECT muted, intro_sent, root_message_id, thread_id, session_title
    FROM session_state
    WHERE project_id = ? AND session_id = ?
  `)
  const selectSessionByRoot = db.query(`
    SELECT session_id, directory, muted
    FROM session_state
    WHERE root_message_id = ?
    ORDER BY updated_at DESC
    LIMIT 1
  `)
  const selectSessionByThread = db.query(`
    SELECT session_id, directory, muted
    FROM session_state
    WHERE thread_id = ?
    ORDER BY updated_at DESC
    LIMIT 1
  `)
  const upsertSession = db.query(`
    INSERT INTO session_state (
      project_id, session_id, directory, muted, intro_sent, root_message_id, thread_id, session_title, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(project_id, session_id) DO UPDATE SET
      directory = excluded.directory,
      muted = excluded.muted,
      intro_sent = excluded.intro_sent,
      root_message_id = excluded.root_message_id,
      thread_id = excluded.thread_id,
      session_title = excluded.session_title,
      updated_at = excluded.updated_at
  `)
  const insertSentMessage = db.query(`
    INSERT OR REPLACE INTO sent_message (message_id, updated_at)
    VALUES (?, ?)
  `)
  const selectSentMessage = db.query(`
    SELECT message_id
    FROM sent_message
    WHERE message_id = ?
  `)
  const upsertPollLock = db.query(`
    INSERT INTO lark_poll_lock (id, owner, expires_at)
    VALUES ('lark-input', ?, ?)
    ON CONFLICT(id) DO UPDATE SET
      owner = excluded.owner,
      expires_at = excluded.expires_at
    WHERE lark_poll_lock.owner = excluded.owner OR lark_poll_lock.expires_at < ?
  `)
  const releasePollLock = db.query(`
    DELETE FROM lark_poll_lock
    WHERE id = 'lark-input' AND owner = ?
  `)

  const activeSessions = new Set<string>()
  const notifiedRequests = new Set<string>()
  const pendingPermissionTimers = new Map<string, { sessionID: string; timer: ReturnType<typeof setTimeout> }>()
  const statsBySession = new Map<string, SessionStats>()
  const processedMessageIDs = new Set<string>()
  const pollOwner = `${input.project.id}:${input.directory}:${Math.random().toString(36).slice(2)}`
  let providerListPromise: Promise<unknown[] | undefined> | undefined
  let token: { value: string; expiresAt: number } | undefined

  function providerList() {
    providerListPromise ??= input.client.config
      .providers({ directory: input.directory })
      .then((response) => {
        const providers = prop(prop(response, "data"), "providers")
        if (Array.isArray(providers)) return providers
      })
      .catch((error) => {
        console.warn("lark-notify plugin: failed to load providers", error instanceof Error ? error.message : error)
        return undefined
      })
    return providerListPromise
  }

  async function modelContextLimit(model: unknown) {
    const direct = contextLimitFrom(model)
    if (direct !== undefined) return direct
    const ref = modelRef(model)
    if (!ref) return
    const provider = (await providerList())?.find((item) => prop(item, "id") === ref.providerID)
    const models = prop(provider, "models")
    if (!record(models)) return
    const found =
      prop(models, ref.modelID) ??
      Object.values(models).find(
        (item) => prop(item, "id") === ref.modelID || prop(prop(item, "api"), "id") === ref.modelID,
      )
    return contextLimitFrom(found)
  }

  function stats(sessionID: string) {
    const existing = statsBySession.get(sessionID)
    if (existing) return existing
    const row = selectSession.get(input.project.id, sessionID) as SessionRow | null
    const next = {
      muted: row?.muted === 1,
      introSent: row?.intro_sent === 1,
      rootMessageID: row?.root_message_id ?? undefined,
      rootMessagePromise: undefined,
      threadID: row?.thread_id ?? undefined,
      sessionTitle: row?.session_title ?? undefined,
      userInput: "",
      userPartIDs: new Set<string>(),
      ignoredPartIDs: new Set<string>(),
      textByPart: new Map<string, string>(),
      toolCalls: new Set<string>(),
      readFiles: new Set<string>(),
      writtenFiles: new Set<string>(),
      changedFiles: new Set<string>(),
      contextTokens: undefined,
      contextLimit: undefined,
    }
    statsBySession.set(sessionID, next)
    return next
  }

  function saveSession(sessionID: string) {
    const current = statsBySession.get(sessionID)
    if (!current) return
    upsertSession.run(
      input.project.id,
      sessionID,
      input.directory,
      current.muted ? 1 : 0,
      current.introSent ? 1 : 0,
      current.rootMessageID ?? null,
      current.threadID ?? null,
      current.sessionTitle ?? null,
      Date.now(),
    )
  }

  function clearRound(sessionID: string) {
    const current = statsBySession.get(sessionID)
    if (!current) return
    current.userInput = ""
    current.userPartIDs.clear()
    current.ignoredPartIDs.clear()
    current.textByPart.clear()
    current.toolCalls.clear()
    current.readFiles.clear()
    current.writtenFiles.clear()
    current.changedFiles.clear()
    current.contextTokens = undefined
    current.contextLimit = undefined
    saveSession(sessionID)
  }

  function muteSession(sessionID: string) {
    const current = stats(sessionID)
    current.muted = true
    saveSession(sessionID)
    clearSessionPermissionTimers(sessionID)
  }

  function isMuted(sessionID: unknown) {
    return typeof sessionID === "string" && statsBySession.get(sessionID)?.muted === true
  }

  function trackUserParts(sessionID: string, parts: unknown) {
    if (!Array.isArray(parts)) return ""
    const current = stats(sessionID)
    for (const part of parts) {
      const id = prop(part, "id")
      if (typeof id === "string") current.userPartIDs.add(id)
    }
    current.userInput = parts
      .map((part) => prop(part, "text"))
      .filter((text): text is string => typeof text === "string" && text.trim().length > 0)
      .join("\n\n")
      .trim()
    return current.userInput
  }

  function trackTool(sessionID: string, part: unknown) {
    const tool = prop(part, "tool")
    const callID = prop(part, "callID")
    if (typeof tool !== "string" || typeof callID !== "string") return
    const current = stats(sessionID)
    if (current.toolCalls.has(callID)) return
    current.toolCalls.add(callID)
    current.textByPart.clear()

    const paths = [...inputPaths(toolInput(part)), ...patchPaths(toolInput(part))]
    if (["read", "grep", "glob", "lsp"].includes(tool)) {
      for (const file of paths) current.readFiles.add(file)
      return
    }
    if (["write", "edit", "apply_patch"].includes(tool)) {
      for (const file of paths) {
        current.writtenFiles.add(file)
        current.changedFiles.add(file)
      }
    }
  }

  function trackContext(sessionID: string, source: unknown) {
    const current = stats(sessionID)
    const tokens = tokenCount(prop(source, "tokens") ?? source)
    if (tokens !== undefined && tokens > 0) current.contextTokens = tokens
    current.contextLimit = contextLimitFrom(source) ?? current.contextLimit
  }

  function trackPart(sessionID: string, part: unknown) {
    const partType = prop(part, "type")
    const partID = prop(part, "id")
    if ((partType === "reasoning" || prop(part, "synthetic") === true) && typeof partID === "string") {
      const current = stats(sessionID)
      current.ignoredPartIDs.add(partID)
      current.textByPart.delete(partID)
      return
    }
    if (partType === "text" && typeof partID === "string") {
      const current = stats(sessionID)
      if (current.userPartIDs.has(partID) || current.ignoredPartIDs.has(partID)) return
      const text = prop(part, "text")
      if (typeof text === "string") current.textByPart.set(partID, text)
      return
    }
    if (partType === "tool") {
      trackTool(sessionID, part)
      return
    }
    if (partType === "step-finish") {
      trackContext(sessionID, part)
      return
    }
    if (partType === "patch") {
      for (const file of stringList(prop(part, "files"))) {
        stats(sessionID).writtenFiles.add(file)
        stats(sessionID).changedFiles.add(file)
      }
    }
  }

  function doneMessage(sessionID: string) {
    const current = statsBySession.get(sessionID)
    const user = current?.userInput ? `> user: ${truncateEnd(current.userInput, 200)}\n\n` : ""
    const output = truncate(
      Array.from(current?.textByPart.values() ?? [])
        .join("\n\n")
        .trim() || "(no text output)",
      maxOutputChars,
    )
    const context = current?.contextTokens
      ? ` · ${
          current.contextLimit
            ? `${Math.round((current.contextTokens / current.contextLimit) * 100)}%`
            : compactNumber(current.contextTokens)
        }`
      : ""
    const meta = `\n\n> tools ${current?.toolCalls.size ?? 0} · r/w/c ${current?.readFiles.size ?? 0}/${
      current?.writtenFiles.size ?? 0
    }/${current?.changedFiles.size ?? 0}${context}`
    return `${user}${output}${meta}`
  }

  async function tenantToken() {
    if (token && token.expiresAt > Date.now() + 60_000) return token.value
    if (!appID || !appSecret) {
      console.warn("lark-notify plugin: missing LARK_APP_ID or LARK_APP_SECRET")
      return
    }
    const response = await fetch("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ app_id: appID, app_secret: appSecret }),
    })
    const data = (await response.json()) as {
      code?: number
      msg?: string
      tenant_access_token?: string
      expire?: number
    }
    if (response.ok && data.code === 0 && data.tenant_access_token) {
      token = { value: data.tenant_access_token, expiresAt: Date.now() + (data.expire ?? 3600) * 1000 }
      return token.value
    }
    console.warn(`lark-notify plugin: tenant token failed (${response.status}) ${data.msg ?? JSON.stringify(data)}`)
  }

  async function larkAPI(method: "GET" | "POST", path: string, query?: Record<string, string>, body?: unknown) {
    const nextToken = await tenantToken()
    if (!nextToken) return
    const url = new URL(`https://open.feishu.cn/open-apis${path}`)
    for (const [key, value] of Object.entries(query ?? {})) url.searchParams.set(key, value)
    const response = await fetch(url, {
      method,
      headers: {
        authorization: `Bearer ${nextToken}`,
        ...(method === "POST" ? { "content-type": "application/json" } : {}),
      },
      body: method === "POST" ? JSON.stringify(body ?? {}) : undefined,
    })
    const data = (await response.json()) as { code?: number; msg?: string; data?: unknown }
    if (response.ok && data.code === 0) return data.data
    console.warn(`lark-notify plugin: ${path} failed (${response.status}) ${data.msg ?? JSON.stringify(data)}`)
  }

  async function send(text: string, rootMessageID?: string) {
    if (!chatID) {
      console.warn("lark-notify plugin: missing LARK_CHAT_ID")
      return
    }
    const data = rootMessageID
      ? await larkAPI("POST", `/im/v1/messages/${encodeURIComponent(rootMessageID)}/reply`, undefined, {
          msg_type: "text",
          content: larkText(text),
        })
      : await larkAPI(
          "POST",
          "/im/v1/messages",
          { receive_id_type: "chat_id" },
          { receive_id: chatID, msg_type: "text", content: larkText(text) },
        )
    const messageID = textOption(prop(data, "message_id") ?? prop(data, "messageId"))
    if (messageID) {
      insertSentMessage.run(messageID, Date.now())
      processedMessageIDs.add(messageID)
    }
    return {
      messageID,
      threadID: textOption(prop(data, "thread_id") ?? prop(data, "threadId") ?? prop(data, "root_id")),
    }
  }

  async function rootMessage(sessionID: string) {
    const current = stats(sessionID)
    if (current.muted) return
    if (current.rootMessageID) return current.rootMessageID
    if (current.rootMessagePromise) return current.rootMessagePromise
    current.rootMessagePromise = send(
      [
        `${sessionID} · ${input.directory}`,
        `> user: ${current.userInput || "(unknown input)"}`,
        "",
        "OpenCode session",
      ].join("\n"),
    ).then((result) => {
      current.rootMessageID = result?.messageID
      current.threadID = result?.threadID
      current.rootMessagePromise = undefined
      current.introSent = true
      saveSession(sessionID)
      return current.rootMessageID
    })
    return current.rootMessagePromise
  }

  async function sendReply(sessionID: string, text: string) {
    const rootID = await rootMessage(sessionID)
    if (!rootID) return
    const result = await send(text, rootID)
    if (result?.threadID) {
      stats(sessionID).threadID = result.threadID
      saveSession(sessionID)
    }
  }

  function clearPermissionTimer(requestID: string) {
    const pending = pendingPermissionTimers.get(requestID)
    if (!pending) return
    clearTimeout(pending.timer)
    pendingPermissionTimers.delete(requestID)
  }

  function clearSessionPermissionTimers(sessionID: string) {
    for (const [requestID, pending] of pendingPermissionTimers) {
      if (pending.sessionID !== sessionID) continue
      clearTimeout(pending.timer)
      pendingPermissionTimers.delete(requestID)
    }
  }

  function sessionForLarkMessage(message: unknown) {
    const rootID = textOption(prop(message, "root_id") ?? prop(message, "parent_id"))
    if (rootID) {
      const row = selectSessionByRoot.get(rootID) as SessionLookupRow | null
      if (row && row.muted !== 1) return { sessionID: row.session_id, directory: row.directory ?? input.directory }
    }

    const threadID = textOption(prop(message, "thread_id"))
    if (!threadID) return
    const row = selectSessionByThread.get(threadID) as SessionLookupRow | null
    if (row && row.muted !== 1) return { sessionID: row.session_id, directory: row.directory ?? input.directory }
  }

  async function handleLarkMessage(message: unknown) {
    const messageID = textOption(prop(message, "message_id"))
    if (!messageID) return
    if (processedMessageIDs.has(messageID)) return
    processedMessageIDs.add(messageID)
    if (processedMessageIDs.size > 1000) processedMessageIDs.clear()
    if (selectSentMessage.get(messageID)) return
    if (prop(prop(message, "sender"), "sender_type") === "app") return

    const target = sessionForLarkMessage(message)
    if (!target) return
    const text = larkMessageText(message)
    if (!text) return

    const result = await input.client.session.promptAsync({
      path: { id: target.sessionID },
      query: { directory: target.directory },
      body: { parts: [{ type: "text", text }] },
    })
    if (prop(result, "error")) console.warn("lark-notify plugin: failed to forward Lark message", prop(result, "error"))
  }

  function acquirePollLock(ttl = 35_000) {
    upsertPollLock.run(pollOwner, Date.now() + ttl, Date.now())
    return db.query("SELECT changes() AS count").get() as { count: number }
  }

  async function pollMessages(startTime: number, endTime: number) {
    if (!chatID) return
    let pageToken: string | undefined
    do {
      const data = await larkAPI("GET", "/im/v1/messages", {
        container_id_type: "chat",
        container_id: chatID,
        start_time: String(startTime),
        end_time: String(endTime),
        page_size: "50",
        sort_type: "ByCreateTimeAsc",
        ...(pageToken ? { page_token: pageToken } : {}),
      })
      const items = prop(data, "items")
      if (Array.isArray(items)) {
        for (const item of items) await handleLarkMessage(item)
      }
      pageToken = textOption(prop(data, "page_token"))
      if (prop(data, "has_more") !== true) return
    } while (pageToken)
  }

  async function pollLark() {
    if (!appID || !appSecret || !chatID) return
    let cursor = Math.floor(Date.now() / 1000)
    while (true) {
      try {
        if (acquirePollLock().count !== 1) {
          await new Promise((resolve) => setTimeout(resolve, pollInterval))
          continue
        }
        const now = Math.floor(Date.now() / 1000)
        if (now > cursor) {
          await pollMessages(cursor, now)
          cursor = now
        }
      } catch (error) {
        console.warn("lark-notify plugin: polling failed", error instanceof Error ? error.message : error)
        releasePollLock.run(pollOwner)
      }
      await new Promise((resolve) => setTimeout(resolve, pollInterval))
    }
  }

  void pollLark()

  return {
    async "chat.message"(messageInput, output) {
      stats(messageInput.sessionID).userInput = trackUserParts(messageInput.sessionID, output.parts)
      if (!isMuted(messageInput.sessionID)) {
        const limit = await modelContextLimit(messageInput.model)
        if (limit !== undefined) stats(messageInput.sessionID).contextLimit = limit
        await rootMessage(messageInput.sessionID)
      }
    },

    async event(eventInput) {
      const event = eventInput.event
      const eventType = prop(event, "type")
      const properties = prop(event, "properties")
      const sessionID = prop(properties, "sessionID")

      try {
        if (eventType === "session.created" || eventType === "session.updated") {
          const info = prop(properties, "info")
          if (typeof sessionID !== "string") return
          if (typeof prop(info, "parentID") === "string") {
            muteSession(sessionID)
            return
          }
          const sessionTitle = textOption(prop(info, "title"))
          trackContext(sessionID, info)
          const limit = await modelContextLimit(prop(info, "model"))
          if (limit !== undefined) stats(sessionID).contextLimit = limit
          if (sessionTitle) {
            stats(sessionID).sessionTitle = sessionTitle
            saveSession(sessionID)
          }
          return
        }

        if (eventType === "session.next.step.started") {
          if (typeof sessionID !== "string") return
          const limit = await modelContextLimit(prop(properties, "model"))
          if (limit !== undefined) stats(sessionID).contextLimit = limit
          return
        }

        if (eventType === "session.next.step.ended") {
          if (typeof sessionID !== "string") return
          trackContext(sessionID, properties)
          return
        }

        if (eventType === "message.part.delta") {
          const partID = prop(properties, "partID")
          const field = prop(properties, "field")
          const delta = prop(properties, "delta")
          if (
            typeof sessionID !== "string" ||
            typeof partID !== "string" ||
            field !== "text" ||
            typeof delta !== "string"
          )
            return
          const current = stats(sessionID)
          if (current.userPartIDs.has(partID) || current.ignoredPartIDs.has(partID)) return
          current.textByPart.set(partID, `${current.textByPart.get(partID) ?? ""}${delta}`)
          return
        }

        if (eventType === "message.part.updated") {
          if (typeof sessionID !== "string") return
          trackPart(sessionID, prop(properties, "part"))
          return
        }

        if (eventType === "session.status") {
          const status = prop(properties, "status")
          const statusType = prop(status, "type")
          if (typeof sessionID !== "string") return
          if (statusType === "busy" || statusType === "retry") {
            activeSessions.add(sessionID)
            return
          }
          clearSessionPermissionTimers(sessionID)
          if (statusType !== "idle" || !activeSessions.delete(sessionID) || !notifyDone) return
          if (isMuted(sessionID)) {
            clearRound(sessionID)
            return
          }
          await sendReply(sessionID, doneMessage(sessionID))
          clearRound(sessionID)
          return
        }

        if (eventType === "permission.replied") {
          const requestID = prop(properties, "requestID")
          if (typeof requestID !== "string") return
          clearPermissionTimer(requestID)
          return
        }

        if (eventType === "permission.asked" && notifyPermission) {
          const requestID = prop(properties, "id")
          if (typeof sessionID !== "string") return
          if (isMuted(sessionID)) return
          if (typeof requestID !== "string") return
          const key = `permission:${sessionID}:${prop(properties, "permission")}:${patterns(prop(properties, "patterns"))}`
          if (notifiedRequests.has(key)) return
          notifiedRequests.add(key)
          pendingPermissionTimers.set(requestID, {
            sessionID,
            timer: setTimeout(() => {
              pendingPermissionTimers.delete(requestID)
              void sendReply(
                sessionID,
                [
                  `permission needed · ${shortID(sessionID)}`,
                  `permission: ${textOption(prop(properties, "permission"), "unknown")}`,
                  `patterns: ${patterns(prop(properties, "patterns")) || "unknown"}`,
                ].join("\n"),
              )
            }, permissionNotifyDelay),
          })
          return
        }

        if (eventType === "question.asked" && notifyQuestion) {
          const requestID = prop(properties, "id")
          if (typeof sessionID !== "string") return
          if (isMuted(sessionID)) return
          const key = `question:${requestID}`
          if (notifiedRequests.has(key)) return
          notifiedRequests.add(key)
          await sendReply(sessionID, `opencode is waiting for your answer\nsession: ${shortID(sessionID)}`)
        }
      } catch (error) {
        console.warn("lark-notify plugin:", error instanceof Error ? error.message : error)
      }
    },
  }
}) satisfies Plugin
