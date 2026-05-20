#!/usr/bin/env bash
set -euo pipefail

: "${HOST_UID:=1000}"
: "${HOST_GID:=1000}"
: "${HOST_USER:=${USER:-node}}"
: "${HOST_HOME:=${HOME:-/home/node}}"
: "${OPENCODE_WORKDIR:=${HOST_HOME}}"

if [ -z "${OPENCODE_SERVER_PASSWORD:-}" ]; then
  printf '%s\n' 'WARNING: OPENCODE_SERVER_PASSWORD is empty. Set it in .env before exposing this server beyond localhost.' >&2
fi

if ! getent group "${HOST_GID}" >/dev/null; then
  groupadd --gid "${HOST_GID}" "${HOST_USER}"
fi

if ! id -u "${HOST_UID}" >/dev/null 2>&1; then
  useradd --uid "${HOST_UID}" --gid "${HOST_GID}" --home-dir "${HOST_HOME}" --shell /bin/bash "${HOST_USER}"
fi

apt-get update
apt-get install -y --no-install-recommends fish openssh-client python3 ripgrep
rm -rf /var/lib/apt/lists/*

npm install -g opencode-ai@1.15.5 @larksuite/cli@1.0.34

cd "${OPENCODE_WORKDIR}"
exec runuser -u "$(id -nu "${HOST_UID}")" -- env \
  HOME="${HOST_HOME}" \
  USER="${HOST_USER}" \
  XDG_CONFIG_HOME="${HOST_HOME}/.config" \
  XDG_CACHE_HOME="${HOST_HOME}/.cache" \
  XDG_DATA_HOME="${HOST_HOME}/.local/share" \
  OPENCODE_SERVER_PASSWORD="${OPENCODE_SERVER_PASSWORD:-}" \
  OPENCODE_HOST="0.0.0.0" \
  OPENCODE_PORT="4096" \
  opencode serve --hostname 0.0.0.0
