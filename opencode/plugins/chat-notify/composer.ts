export type SessionNotice = {
  sessionID: string
  directory: string
  userInput: string
  sessionTitle?: string
  contextTokens?: number
  contextLimit?: number
}

export type DoneNotice = SessionNotice & {
  output: string
  tools: number
  read: number
  written: number
  changed: number
}

export type PermissionNotice = {
  sessionID: string
  requestID: string
  permission: string
  patterns: string
}

export type QuestionNotice = {
  sessionID: string
  requestID: string
}

export type CompactionNotice = {
  sessionID: string
  beforeTokens?: number
  beforeLimit?: number
  afterTokens?: number
  afterLimit?: number
}

type SessionStats = {
  muted: boolean
  sessionTitle?: string
  userInput: string
  userPartIDs: Set<string>
  ignoredPartIDs: Set<string>
  pendingTextByPart: Map<string, string>
  textByPart: Map<string, string>
  toolCalls: Set<string>
  readFiles: Set<string>
  writtenFiles: Set<string>
  changedFiles: Set<string>
  contextTokens?: number
  contextLimit?: number
  compactionBeforeTokens?: number
  compactionBeforeLimit?: number
}

export const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object"

export const prop = (value: unknown, key: string) => {
  if (!record(value)) return
  return value[key]
}

export const textOption = (value: unknown, fallback?: string) => {
  if (typeof value !== "string") return fallback
  const trimmed = value.trim()
  if (!trimmed) return fallback
  return trimmed
}

export const stringList = (value: unknown) => {
  if (!Array.isArray(value)) return []
  return value.filter((item): item is string => typeof item === "string" && item.trim().length > 0)
}

export const patterns = (value: unknown) => stringList(value).join(", ")

export const tokenCount = (tokens: unknown) => {
  if (!record(tokens)) return
  const cache = prop(tokens, "cache")
  return (
    (numberValue(prop(tokens, "input")) ?? 0) +
    (numberValue(prop(tokens, "output")) ?? 0) +
    (numberValue(prop(tokens, "reasoning")) ?? 0) +
    (numberValue(prop(cache, "read")) ?? 0) +
    (numberValue(prop(cache, "write")) ?? 0)
  )
}

export const contextLimitFrom = (value: unknown) =>
  numberValue(prop(prop(value, "limit"), "context")) ?? numberValue(prop(value, "context")) ?? numberValue(prop(value, "contextLimit"))

const numberValue = (value: unknown) => {
  if (typeof value === "number" && Number.isFinite(value)) return value
  if (typeof value !== "string") return
  const parsed = Number(value)
  if (Number.isFinite(parsed)) return parsed
}

const toolInput = (part: unknown) => prop(prop(part, "state"), "input")

const inputPaths = (input: unknown) =>
  [
    prop(input, "filePath"),
    prop(input, "filepath"),
    prop(input, "path"),
    prop(input, "cwd"),
    ...stringList(prop(input, "files")),
    ...stringList(prop(input, "paths")),
  ].filter((item): item is string => typeof item === "string" && item.trim().length > 0)

const patchPaths = (input: unknown) => {
  const patch = prop(input, "patch")
  if (typeof patch !== "string") return []
  return Array.from(patch.matchAll(/^\*\*\* (?:Add File|Update File|Delete File): (.+)$/gm), (match) => match[1])
}

const syntheticText = (value: string) => {
  const text = value.trimStart()
  return (
    /^Called the .+ tool with the following input:/.test(text) ||
    text.startsWith("<path>") ||
    text.startsWith("<type>") ||
    text.startsWith("<content>") ||
    text.startsWith("<entries>") ||
    text.startsWith("Read tool failed to read") ||
    text.startsWith("Referenced configured reference ")
  )
}

const truncate = (value: string, limit: number) => {
  if (value.length <= limit) return value
  return `[truncated first ${value.length - limit} chars]\n\n${value.slice(-limit).trimStart()}`
}

