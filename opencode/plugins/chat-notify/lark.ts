import { Database } from "bun:sqlite"
import { mkdirSync } from "node:fs"
import { dirname } from "node:path"
import { fileURLToPath } from "node:url"
import type { Plugin } from "@opencode-ai/plugin"
import {
  NotificationComposer,
  contextLimitFrom,
  type DoneNotice,
  type ProgressNotice,
  type SessionNotice,
} from "./composer"
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

type CardActionRoute = {
  handle(action: string, formValue: Record<string, unknown>): Promise<void>
}

type CardActionEnqueueResult = "accepted" | "busy" | "forbidden" | "missing" | "stale"

type CardActionStore = {
  enqueue(input: {
    eventID: string
    token: string
    action: string
    formValue: Record<string, unknown>
    userID?: string
    messageID?: string
  }): CardActionEnqueueResult
}

const cardActionRoutes = new Map<string, CardActionRoute>()
const cardActionStores = new Set<CardActionStore>()
const cardActionClients = new Map<string, Promise<void>>()
const cardActionProcessorPaths = new Set<string>()
const cardActionOwner = `${process.pid}:${crypto.randomUUID()}`

const callbackToast = (type: "success" | "warning" | "error", content: string) => ({
  toast: { type, content },
})

function startCardActionClient(appID: string, appSecret: string) {
  if (cardActionClients.has(appID)) return
  const start = (async () => {
    const packageName = "@larksuiteoapi/node-sdk"
    const lark = await import(packageName)
    const eventDispatcher = new lark.EventDispatcher({}).register({
      "card.action.trigger": async (payload: unknown) => {
        const event = prop(payload, "event") ?? payload
        const action = prop(event, "action")
        const value = prop(action, "value")
        const token = textOption(prop(value, "token"))
        const actionName = textOption(prop(value, "action"))
        if (!token || !actionName) return callbackToast("warning", "这个操作已经失效")

        const input = {
          eventID: textOption(prop(event, "event_id")) ?? crypto.randomUUID(),
          token,
          action: actionName,
          formValue: record(prop(action, "form_value")) ? (prop(action, "form_value") as Record<string, unknown>) : {},
          userID: textOption(prop(prop(event, "operator"), "open_id") ?? prop(event, "open_id")),
          messageID: textOption(prop(prop(event, "context"), "open_message_id") ?? prop(event, "open_message_id")),
        }
        let result: CardActionEnqueueResult = "missing"
        for (const store of cardActionStores) {
          result = store.enqueue(input)
          if (result !== "missing") break
        }
        if (result === "accepted") return callbackToast("success", "已提交")
        if (result === "forbidden") return callbackToast("error", "只有被通知人可以操作")
        if (result === "busy") return callbackToast("warning", "操作正在提交，请稍候")
        if (result === "stale") return callbackToast("warning", "卡片状态已更新，请使用最新操作")
        return callbackToast("warning", "这个操作已经失效")
      },
    })
    const wsClient = new lark.WSClient({
      appId: appID,
      appSecret,
      loggerLevel: lark.LoggerLevel.warn,
    })
    await wsClient.start({ eventDispatcher })
  })().catch((error) => {
    cardActionClients.delete(appID)
    console.warn(
      "lark-notify plugin: card action connection failed",
      error instanceof Error ? error.message : error,
    )
  })
  cardActionClients.set(appID, start)
}

const truncateEnd = (value: string, limit: number) => {
  if (value.length <= limit) return value
  return `${value.slice(0, limit).trimEnd()}...`
}

const truncateStart = (value: string, limit: number) => {
  if (value.length <= limit) return value
  return `[前文已省略]\n\n${value.slice(-limit).trimStart()}`
}

const STREAM_ELEMENT_ID = "stream_result"
const STREAM_SESSION_INTERVAL = 1000
// Feishu allows 50 updates/s and 1000 updates/min per app and tenant; keep headroom for bursts and finalization.
const STREAM_SECOND_LIMIT = 15
const STREAM_MINUTE_LIMIT = 900

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
  lastMessageID: string | undefined
}

type SessionRow = {
  muted: number | null
  intro_sent: number | null
  root_message_id: string | null
  thread_id: string | null
  session_title: string | null
  last_message_id: string | null
}

type SessionLookupRow = {
  session_id: string
  directory: string | null
  muted: number | null
}

type LastMessageRow = {
  session_id: string
  directory: string | null
  last_message_id: string | null
}

type StreamRateRow = {
  count: number
  oldest: number | null
}

type StreamState = {
  pendingContent: string
  lastSentContent: string
  cardID?: string
  messageID?: string
  sequence: number
  lastSentAt: number
  timer?: ReturnType<typeof setTimeout>
  startPromise?: Promise<void>
  flushPromise?: Promise<void>
  attention?: { requestID: string; kind: "permission" | "question"; token?: string }
  closing: boolean
  failed: boolean
}

