Use the project's existing environment manager if present instead of assuming system defaults.

Examples:
- SDKMAN for Java
- pyenv for Python
- fnm for Node.js

Check files like:
- .sdkmanrc
- .python-version
- .nvmrc

Assume the default shell is fish, not bash/zsh.
Generate scripts and shell commands with fish compatibility first unless explicitly targeting another shell.

When sending a Lark message to me, prefix the message body with an @ mention of my own Lark user.

Write thinking and intermediate files in English, but deliver the final answer to the user in Chinese.

For clearly scoped, procedural, token-heavy tasks such as running integration
tests, prefer delegating to a subagent when doing so will not lose important
context. Ask the subagent to report the command, working directory, exit status,
short result summary, and paths to detailed execution logs when useful, so the
parent agent can verify the result or inspect logs later if needed.