export class NotificationComposer {
  private activeSessions = new Set<string>()
  private statsBySession = new Map<string, SessionStats>()

  constructor(
    private options: {
      directory: string
      maxOutputChars: number
    },
  ) {}

  session(sessionID: string): SessionNotice {
    const current = this.stats(sessionID)
    return {
      sessionID,
      directory: this.options.directory,
      userInput: current.userInput,
      sessionTitle: current.sessionTitle,
      contextTokens: current.contextTokens,
      contextLimit: current.contextLimit,
    }
  }

  isMuted(sessionID: string) {
    return this.stats(sessionID).muted
  }

  mute(sessionID: string) {
    this.stats(sessionID).muted = true
  }

  chatMessage(sessionID: string, parts: unknown, contextLimit?: number) {
    const current = this.stats(sessionID)
    if (contextLimit !== undefined) current.contextLimit = contextLimit
    current.userInput = this.trackUserParts(current, parts)
    return this.isMuted(sessionID) ? undefined : this.session(sessionID)
  }

  sessionInfo(sessionID: string, info: unknown, contextLimit?: number) {
    if (typeof prop(info, "parentID") === "string") {
      this.mute(sessionID)
      return
    }
    this.trackContext(sessionID, info)
    if (contextLimit !== undefined) this.stats(sessionID).contextLimit = contextLimit
    const title = textOption(prop(info, "title"))
    if (title) this.stats(sessionID).sessionTitle = title
    return title
  }

  stepStarted(sessionID: string, contextLimit?: number) {
    if (contextLimit !== undefined) this.stats(sessionID).contextLimit = contextLimit
  }

  stepEnded(sessionID: string, properties: unknown) {
    this.trackContext(sessionID, properties)
  }

  compactionStarted(sessionID: string) {
    const current = this.stats(sessionID)
    current.compactionBeforeTokens = current.contextTokens
    current.compactionBeforeLimit = current.contextLimit
  }

  compactionEnded(sessionID: string, afterTokens?: number): CompactionNotice | undefined {
    if (this.isMuted(sessionID)) return
    const current = this.stats(sessionID)
    current.contextTokens = afterTokens
    return {
      sessionID,
      beforeTokens: current.compactionBeforeTokens,
      beforeLimit: current.compactionBeforeLimit,
      afterTokens,
      afterLimit: current.compactionBeforeLimit,
    }
  }

  partDelta(sessionID: string, partID: string, delta: string) {
    const current = this.stats(sessionID)
    if (current.userPartIDs.has(partID) || current.ignoredPartIDs.has(partID)) return
    const next = `${current.pendingTextByPart.get(partID) ?? current.textByPart.get(partID) ?? ""}${delta}`
    if (syntheticText(next)) {
      this.ignorePart(current, partID)
      return
    }
    current.pendingTextByPart.set(partID, next)
  }

  partUpdated(sessionID: string, part: unknown) {
    this.trackPart(sessionID, part)
  }

  status(sessionID: string, statusType: unknown) {
    if (statusType === "busy" || statusType === "retry") {
      this.activeSessions.add(sessionID)
      return
    }
    if (statusType !== "idle" || !this.activeSessions.delete(sessionID)) return
    if (this.isMuted(sessionID)) {
      this.clearRound(sessionID)
      return
    }
    const done = this.done(sessionID)
    this.clearRound(sessionID)
    return done
  }

  private done(sessionID: string): DoneNotice {
    const current = this.stats(sessionID)
    return {
      ...this.session(sessionID),
      output: truncate(
        Array.from(current.textByPart.values())
          .filter((text) => !syntheticText(text))
          .join("\n\n")
          .trim() || "(no text output)",
        this.options.maxOutputChars,
      ),
      tools: current.toolCalls.size,
      read: current.readFiles.size,
      written: current.writtenFiles.size,
      changed: current.changedFiles.size,
    }
  }