type CardElement = {
  tag: "markdown" | "hr" | "collapsible_panel" | "form" | "input" | "column_set" | "column" | "button"
  element_id?: string
  content?: string
  text_size?: "notation"
  margin?: string
  name?: string
  required?: boolean
  input_type?: "multiline_text"
  rows?: number
  auto_resize?: boolean
  max_rows?: number
  width?: "default" | "fill" | "auto"
  flex_mode?: "none"
  horizontal_spacing?: "small" | "default"
  vertical_align?: "top"
  type?: "default" | "primary" | "danger"
  action_type?: "form_submit" | "form_reset"
  text?: { tag: "plain_text"; content: string }
  placeholder?: { tag: "plain_text"; content: string }
  behaviors?: Array<{ type: "callback"; value: Record<string, string> }>
  value?: Record<string, string>
  confirm?: {
    title: { tag: "plain_text"; content: string }
    text: { tag: "plain_text"; content: string }
  }
  expanded?: boolean
  header?: {
    title: { tag: "plain_text"; content: string }
    icon_position: "right"
  }
  border?: {
    color: "grey"
    corner_radius: string
  }
  padding?: string
  elements?: CardElement[]
  columns?: CardElement[]
}

type CardSpec = {
  schema: "2.0"
  config?: {
    update_multi?: boolean
    streaming_mode?: boolean
    summary?: { content: string }
    streaming_config?: {
      print_frequency_ms: { default: number }
      print_step: { default: number }
      print_strategy: "fast"
    }
  }
  header: {
    title: { tag: "plain_text"; content: string }
    template: "blue" | "green" | "orange"
    padding: string
  }
  body: {
    direction: "vertical"
    padding: string
    elements: CardElement[]
  }
}

type CardMessage = {
  type: "interactive"
  card: CardSpec
}

type CardReference = {
  type: "card_reference"
  cardID: string
}

type LarkMessage = string | CardMessage | CardReference

const modelRef = (value: unknown) => {
  const providerID = textOption(prop(value, "providerID"))
  const modelID = textOption(prop(value, "modelID") ?? prop(value, "id"))
  if (!providerID || !modelID) return
  return { providerID, modelID }
}

const larkText = (text: string) => JSON.stringify({ text })

const card = (
  title: string,
  template: "blue" | "green" | "orange",
  elements: CardElement[],
): CardMessage => ({
  type: "interactive",
  card: {
    schema: "2.0",
    header: {
      title: { tag: "plain_text", content: `OpenCode · ${title}` },
      template,
      padding: "10px 12px 10px 12px",
    },
    body: {
      direction: "vertical",
      padding: "10px 12px 10px 12px",
      elements,
    },
  },
})

const cardMarkdown = (content: string, margin = "0px 0px 8px 0px"): CardElement => ({
  tag: "markdown",
  content,
  margin,
})

const streamCard = (
  userInput: string,
  content: string,
  title: "处理中" | "需要授权" | "等待回答" = "处理中",
  template: "blue" | "orange" = "blue",
  controls: CardElement[] = [],
) => {
  const message = card(title, template, [
    ...(userInput ? [cardMarkdown(truncateEnd(userInput, 200)), cardDivider()] : []),
    { ...cardMarkdown(content, controls.length > 0 ? "0px 0px 10px 0px" : "0px"), element_id: STREAM_ELEMENT_ID },
    ...controls,
  ])
  message.card.config = {
    update_multi: true,
    streaming_mode: true,
    summary: { content: "OpenCode 正在处理" },
    streaming_config: {
      print_frequency_ms: { default: 70 },
      print_step: { default: 1 },
      print_strategy: "fast",
    },
  }
  return message.card
}

const actionButton = (
  content: string,
  type: "default" | "primary" | "danger",
  token: string,
  action: string,
  confirm?: { title: string; text: string },
): CardElement => ({
  tag: "button",
  text: { tag: "plain_text", content },
  type,
  behaviors: [{ type: "callback", value: { token, action } }],
  ...(confirm
    ? {
        confirm: {
          title: { tag: "plain_text", content: confirm.title },
          text: { tag: "plain_text", content: confirm.text },
        },
      }
    : {}),
})

const buttonRow = (buttons: CardElement[]): CardElement => ({
  tag: "column_set",
  flex_mode: "none",
  horizontal_spacing: "small",
  margin: "0px",
  columns: buttons.map((button) => ({
    tag: "column",
    width: "auto",
    vertical_align: "top",
    elements: [button],
  })),
})

const permissionControls = (token: string): CardElement[] => [
  buttonRow([
    actionButton("允许一次", "primary", token, "permission_once"),
    actionButton("始终允许", "default", token, "permission_always", {
      title: "始终允许？",
      text: "后续匹配的权限请求将不再确认。",
    }),
    actionButton("拒绝", "danger", token, "permission_reject", {
      title: "拒绝请求？",
      text: "OpenCode 可能无法继续当前操作。",
    }),
  ]),
]

const questionControls = (token: string): CardElement[] => [
  {
    tag: "form",
    name: "question_form",
    elements: [
      {
        tag: "input",
        element_id: "question_answer",
        name: "answer",
        required: true,
        input_type: "multiline_text",
        rows: 2,
        auto_resize: true,
        max_rows: 5,
        width: "fill",
        margin: "0px 0px 8px 0px",
        placeholder: { tag: "plain_text", content: "输入回答" },
      },
      buttonRow([
        {
          tag: "button",
          text: { tag: "plain_text", content: "提交回答" },
          type: "primary",
          action_type: "form_submit",
          name: "submit_answer",
          value: { token, action: "question_reply" },
        },
        actionButton("拒绝回答", "default", token, "question_reject", {
          title: "拒绝回答？",
          text: "OpenCode 将取消这次提问。",
        }),
      ]),
    ],
  },
]

