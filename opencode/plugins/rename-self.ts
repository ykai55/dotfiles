import type { Plugin } from "@opencode-ai/plugin"
import { tool } from "@opencode-ai/plugin"

export default (async ({ client }) => {
  return {
    tool: {
      rename_session: tool({
        description:
          "Rename the current opencode session (chat). Use when asked to name, rename, or retitle this session.",
        args: {
          name: tool.schema.string().describe("New session title"),
        },
        async execute(args, context) {
          const result = await client.session.update({
            path: { id: context.sessionID },
            body: { title: args.name },
          })
          if (result.error) {
            throw new Error(`rename_session failed: ${JSON.stringify(result.error)}`)
          }
          return `Renamed session ${context.sessionID} to "${result.data?.title ?? args.name}"`
        },
      }),
    },
  }
}) satisfies Plugin
