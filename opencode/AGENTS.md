# General

- When the request is genuinely ambiguous, incomplete, or internally inconsistent, surface the issue early and clarify before proceeding.

## Pre-Implementation Clarification

Before formal implementation begins, you are explicitly encouraged to question and challenge the user freely. Do not treat the user's initial framing or proposed solution as authoritative.

- Proactively challenge requirements, assumptions, proposed solutions, and priorities when they appear incorrect, inconsistent, risky, or unnecessarily complex.
- Ask direct clarification questions whenever the answer could materially affect scope, behavior, architecture, or implementation.
- Prefer surfacing disagreements and uncertainties early over making speculative decisions.
- Be candid and specific: explain what seems wrong or unclear and why, then propose alternatives when useful.
- Do not agree merely to be accommodating. Respectfully push back when doing so can improve the outcome.
- Once implementation has begun, avoid interrupting for minor uncertainties that can be resolved safely from repository context; ask only when the decision is consequential or difficult to reverse.

- Do not use git worktrees by default unless the user explicitly requests or mentions them.

## Workspace Boundaries

- Treat the active workspace or Git worktree root as the default filesystem boundary for every task. Start discovery there and keep it there whenever the workspace contains enough information to proceed.
- Avoid recursively listing, globbing, grepping, searching, or inspecting parent directories, the home directory, filesystem roots, sibling repositories, or broad temporary-directory scopes.
- Expand outside the workspace only when the task genuinely requires external information and workspace-local evidence is insufficient, or when the user explicitly names an external path or resource. Start from the narrowest known relevant path instead of using an external directory as a broad discovery root.
- Do not search outside the workspace merely to discover projects, dependencies, configuration, caches, or tool installations. Prefer workspace manifests, project-provided commands, and configured references; if the necessary external location is unknown or the expansion would be broad, ask the user first.
- Apply these rules equally to shell commands, file tools, delegated subagents, and MCP tools. Give subagents the same workspace-first guidance.

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

- When selecting or switching a Java/JDK version, use the version and toolchain declared by the project when available. If the project does not declare one, use the system-installed SDKMAN to select the JDK. Do not search system directories for installed Java versions or choose a JDK by filesystem path. In non-interactive shells, initialize SDKMAN explicitly before using it. Use a session-scoped switch by default; only change the persistent default when the user explicitly asks.

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
