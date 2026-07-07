# General

- When the request is genuinely ambiguous, incomplete, or internally inconsistent, surface the issue early and clarify before proceeding.

- Do not use git worktrees by default unless the user explicitly requests or mentions them.

# Code Style

## Minimal Abstraction Rule

Prefer direct, local code over introducing new abstractions. Every helper, wrapper, constant, or utility should justify its existence by improving clarity, expressing a stable domain concept, or eliminating meaningful duplication.

Before introducing an abstraction, ask:

- Does it represent a business or domain concept rather than merely compressing syntax?
- Does its name communicate intent more clearly than the implementation?
- Is the logic reused enough that maintaining it in one place is beneficial?
- Would inlining make the surrounding code easier to understand?
- Does this abstraction hide important local behavior that readers should see?

If the answer to most of these questions is **no**, keep the code inline.

Prefer:

- Direct code over one-line wrappers.
- Local literals unless a value represents a shared contract or domain concept.
- Small, obvious duplication over premature abstraction.
- Tests that express behavior directly rather than hiding setup behind helpers.

Exceptions:

Introduce an abstraction when it materially improves readability by expressing business or domain intent, even if it is used only once.

Examples include:

- `isExpired()`
- `hasPermission()`
- `RetryPolicy`
- `CacheKey`

These names communicate concepts that are more meaningful than their underlying implementation.

During code review, treat an abstraction as a readability regression when it:

- Only wraps one or two lines without adding semantic value.
- Exists solely to reduce trivial duplication.
- Hides important local behavior that readers should see.
- Forces readers to jump elsewhere to understand straightforward logic.

Favor removing abstractions that do not improve reuse, readability, or domain clarity.

# Development Environment

- Respect the project's existing development environment instead of assuming system defaults.

- Before running language-specific tools, check whether the project specifies an environment manager or version file, for example:
  - `.sdkmanrc` (SDKMAN)
  - `.python-version` (pyenv)
  - `.nvmrc` (fnm / nvm)
  - or other project-specific configuration files.

- Use the project's configured toolchain whenever possible rather than the system-wide installation.

- Prefer project-provided wrapper commands (e.g. make, just, task, mise, pnpm, uv, project scripts) over invoking language tools directly.

# Communication

- For Feishu/Lark related tasks, use `larkcli` by default instead of `bytedcli` unless the user explicitly requests `bytedcli` or `larkcli` cannot satisfy the task.

- When sending a Lark message on the user's behalf, always prefix the message body with an `@` mention of the user's own Lark account.

# Workflow

- For clearly scoped, procedural, or token-heavy tasks (for example, running integration tests, executing large test suites, or lengthy build commands), prefer delegating the work to a subagent when doing so will not lose important context.

- Ask the subagent to report:
  - Working directory
  - Commands executed
  - Exit status
  - Short result summary
  - Paths to detailed logs, when applicable

- Verify the reported results before continuing whenever appropriate.

# Language

- Write reasoning, scratch notes, and intermediate artifacts in English.

- Deliver all user-facing responses in Chinese unless the user explicitly requests another language.

<!-- codebase-memory-mcp:start -->
# Codebase Knowledge Graph (codebase-memory-mcp)

This project uses codebase-memory-mcp to maintain a knowledge graph of the codebase.
ALWAYS prefer MCP graph tools over grep/glob/file-search for code discovery.

## Priority Order
1. `search_graph` — find functions, classes, routes, variables by pattern
2. `trace_path` — trace who calls a function or what it calls
3. `get_code_snippet` — read specific function/class source code
4. `query_graph` — run Cypher queries for complex patterns
5. `get_architecture` — high-level project summary

## When to fall back to grep/glob
- Searching for string literals, error messages, config values
- Searching non-code files (Dockerfiles, shell scripts, configs)
- When MCP tools return insufficient results

## Examples
- Find a handler: `search_graph(name_pattern=".*OrderHandler.*")`
- Who calls it: `trace_path(function_name="OrderHandler", direction="inbound")`
- Read source: `get_code_snippet(qualified_name="pkg/orders.OrderHandler")`
<!-- codebase-memory-mcp:end -->
