import { Database } from "bun:sqlite"
import { mkdirSync } from "node:fs"
import { dirname } from "node:path"
import { fileURLToPath } from "node:url"
import type { Plugin } from "@opencode-ai/plugin"
import { NotificationComposer, type CompactionNotice, type DoneNotice, type SessionNotice } from "./composer"
import { createDispatcher } from "./dispatcher"

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

const readPluginConfig = () => readConfigFile(`${dirname(dirname(fileURLToPath(import.meta.url)))}/chat-notify.conf`)

const env = (values: Record<string, string>, key: string) => values[key]?.trim()

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

const html = (value: string) =>
  value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;")

const width = (value: string) => Array.from(value).length

const pad = (value: string, size: number) => `${value}${" ".repeat(Math.max(0, size - width(value)))}`

const tableCells = (line: string) =>
  line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((cell) => cell.trim())

const tableSeparator = (line: string) => /^:?-{3,}:?$/.test(line.trim())

const tableDivider = (line: string) => {
  if (!line.includes("|")) return false
  return tableCells(line).every(tableSeparator)
}

const renderTable = (rows: string[][]) => {
  const widths = rows[0].map((_, index) => Math.max(...rows.map((row) => width(row[index] ?? ""))))
  return rows.map((row) => row.map((cell, index) => pad(cell, widths[index])).join("  ")).join("\n")
}

const inlineMarkdown = (value: string) =>
  html(value)
    .replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, '<a href="$2">$1</a>')
    .replace(/`([^`\n]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*\n]+)\*\*/g, "<b>$1</b>")

const markdownTables = (value: string) => {
  const lines = value.split("\n")
  const result: string[] = []
  for (let index = 0; index < lines.length; index++) {
    if (!lines[index].includes("|") || !tableDivider(lines[index + 1] ?? "")) {
      result.push(inlineMarkdown(lines[index]))
      continue
    }

    const rows = [tableCells(lines[index])]
    index += 2
    while (index < lines.length && lines[index].includes("|")) {
      rows.push(tableCells(lines[index]))
      index++
    }
    index--
    result.push(`<pre><code>${html(renderTable(rows))}</code></pre>`)
  }
  return result.join("\n")
}

const renderMarkdownChunk = (value: string) => (value ? markdownTables(value) : "")

const markdown = (value: string) => {
  const lines = value.split("\n")
  const result: string[] = []
  let text: string[] = []
  let code: string[] | undefined

  function flushText() {
    if (!text.length) return
    result.push(renderMarkdownChunk(text.join("\n")))
    text = []
  }

  function flushCode() {
    if (!code) return
    result.push(`<pre><code>${html(code.join("\n").trim())}</code></pre>`)
    code = undefined
  }

  for (const line of lines) {
    if (line.trimStart().startsWith("```")) {
      if (code) {
        flushCode()
        continue
      }
      flushText()
      code = []
      continue
    }
    if (code) {
      code.push(line)
      continue
    }
    text.push(line)
  }

  flushText()
  flushCode()
  return result.join("\n")
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
  rootMessageID: number | undefined
  rootMessagePromise: Promise<number | undefined> | undefined
  threadID: number | undefined
  threadPromise: Promise<number | undefined> | undefined
  threadName: string | undefined
  sessionTitle: string | undefined
  userInput: string
  contextTokens: number | undefined
  contextLimit: number | undefined
}

type SessionRow = {
  muted: number | null
  intro_sent: number | null
  root_message_id: number | null
  thread_id: number | null
  thread_name: string | null
  session_title: string | null
}

type SessionLookupRow = {
  session_id: string
  directory: string | null
  muted: number | null
}

