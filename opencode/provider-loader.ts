import { readdir, readFile } from "node:fs/promises"
import { homedir } from "node:os"
import { dirname, extname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import type { Config } from "@opencode-ai/sdk"
import type { Plugin } from "@opencode-ai/plugin"

const providersDirectory = join(dirname(fileURLToPath(import.meta.url)), "providers")
const envFile = join(homedir(), ".config", "opencode", ".env")
const fixedEnvironmentNames = new Set(["OPENAI_BASE_URL", "OPENAI_API_KEY"])
const reservedNames = new Set(["__proto__", "constructor", "prototype"])

const object = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === "object" && !Array.isArray(value)

const parseEnvValue = (rawValue: string, lineNumber: number) => {
  let value = rawValue.trim()
  if (!value) return ""
  if (value[0] === "'" || value[0] === '"') {
    const quote = value[0]
    let escaped = false
    let result = ""
    for (let index = 1; index < value.length; index += 1) {
      const char = value[index]
      if (quote === '"' && escaped) {
        result += ({ n: "\n", r: "\r", t: "\t" } as Record<string, string>)[char] ?? char
        escaped = false
        continue
      }
      if (quote === '"' && char === "\\") {
        escaped = true
        continue
      }
      if (char === quote) {
        const rest = value.slice(index + 1).trim()
        if (rest && !rest.startsWith("#")) {
          throw new Error(`${envFile}:${lineNumber}: unexpected content after quoted value`)
        }
        return result
      }
      result += char
    }
    throw new Error(`${envFile}:${lineNumber}: unterminated quoted value`)
  }
  const comment = value.search(/\s+#/)
  if (comment >= 0) value = value.slice(0, comment).trimEnd()
  if (/\s/.test(value)) throw new Error(`${envFile}:${lineNumber}: unquoted whitespace is not supported`)
  return value
}

const parseEnv = (text: string) => {
  const values: Record<string, string> = Object.create(null)
  for (const [index, rawLine] of text.split(/\r?\n/).entries()) {
    let line = rawLine.trim()
    if (!line || line.startsWith("#")) continue
    if (line.startsWith("export ")) line = line.slice(7).trimStart()
    const separator = line.indexOf("=")
    if (separator < 1) throw new Error(`${envFile}:${index + 1}: expected NAME=VALUE`)
    const name = line.slice(0, separator).trim()
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) {
      throw new Error(`${envFile}:${index + 1}: invalid environment variable name`)
    }
    if (!fixedEnvironmentNames.has(name)) continue
    values[name] = parseEnvValue(line.slice(separator + 1), index + 1)
  }
  return values
}

const loadEnvironment = async () => {
  let fileValues: Record<string, string> = Object.create(null)
  try {
    fileValues = parseEnv(await readFile(envFile, "utf8"))
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error
  }
  return {
    OPENAI_BASE_URL: process.env.OPENAI_BASE_URL ?? fileValues.OPENAI_BASE_URL ?? "",
    OPENAI_API_KEY: process.env.OPENAI_API_KEY ?? fileValues.OPENAI_API_KEY ?? "",
  }
}

const substitute = (value: unknown, environment: Record<string, string>): unknown => {
  if (typeof value === "string") {
    return value.replace(/\{env:([^}]+)\}/g, (_match, name: string) => environment[name] ?? "")
  }
  if (Array.isArray(value)) return value.map((child) => substitute(child, environment))
  if (!object(value)) return value
  return Object.fromEntries(Object.entries(value).map(([key, child]) => [key, substitute(child, environment)]))
}

const validateFile = (value: unknown, filename: string, environment: Record<string, string>) => {
  if (!object(value) || !object(value.provider)) {
    throw new Error(`${filename}: expected an object with a provider object`)
  }
  const entries = Object.entries(value.provider)
  if (entries.length !== 1) throw new Error(`${filename}: expected exactly one provider`)
  const [name, provider] = entries[0]
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(name) || reservedNames.has(name) || `${name}.json` !== filename) {
    throw new Error(`${filename}: provider name must match the filename`)
  }
  if (!object(provider) || typeof provider.npm !== "string" || !object(provider.options) || !object(provider.models)) {
    throw new Error(`${filename}: invalid provider structure`)
  }
  for (const model of Object.keys(provider.models)) {
    if (reservedNames.has(model)) throw new Error(`${filename}: unsafe model ID`)
  }
  return [name, substitute(provider, environment)] as const
}

export default (async () => ({
  async config(config: Config) {
    const environment = await loadEnvironment()
    let files: string[]
    try {
      files = (await readdir(providersDirectory)).filter((file) => extname(file) === ".json").sort()
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return
      throw error
    }
    const loaded: Record<string, unknown> = Object.create(null)
    for (const filename of files) {
      const file = resolve(providersDirectory, filename)
      if (dirname(file) !== providersDirectory) throw new Error(`invalid provider filename: ${filename}`)
      const value = JSON.parse(await readFile(file, "utf8"))
      const [name, provider] = validateFile(value, filename, environment)
      if (Object.hasOwn(loaded, name)) throw new Error(`duplicate provider: ${name}`)
      loaded[name] = provider
    }
    config.provider = { ...(config.provider ?? {}), ...loaded } as Config["provider"]
  },
})) satisfies Plugin
