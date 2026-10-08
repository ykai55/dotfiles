import { afterEach, expect, test } from "bun:test"
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import providerLoader from "../provider-loader/index.ts"

let directory
const environment = { baseURL: process.env.OPENAI_BASE_URL, apiKey: process.env.OPENAI_API_KEY }
afterEach(async () => {
  if (directory) await rm(directory, { recursive: true, force: true })
  for (const [key, value] of [["OPENAI_BASE_URL", environment.baseURL], ["OPENAI_API_KEY", environment.apiKey]]) {
    if (value === undefined) delete process.env[key]
    else process.env[key] = value
  }
})

test("provider transform is replayable and preserves environment precedence and model IDs", async () => {
  directory = await mkdtemp(join(tmpdir(), "opencode-provider-test-"))
  const providersDirectory = join(directory, "providers")
  await mkdir(providersDirectory)
  const envFile = join(directory, ".env")
  await writeFile(envFile, 'OPENAI_BASE_URL="https://example.invalid/v1"\nOPENAI_API_KEY="file-key"\n')
  process.env.OPENAI_API_KEY = "process-key"
  delete process.env.OPENAI_BASE_URL
  await writeFile(join(providersDirectory, "test.json"), JSON.stringify({ provider: { test: { npm: "@ai-sdk/openai-compatible", name: "Test", options: { baseURL: "{env:OPENAI_BASE_URL}", apiKey: "{env:OPENAI_API_KEY}" }, models: { "test-model": { name: "Test model" } } } } }))
  let transform
  await providerLoader.setup({ options: { providersDirectory, envFile }, provider: { async transform(callback) { transform = callback } } })
  await rm(providersDirectory, { recursive: true })
  const entries = []
  transform({ add(entry) { entries.push(entry) } })
  transform({ add(entry) { entries.push(entry) } })
  expect(entries[0]).toEqual(entries[1])
  expect(entries[0].info).toMatchObject({ id: "test", package: "aisdk:@ai-sdk/openai-compatible", settings: { apiKey: "process-key", baseURL: "https://example.invalid/v1" } })
  expect(entries[0].models[0]).toMatchObject({ id: "test-model", name: "Test model", providerID: "test" })
})

test("provider filename mismatches fail before registration", async () => {
  directory = await mkdtemp(join(tmpdir(), "opencode-provider-test-"))
  await writeFile(join(directory, "wrong.json"), JSON.stringify({ provider: { other: {} } }))
  await expect(providerLoader.setup({ options: { providersDirectory: directory, envFile: join(directory, "missing") }, provider: { transform() { throw new Error("unexpected registration") } } })).rejects.toThrow("provider name must match")
})
