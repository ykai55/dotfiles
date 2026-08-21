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

## Clean Final-State Revisions

When revising code, documentation, or user-facing content after feedback, make the result read as though the corrected requirement had been known from the start. Do not preserve the rejected path merely to demonstrate that the correction was applied.

- Rewrite the affected content coherently instead of appending negations, disclaimers, or phrases such as "but not A" to the original approach.
- State the intended behavior directly and positively. Remove obsolete assumptions, branches, comments, names, tests, and examples that exist only because of the superseded direction.
- Judge each affected section by its reader task: does the reader need to understand change over time? Do not classify the entire document.
- Artifact types are signals, not verdicts. Implementations, API references, user guides, and current design descriptions often present current state; changelogs, migration guides, ADRs, deprecation sections, and compatibility contracts often require temporal context.
- If time is irrelevant to the task, describe only the resulting system and omit superseded mechanisms.
- If time matters, retain only the minimum history needed to complete a migration, understand a decision, or verify a compatibility boundary.
- Do not preserve history merely because feedback mentions it, and do not erase necessary history merely to sound positive.
- Still-supported deprecated behavior is part of the current contract and must be documented, including in API references or user guides.
- After revising, reread the affected area as a standalone final artifact and remove any residue of the drafting or correction process.

### Paired Example: One API Change

**Current API reference**

`contact.channels.email` contains the notification email address inside the `contact.channels` object.

**Bad current-state revision**

Do not use `email_address`; use `contact.channels.email` instead.

**Migration guide**

In v1, requests used top-level `email_address`. For v2, map `email_address` to `contact.channels.email`; v2 removes the top-level `email_address` field.

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

- For Java, use the version and toolchain declared by the project when available. If the project does not declare one, use SDKMAN to select the JDK. Respect an existing `SDKMAN_DIR`; only default it to `$HOME/.sdkman` when unset. Do not search system directories for installed Java versions or choose a JDK by filesystem path.

- In non-interactive shells, source `$SDKMAN_DIR/bin/sdkman-init.sh` before using SDKMAN.

- Use a session-scoped Java switch by default. Never install a missing runtime or change SDKMAN's persistent default unless the user explicitly requests it. If a project-declared runtime is unavailable, report it and ask before installation.

- Prefer project-provided wrapper commands (e.g. make, just, task, mise, pnpm, uv, project scripts) over invoking language tools directly.

# Communication

- For Feishu/Lark related tasks, use `larkcli` by default instead of `bytedcli` unless the user explicitly requests `bytedcli` or `larkcli` cannot satisfy the task.

- When sending a Lark message on the user's behalf, always prefix the message body with an `@` mention of the user's own Lark account.

# Workflow

- Delegate clearly scoped procedural work only when it is long-running, parallelizable, or token-heavy enough to justify the handoff overhead. Do not delegate when the handoff would lose important context.

- Keep a short single-command task in the primary agent.

- For delegated command execution, require:
  - Working directory
  - Exact commands
  - Exit status
  - Short result summary
  - Paths to detailed logs, when applicable

- Before relying on delegated results, independently verify at least one material log, artifact, or reported state when feasible.

# Language

- Write reasoning, scratch notes, and intermediate artifacts in English.

- Deliver all user-facing responses in Chinese unless the user explicitly requests another language.