  private stats(sessionID: string) {
    const existing = this.statsBySession.get(sessionID)
    if (existing) return existing
    const next: SessionStats = {
      muted: false,
      userInput: "",
      userPartIDs: new Set<string>(),
      ignoredPartIDs: new Set<string>(),
      pendingTextByPart: new Map<string, string>(),
      textByPart: new Map<string, string>(),
      toolCalls: new Set<string>(),
      readFiles: new Set<string>(),
      writtenFiles: new Set<string>(),
      changedFiles: new Set<string>(),
    }
    this.statsBySession.set(sessionID, next)
    return next
  }

  private clearRound(sessionID: string) {
    const current = this.statsBySession.get(sessionID)
    if (!current) return
    current.userInput = ""
    current.userPartIDs.clear()
    current.ignoredPartIDs.clear()
    current.pendingTextByPart.clear()
    current.textByPart.clear()
    current.toolCalls.clear()
    current.readFiles.clear()
    current.writtenFiles.clear()
    current.changedFiles.clear()
    current.contextTokens = undefined
    current.contextLimit = undefined
    current.compactionBeforeTokens = undefined
    current.compactionBeforeLimit = undefined
  }

  private trackUserParts(current: SessionStats, parts: unknown) {
    if (!Array.isArray(parts)) return ""
    for (const part of parts) {
      const id = prop(part, "id")
      if (typeof id === "string") current.userPartIDs.add(id)
    }
    return parts
      .map((part) => prop(part, "text"))
      .filter((text): text is string => typeof text === "string" && text.trim().length > 0)
      .join("\n\n")
      .trim()
  }

  private trackContext(sessionID: string, source: unknown) {
    const current = this.stats(sessionID)
    const tokens = tokenCount(prop(source, "tokens") ?? source)
    if (tokens !== undefined && tokens > 0) current.contextTokens = tokens
    current.contextLimit = contextLimitFrom(source) ?? current.contextLimit
  }

  private trackPart(sessionID: string, part: unknown) {
    const partType = prop(part, "type")
    const partID = prop(part, "id")
    if (typeof partID !== "string") return
    const current = this.stats(sessionID)
    if (partType === "reasoning" || prop(part, "synthetic") === true || prop(part, "ignored") === true) {
      this.ignorePart(current, partID)
      return
    }
    if (partType === "text") {
      if (current.userPartIDs.has(partID) || current.ignoredPartIDs.has(partID)) return
      const text = prop(part, "text")
      if (typeof text !== "string") return
      if (syntheticText(text)) {
        this.ignorePart(current, partID)
        return
      }
      current.pendingTextByPart.delete(partID)
      current.textByPart.set(partID, text)
      return
    }
    if (partType === "tool") {
      this.trackTool(sessionID, part)
      return
    }
    if (partType === "step-finish") {
      this.trackContext(sessionID, part)
      return
    }
    if (partType === "patch") {
      for (const file of stringList(prop(part, "files"))) {
        current.writtenFiles.add(file)
        current.changedFiles.add(file)
      }
    }
  }

  private trackTool(sessionID: string, part: unknown) {
    const tool = prop(part, "tool")
    const callID = prop(part, "callID")
    if (typeof tool !== "string" || typeof callID !== "string") return
    const current = this.stats(sessionID)
    if (current.toolCalls.has(callID)) return
    current.toolCalls.add(callID)
    current.pendingTextByPart.clear()
    current.textByPart.clear()

    const paths = [...inputPaths(toolInput(part)), ...patchPaths(toolInput(part))]
    if (["read", "grep", "glob", "lsp"].includes(tool)) {
      for (const file of paths) current.readFiles.add(file)
      return
    }
    if (["write", "edit", "apply_patch"].includes(tool)) {
      for (const file of paths) {
        current.writtenFiles.add(file)
        current.changedFiles.add(file)
      }
    }
  }

  private ignorePart(current: SessionStats, partID: string) {
    current.ignoredPartIDs.add(partID)
    current.pendingTextByPart.delete(partID)
    current.textByPart.delete(partID)
  }
}
