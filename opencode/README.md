# OpenCode Serve

## Session title maintenance

`plugins/rename-self.ts` adds one stable emoji after OpenCode generates a root
session's first non-placeholder title, provides `rename_session`, and adds a small
request-local reminder on every third real user message. It does not replace the
native title agent, start another agent, or generate an extra model request. The
agent decides whether later titles need changing: summarize related discussions
under a common category, and only blend a new topic after it has persisted for
2-3 user turns. No separate skill is needed.

- Initial emoji assignment is event-driven. The plugin must first observe the
  exact `New session - <ISO timestamp>` placeholder, so existing/custom sessions
  are not backfilled. When that root session receives its first native title, the
  plugin preserves the title text verbatim and only adds a selected prefix. The
  native title may therefore temporarily exceed the later 30-character rule.
  Child sessions and titles that already start with an emoji are left unchanged.
  The plugin rereads the title before writing so a queued event cannot overwrite a
  newer rename; failures log a warning and can retry once when the session idles.
- Fork sessions are detected on `session.created` through OpenCode's stable
  `<source title> (fork #N)` format. The plugin removes any inherited leading
  emoji, allocates a fresh one, and preserves the title body and fork number.
  Only creation events are eligible, so later manual titles that resemble a fork
  are not rewritten. A final session read prevents stale events from overwriting
  a newer title; malformed suffixes and child sessions are ignored.
- The reminder is appended as a synthetic text part on the triggering user
  message through `experimental.chat.messages.transform`; it is not a persisted
  chat message and does not change the system-prompt prefix. This follows
  OpenCode's own request-local reminder pattern and keeps prior prompt-cache
  prefixes stable. During a check turn it appears once per model request
  (including tool continuations and retries), until a rename succeeds, the
  session becomes idle, or a new real user turn changes eligibility. The hook
  infers the target session from the message and injects only when that message
  ID matches the third-turn marker.
- User message IDs deduplicate turn counting. Synthetic/ignored text, tool steps,
  compaction markers and empty messages do not count; file-only input does.
  Counting is restored from stored history on first use after a restart, without
  writing a sidecar state file. Child sessions do not receive periodic reminders.
- `rename_session({ name, reason?, emoji? })` accepts a plain title. `reason` is
  `refinement` (default), `topic_shift`, or `manual`; `emoji` defaults to false.
  `topic_shift` always adds a plugin-selected prefix. Existing emoji prefixes are
  retained on every rename, including manual renames.
- First/explicit assignment randomly excludes prefixes in the 50 most recently
  updated other root sessions in the current project directory. The 96-symbol pool
  falls back to its least-recently-used symbol when exhausted. Writes are
  serialized within one plugin instance; separate OpenCode processes may still
  race.
- The final title is limited to 30 Unicode grapheme clusters, including emoji,
  space and punctuation (28 for the plain title with a prefix). Overlong input is
  rejected for the agent to shorten, never silently truncated. Identical titles
  do not produce writes. Read failures skip reminders with a warning; rename
  failures return an error.

Requires the messages-transform hook in the declared `@opencode-ai/plugin@1.2.27`
API and a compatible OpenCode runtime. Restart OpenCode to reload the plugin.
The semantics and 2-3-turn topic stability remain agent decisions, not a keyword
classifier or a guarantee of renaming. Existing `rename_session({ name })` calls
remain supported for plain titles.

Development checks (Node.js 22.18+ or 24+, independent of the Bun plugin runtime):

```bash
cd ~/dotfiles/opencode
npm install --ignore-scripts --no-package-lock
npm run test:rename-self
npm run typecheck:rename-self
# Run one test by name:
node --experimental-default-type=module --test --test-name-pattern="topic shifts" tests/rename-self.test.mjs
```

Tests exercise exported hooks and the real SDK client with a fake HTTP transport;
no real sessions are renamed and no provider credentials are used.

For standalone provider configuration generation from an OpenAI-compatible API,
see [OpenCode provider generator](provider-gen.md).

Portable Docker Compose setup for running `opencode serve` while using the host user's filesystem and configuration.

The image is built locally so Debian packages and npm packages are cached in the image instead of being installed on every container start.

## Host Bind Mounts

- `${HOME}:${HOME}` keeps the same workspace, OpenCode config, skills, Lark config, git config, SSH keys, and caches visible in the container.
- `/tmp/opencode:/tmp/opencode` keeps OpenCode temp files on the host.
- `/var/run/docker.sock:/var/run/docker.sock` lets commands inside OpenCode talk to the host Docker daemon when needed.

## First Run

```bash
cd ~/dotfiles/opencode
cp .env.example .env
```

`OPENCODE_SERVER_PASSWORD` is empty by default. Set it in `.env` before exposing OpenCode beyond localhost; the entrypoint prints a warning when it is empty. `HOME` and `USER` are read from the shell environment that runs `docker compose`.

Set `OPENCODE_WORKDIR` only if you want a different start directory. It defaults to `$HOME`, and can be any host path under `$HOME` because the whole home directory is bind-mounted.

```bash
./server.sh
```

`server.sh` builds the image with `--pull --no-cache` before starting it, so every script run installs the latest `opencode-ai` package during the image build.

To start in a specific project for one run:

```bash
OPENCODE_WORKDIR="$HOME/src/lllw" docker compose up -d
```

OpenCode will listen on `http://0.0.0.0:4096`.

The Compose service is named `opencode-server`, and the local image is tagged `local/opencode-server:1.15.5`.

## Operations

```bash
docker compose logs -f
docker compose restart
docker compose down
docker compose pull
docker compose build --pull
```

## Notes

- The image installs `opencode-ai@latest` on every no-cache build and `@larksuite/cli@1.0.34`.
- `DEBIAN_MIRROR=auto` benchmarks several Debian mirrors at build time. Set `DEBIAN_MIRROR=https://.../debian` in `.env` to force a mirror.
- Config changes under `~/.config/opencode`, skills, agents, or plugins still require restarting the container with `docker compose restart`.
- UID/GID use exported `UID` and `GID` when present. If your shell does not export them, run `UID=$(id -u) GID=$(id -g) docker compose up -d` or leave the default `1000:1000`.
