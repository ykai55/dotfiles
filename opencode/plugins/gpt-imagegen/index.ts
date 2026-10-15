import type { Plugin } from "@opencode/plugin"
import { readFile, mkdir, writeFile } from "node:fs/promises"
import { resolve, join } from "node:path"
import { randomUUID } from "node:crypto"

type Input = { prompt: string; images?: string[]; attachments?: string[]; outputDirectory?: string; model?: string }

export default {
  id: "gpt-imagegen",
  async setup(ctx) {
    const generate = async (args: Input, sessionID: string, signal: AbortSignal) => {
      if (!args || typeof args.prompt !== "string" || !args.prompt.trim()) throw new Error("Provide a non-empty prompt")
      if (args.images !== undefined && (!Array.isArray(args.images) || args.images.some(path => typeof path !== "string") || args.images.length > 4)) throw new Error("Provide at most four image paths")
      if (args.attachments !== undefined && (!Array.isArray(args.attachments) || args.attachments.some(name => typeof name !== "string" || !name.trim()))) throw new Error("attachments must contain image attachment names")
      if ((args.images?.length ?? 0) + (args.attachments?.length ?? 0) > 4) throw new Error("Provide at most four images including chat attachments")
      if (args.outputDirectory !== undefined && typeof args.outputDirectory !== "string") throw new Error("outputDirectory must be a path")
      if (args.model !== undefined && (typeof args.model !== "string" || !args.model.trim())) throw new Error("model must be a non-empty model ID")
      const session = await ctx.session.get({ sessionID })
      const directory = session.location.directory
      if (!directory) throw new Error("Session has no working directory")
      const output = resolve(directory, args.outputDirectory ?? "output/gpt-imagegen")
      const content: Array<Record<string, unknown>> = [{ type: "input_text", text: args.prompt }]
      const inputs: Buffer[] = []
      for (const path of args.images ?? []) {
        const absolute = resolve(directory, path)
        inputs.push(await readFile(absolute))
      }
      if (args.attachments?.length) {
        const messages = await ctx.session.context({ sessionID })
        for (const name of args.attachments) {
          const message = messages.findLast(message => message.type === "user" && message.files?.some(file => file.name === name))
          if (message?.type !== "user") throw new Error(`Attachment not found: ${name}. Upload the image in this session or provide a local path.`)
          const matches = message.files?.filter(file => file.name === name) ?? []
          if (matches.length !== 1) throw new Error(`Ambiguous attachment name: ${name}. Upload images with unique names.`)
          const file = matches[0]
          if (!file.mime.startsWith("image/") || !file.data) throw new Error(`Attachment has no inline image data: ${name}. Provide a local path instead.`)
          if (file.data.length > 28 * 1024 * 1024) throw new Error("Input image exceeds 20 MiB")
          inputs.push(Buffer.from(file.data, "base64"))
        }
      }
      for (const data of inputs) {
        if (data.length > 20 * 1024 * 1024) throw new Error("Input image exceeds 20 MiB")
        const mime = data.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])) ? "image/png"
          : data[0] === 255 && data[1] === 216 && data[2] === 255 ? "image/jpeg"
          : data.toString("ascii", 0, 4) === "RIFF" && data.toString("ascii", 8, 12) === "WEBP" ? "image/webp" : undefined
        if (!mime) throw new Error("Input must be a PNG, JPEG, or WebP image")
        content.push({ type: "input_image", image_url: `data:${mime};base64,${data.toString("base64")}` })
      }
      const connection = await ctx.integration.connection.active("openai")
      if (!connection) throw new Error("Sign in to OpenAI with your ChatGPT/Codex account in OpenCode first")
      const credential = await ctx.integration.connection.resolve(connection)
      if (credential?.type !== "oauth") throw new Error("gpt-imagegen requires the active OpenAI ChatGPT/Codex OAuth connection")
      const headers: Record<string, string> = { Authorization: `Bearer ${credential.access}`, "Content-Type": "application/json" }
      const account = credential.metadata?.accountId
      if (typeof account === "string") headers["ChatGPT-Account-Id"] = account
      const model = args.model ?? "gpt-6-astra"
      const requestSignal = AbortSignal.any([signal, AbortSignal.timeout(300000)])
      const redact = (value: unknown) => {
        let message = typeof value === "string" ? value : "Request failed"
        for (const secret of [credential.access, credential.refresh, account]) {
          if (typeof secret === "string" && secret) message = message.split(secret).join("[REDACTED]")
        }
        return message.replace(/(?:sk-|eyJ)[A-Za-z0-9_.-]+/g, "[REDACTED]").slice(0, 1200)
      }
      let response: Response
      try {
        response = await fetch("https://chatgpt.com/backend-api/codex/responses", {
          method: "POST", headers, redirect: "error", signal: requestSignal,
          body: JSON.stringify({
            model, instructions: "Use the image generation tool to fulfill the request. For edits, preserve everything the user did not ask to change.",
            input: [{ role: "user", content }],
            tools: [{ type: "image_generation", output_format: "png" }],
            tool_choice: { type: "image_generation" }, store: false, stream: true,
          }),
        })
      } catch {
        requestSignal.throwIfAborted()
        throw new Error("Codex image request failed to connect")
      }
      if (!response.ok) {
        const body = await response.json().catch(() => undefined) as any
        throw new Error(`Codex image request HTTP ${response.status}: ${redact(body?.error?.message ?? body?.detail ?? body?.message)}`)
      }
      if (!response.body) throw new Error("Codex returned no response body")
      const results = new Map<string, string>()
      let completed = false
      let pending = ""
      const decoder = new TextDecoder()
      const consume = (line: string) => {
        if (!line.startsWith("data:")) return
        const text = line.slice(5).trim()
        if (!text || text === "[DONE]") return
        const event = JSON.parse(text)
        if (event.type === "error" || event.type === "response.failed" || event.type === "response.incomplete") {
          throw new Error(redact(event.message ?? event.error?.message ?? event.response?.error?.message ?? "Image generation did not complete"))
        }
        if (event.type === "response.output_item.done" && event.item?.type === "image_generation_call" && typeof event.item.result === "string") results.set(event.item.id, event.item.result)
        if (event.type === "response.completed") {
          completed = true
          for (const item of event.response?.output ?? []) {
            if (item.type === "image_generation_call" && typeof item.result === "string") results.set(item.id, item.result)
          }
        }
      }
      const reader = response.body.getReader()
      try {
        while (true) {
          const { done, value } = await reader.read()
          pending += decoder.decode(value, { stream: !done })
          let newline: number
          while ((newline = pending.indexOf("\n")) !== -1) {
            consume(pending.slice(0, newline))
            pending = pending.slice(newline + 1)
          }
          if (done) break
        }
        if (pending.trim()) consume(pending)
      } finally {
        await reader.cancel().catch(() => undefined)
        reader.releaseLock()
      }
      if (!completed || !results.size) throw new Error("Codex did not return a completed image; no output saved")
      signal.throwIfAborted()
      const images = [...results.values()].map(base64 => {
        const image = Buffer.from(base64, "base64")
        if (!image.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) throw new Error("Codex returned an invalid PNG")
        return image
      })
      await mkdir(output, { recursive: true })
      const paths = []
      for (const image of images) {
        const path = join(output, `${randomUUID()}.png`)
        await writeFile(path, image, { flag: "wx" })
        paths.push(path)
      }
      return { model, images: paths }
    }

    await ctx.tool.transform(editor => {
      editor.add({
        name: "gpt_imagegen", options: { codemode: false },
        description: "Generate or edit images using the active Codex OAuth connection. Accepts local image paths or chat attachment names (for example image-1.png); the plugin reads attachment data from the current session, so pasted images do not need a local path. Load the gpt-imagegen skill for guidance. Returns saved PNG paths. Optional model selects the orchestrating model, not the image model.",
        input: {
          type: "object", properties: {
            prompt: { type: "string", description: "Describe the requested image or exact edits and preservation constraints" },
            images: { type: "array", items: { type: "string" }, maxItems: 4, description: "Local input image paths for editing or references" },
            attachments: { type: "array", items: { type: "string" }, maxItems: 4, description: "Names of pasted/uploaded chat images, e.g. image-1.png. Uses the newest user message containing each name in the current session. No local path needed." },
            outputDirectory: { type: "string", description: "Defaults to output/gpt-imagegen in the session working directory" },
            model: { type: "string", description: "Codex orchestrating model ID; default gpt-6-astra" },
          }, required: ["prompt"], additionalProperties: false,
        },
        async execute(input, context) {
          return { content: JSON.stringify(await generate(input as Input, context.sessionID, context.signal)) }
        },
      })
    })
    await ctx.command.transform(editor => {
      editor.add({
        name: "gpt-imagegen", description: "Generate/edit an image; accepts a prompt or a JSON object matching gpt_imagegen arguments",
        async execute({ sessionID, prompt }) {
          const args = prompt.text.trim().startsWith("{") ? JSON.parse(prompt.text) as Input : { prompt: prompt.text }
          const result = await generate(args, sessionID, AbortSignal.timeout(300000))
          await ctx.session.synthetic({ sessionID, text: `gpt-imagegen saved images: ${JSON.stringify(result)}` })
        },
      })
    })
  },
} satisfies Parameters<typeof Plugin.define>[0]
