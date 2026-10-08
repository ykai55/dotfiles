# OpenCode Serve

## OpenChamber on macOS

The complete OpenChamber preference snapshot and copy-based export/apply script
are maintained in [openchamber/](openchamber/README.md).

`openchamber.plist` runs the OpenChamber CLI/Web server in the background after
login. `dotfiles-apply` links it to
`~/Library/LaunchAgents/dev.openchamber.web.plist` on macOS and maintains the
`~/dotfiles` compatibility path. Each start runs `bin/useenv` to load the current
exported Fish environment, then executes
`fnm exec --using default openchamber serve --foreground`. Install Fish, fnm,
and the OpenChamber CLI in fnm's default Node version first. The explicit fnm
selection works without an interactive shell or a temporary terminal PATH.

If OpenChamber's built-in startup service is enabled, run
`openchamber startup disable` before applying this mapping. Stop any separately
running CLI server with `openchamber stop`. Manage this LaunchAgent through
dotfiles and `launchctl`; `openchamber startup enable` writes to the same plist
path and would overwrite the managed configuration.

Install the mappings, then load the agent for the current login session
(commands below use Bash/Zsh syntax):

```bash
bin/dotfiles-apply --apply
launchctl bootstrap "gui/$(id -u)" "$HOME/Library/LaunchAgents/dev.openchamber.web.plist"
```

`dotfiles-apply` manages the file link; `launchctl bootstrap` loads and starts the
service immediately. Subsequent logins start it automatically. This LaunchAgent
requires a logged-in user session.

After changing Fish environment settings, restart the service:

```bash
launchctl kickstart -k "gui/$(id -u)/dev.openchamber.web"
```

After editing the plist, unload and load it again:

```bash
launchctl bootout "gui/$(id -u)" "$HOME/Library/LaunchAgents/dev.openchamber.web.plist"
launchctl bootstrap "gui/$(id -u)" "$HOME/Library/LaunchAgents/dev.openchamber.web.plist"
```

Inspect status with `launchctl print "gui/$(id -u)/dev.openchamber.web"`.
Output is appended to `~/Library/Logs/OpenChamber/launchagent.log`.
To disable automatic startup, run
`launchctl disable "gui/$(id -u)/dev.openchamber.web"`, then the `bootout` command
above. To restore it, run `launchctl enable "gui/$(id -u)/dev.openchamber.web"`
before `bootstrap`.

## OpenCode V2 plugins

These plugins target OpenCode **2.0.16**, with matching `@opencode/plugin` and
`@opencode/client` dependencies. Each plugin is a directory with a `package.json`
entrypoint and a default `Plugin.define({ id, setup })` export.

Install dependencies, then apply the dotfiles mappings from the repository root:

```bash
cd ~/dotfiles/opencode
npm install --no-package-lock
cd ..
bin/dotfiles-apply --apply
```

The global `plugins/` directory discovers `current-time`, `rename-self`, and
`chat-notify`. The `plugins` entry in `opencode.json` loads `./provider-loader`.
OpenCode 2.0.16 requires directory paths for explicitly configured local plugins.
Configuration-directory changes reload automatically; use `opencode service
restart` when a changed shared dependency needs a full reload. Inspect loading
with `opencode plugin list`.

Some 2.0.16 operations (message history and forms) are available only through the
HTTP client. `lib/service-client.ts` discovers the authenticated managed service,
or uses the current `serve --hostname ... --port ...` listener. An embedded server
or a dynamically assigned port can supply a `serverURL` option on `rename-self`
and `chat-notify`. Explicit-listener authentication uses
`OPENCODE_SERVER_USERNAME` (default `opencode`) and `OPENCODE_SERVER_PASSWORD`.

### Current time reminders

`plugins/current-time/index.ts` tracks successful and failed tools through
`ctx.tool.hook("execute.after")`. The next `context` hook inserts a user-role time
reminder immediately after the matching tool-result message, preserving tool
call/result pairs. The reminder affects only that model request.

Each session is throttled independently. The first result is eligible immediately;
later reminders require more than one minute since the previous insertion. The
latest completed tool supersedes an unconsumed reminder. Session deletion clears
the state, and unloading aborts the event subscription. Display and throttling
both use the system wall clock.

### Session title maintenance

`plugins/rename-self/index.ts` adds a stable emoji to a root session's first native
title, exposes `rename_session`, and adds a request-local reminder on every third
real user turn. OpenCode's native title agent continues to generate initial titles.

- Initial naming starts from an observed untitled session. A `session.renamed`
  event triggers prefix allocation; existing custom titles are not backfilled.
  A final read avoids overwriting an intervening edit. Native title text is
  preserved, including text longer than the later 30-character tool limit.
- Fork naming uses `session.forked` and the structured `Session.Info.fork` field.
  It replaces an inherited emoji while preserving the title body.
- Reminders use delivered user-message history with pagination, including turns
  before compaction. Message IDs deduplicate the count; file-only input counts,
  while empty, synthetic, and assistant messages do not. The `context` hook appends
  the reminder to the matching user message only in the outgoing request. Tool
  continuations retain the reminder until a successful rename dismisses that turn.
- `rename_session({ name, reason?, emoji? })` accepts a plain title. `reason` is
  `refinement`, `topic_shift`, or `manual`. Initial names, `topic_shift`, and
  `emoji=true` receive a prefix; existing prefixes are preserved. The final title
  must fit 30 grapheme clusters, including the emoji and space. Overlong input is
  rejected rather than truncated, and identical titles do not produce writes.
- Prefix allocation excludes emojis from the 50 most recently updated other root
  sessions in the same directory. The 96-symbol pool falls back to the
  least-recently-used emoji. Writes are serialized within each plugin instance.
- Child sessions do not receive automatic naming or periodic reminders. Related
  topics should be summarized together; sustained topic shifts should be blended
  after 2-3 real user turns. These semantic decisions remain the agent's responsibility.

### Chat notifications

`plugins/chat-notify/index.ts` reads the ignored `plugins/chat-notify.conf`; the
example file documents connection settings. Existing Lark and Telegram SQLite
state files retain their paths and tables.

The dispatcher consumes V2 `session.*`, `permission.*`, and `form.*` events. It
filters notifications to root sessions in the plugin's location. Channel replies
submit V2 prompts with `delivery: "steer"`; permission buttons submit the selected
decision. Questions use session forms: single-choice replies accept option labels
or values, multi-select accepts a JSON array, and multi-field forms accept a JSON
object keyed by field name. The notification includes these field names.

Unload cancels event subscriptions, polls, delayed permission notifications and
stream updates, releases poll locks, and closes SQLite connections. Lark WebSocket
connections are shared by app ID and closed when their last plugin instance unloads.

### Verification

The project-local Bun development dependency runs tests, including `bun:sqlite`.

```bash
cd ~/dotfiles/opencode
npm test
npm run typecheck
npm run test:smoke
npm run test:smoke -- --explicit
```

Unit tests use the released HTTP client with a fake transport and mock channel
requests. Smoke tests start an isolated OpenCode 2.0.16 service and a local fake
model, verify all four plugins load, exercise three user turns and a rename tool
call, check both reminders, and reload the plugins. The explicit mode also tests
an authenticated `serve` listener. Notification channels are disabled in smoke
tests; their outbound APIs are covered by the simulated channel tests.

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
