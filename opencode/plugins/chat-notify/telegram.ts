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
  let paragraph: string[] = []

  function flushParagraph() {
    if (!paragraph.length) return
    result.push(`<p>${paragraph.map(inlineMarkdown).join("<br/>")}</p>`)
    paragraph = []
  }

  for (let index = 0; index < lines.length; index++) {
    if (!lines[index].trim()) {
      flushParagraph()
      continue
    }
    if (!lines[index].includes("|") || !tableDivider(lines[index + 1] ?? "")) {
      paragraph.push(lines[index])
      continue
    }

    flushParagraph()
    const rows = [tableCells(lines[index])]
    index += 2
    while (index < lines.length && lines[index].includes("|")) {
      rows.push(tableCells(lines[index]))
      index++
    }
    index--
    result.push(`<pre><code>${html(renderTable(rows))}</code></pre>`)
  }
  flushParagraph()
  return result.join("")
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
  return result.join("")
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

const gitBranch = async (directory: string) => {
  try {
    const branchProcess = Bun.spawn(["git", "-C", directory, "rev-parse", "--abbrev-ref", "HEAD"], {
      stdout: "pipe",
      stderr: "ignore",
    })
    const [branch, branchExitCode] = await Promise.all([
      new Response(branchProcess.stdout).text(),
      branchProcess.exited,
    ])
    if (branchExitCode !== 0) return "非 Git 仓库"
    const name = branch.trim()
    if (name !== "HEAD") return name

    const commitProcess = Bun.spawn(["git", "-C", directory, "rev-parse", "--short", "HEAD"], {
      stdout: "pipe",
      stderr: "ignore",
    })
    const [commit, commitExitCode] = await Promise.all([
      new Response(commitProcess.stdout).text(),
      commitProcess.exited,
    ])
    return commitExitCode === 0 ? `detached@${commit.trim()}` : "detached HEAD"
  } catch {
    return "无法获取"
  }
}

type TelegramRichMessage = {
  html: string
}

