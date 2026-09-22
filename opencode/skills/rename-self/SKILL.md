---
name: rename-self
description: Rename the current opencode session/chat. Use when the user asks to name, rename, or retitle this session, chat, or conversation, or when a workflow requires setting the session title.
---

# Rename the current opencode session

Call the `rename_session` tool with the new title. It is provided by the `rename-self` plugin and renames the exact current session through the SDK (`sessionID` from tool context). The rename is done only when the title it returns equals the requested name.

If the tool is missing, the `rename-self` plugin is not loaded — opencode has not been restarted since the plugin was added, or plugins are disabled. Report that; do not try to identify the session another way.

## Choosing the name

Derive a short, specific title from the current work (a few words, no trailing punctuation). A caller may impose a prefix, e.g. task-watch uses `Watch: `.

## Boundaries

- Rename only the current session; never another session or a subagent.
