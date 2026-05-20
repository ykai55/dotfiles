# OpenCode Serve

Portable Docker Compose setup for running `opencode serve` while using the host user's filesystem and configuration.

This Compose file intentionally avoids a local Docker build because some hosts have Docker Compose without the Buildx plugin installed.

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
docker compose up -d
```

To start in a specific project for one run:

```bash
OPENCODE_WORKDIR="$HOME/src/lllw" docker compose up -d
```

OpenCode will listen on `http://0.0.0.0:4096`.

## Operations

```bash
docker compose logs -f
docker compose restart
docker compose down
docker compose pull
```

## Notes

- The container installs `opencode-ai@1.15.5` and `@larksuite/cli@1.0.34` at startup, matching the currently installed host versions.
- Config changes under `~/.config/opencode`, skills, agents, or plugins still require restarting the container with `docker compose restart`.
- UID/GID use exported `UID` and `GID` when present. If your shell does not export them, run `UID=$(id -u) GID=$(id -g) docker compose up -d` or leave the default `1000:1000`.