type SessionStats = {
  directory: string
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
  directory: string | null
  muted: number | null
  intro_sent: number | null
  root_message_id: number | null
  thread_id: number | null
  thread_name: string | null
  session_title: string | null
  user_input: string | null
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

type QuestionLookupRow = PermissionLookupRow

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

let warnedMissingConfig = false

export default (async (input, options) => {
  const config = await readPluginConfig()
  const token = textOption(options?.token, env(config, "TELEGRAM_BOT_TOKEN"))
  const chatID = textOption(options?.chatID, env(config, "TELEGRAM_CHAT_ID"))
  if ((!token || !chatID) && !warnedMissingConfig) {
    warnedMissingConfig = true
    console.warn("telegram-notify plugin: missing TELEGRAM_BOT_TOKEN or TELEGRAM_CHAT_ID")
  }
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
      user_input TEXT,
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
  db.run(`
    CREATE TABLE IF NOT EXISTS question_request (
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
  if (!columns.has("user_input")) db.run("ALTER TABLE session_state ADD COLUMN user_input TEXT")
  const selectSession = db.query(`
    SELECT directory, muted, intro_sent, root_message_id, thread_id, thread_name, session_title, user_input
    FROM session_state
    WHERE project_id = ? AND session_id = ?
  `)
  const selectSessionByID = db.query(`
    SELECT directory, muted, intro_sent, root_message_id, thread_id, thread_name, session_title, user_input
    FROM session_state
    WHERE session_id = ?
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
  const selectSessionByRootMessage = db.query(`
    SELECT session_id, directory, muted
    FROM session_state
    WHERE root_message_id = ?
    ORDER BY updated_at DESC
    LIMIT 1
  `)
  const upsertSession = db.query(`
    INSERT INTO session_state (
      project_id, session_id, directory, muted, intro_sent, root_message_id, thread_id, thread_name, session_title, user_input,
      updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(project_id, session_id) DO UPDATE SET
      directory = excluded.directory,
      muted = excluded.muted,
      intro_sent = excluded.intro_sent,
      root_message_id = excluded.root_message_id,
      thread_id = excluded.thread_id,
      thread_name = excluded.thread_name,
      session_title = excluded.session_title,
      user_input = excluded.user_input,
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
  const upsertQuestion = db.query(`
    INSERT INTO question_request (request_id, session_id, directory, thread_id, message_id, updated_at)
    VALUES (?, ?, ?, ?, ?, ?)
    ON CONFLICT(request_id) DO UPDATE SET
      session_id = excluded.session_id,
      directory = excluded.directory,
      thread_id = excluded.thread_id,
      message_id = excluded.message_id,
      updated_at = excluded.updated_at
  `)
  const selectQuestion = db.query(`
    SELECT request_id, session_id, directory, thread_id, message_id
    FROM question_request
    WHERE request_id = ?
  `)
  const selectQuestionBySession = db.query(`
    SELECT request_id, session_id, directory, thread_id, message_id
    FROM question_request
    WHERE session_id = ?
    ORDER BY updated_at DESC
    LIMIT 1
  `)
  const deleteQuestion = db.query(`
    DELETE FROM question_request
    WHERE request_id = ?
  `)
  const deletePermissionsBySession = db.query("DELETE FROM permission_request WHERE session_id = ?")
  const deleteQuestionsBySession = db.query("DELETE FROM question_request WHERE session_id = ?")
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
    const row = (selectSession.get(input.project.id, sessionID) ?? selectSessionByID.get(sessionID)) as SessionRow | null
    const next = {
      directory: row?.directory ?? input.directory,
      muted: row?.muted === 1,
      introSent: row?.intro_sent === 1,
      rootMessageID: row?.root_message_id ?? undefined,
      rootMessagePromise: undefined,
      threadID: row?.thread_id ?? undefined,
      threadPromise: undefined,
      threadName: row?.thread_name ?? undefined,
      sessionTitle: row?.session_title ?? undefined,
      userInput: row?.user_input ?? "",
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
      current.directory,
      current.muted ? 1 : 0,
      current.introSent ? 1 : 0,
      current.rootMessageID ?? null,
      current.threadID ?? null,
      current.threadName ?? null,
      current.sessionTitle ?? null,
      current.userInput || null,
      Date.now(),
    )
  }

  function applySessionNotice(notice: SessionNotice) {
    const current = stats(notice.sessionID)
    current.directory = input.directory
    current.userInput = notice.userInput
    current.sessionTitle = notice.sessionTitle
    current.contextTokens = notice.contextTokens
    current.contextLimit = notice.contextLimit
  }

  async function statusMessage(
    sessionID: string,
    title: "已开始" | "处理中" | "需要授权" | "等待回答" | "已完成",
    content = "",
    footer = "",
  ): Promise<TelegramRichMessage> {
    const current = stats(sessionID)
    const branch = await gitBranch(current.directory)
    const task = markdown(truncateEnd(current.userInput || "等待任务内容", 200))
    const details = [
      `<details><summary>详情</summary>`,
      `<p><b>会话 ID</b><br/><code>${html(sessionID)}</code></p>`,
      `<p><b>工作路径</b><br/><code>${html(current.directory)}</code></p>`,
      `<p><b>所属分支</b><br/><code>${html(branch)}</code></p>`,
      `</details>`,
    ].join("")
    return {
      html: [
        `<h3>OpenCode · ${title}</h3>`,
        task,
        `<hr/>`,
        content,
        footer ? `<footer>${html(footer)}</footer>` : "",
        details,
      ].join(""),
    }
  }

  async function doneNoticeMessage(notice: DoneNotice) {
    const context =
      notice.contextTokens && notice.contextLimit
        ? `上下文 ${Math.round((notice.contextTokens / notice.contextLimit) * 100)}%`
        : undefined
    const files = notice.changed > 0 ? `修改 ${notice.changed} 个文件` : "未修改文件"
    return statusMessage(
      notice.sessionID,
      "已完成",
      markdown(notice.output),
      [files, context].filter(Boolean).join(" · "),
    )
  }

  const compactionMessage = (notice: CompactionNotice): TelegramRichMessage => ({
    html: `<p>${html(
      `上下文已压缩 · ${contextLabel(notice.beforeTokens, notice.beforeLimit)} → ${contextLabel(
        notice.afterTokens,
        notice.afterLimit,
        notice.afterTokens !== undefined,
      )}`,
    )}</p>`,
  })

  async function send(text: string, options?: { replyTo?: number; threadID?: number; replyMarkup?: unknown }) {
    if (!token || !chatID) return

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

  async function sendRich(
    richMessage: TelegramRichMessage,
    options?: { replyTo?: number; threadID?: number; replyMarkup?: unknown },
  ) {
    if (!token || !chatID) return
    const threadID = options?.threadID ?? messageThreadID
    const response = await fetch(`https://api.telegram.org/bot${token}/sendRichMessage`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        chat_id: chatID,
        rich_message: richMessage,
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
      `telegram-notify plugin: sendRichMessage failed (${response.status}) ${data.description ?? JSON.stringify(data)}`,
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

  async function editRichMessage(messageID: number, richMessage: TelegramRichMessage, replyMarkup?: unknown) {
    if (!chatID || messageID <= 0) return false
    return telegramPost("editMessageText", {
      chat_id: chatID,
      message_id: messageID,
      rich_message: richMessage,
      ...(replyMarkup === undefined ? {} : { reply_markup: replyMarkup }),
    })
  }

  async function answerCallback(callbackID: string, text?: string) {
    await telegramPost("answerCallbackQuery", {
      callback_query_id: callbackID,
      ...(text ? { text } : {}),
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

  async function replyQuestion(row: QuestionLookupRow, answer: string) {
    const question = prop(input.client, "question")
    const replyFn = prop(question, "reply")
    if (typeof replyFn !== "function") return false
    const result = await replyFn.call(question, {
      requestID: row.request_id,
      answers: [[answer]],
      directory: row.directory ?? input.directory,
    })
    return !prop(result, "error")
  }

  async function rejectQuestion(row: QuestionLookupRow) {
    const question = prop(input.client, "question")
    const rejectFn = prop(question, "reject")
    if (typeof rejectFn !== "function") return false
    const result = await rejectFn.call(question, {
      requestID: row.request_id,
      directory: row.directory ?? input.directory,
    })
    return !prop(result, "error")
  }

  async function restoreProcessing(row: PermissionLookupRow) {
    const current = stats(row.session_id)
    current.directory = row.directory ?? current.directory
    const messageID = row.message_id ?? current.rootMessageID
    if (messageID === undefined) return
    await editRichMessage(messageID, await statusMessage(row.session_id, "处理中"), { inline_keyboard: [] })
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
    await restoreProcessing({
      ...row,
      message_id: numberOption(prop(message, "message_id")) ?? row.message_id,
    })
  }

  async function handleQuestionCallback(callback: unknown) {
    const callbackID = textOption(prop(callback, "id"))
    const data = textOption(prop(callback, "data"))
    if (!callbackID || !data) return
    const match = /^op:question:reject:(.+)$/.exec(data)
    if (!match) return

    const message = prop(callback, "message")
    if (record(message) && `${prop(prop(message, "chat"), "id")}` !== chatID) {
      await answerCallback(callbackID, "Wrong chat")
      return
    }

    const row = selectQuestion.get(match[1]) as QuestionLookupRow | null
    if (!row) {
      await answerCallback(callbackID, "Question is no longer pending")
      return
    }
    if (!(await rejectQuestion(row))) {
      await answerCallback(callbackID, "Failed")
      return
    }

    deleteQuestion.run(row.request_id)
    await answerCallback(callbackID, "Question rejected")
    await restoreProcessing({
      ...row,
      message_id: numberOption(prop(message, "message_id")) ?? row.message_id,
    })
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
      const data = textOption(prop(callback, "data"))
      if (data?.startsWith("op:perm:")) await handlePermissionCallback(callback)
      if (data?.startsWith("op:question:")) await handleQuestionCallback(callback)
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

    const question = selectQuestionBySession.get(target.sessionID) as QuestionLookupRow | null
    if (question) {
      if (!(await replyQuestion(question, text))) {
        console.warn("telegram-notify plugin: failed to answer OpenCode question")
        await send(html("failed to submit this answer to opencode"), {
          threadID: numberOption(prop(message, "message_thread_id")),
        })
        return
      }
      deleteQuestion.run(question.request_id)
      await restoreProcessing(question)
      await react(message)
      return
    }

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
    current.rootMessagePromise = Promise.all([
      statusMessage(sessionID, "已开始"),
      forumTopics ? sessionThread(sessionID) : Promise.resolve(undefined),
    ])
      .then(([message, threadID]) => sendRich(message, { threadID }))
      .then((messageID) => {
        current.rootMessageID = messageID
        current.rootMessagePromise = undefined
        current.introSent = true
        saveSession(sessionID)
        return messageID
      })
    return current.rootMessagePromise
  }

  async function updateStatus(
    sessionID: string,
    title: "处理中" | "需要授权" | "等待回答" | "已完成",
    content = "",
    footer = "",
    replyMarkup?: unknown,
  ) {
    const messageID = await rootMessage(sessionID)
    if (messageID === undefined) return
    await editRichMessage(messageID, await statusMessage(sessionID, title, content, footer), replyMarkup)
    return messageID
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
        saveSession(notice.sessionID)
        await rootMessage(notice.sessionID)
      },
      async syncSessionTitle(notice) {
        applySessionNotice(notice)
        saveSession(notice.sessionID)
        await syncTopicTitle(notice.sessionID)
      },
      async sendDone(notice) {
        applySessionNotice(notice)
        saveSession(notice.sessionID)
        deletePermissionsBySession.run(notice.sessionID)
        deleteQuestionsBySession.run(notice.sessionID)
        const messageID = await rootMessage(notice.sessionID)
        if (messageID !== undefined) {
          await editRichMessage(messageID, await doneNoticeMessage(notice), { inline_keyboard: [] })
        }
      },
      async sendCompaction(notice) {
        await sendRich(compactionMessage(notice), await sessionTarget(notice.sessionID))
      },
      async sendPermission(notice) {
        const replyMarkup = {
          inline_keyboard: [
            [
              { text: "允许一次", callback_data: `op:perm:once:${notice.requestID}` },
              { text: "始终允许", callback_data: `op:perm:always:${notice.requestID}` },
              { text: "拒绝", callback_data: `op:perm:reject:${notice.requestID}` },
            ],
          ],
        }
        const messageID = await updateStatus(
          notice.sessionID,
          "需要授权",
          [
            `<p><b>权限</b><br/>${html(notice.permission)}</p>`,
            `<p><b>范围</b><br/>${html(truncateEnd(notice.patterns, 400))}</p>`,
          ].join(""),
          "",
          replyMarkup,
        )
        const current = stats(notice.sessionID)
        upsertPermission.run(
          notice.requestID,
          notice.sessionID,
          current.directory,
          current.threadID ?? null,
          messageID ?? null,
          Date.now(),
        )
      },
      async clearPermission(requestID) {
        const row = selectPermission.get(requestID) as PermissionLookupRow | null
        deletePermission.run(requestID)
        if (row) await restoreProcessing(row)
      },
      async sendQuestion(notice) {
        const content = [
          notice.header ? `<p><b>${html(notice.header)}</b></p>` : "",
          `<p>${html(notice.question ?? "请回复这条消息，OpenCode 会继续处理。")}</p>`,
          `<footer>${forumTopics ? "在当前话题中输入回答" : "请回复这条消息输入回答"}</footer>`,
        ].join("")
        const replyMarkup = {
          inline_keyboard: [
            [{ text: "拒绝回答", callback_data: `op:question:reject:${notice.requestID}` }],
          ],
        }
        const messageID = await updateStatus(notice.sessionID, "等待回答", content, "", replyMarkup)
        const current = stats(notice.sessionID)
        upsertQuestion.run(
          notice.requestID,
          notice.sessionID,
          current.directory,
          current.threadID ?? null,
          messageID ?? null,
          Date.now(),
        )
      },
      async clearQuestion(requestID) {
        const row = selectQuestion.get(requestID) as QuestionLookupRow | null
        deleteQuestion.run(requestID)
        if (row) await restoreProcessing(row)
      },
    },
  })
}) satisfies Plugin
