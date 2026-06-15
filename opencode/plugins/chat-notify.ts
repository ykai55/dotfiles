import type { Hooks, Plugin } from "@opencode-ai/plugin"
import { dirname } from "node:path"
import { fileURLToPath } from "node:url"
import lark from "./chat-notify/lark"
import telegram from "./chat-notify/telegram"

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

const readPluginConfig = () => readConfigFile(`${dirname(fileURLToPath(import.meta.url))}/chat-notify.conf`)

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

export default (async (input, options) => {
  const config = await readPluginConfig()
  const plugins = (
    await Promise.all([
      enabled(options?.telegram, enabled(config.ENABLE_TELEGRAM_NOTIFY, true))
        ? telegram(input, option(options, "telegram"))
        : undefined,
      enabled(options?.lark, enabled(config.ENABLE_LARK_NOTIFY, true)) ? lark(input, option(options, "lark")) : undefined,
    ])
  ).filter((plugin): plugin is Hooks => !!plugin)

  return {
    async "chat.message"(message, output) {
      await Promise.all(plugins.map((plugin) => plugin["chat.message"]?.(message, output)))
    },

    async event(input) {
      await Promise.all(plugins.map((plugin) => plugin.event?.(input)))
    },
  }
}) satisfies Plugin