const cardFooter = (content: string): CardElement => ({
  tag: "markdown",
  content: `<font color='grey'>${content}</font>`,
  text_size: "notation",
  margin: "0px",
})

const cardDivider = (): CardElement => ({ tag: "hr", margin: "0px 0px 8px 0px" })

const cardDetails = (content: string): CardElement => ({
  tag: "collapsible_panel",
  expanded: false,
  header: {
    title: { tag: "plain_text", content: "详情" },
    icon_position: "right",
  },
  border: { color: "grey", corner_radius: "5px" },
  padding: "8px 10px 8px 10px",
  margin: "0px",
  elements: [cardMarkdown(content, "0px")],
})

const mention = (userID: string | undefined) => (userID ? `<at id=${userID}></at>` : "")

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

let warnedMissingCredentials = false
let warnedMissingChatID = false

export default (async (input, options) => {
  const config = await readPluginConfig()
  const appID = textOption(options?.appID, conf(config, "LARK_APP_ID"))
  const appSecret = textOption(options?.appSecret, conf(config, "LARK_APP_SECRET"))
  const chatID = textOption(options?.chatID, conf(config, "LARK_CHAT_ID"))
  if ((!appID || !appSecret) && !warnedMissingCredentials) {
    warnedMissingCredentials = true
    console.warn("lark-notify plugin: missing LARK_APP_ID or LARK_APP_SECRET")
  }
  if (!chatID && !warnedMissingChatID) {
    warnedMissingChatID = true
    console.warn("lark-notify plugin: missing LARK_CHAT_ID")
  }
  const mentionEmail = textOption(options?.mentionEmail, conf(config, "LARK_MENTION_EMAIL"))
  const notifyDone = boolOption(options?.notifyDone, true)
  const notifyPermission = boolOption(options?.notifyPermission, true)
  const notifyQuestion = boolOption(options?.notifyQuestion, true)
  const streamOutput = boolOption(options?.streamOutput, boolText(conf(config, "LARK_STREAM_OUTPUT"), true))
  const cardActions = boolOption(options?.cardActions, boolText(conf(config, "LARK_CARD_ACTIONS"), true))
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
      last_message_id TEXT,
      updated_at INTEGER NOT NULL,
      PRIMARY KEY (project_id, session_id)
    )
  `)
  const sessionColumns = db.query("PRAGMA table_info(session_state)").all() as Array<{ name: string }>
  if (!sessionColumns.some((column) => column.name === "last_message_id"))
    db.run("ALTER TABLE session_state ADD COLUMN last_message_id TEXT")
  db.run(`
    CREATE TABLE IF NOT EXISTS sent_message (
      message_id TEXT PRIMARY KEY,
      updated_at INTEGER NOT NULL
    )
  `)
  db.run(`
    CREATE TABLE IF NOT EXISTS reaction_input (
      reaction_key TEXT PRIMARY KEY,
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
  db.run(`
    CREATE TABLE IF NOT EXISTS lark_stream_rate_limit (
      sent_at INTEGER NOT NULL
    )
  `)
  db.run(`
    CREATE TABLE IF NOT EXISTS lark_card_action_route (
      token TEXT PRIMARY KEY,
      owner TEXT NOT NULL,
      user_id TEXT NOT NULL,
      message_id TEXT NOT NULL,
      event_id TEXT,
      expires_at INTEGER NOT NULL
    )
  `)
  db.run(`
    CREATE TABLE IF NOT EXISTS lark_card_action_queue (
      event_id TEXT PRIMARY KEY,
      token TEXT NOT NULL,
      action TEXT NOT NULL,
      form_value TEXT NOT NULL,
      claimed_at INTEGER,
      created_at INTEGER NOT NULL
    )
  `)
  db.run("CREATE INDEX IF NOT EXISTS lark_stream_rate_limit_sent_at ON lark_stream_rate_limit (sent_at)")
  const selectSession = db.query(`
    SELECT muted, intro_sent, root_message_id, thread_id, session_title, last_message_id
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
  const selectLastMessages = db.query(`
    SELECT session_id, directory, last_message_id
    FROM session_state
    WHERE last_message_id IS NOT NULL AND muted = 0
  `)
  const upsertSession = db.query(`
    INSERT INTO session_state (
      project_id, session_id, directory, muted, intro_sent, root_message_id, thread_id, session_title, last_message_id, updated_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(project_id, session_id) DO UPDATE SET
      directory = excluded.directory,
      muted = excluded.muted,
      intro_sent = excluded.intro_sent,
      root_message_id = excluded.root_message_id,
      thread_id = excluded.thread_id,
      session_title = excluded.session_title,
      last_message_id = excluded.last_message_id,
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
  const insertReactionInput = db.query(`
    INSERT OR IGNORE INTO reaction_input (reaction_key, updated_at)
    VALUES (?, ?)
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
  const deleteOldStreamRates = db.query("DELETE FROM lark_stream_rate_limit WHERE sent_at <= ?")
  const selectSecondStreamRate = db.query(`
    SELECT COUNT(*) AS count, MIN(sent_at) AS oldest
    FROM lark_stream_rate_limit
    WHERE sent_at > ?
  `)
  const selectMinuteStreamRate = db.query(`
    SELECT COUNT(*) AS count, MIN(sent_at) AS oldest
    FROM lark_stream_rate_limit
    WHERE sent_at > ?
  `)
  const insertStreamRate = db.query("INSERT INTO lark_stream_rate_limit (sent_at) VALUES (?)")
  const selectCardActionRoute = db.query(`
    SELECT owner, user_id, message_id, event_id, expires_at
    FROM lark_card_action_route
    WHERE token = ?
  `)
  const insertCardActionRoute = db.query(`
    INSERT OR REPLACE INTO lark_card_action_route (token, owner, user_id, message_id, event_id, expires_at)
    VALUES (?, ?, ?, ?, NULL, ?)
  `)
  const claimCardActionRoute = db.query(`
    UPDATE lark_card_action_route
    SET event_id = ?
    WHERE token = ? AND event_id IS NULL
  `)
  const resetCardActionRoute = db.query(`
    UPDATE lark_card_action_route
    SET event_id = NULL
    WHERE token = ? AND event_id = ?
  `)
  const deleteCardActionRoute = db.query("DELETE FROM lark_card_action_route WHERE token = ?")
  const insertCardActionQueue = db.query(`
    INSERT OR IGNORE INTO lark_card_action_queue (event_id, token, action, form_value, claimed_at, created_at)
    VALUES (?, ?, ?, ?, NULL, ?)
  `)
  const selectCardActionQueue = db.query(`
    SELECT queue.event_id, queue.token, queue.action, queue.form_value
    FROM lark_card_action_queue AS queue
    JOIN lark_card_action_route AS route ON route.token = queue.token
    WHERE route.owner = ? AND queue.claimed_at IS NULL
    ORDER BY queue.created_at
    LIMIT 1
  `)
  const claimCardActionQueue = db.query(`
    UPDATE lark_card_action_queue
    SET claimed_at = ?
    WHERE event_id = ? AND claimed_at IS NULL
  `)
  const deleteCardActionQueue = db.query("DELETE FROM lark_card_action_queue WHERE event_id = ?")
  db.query("DELETE FROM lark_card_action_route WHERE expires_at < ?").run(Date.now())
  db.query("DELETE FROM lark_card_action_queue WHERE created_at < ?").run(Date.now() - 24 * 60 * 60 * 1000)

  const cardActionStore: CardActionStore = {
    enqueue(actionInput) {
      db.run("BEGIN IMMEDIATE")
      try {
        const route = selectCardActionRoute.get(actionInput.token) as {
          user_id: string
          message_id: string
          event_id: string | null
          expires_at: number
        } | null
        if (!route || route.expires_at < Date.now()) {
          if (route) deleteCardActionRoute.run(actionInput.token)
          db.run("COMMIT")
          return "missing"
        }
        if (actionInput.userID !== route.user_id) {
          db.run("COMMIT")
          return "forbidden"
        }
        if (actionInput.messageID !== route.message_id) {
          db.run("COMMIT")
          return "stale"
        }
        if (route.event_id) {
          db.run("COMMIT")
          return route.event_id === actionInput.eventID ? "accepted" : "busy"
        }
        claimCardActionRoute.run(actionInput.eventID, actionInput.token)
        insertCardActionQueue.run(
          actionInput.eventID,
          actionInput.token,
          actionInput.action,
          JSON.stringify(actionInput.formValue),
          Date.now(),
        )
        db.run("COMMIT")
        return "accepted"
      } catch (error) {
        db.run("ROLLBACK")
        throw error
      }
    },
  }
  cardActionStores.add(cardActionStore)

  if (cardActions && !cardActionProcessorPaths.has(dbPath)) {
    cardActionProcessorPaths.add(dbPath)
    void (async () => {
      while (true) {
        const row = selectCardActionQueue.get(cardActionOwner) as {
          event_id: string
          token: string
          action: string
          form_value: string
        } | null
        if (row) {
          const claim = claimCardActionQueue.run(Date.now(), row.event_id)
          if (claim.changes > 0) {
            const route = cardActionRoutes.get(row.token)
            try {
              if (!route) throw new Error("card action owner is unavailable")
              const formValue = JSON.parse(row.form_value)
              await route.handle(row.action, record(formValue) ? formValue : {})
              deleteCardActionQueue.run(row.event_id)
            } catch (error) {
              deleteCardActionQueue.run(row.event_id)
              resetCardActionRoute.run(row.token, row.event_id)
              console.warn(
                "lark-notify plugin: card action failed",
                error instanceof Error ? error.message : error,
              )
            }
          }
        }
        await new Promise((resolve) => setTimeout(resolve, row ? 25 : 250))
      }
    })()
  }
  if (cardActions && appID && appSecret) startCardActionClient(appID, appSecret)

  const statsBySession = new Map<string, SessionStats>()
  const streamsBySession = new Map<string, StreamState>()
  const processedMessageIDs = new Set<string>()
  const pollOwner = `${input.project.id}:${input.directory}:${Math.random().toString(36).slice(2)}`
  const pollStartedAt = Date.now()
  let providerListPromise: Promise<unknown[] | undefined> | undefined
  let streamReservationQueue = Promise.resolve()
  let token: { value: string; expiresAt: number } | undefined

  function reserveStreamRateNow() {
    db.run("BEGIN IMMEDIATE")
    try {
      const now = Date.now()
      deleteOldStreamRates.run(now - 60_000)
      const second = selectSecondStreamRate.get(now - 1000) as StreamRateRow
      const minute = selectMinuteStreamRate.get(now - 60_000) as StreamRateRow
      const waits = [
        second.count >= STREAM_SECOND_LIMIT && second.oldest !== null ? second.oldest + 1001 - now : 0,
        minute.count >= STREAM_MINUTE_LIMIT && minute.oldest !== null ? minute.oldest + 60_001 - now : 0,
      ]
      const wait = Math.max(...waits)
      if (wait <= 0) insertStreamRate.run(now)
      db.run("COMMIT")
      return Math.max(0, wait)
    } catch (error) {
      db.run("ROLLBACK")
      throw error
    }
  }

  function reserveStreamRate() {
    const reservation = streamReservationQueue.then(async () => {
      while (true) {
        const wait = reserveStreamRateNow()
        if (wait <= 0) return
        await new Promise((resolve) => setTimeout(resolve, wait))
      }
    })
    streamReservationQueue = reservation.catch(() => undefined)
    return reservation
  }

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
      lastMessageID: row?.last_message_id ?? undefined,
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
      current.lastMessageID ?? null,
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

  function doneNoticeMessage(notice: DoneNotice, userID: string | undefined) {
    const contextPercent =
      notice.contextTokens && notice.contextLimit
        ? Math.round((notice.contextTokens / notice.contextLimit) * 100)
        : undefined
    const context = contextPercent === undefined ? undefined : `上下文 ${contextPercent}%`
    const fileChanges = notice.changed > 0 ? `修改 ${notice.changed} 个文件` : "未修改文件"
    return card("已完成", "green", [
      ...(userID ? [cardMarkdown(mention(userID), "0px 0px 6px 0px")] : []),
      ...(notice.userInput
        ? [cardMarkdown(truncateEnd(notice.userInput, 200)), cardDivider()]
        : []),
      cardMarkdown(notice.output),
      cardFooter([fileChanges, context].filter(Boolean).join("  ·  ")),
    ])
  }

  async function tenantToken() {
    if (token && token.expiresAt > Date.now() + 60_000) return token.value
    if (!appID || !appSecret) return
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

  async function larkAPI(
    method: "GET" | "POST" | "PUT",
    path: string,
    query?: Record<string, string>,
    body?: unknown,
    strict = false,
  ) {
    try {
      const nextToken = await tenantToken()
      if (!nextToken) throw new Error("tenant access token is unavailable")
      const url = new URL(`https://open.feishu.cn/open-apis${path}`)
      for (const [key, value] of Object.entries(query ?? {})) url.searchParams.set(key, value)
      const response = await fetch(url, {
        method,
        headers: {
          authorization: `Bearer ${nextToken}`,
          ...(method !== "GET" ? { "content-type": "application/json" } : {}),
        },
        body: method !== "GET" ? JSON.stringify(body ?? {}) : undefined,
      })
      const data = (await response.json()) as { code?: number; msg?: string; data?: unknown }
      if (response.ok && data.code === 0) return data.data
      throw new Error(`${path} failed (${response.status}) ${data.msg ?? JSON.stringify(data)}`)
    } catch (error) {
      if (strict) throw error
      console.warn("lark-notify plugin:", error instanceof Error ? error.message : error)
    }
  }

  let mentionUserIDPromise: Promise<string | undefined> | undefined

  async function mentionUserID() {
    if (!mentionEmail) return
    if (mentionUserIDPromise) return mentionUserIDPromise
    mentionUserIDPromise = larkAPI(
      "POST",
      "/contact/v3/users/batch_get_id",
      { user_id_type: "open_id" },
      { emails: [mentionEmail] },
    ).then((data) => {
      const users = prop(data, "user_list")
      if (!Array.isArray(users)) return
      const user = users.find((item) => textOption(prop(item, "email")) === mentionEmail) ?? users[0]
      const userID = textOption(prop(user, "user_id") ?? prop(user, "open_id"))
      if (!userID) console.warn(`lark-notify plugin: cannot resolve LARK_MENTION_EMAIL ${mentionEmail}`)
      return userID
    })
    return mentionUserIDPromise
  }

  function larkBody(message: LarkMessage) {
    if (typeof message === "string") return { msg_type: "text", content: larkText(message) }
    if (message.type === "card_reference") {
      return {
        msg_type: "interactive",
        content: JSON.stringify({ type: "card", data: { card_id: message.cardID } }),
      }
    }
    return { msg_type: message.type, content: JSON.stringify(message.card) }
  }

  async function send(message: LarkMessage, rootMessageID?: string, strict = false) {
    if (!chatID) {
      if (strict) throw new Error("LARK_CHAT_ID is unavailable")
      return
    }
    const data = rootMessageID
      ? await larkAPI("POST", `/im/v1/messages/${encodeURIComponent(rootMessageID)}/reply`, undefined, {
          ...larkBody(message),
        }, strict)
      : await larkAPI(
          "POST",
          "/im/v1/messages",
          { receive_id_type: "chat_id" },
          { receive_id: chatID, ...larkBody(message) },
          strict,
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
    current.rootMessagePromise = gitBranch(input.directory)
      .then((branch) =>
        send(
          card("已开始", "blue", [
            cardMarkdown(truncateEnd(current.userInput || "等待任务内容", 200)),
            cardDivider(),
            cardDetails(
              `**会话 ID**\n${sessionID}\n\n**工作路径**\n${input.directory}\n\n**所属分支**\n${branch}`,
            ),
          ]),
        ),
      )
      .then((result) => {
        current.rootMessageID = result?.messageID
        current.threadID = result?.threadID
        current.lastMessageID = result?.messageID
        current.rootMessagePromise = undefined
        current.introSent = true
        saveSession(sessionID)
        return current.rootMessageID
      })
    return current.rootMessagePromise
  }

  async function sendReply(sessionID: string, message: LarkMessage, strict = false) {
    const rootID = await rootMessage(sessionID)
    if (!rootID) {
      if (strict) throw new Error(`root message is unavailable for session ${sessionID}`)
      return
    }
    const result = await send(message, rootID, strict)
    if (result?.threadID) {
      stats(sessionID).threadID = result.threadID
    }
    if (result?.messageID) stats(sessionID).lastMessageID = result.messageID
    if (result?.threadID || result?.messageID) saveSession(sessionID)
    return result
  }

  function scheduleStreamFlush(sessionID: string, state: StreamState) {
    if (
      state.closing ||
      state.failed ||
      state.attention ||
      !state.cardID ||
      !state.messageID ||
      state.timer ||
      state.flushPromise
    )
      return
    const delay = Math.max(0, state.lastSentAt + STREAM_SESSION_INTERVAL - Date.now())
    state.timer = setTimeout(() => {
      state.timer = undefined
      void flushStream(sessionID, state)
    }, delay)
  }

  async function flushStream(sessionID: string, state: StreamState) {
    if (state.flushPromise) return state.flushPromise
    const flush = (async () => {
      if (state.closing || state.attention || !state.cardID || !state.messageID) return
      const content = state.pendingContent
      if (content === state.lastSentContent) return
      await reserveStreamRate()
      if (state.closing || state.attention || !state.cardID) return
      const sequence = ++state.sequence
      await larkAPI(
        "PUT",
        `/cardkit/v1/cards/${encodeURIComponent(state.cardID)}/elements/${STREAM_ELEMENT_ID}/content`,
        undefined,
        { content, sequence, uuid: crypto.randomUUID() },
        true,
      )
      state.lastSentContent = content
      state.lastSentAt = Date.now()
    })()
    state.flushPromise = flush
    try {
      await flush
    } catch (error) {
      state.failed = true
      console.warn(
        `lark-notify plugin: streaming update failed for ${sessionID}`,
        error instanceof Error ? error.message : error,
      )
    } finally {
      state.flushPromise = undefined
      if (state.pendingContent !== state.lastSentContent) scheduleStreamFlush(sessionID, state)
    }
  }

  async function startStream(
    sessionID: string,
    state: StreamState,
    initialCard?: CardSpec,
    displayedContent?: string,
  ) {
    const initialContent = state.pendingContent
    try {
      const cardData = await larkAPI(
        "POST",
        "/cardkit/v1/cards",
        undefined,
        {
          type: "card_json",
          data: JSON.stringify(initialCard ?? streamCard(stats(sessionID).userInput, initialContent)),
        },
        true,
      )
      const cardID = textOption(prop(cardData, "card_id") ?? prop(cardData, "cardId"))
      if (!cardID) throw new Error("CardKit create response did not include card_id")
      state.cardID = cardID
      const result = await sendReply(sessionID, { type: "card_reference", cardID }, true)
      if (!result?.messageID) throw new Error("CardKit message response did not include message_id")
      state.messageID = result.messageID
      state.lastSentContent = displayedContent ?? initialContent
      state.lastSentAt = Date.now()
      if (state.pendingContent !== initialContent) scheduleStreamFlush(sessionID, state)
    } catch (error) {
      state.failed = true
      console.warn(
        `lark-notify plugin: failed to start streaming card for ${sessionID}`,
        error instanceof Error ? error.message : error,
      )
    }
  }

  async function updateProgress(notice: ProgressNotice) {
    const content = truncateStart(
      notice.kind === "tool" ? `正在运行：${truncateEnd(notice.content, 200)}` : notice.content,
      maxOutputChars,
    )
    const existing = streamsBySession.get(notice.sessionID)
    if (existing) {
      if (existing.closing || existing.pendingContent === content) return
      existing.pendingContent = content
      if (!existing.startPromise) existing.startPromise = startStream(notice.sessionID, existing)
      await existing.startPromise
      if (existing.attention) return
      scheduleStreamFlush(notice.sessionID, existing)
      return
    }

    const state: StreamState = {
      pendingContent: content,
      lastSentContent: "",
      sequence: 0,
      lastSentAt: 0,
      closing: false,
      failed: false,
    }
    streamsBySession.set(notice.sessionID, state)
    state.startPromise = startStream(notice.sessionID, state)
    await state.startPromise
  }

  async function replaceStreamCard(state: StreamState, nextCard: CardSpec) {
    if (!state.cardID || !state.messageID) throw new Error("streaming card is unavailable")
    await larkAPI(
      "PUT",
      `/cardkit/v1/cards/${encodeURIComponent(state.cardID)}`,
      undefined,
      {
        card: { type: "card_json", data: JSON.stringify(nextCard) },
        sequence: ++state.sequence,
        uuid: crypto.randomUUID(),
      },
      true,
    )
  }

  async function callOpenCode(serviceName: "permission" | "question", method: "reply" | "reject", payload: object) {
    const service = prop(input.client, serviceName)
    const operation = prop(service, method)
    if (!service || typeof operation !== "function") {
      throw new Error(`OpenCode client does not support ${serviceName}.${method}`)
    }
    const result = await Reflect.apply(operation, service, [payload])
    const error = prop(result, "error")
    if (error) throw new Error(`${serviceName}.${method} failed: ${JSON.stringify(error)}`)
  }

  function removeAttentionAction(token: string | undefined) {
    if (!token) return
    cardActionRoutes.delete(token)
    deleteCardActionRoute.run(token)
  }

  function registerAttentionAction(
    sessionID: string,
    state: StreamState,
    requestID: string,
    kind: "permission" | "question",
    token: string | undefined,
    userID: string | undefined,
  ) {
    if (!token || !userID || !state.messageID) return
    insertCardActionRoute.run(
      token,
      cardActionOwner,
      userID,
      state.messageID,
      Date.now() + 24 * 60 * 60 * 1000,
    )
    cardActionRoutes.set(token, {
      async handle(action, formValue) {
        const current = streamsBySession.get(sessionID)
        if (current?.attention?.token !== token || current.attention.requestID !== requestID) {
          throw new Error("card action is stale")
        }

        if (kind === "permission") {
          const reply =
            action === "permission_once"
              ? "once"
              : action === "permission_always"
                ? "always"
                : action === "permission_reject"
                  ? "reject"
                  : undefined
          if (!reply) throw new Error(`unsupported permission action ${action}`)
          await callOpenCode("permission", "reply", {
            requestID,
            directory: input.directory,
            reply,
          })
        } else if (action === "question_reply") {
          const answer = textOption(prop(formValue, "answer"))
          if (!answer) throw new Error("question answer is empty")
          await callOpenCode("question", "reply", {
            requestID,
            directory: input.directory,
            answers: [[answer]],
          })
        } else if (action === "question_reject") {
          await callOpenCode("question", "reject", {
            requestID,
            directory: input.directory,
          })
        } else {
          throw new Error(`unsupported question action ${action}`)
        }

        await clearAttention(requestID, sessionID)
      },
    })
  }

  async function showAttention(
    sessionID: string,
    requestID: string,
    kind: "permission" | "question",
    detail: string,
  ) {
    const userID = await mentionUserID()
    const token = cardActions && userID ? crypto.randomUUID() : undefined
    const content = [mention(userID), detail].filter(Boolean).join("\n\n")
    const title = kind === "permission" ? "需要授权" : "等待回答"
    const controls = token
      ? kind === "permission"
        ? permissionControls(token)
        : questionControls(token)
      : []
    const nextCard = streamCard(stats(sessionID).userInput, content, title, "orange", controls)
    let state = streamsBySession.get(sessionID)

    if (!state) {
      state = {
        pendingContent: "正在处理…",
        lastSentContent: "",
        sequence: 0,
        lastSentAt: 0,
        attention: { requestID, kind, token },
        closing: false,
        failed: false,
      }
      streamsBySession.set(sessionID, state)
      state.startPromise = startStream(sessionID, state, nextCard, content)
      await state.startPromise
      registerAttentionAction(sessionID, state, requestID, kind, token, userID)
      return
    }

    removeAttentionAction(state.attention?.token)
    state.attention = { requestID, kind, token }
    if (state.timer) clearTimeout(state.timer)
    state.timer = undefined
    await state.startPromise
    await state.flushPromise
    if (state.closing || !state.cardID || !state.messageID) return
    try {
      await replaceStreamCard(state, nextCard)
      state.lastSentContent = content
      state.lastSentAt = Date.now()
      state.failed = false
      registerAttentionAction(sessionID, state, requestID, kind, token, userID)
    } catch (error) {
      state.failed = true
      console.warn(
        `lark-notify plugin: failed to show ${kind} state for ${sessionID}`,
        error instanceof Error ? error.message : error,
      )
    }
  }

  async function clearAttention(requestID: string, sessionID?: string) {
    const entry = sessionID
      ? ([sessionID, streamsBySession.get(sessionID)] as const)
      : Array.from(streamsBySession.entries()).find(([, state]) => state.attention?.requestID === requestID)
    if (!entry) return
    const [targetSessionID, state] = entry
    if (!state || state.attention?.requestID !== requestID) return
    removeAttentionAction(state.attention.token)
    state.attention = undefined
    await state.startPromise
    if (state.closing || !state.cardID || !state.messageID) return
    try {
      await replaceStreamCard(state, streamCard(stats(targetSessionID).userInput, state.pendingContent))
      state.lastSentContent = state.pendingContent
      state.lastSentAt = Date.now()
      state.failed = false
    } catch (error) {
      state.failed = true
      console.warn(
        `lark-notify plugin: failed to resume streaming card for ${targetSessionID}`,
        error instanceof Error ? error.message : error,
      )
    }
  }

  async function finalizeStream(notice: DoneNotice, userID: string | undefined) {
    const state = streamsBySession.get(notice.sessionID)
    if (!state) return false
    removeAttentionAction(state.attention?.token)
    state.closing = true
    if (state.timer) clearTimeout(state.timer)
    state.timer = undefined
    await state.startPromise
    await state.flushPromise
    if (!state.cardID || !state.messageID) {
      streamsBySession.delete(notice.sessionID)
      return false
    }

    try {
      const finalCard = doneNoticeMessage(notice, userID).card
      finalCard.config = { update_multi: true, streaming_mode: false }
      await replaceStreamCard(state, finalCard)
      streamsBySession.delete(notice.sessionID)
      return true
    } catch (error) {
      streamsBySession.delete(notice.sessionID)
      console.warn(
        `lark-notify plugin: failed to finalize streaming card for ${notice.sessionID}`,
        error instanceof Error ? error.message : error,
      )
      return false
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

  async function handleOKReaction(row: LastMessageRow, reaction: unknown) {
    const messageID = row.last_message_id
    if (!messageID) return
    const reactionType = textOption(prop(prop(reaction, "reaction_type"), "emoji_type") ?? prop(reaction, "emoji_type"))
    if (reactionType !== "OK") return
    const operator = prop(reaction, "operator")
    if (prop(operator, "operator_type") === "app") return
    const actionTime = numberOption(prop(reaction, "action_time"))
    if (actionTime !== undefined && actionTime < pollStartedAt) return
    const reactionID = textOption(prop(reaction, "reaction_id"))
    const operatorID = textOption(prop(operator, "operator_id"), "unknown") ?? "unknown"
    const key = reactionID ? `${messageID}:${reactionID}` : `${messageID}:${operatorID}:${actionTime ?? "unknown"}`

    insertReactionInput.run(key, Date.now())
    if ((db.query("SELECT changes() AS count").get() as { count: number }).count !== 1) return
    const result = await input.client.session.promptAsync({
      path: { id: row.session_id },
      query: { directory: row.directory ?? input.directory },
      body: { parts: [{ type: "text", text: "OK" }] },
    })
    if (prop(result, "error")) console.warn("lark-notify plugin: failed to forward Lark OK reaction", prop(result, "error"))
  }

  async function pollOKReactions(row: LastMessageRow) {
    const messageID = row.last_message_id
    if (!messageID) return
    let pageToken: string | undefined
    do {
      const data = await larkAPI("GET", `/im/v1/messages/${encodeURIComponent(messageID)}/reactions`, {
        reaction_type: "OK",
        user_id_type: "open_id",
        page_size: "50",
        ...(pageToken ? { page_token: pageToken } : {}),
      })
      const items = prop(data, "items")
      if (Array.isArray(items)) {
        for (const item of items) await handleOKReaction(row, item)
      }
      pageToken = textOption(prop(data, "page_token"))
      if (prop(data, "has_more") !== true) return
    } while (pageToken)
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
          for (const row of selectLastMessages.all() as LastMessageRow[]) await pollOKReactions(row)
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
      ...(streamOutput ? { updateProgress } : {}),
      async sendDone(notice) {
        applySessionNotice(notice)
        const userID = await mentionUserID()
        if (await finalizeStream(notice, userID)) return
        await sendReply(notice.sessionID, doneNoticeMessage(notice, userID))
      },
      async sendPermission(notice) {
        await showAttention(
          notice.sessionID,
          notice.requestID,
          "permission",
          `**权限**\n${notice.permission}\n\n**范围**\n${truncateEnd(notice.patterns, 400)}`,
        )
      },
      async clearPermission(requestID, sessionID) {
        await clearAttention(requestID, sessionID)
      },
      async sendQuestion(notice) {
        const detail = [
          ...(notice.header ? [`**${notice.header}**`] : []),
          notice.question ?? "请在此话题中回复，OpenCode 会继续处理。",
        ].join("\n\n")
        await showAttention(
          notice.sessionID,
          notice.requestID,
          "question",
          detail,
        )
      },
      async clearQuestion(requestID, sessionID) {
        await clearAttention(requestID, sessionID)
      },
    },
  })
}) satisfies Plugin
