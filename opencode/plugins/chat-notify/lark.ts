import { Database } from "bun:sqlite"
import { mkdirSync } from "node:fs"
import { dirname } from "node:path"
import { fileURLToPath } from "node:url"
import type { Plugin } from "@opencode-ai/plugin"
import { NotificationComposer, contextLimitFrom, type DoneNotice, type SessionNotice } from "./composer"
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

type PostElement = {
  tag: "text" | "a" | "at" | "img" | "media" | "emotion" | "hr" | "code_block" | "md"
  text?: string
  style?: string[]
  language?: string
}

type PostMessage = {
  type: "post"
  title?: string
  content: PostElement[][]
}

type LarkMessage = string | PostMessage

const modelRef = (value: unknown) => {
  const providerID = textOption(prop(value, "providerID"))
  const modelID = textOption(prop(value, "modelID") ?? prop(value, "id"))
  if (!providerID || !modelID) return
  return { providerID, modelID }
}

const larkText = (text: string) => JSON.stringify({ text })

const post = (content: PostElement[][], title?: string): PostMessage => ({ type: "post", title, content })

const textNode = (text: string, style?: string[]): PostElement => ({ tag: "text", text, ...(style ? { style } : {}) })

const br = () => textNode("\n")

const codeBlock = (text: string, language?: string): PostElement => ({
  tag: "code_block",
  text,
  ...(language ? { language } : {}),
})

const md = (text: string): PostElement => ({ tag: "md", text })

const paragraph = (text: string, style?: string[]) => [textNode(text, style)]

const markdown = (text: string) => [md(text)]

const quote = (label: string, text: string) => markdown(`> **${label}:** ${text}`)

const metaLine = (text: string) => markdown(`> ${text}`)

const postContent = (value: string) => {
  const lines = value.split("\n")
  const result: PostElement[][] = []
  let text: string[] = []
  let code: string[] | undefined
  let language: string | undefined

  function flushText() {
    if (!text.length) return
    result.push([md(text.join("\n"))])
    text = []
  }

  function flushCode() {
    if (!code) return
    result.push([codeBlock(code.join("\n"), language)])
    code = undefined
    language = undefined
  }

  for (const line of lines) {
    if (line.trimStart().startsWith("```")) {
      if (code) {
        flushCode()
        continue
      }
      flushText()
      code = []
      language = line.trim().slice(3).trim() || undefined
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
  return result.length ? result : [paragraph("(no text output)")]
}

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
  const selectThreads = db.query(`
    SELECT DISTINCT thread_id
    FROM session_state
    WHERE thread_id IS NOT NULL AND muted = 0
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

  function applySessionNotice(notice: SessionNotice) {
    const current = stats(notice.sessionID)
    current.userInput = notice.userInput
    current.sessionTitle = notice.sessionTitle
    current.contextTokens = notice.contextTokens
    current.contextLimit = notice.contextLimit
  }

  function doneNoticeMessage(notice: DoneNotice) {
    const context = notice.contextTokens
      ? ` · ${
          notice.contextLimit
            ? `${Math.round((notice.contextTokens / notice.contextLimit) * 100)}%`
            : compactNumber(notice.contextTokens)
        }`
      : ""
    return post([
      ...(notice.userInput ? [quote("user", truncateEnd(notice.userInput, 200)), [br()]] : []),
      ...postContent(notice.output),
      [br()],
      metaLine(`tools ${notice.tools} · r/w/c ${notice.read}/${notice.written}/${notice.changed}${context}`),
    ])
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

  function larkBody(message: LarkMessage) {
    if (typeof message === "string") return { msg_type: "text", content: larkText(message) }
    return {
      msg_type: "post",
      content: JSON.stringify({
        zh_cn: {
          ...(message.title ? { title: message.title } : {}),
          content: message.content,
        },
      }),
    }
  }

  async function send(message: LarkMessage, rootMessageID?: string) {
    if (!chatID) {
      console.warn("lark-notify plugin: missing LARK_CHAT_ID")
      return
    }
    const data = rootMessageID
      ? await larkAPI("POST", `/im/v1/messages/${encodeURIComponent(rootMessageID)}/reply`, undefined, {
          ...larkBody(message),
        })
      : await larkAPI(
          "POST",
          "/im/v1/messages",
          { receive_id_type: "chat_id" },
          { receive_id: chatID, ...larkBody(message) },
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
      post(
        [
          markdown(`**${sessionID}** · ${input.directory}`),
          quote("user", current.userInput || "(unknown input)"),
        ],
      ),
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

  async function sendReply(sessionID: string, message: LarkMessage) {
    const rootID = await rootMessage(sessionID)
    if (!rootID) return
    const result = await send(message, rootID)
    if (result?.threadID) {
      stats(sessionID).threadID = result.threadID
      saveSession(sessionID)
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

    insertSentMessage.run(messageID, Date.now())
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

  async function pollMessages(
    containerIDType: "chat" | "thread",
    containerID: string,
    startTime: number,
    endTime: number,
  ) {
    let pageToken: string | undefined
    do {
      const data = await larkAPI("GET", "/im/v1/messages", {
        container_id_type: containerIDType,
        container_id: containerID,
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
          for (const row of selectThreads.all() as Array<{ thread_id: string | null }>) {
            if (row.thread_id) await pollMessages("thread", row.thread_id, cursor, now)
          }
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

  return createDispatcher({
    plugin: input,
    composer: new NotificationComposer({ directory: input.directory, maxOutputChars }),
    notifyDone,
    notifyPermission,
    notifyQuestion,
    permissionNotifyDelay,
    contextLimit: modelContextLimit,
    sender: {
      errorLabel: "lark-notify plugin",
      async ensureSession(notice) {
        applySessionNotice(notice)
        await rootMessage(notice.sessionID)
      },
      async syncSessionTitle(notice) {
        applySessionNotice(notice)
        saveSession(notice.sessionID)
      },
      async sendDone(notice) {
        applySessionNotice(notice)
        await sendReply(notice.sessionID, doneNoticeMessage(notice))
      },
      async sendPermission(notice) {
        await sendReply(
          notice.sessionID,
          post(
            [
              paragraph(`permission needed · ${shortID(notice.sessionID)}`, ["bold"]),
              paragraph(`permission: ${notice.permission}`),
              paragraph(`patterns: ${notice.patterns}`),
            ],
            "OpenCode permission",
          ),
        )
      },
      async sendQuestion(notice) {
        await sendReply(
          notice.sessionID,
          post(
            [paragraph("opencode is waiting for your answer", ["bold"]), paragraph(`session: ${shortID(notice.sessionID)}`)],
            "OpenCode question",
          ),
        )
      },
    },
  })
}) satisfies Plugin
