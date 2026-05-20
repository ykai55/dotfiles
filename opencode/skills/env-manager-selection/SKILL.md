---
name: env-manager-selection
description: Use when selecting or diagnosing language/runtime versions, environment managers, or build/compile failures that may be caused by the wrong Java/JDK, Node.js, Python, Ruby, or Go version. Trigger for SDKMAN, sdk env, sdk use, .sdkmanrc, .nvmrc, .python-version, fnm, pyenv, jenv, asdf, version mismatch, unsupported class file, and phrases like "切 Java" or "环境不对".
---

# Environment Manager Selection

## Core Rule

Prefer the project's declared environment manager over system defaults or personal guesses. This applies both when the user asks directly and when a command, build, compile, test, or install step reveals a likely runtime/version mismatch. The user's explicit instruction wins; if they say not to inspect the environment, answer only from the provided context.

## Quick Mapping

| Runtime | Preferred project manager cue | Command cue |
| --- | --- | --- |
| Java/JDK | `.sdkmanrc`, SDKMAN mentioned | `sdk env`, `sdk use java <version>` |
| Node.js | `.nvmrc`, fnm mentioned | `fnm use`, `nvm use` only if project says nvm |
| Python | `.python-version`, pyenv mentioned | `pyenv local`, `pyenv shell` |
| Ruby | `.ruby-version`, rbenv mentioned | `rbenv local`, `rbenv shell` |
| Generic | `.tool-versions`, asdf mentioned | `asdf install`, `asdf shell` |

## Decision Rules

1. If the user provides enough context and says not to read files or environment, do not call tools. Answer from context.
2. If the context says "SDKMAN for Java", then Java switching uses SDKMAN: `sdk env` when `.sdkmanrc` exists, or `sdk use java <version>` when a version is specified.
3. If a build/compile/test/install command fails with signs of the wrong runtime version, pause before changing code or build config. Check the project manager files and activate the declared environment, then retry.
4. If inspection is allowed, check project manager files before suggesting system commands: `.sdkmanrc`, `.nvmrc`, `.python-version`, `.ruby-version`, `.tool-versions`.
5. Do not suggest `jenv`, `asdf`, `/usr/libexec/java_home`, system package managers, or shell profile edits for Java unless the project or user explicitly points there.
6. When uncertain, state the uncertainty and ask one short question instead of inventing a manager.

## Common Mistakes

| Mistake | Better behavior |
| --- | --- |
| Reading the environment after the user says not to | Answer only from supplied context |
| Suggesting `jenv` because the task mentions Java | Prefer SDKMAN when project instructions say SDKMAN for Java |
| Fixing build files before checking runtime mismatch | Activate the project environment first, then retry |
| Treating compile failure as unrelated to this skill | Trigger this skill when version/runtime mismatch is plausible |
| Treating examples as optional trivia | Use examples as the default mapping unless project evidence contradicts them |
| Listing every possible manager | Give the selected tool first, with alternatives only if relevant |

## Example

User: "基于你的输入，不要读环境，判断现在要切换java的话要用什么工具"

Answer: "用 SDKMAN。基于当前上下文，Java 对应 SDKMAN；如果有 `.sdkmanrc` 通常用 `sdk env`，指定版本则用 `sdk use java <version>`。"
