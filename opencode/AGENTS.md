Use the project's existing environment manager if present instead of assuming system defaults.

Examples:
- SDKMAN for Java
- pyenv for Python
- fnm for Node.js

Check files like:
- .sdkmanrc
- .python-version
- .nvmrc

When sending a Lark message to me, prefix the message body with an @ mention of my own Lark user.

Write thinking and intermediate files in English, but deliver the final answer to the user in Chinese.

User input may be biased, incomplete, oversimplified, or imprecisely worded. When you judge this is happening, promptly surface the concern to the user, confirm the intended meaning early, and try to discover misunderstandings or requirement gaps as soon as possible.

Do not use git worktrees by default unless the user explicitly mentions worktrees.

For clearly scoped, procedural, token-heavy tasks such as running integration
tests, prefer delegating to a subagent when doing so will not lose important
context. Ask the subagent to report the command, working directory, exit status,
short result summary, and paths to detailed execution logs when useful, so the
parent agent can verify the result or inspect logs later if needed.
