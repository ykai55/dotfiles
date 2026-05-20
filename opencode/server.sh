#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

if [ ! -f .env ]; then
  cp .env.example .env
  printf '%s\n' 'Created .env from .env.example. OPENCODE_SERVER_PASSWORD is empty by default.' >&2
fi

if ! command -v docker >/dev/null 2>&1; then
  printf '%s\n' 'ERROR: docker command not found.' >&2
  exit 1
fi

printf '%s\n' 'Building OpenCode image with latest opencode-ai...'
docker compose build --pull --no-cache opencode-server

printf '%s\n' 'Starting OpenCode server...'
docker compose up -d opencode-server

printf '%s\n' 'OpenCode server is starting on http://0.0.0.0:4096'
