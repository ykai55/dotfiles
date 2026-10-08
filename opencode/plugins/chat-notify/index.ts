import { Plugin } from "@opencode/plugin"
import type { NotifyChannel } from "./dispatcher"
import { dirname } from "node:path"
import { fileURLToPath } from "node:url"
import lark from "./lark"
import telegram from "./telegram"

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

const enabled = (value: unknown, fallback = true) => {
  if (value === false || value === "0" || value === "false") return false
  if (value === true || value === "1" || value === "true") return true
  return fallback
}

const option = (options: Record<string, unknown> | undefined, key: string) => {
  const value = options?.[key]
  if (value && typeof value === "object" && !Array.isArray(value)) return value as Record<string, unknown>
  return undefined
}

export default Plugin.define({
  id: "chat-notify",
  async setup(ctx) {
    const options = ctx.options
    const config = await readPluginConfig()
    const channels: NotifyChannel[] = []
    try {
      if (enabled(options.telegram, enabled(config.ENABLE_TELEGRAM_NOTIFY, true))) {
        channels.push(await telegram(ctx, option(options, "telegram")))
      }
      if (enabled(options.lark, enabled(config.ENABLE_LARK_NOTIFY, true))) {
        channels.push(await lark(ctx, option(options, "lark")))
      }
    } catch (error) {
      await Promise.allSettled(channels.map((channel) => channel.dispose()))
      throw error
    }
    const controller = new AbortController()
    const events = (async () => {
      for await (const event of ctx.event.subscribe({ signal: controller.signal })) {
        await Promise.all(channels.map((channel) => channel.event(event)))
      }
    })().catch((error) => {
      if (!controller.signal.aborted) console.warn("[chat-notify] Event subscription failed:", error)
    })
    return async () => {
      controller.abort()
      await Promise.all(channels.map((channel) => channel.dispose()))
      await events
    }
  },
})
