---
name: rename-self
description: Rename the current opencode session/chat. Use when the user asks to name, rename, or retitle this session, chat, or conversation, or when a workflow requires setting the session title.
---

# Rename the current opencode session

Call the `rename_session` tool with the new title. It is provided by the `rename-self` plugin and renames the exact current session through the SDK (`sessionID` from tool context). The rename is done only when the title it returns equals the requested name.

If the user supplied the exact title, pass `reason: "manual"`. This records the title as user-owned, so later automatic renames stop. If the user asked you to choose or derive a title, pass `reason: "requested"`; the resulting title remains model-managed. Both reasons may replace a user-owned title because they represent explicit user authorization, but `requested` removes that ownership after the rename. Workflows that temporarily decorate and later restore a title pass `reason: "temporary"`, which preserves its existing ownership.

If the tool is missing, the `rename-self` plugin is not loaded — opencode has not been restarted since the plugin was added, or plugins are disabled. Report that; do not try to identify the session another way.

## Choosing the name

Derive a short, specific title from the current work (a few words, no trailing punctuation). A caller may impose a prefix, e.g. task-watch uses `Watch: `.

## Boundaries

- Rename only the current session; never another session or a subagent.