type PermissionLookupRow = {
  request_id: string
  session_id: string
  directory: string | null
  thread_id: number | null
  message_id: number | null
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

const contextLabel = (tokens: number | undefined, limit: number | undefined, approximate = false) => {
  if (tokens === undefined) return "unknown"
  const value = limit ? `${Math.round((tokens / limit) * 100)}%` : compactNumber(tokens)
  return approximate ? `~${value}` : value
}

const estimateTextTokens = (value: unknown) =>
  typeof value === "string" && value.trim().length > 0 ? Math.ceil(Array.from(value).length / 3) : 0

export default (async (input, options) => {
  const config = await readPluginConfig()
  const token = textOption(options?.token, env(config, "TELEGRAM_BOT_TOKEN"))
  const chatID = textOption(options?.chatID, env(config, "TELEGRAM_CHAT_ID"))
  const messageThreadID = numberOption(options?.messageThreadID ?? env(config, "TELEGRAM_MESSAGE_THREAD_ID"))
  const forumTopics = boolOption(options?.forumTopics, boolText(env(config, "TELEGRAM_FORUM_TOPICS"), true))
  const notifyDone = boolOption(options?.notifyDone, true)
  const notifyPermission = boolOption(options?.notifyPermission, true)
  const notifyQuestion = boolOption(options?.notifyQuestion, true)
  const maxOutputChars = numberOption(options?.maxOutputChars ?? env(config, "TELEGRAM_MAX_OUTPUT_CHARS")) ?? 3000
  const permissionNotifyDelay =
    numberOption(options?.permissionNotifyDelay ?? env(config, "TELEGRAM_PERMISSION_NOTIFY_DELAY")) ?? 5000
  const dbPath =
    textOption(options?.statePath, env(config, "TELEGRAM_STATE_DB")) ??
    `${process.env.HOME ?? input.directory}/.config/opencode/telegram-notify.sqlite`
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
      root_message_id INTEGER,
      thread_id INTEGER,
      thread_name TEXT,
      session_title TEXT,
      updated_at INTEGER NOT NULL,
      PRIMARY KEY (project_id, session_id)
    )
  `)
  db.run(`
    CREATE TABLE IF NOT EXISTS telegram_poll_lock (
      id TEXT PRIMARY KEY,
      owner TEXT NOT NULL,
      expires_at INTEGER NOT NULL
    )
  `)
  db.run(`
    CREATE TABLE IF NOT EXISTS permission_request (
      request_id TEXT PRIMARY KEY,
      session_id TEXT NOT NULL,
      directory TEXT,
      thread_id INTEGER,
      message_id INTEGER,
      updated_at INTEGER NOT NULL
    )
  `)
  const columns = new Set(
    db
      .query("PRAGMA table_info(session_state)")
      .all()
      .map((column) => (column as { name: string }).name),
  )
  if (!columns.has("intro_sent")) db.run("ALTER TABLE session_state ADD COLUMN intro_sent INTEGER NOT NULL DEFAULT 0")
  if (!columns.has("directory")) db.run("ALTER TABLE session_state ADD COLUMN directory TEXT")
  const selectSession = db.query(`
    SELECT muted, intro_sent, root_message_id, thread_id, thread_name, session_title
    FROM session_state
    WHERE project_id = ? AND session_id = ?
  `)
  const selectSessionByThread = db.query(`
    SELECT session_id, directory, muted
    FROM session_state
    WHERE thread_id = ?
    ORDER BY updated_at DESC
    LIMIT 1
  `)
  const selectSessionByRootMessage = db.query(`
    SELECT session_id, directory, muted
    FROM session_state
    WHERE root_message_id = ?
    ORDER BY updated_at DESC
    LIMIT 1
  `)
  const upsertSession = db.query(`
    INSERT INTO session_state (
      project_id, session_id, directory, muted, intro_sent, root_message_id, thread_id, thread_name, session_title, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(project_id, session_id) DO UPDATE SET
      directory = excluded.directory,
      muted = excluded.muted,
      intro_sent = excluded.intro_sent,
      root_message_id = excluded.root_message_id,
      thread_id = excluded.thread_id,
      thread_name = excluded.thread_name,
      session_title = excluded.session_title,
      updated_at = excluded.updated_at
  `)
  const upsertPollLock = db.query(`
    INSERT INTO telegram_poll_lock (id, owner, expires_at)
    VALUES ('telegram-input', ?, ?)
    ON CONFLICT(id) DO UPDATE SET
      owner = excluded.owner,
      expires_at = excluded.expires_at
    WHERE telegram_poll_lock.owner = excluded.owner OR telegram_poll_lock.expires_at < ?
  `)
  const releasePollLock = db.query(`
    DELETE FROM telegram_poll_lock
    WHERE id = 'telegram-input' AND owner = ?
  `)
  const upsertPermission = db.query(`
    INSERT INTO permission_request (request_id, session_id, directory, thread_id, message_id, updated_at)
    VALUES (?, ?, ?, ?, ?, ?)
    ON CONFLICT(request_id) DO UPDATE SET
      session_id = excluded.session_id,
      directory = excluded.directory,
      thread_id = excluded.thread_id,
      message_id = excluded.message_id,
      updated_at = excluded.updated_at
  `)
  const selectPermission = db.query(`
    SELECT request_id, session_id, directory, thread_id, message_id
    FROM permission_request
    WHERE request_id = ?
  `)
  const deletePermission = db.query(`
    DELETE FROM permission_request
    WHERE request_id = ?
  `)
  const statsBySession = new Map<string, SessionStats>()
  const telegramUpdateIDs = new Set<number>()
  const pollOwner = `${input.project.id}:${input.directory}:${Math.random().toString(36).slice(2)}`
  let providerListPromise: Promise<unknown[] | undefined> | undefined

  function providerList() {
    providerListPromise ??= input.client.config
      .providers({ directory: input.directory })
      .then((response) => {
        const providers = prop(prop(response, "data"), "providers")
        if (Array.isArray(providers)) return providers
      })
      .catch((error) => {
        console.warn("telegram-notify plugin: failed to load providers", error instanceof Error ? error.message : error)
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

  function estimateContextTokens(messages: unknown) {
    if (!Array.isArray(messages)) return
    const tokens = messages
      .map((message) => {
        const messageType = prop(message, "type")
        if (messageType === "compaction") return estimateTextTokens(prop(message, "summary"))
        if (messageType === "user" || messageType === "synthetic") return estimateTextTokens(prop(message, "text"))
        if (messageType === "shell")
          return estimateTextTokens(prop(message, "command")) + estimateTextTokens(prop(message, "output"))
        if (messageType !== "assistant") return 0
        const content = prop(message, "content")
        if (!Array.isArray(content)) return 0
        return content
          .map((item) => {
            if (prop(item, "type") === "text" || prop(item, "type") === "reasoning")
              return estimateTextTokens(prop(item, "text"))
            return 0
          })
          .reduce((sum, item) => sum + item, 0)
      })
      .reduce((sum, item) => sum + item, 0)
    if (tokens > 0) return tokens
  }

  async function activeContextTokens(sessionID: string) {
    const session = prop(prop(input.client, "v2"), "session")
    const context = prop(session, "context")
    if (typeof context !== "function") return undefined
    return context
      .call(session, { sessionID, directory: input.directory })
      .then((response) => estimateContextTokens(prop(response, "data")))
      .catch((error) => {
        console.warn(
          "telegram-notify plugin: failed to load active context",
          error instanceof Error ? error.message : error,
        )
        return undefined
      })
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
      threadPromise: undefined,
      threadName: row?.thread_name ?? undefined,
      sessionTitle: row?.session_title ?? undefined,
      userInput: "",
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
      current.threadName ?? null,
      current.sessionTitle ?? null,
      Date.now(),
    )
  }

  function applySessionNotice(notice: SessionNotice) {
    const current = stats(notice.sessionID)
    current.userInput = notice.userInput
    current.sessionTitle = notice.sessionTitle
    current.contextTokens = notice.contextTokens
    current.contextLimit = notice.contextLimit
  }

  function doneNoticeMessage(notice: DoneNotice) {
    const user = notice.userInput
      ? `<blockquote expandable>${html(`user: ${truncateEnd(notice.userInput, 200)}`)}</blockquote>`
      : ""
    const context = notice.contextTokens
      ? ` · ${
          notice.contextLimit
            ? `${Math.round((notice.contextTokens / notice.contextLimit) * 100)}%`
            : compactNumber(notice.contextTokens)
        }`
      : ""
    return [
      user,
      markdown(notice.output),
      "",
      `<blockquote expandable>${html(
        `tools ${notice.tools} · r/w/c ${notice.read}/${notice.written}/${notice.changed}${context}`,
      )}</blockquote>`,
    ]
      .filter(Boolean)
      .join("\n")
  }

  const compactionMessage = (notice: CompactionNotice) =>
    html(
      `compacted · ${contextLabel(notice.beforeTokens, notice.beforeLimit)} -> ${contextLabel(
        notice.afterTokens,
        notice.afterLimit,
        notice.afterTokens !== undefined,
      )}`,
    )

  async function sendIntro(sessionID: string, target: { replyTo?: number; threadID?: number }) {
    const current = stats(sessionID)
    if (current.introSent) return
    await send(
      [
        `<b>${html(sessionID)}</b> · <code>${html(input.directory)}</code>`,
        `<blockquote expandable>${html(`user: ${current.userInput || "(unknown input)"}`)}</blockquote>`,
      ].join("\n"),
      target,
    )
    current.introSent = true
    saveSession(sessionID)
  }

  async function send(text: string, options?: { replyTo?: number; threadID?: number; replyMarkup?: unknown }) {
    if (!token || !chatID) {
      console.warn("telegram-notify plugin: missing TELEGRAM_BOT_TOKEN or TELEGRAM_CHAT_ID")
      return
    }

    const threadID = options?.threadID ?? messageThreadID
    const response = await fetch(`https://api.telegram.org/bot${token}/sendMessage`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        chat_id: chatID,
        text,
        parse_mode: "HTML",
        disable_web_page_preview: true,
        ...(options?.replyTo === undefined
          ? {}
          : { reply_parameters: { message_id: options.replyTo, allow_sending_without_reply: true } }),
        ...(threadID === undefined ? {} : { message_thread_id: threadID }),
        ...(options?.replyMarkup === undefined ? {} : { reply_markup: options.replyMarkup }),
      }),
    })

    const data = (await response.json()) as { ok?: boolean; result?: { message_id?: number }; description?: string }
    if (response.ok && data.ok !== false) return data.result?.message_id
    console.warn(
      `telegram-notify plugin: Telegram sendMessage failed (${response.status}) ${data.description ?? JSON.stringify(data)}`,
    )
  }

  async function telegramPost(method: string, body: Record<string, unknown>) {
    if (!token) return
    const response = await fetch(`https://api.telegram.org/bot${token}/${method}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    })
    const data = (await response.json()) as { ok?: boolean; description?: string }
    if (response.ok && data.ok !== false) return true
    console.warn(
      `telegram-notify plugin: ${method} failed (${response.status}) ${data.description ?? JSON.stringify(data)}`,
    )
    return false
  }

  async function answerCallback(callbackID: string, text?: string) {
    await telegramPost("answerCallbackQuery", {
      callback_query_id: callbackID,
      ...(text ? { text } : {}),
    })
  }

  async function editReplyMarkup(messageID: number, threadID?: number) {
    if (!chatID || messageID <= 0) return
    await telegramPost("editMessageReplyMarkup", {
      chat_id: chatID,
      message_id: messageID,
      reply_markup: { inline_keyboard: [] },
      ...(threadID === undefined ? {} : { message_thread_id: threadID }),
    })
  }

  async function replyPermission(row: PermissionLookupRow, reply: "once" | "always" | "reject") {
    const permission = prop(input.client, "permission")
    const replyFn = prop(permission, "reply")
    if (typeof replyFn === "function") {
      const result = await replyFn.call(permission, {
        requestID: row.request_id,
        reply,
        directory: row.directory ?? input.directory,
      })
      return !prop(result, "error")
    }

    const result = await input.client.postSessionIdPermissionsPermissionId({
      path: { id: row.session_id, permissionID: row.request_id },
      query: { directory: row.directory ?? input.directory },
      body: { response: reply },
    })
    return !prop(result, "error")
  }

  async function handlePermissionCallback(callback: unknown) {
    const callbackID = textOption(prop(callback, "id"))
    const data = textOption(prop(callback, "data"))
    if (!callbackID || !data) return
    const match = /^op:perm:(once|always|reject):(.+)$/.exec(data)
    if (!match) return

    const message = prop(callback, "message")
    if (record(message) && `${prop(prop(message, "chat"), "id")}` !== chatID) {
      await answerCallback(callbackID, "Wrong chat")
      return
    }

    const row = selectPermission.get(match[2]) as PermissionLookupRow | null
    if (!row) {
      await answerCallback(callbackID, "Permission request is no longer pending")
      return
    }

    if (!(await replyPermission(row, match[1] as "once" | "always" | "reject"))) {
      await answerCallback(callbackID, "Failed")
      return
    }

    deletePermission.run(row.request_id)
    await answerCallback(
      callbackID,
      match[1] === "reject" ? "Denied" : match[1] === "always" ? "Always allowed" : "Allowed",
    )
    await editReplyMarkup(numberOption(prop(message, "message_id")) ?? row.message_id ?? 0, row.thread_id ?? undefined)
  }

  async function react(message: unknown, emoji = "👌") {
    const messageID = numberOption(prop(message, "message_id"))
    if (!token || !chatID || messageID === undefined) return
    const response = await fetch(`https://api.telegram.org/bot${token}/setMessageReaction`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        chat_id: chatID,
        message_id: messageID,
        reaction: [{ type: "emoji", emoji }],
      }),
    })
    const data = (await response.json()) as { ok?: boolean; description?: string }
    if (response.ok && data.ok !== false) return
    console.warn(
      `telegram-notify plugin: setMessageReaction failed (${response.status}) ${
        data.description ?? JSON.stringify(data)
      }`,
    )
  }

  async function telegramUpdates(offset: number | undefined, timeout: number) {
    if (!token || !chatID) return []
    const response = await fetch(`https://api.telegram.org/bot${token}/getUpdates`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        timeout,
        allowed_updates: ["message", "callback_query"],
        ...(offset === undefined ? {} : { offset }),
      }),
    })
    const data = (await response.json()) as { ok?: boolean; result?: unknown[]; description?: string }
    if (response.ok && data.ok !== false) return data.result ?? []
    console.warn(
      `telegram-notify plugin: getUpdates failed (${response.status}) ${data.description ?? JSON.stringify(data)}`,
    )
    return []
  }

  function acquirePollLock(ttl = 35_000) {
    upsertPollLock.run(pollOwner, Date.now() + ttl, Date.now())
    return db.query("SELECT changes() AS count").get() as { count: number }
  }

  async function initialTelegramOffset() {
    const updates = await telegramUpdates(undefined, 0)
    return updates
      .map((update) => numberOption(prop(update, "update_id")))
      .filter((updateID): updateID is number => updateID !== undefined)
      .reduce((offset, updateID) => Math.max(offset, updateID + 1), 0)
  }

  function telegramMessageSession(message: unknown) {
    const threadID = numberOption(prop(message, "message_thread_id"))
    if (threadID !== undefined) {
      const row = selectSessionByThread.get(threadID) as SessionLookupRow | null
      if (row && row.muted !== 1) return { sessionID: row.session_id, directory: row.directory ?? input.directory }
    }

    const replyID = numberOption(prop(prop(message, "reply_to_message"), "message_id"))
    if (replyID === undefined) return
    const row = selectSessionByRootMessage.get(replyID) as SessionLookupRow | null
    if (row && row.muted !== 1) return { sessionID: row.session_id, directory: row.directory ?? input.directory }
  }

  async function handleTelegramUpdate(update: unknown) {
    const updateID = numberOption(prop(update, "update_id"))
    if (updateID !== undefined) {
      if (telegramUpdateIDs.has(updateID)) return
      telegramUpdateIDs.add(updateID)
      if (telegramUpdateIDs.size > 1000) telegramUpdateIDs.clear()
    }

    const callback = prop(update, "callback_query")
    if (record(callback)) {
      await handlePermissionCallback(callback)
      return
    }

    const message = prop(update, "message")
    if (!record(message)) return
    if (prop(prop(message, "from"), "is_bot") === true) return
    if (`${prop(prop(message, "chat"), "id")}` !== chatID) return

    const text = textOption(prop(message, "text"))
    if (!text) return

    const target = telegramMessageSession(message)
    if (!target) return

    const result = await input.client.session.promptAsync({
      path: { id: target.sessionID },
      query: { directory: target.directory },
      body: { parts: [{ type: "text", text }] },
    })
    if (prop(result, "error")) {
      console.warn("telegram-notify plugin: failed to forward Telegram message", prop(result, "error"))
      await send(html("failed to forward this message to opencode"), {
        threadID: numberOption(prop(message, "message_thread_id")),
      })
      return
    }
    await react(message)
  }

  async function pollTelegram() {
    if (!token || !chatID) return
    let offset = 0
    let initialized = false
    while (true) {
      try {
        if (acquirePollLock().count !== 1) {
          await new Promise((resolve) => setTimeout(resolve, 5000))
          continue
        }
        if (!initialized) {
          offset = await initialTelegramOffset()
          initialized = true
        }
        const updates = await telegramUpdates(offset || undefined, 25)
        for (const update of updates) {
          const updateID = numberOption(prop(update, "update_id"))
          if (updateID !== undefined) offset = Math.max(offset, updateID + 1)
          await handleTelegramUpdate(update)
        }
      } catch (error) {
        console.warn(
          "telegram-notify plugin: Telegram input polling failed",
          error instanceof Error ? error.message : error,
        )
        releasePollLock.run(pollOwner)
        initialized = false
        await new Promise((resolve) => setTimeout(resolve, 3000))
      }
    }
  }

  function title(prefix: string, sessionID: unknown) {
    return `${prefix}\nproject: ${input.project.id}\nsession: ${shortID(sessionID)}\ndir: ${input.directory}`
  }

  function topicName(sessionID: string) {
    const current = stats(sessionID)
    return (current.sessionTitle || "opencode session").replace(/\s+/g, " ").trim().slice(0, 128) || "opencode session"
  }

  async function syncTopicTitle(sessionID: string) {
    if (!forumTopics || !token || !chatID) return
    const current = stats(sessionID)
    if (current.muted) return
    const threadID = current.threadID ?? (current.threadPromise ? await current.threadPromise : undefined)
    if (threadID === undefined) return
    const name = topicName(sessionID)
    if (current.threadName === name) return

    const response = await fetch(`https://api.telegram.org/bot${token}/editForumTopic`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ chat_id: chatID, message_thread_id: threadID, name }),
    })
    const data = (await response.json()) as { ok?: boolean; description?: string }
    if (response.ok && data.ok !== false) {
      current.threadName = name
      saveSession(sessionID)
      return
    }
    console.warn(
      `telegram-notify plugin: editForumTopic failed (${response.status}) ${data.description ?? JSON.stringify(data)}`,
    )
  }

  async function rootMessage(sessionID: string) {
    const current = stats(sessionID)
    if (current.muted) return
    if (current.rootMessageID !== undefined) return current.rootMessageID
    if (current.rootMessagePromise) return current.rootMessagePromise
    current.rootMessagePromise = send(
      [
        `<b>${html(sessionID)}</b> · <code>${html(input.directory)}</code>`,
        `<blockquote expandable>${html(`user: ${current.userInput || "(unknown input)"}`)}</blockquote>`,
      ].join("\n"),
    ).then((messageID) => {
      current.rootMessageID = messageID
      current.rootMessagePromise = undefined
      current.introSent = true
      saveSession(sessionID)
      return messageID
    })
    return current.rootMessagePromise
  }

  async function sessionThread(sessionID: string) {
    const current = stats(sessionID)
    if (current.muted) return
    if (current.threadID !== undefined) return current.threadID
    if (current.threadPromise) return current.threadPromise
    if (!forumTopics || !token || !chatID) return
    const name = topicName(sessionID)
    current.threadPromise = fetch(`https://api.telegram.org/bot${token}/createForumTopic`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ chat_id: chatID, name }),
    })
      .then(
        (response) =>
          response.json() as Promise<{ ok?: boolean; result?: { message_thread_id?: number }; description?: string }>,
      )
      .then((data) => {
        if (data.ok === false) {
          console.warn(`telegram-notify plugin: createForumTopic failed ${data.description ?? JSON.stringify(data)}`)
          return
        }
        current.threadID = data.result?.message_thread_id
        current.threadName = current.threadID === undefined ? undefined : name
        saveSession(sessionID)
        return current.threadID
      })
      .finally(() => {
        current.threadPromise = undefined
      })
    return current.threadPromise
  }

  async function sessionTarget(sessionID: string) {
    if (forumTopics) return { threadID: await sessionThread(sessionID) }
    return { replyTo: await rootMessage(sessionID) }
  }

  void pollTelegram()

  return createDispatcher({
    plugin: input,
    composer: new NotificationComposer({ directory: input.directory, maxOutputChars }),
    notifyDone,
    notifyPermission,
    notifyQuestion,
    permissionNotifyDelay,
    contextLimit: modelContextLimit,
    activeContextTokens,
    sender: {
      errorLabel: "telegram-notify plugin",
      async ensureSession(notice) {
        applySessionNotice(notice)
        const target = await sessionTarget(notice.sessionID)
        if (forumTopics && target.threadID !== undefined) await sendIntro(notice.sessionID, target)
      },
      async syncSessionTitle(notice) {
        applySessionNotice(notice)
        saveSession(notice.sessionID)
        await syncTopicTitle(notice.sessionID)
      },
      async sendDone(notice) {
        applySessionNotice(notice)
        await send(doneNoticeMessage(notice), await sessionTarget(notice.sessionID))
      },
      async sendCompaction(notice) {
        await send(compactionMessage(notice), await sessionTarget(notice.sessionID))
      },
      async sendPermission(notice) {
        const target = await sessionTarget(notice.sessionID)
        const messageID = await send(
          html(
            [
              `permission needed · ${shortID(notice.sessionID)}`,
              `permission: ${notice.permission}`,
              `patterns: ${notice.patterns}`,
            ].join("\n"),
          ),
          {
            ...target,
            replyMarkup: {
              inline_keyboard: [
                [
                  { text: "Allow once", callback_data: `op:perm:once:${notice.requestID}` },
                  { text: "Always", callback_data: `op:perm:always:${notice.requestID}` },
                  { text: "Deny", callback_data: `op:perm:reject:${notice.requestID}` },
                ],
              ],
            },
          },
        )
        upsertPermission.run(
          notice.requestID,
          notice.sessionID,
          input.directory,
          target.threadID ?? null,
          messageID ?? null,
          Date.now(),
        )
      },
      async clearPermission(requestID) {
        const row = selectPermission.get(requestID) as PermissionLookupRow | null
        if (row?.message_id) await editReplyMarkup(row.message_id, row.thread_id ?? undefined)
        deletePermission.run(requestID)
      },
      async sendQuestion(notice) {
        await send(html(title("opencode is waiting for your answer", notice.sessionID)), await sessionTarget(notice.sessionID))
      },
    },
  })
}) satisfies Plugin
