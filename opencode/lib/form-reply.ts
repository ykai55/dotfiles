import { serviceClient } from "./service-client"

export async function answerForm(sessionID: string, formID: string, text: string, signal?: AbortSignal, options: Record<string, unknown> = {}) {
  const client = await serviceClient(options)
  const form = await client.session.form.get({ sessionID, formID }, { signal })
  const fields = form.fields.filter((field) => field.type !== "external" && !field.hidden)
  let answer: Record<string, string | number | boolean | readonly string[]>
  if (fields.length === 1 && fields[0].type === "string") {
    const field = fields[0]
    answer = { [field.key]: field.options?.find((option) => option.label === text)?.value ?? text }
  } else if (fields.length === 1 && fields[0].type === "multiselect") {
    const field = fields[0]
    const values: unknown = text.trimStart().startsWith("[") ? JSON.parse(text) : [text]
    if (!Array.isArray(values) || !values.every((value) => typeof value === "string")) throw new Error("Expected a JSON array of choices")
    answer = { [field.key]: values.map((value) => field.options.find((option) => option.label === value)?.value ?? value) }
  } else {
    const parsed = JSON.parse(text)
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("Expected a JSON object keyed by form field names")
    answer = parsed
  }
  await client.session.form.reply({ sessionID, formID, answer }, { signal })
}
