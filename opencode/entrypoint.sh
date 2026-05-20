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

choose_debian_mirror() {
  local codename current best mirror elapsed start
  codename="$(. /etc/os-release && printf '%s' "${VERSION_CODENAME}")"
  current="$(awk '/^deb / && $2 ~ /^https?:\/\// { print $2; exit }' /etc/apt/sources.list /etc/apt/sources.list.d/*.list 2>/dev/null || true)"

  best=""
  for mirror in \
    "http://deb.debian.org/debian" \
    "http://mirrors.tuna.tsinghua.edu.cn/debian" \
    "http://mirrors.ustc.edu.cn/debian" \
    "http://mirrors.aliyun.com/debian" \
    "http://mirrors.cloud.tencent.com/debian" \
    "http://mirror.sjtu.edu.cn/debian"; do
    start="$(date +%s%3N)"
    if curl -fsSL --connect-timeout 2 --max-time 4 "${mirror}/dists/${codename}/Release" >/dev/null; then
      elapsed="$(( $(date +%s%3N) - start ))"
      printf 'Debian mirror candidate: %s (%sms)\n' "${mirror}" "${elapsed}" >&2
      if [ -z "${best}" ] || [ "${elapsed}" -lt "${best%% *}" ]; then
        best="${elapsed} ${mirror}"
      fi
    fi
  done

  if [ -z "${best}" ]; then
    printf 'WARNING: no Debian mirror candidate responded; keeping existing apt sources.\n' >&2
    return 0
  fi

  mirror="${best#* }"
  if [ "${mirror}" = "${current}" ]; then
    printf 'Using existing fastest Debian mirror: %s\n' "${mirror}" >&2
    return 0
  fi

  printf 'Using fastest Debian mirror: %s\n' "${mirror}" >&2
  if [ -f /etc/apt/sources.list.d/debian.sources ]; then
    cat >/etc/apt/sources.list.d/debian.sources <<EOF
Types: deb
URIs: ${mirror}
Suites: ${codename} ${codename}-updates
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg

Types: deb
URIs: http://security.debian.org/debian-security
Suites: ${codename}-security
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg
EOF
  else
    printf 'deb %s %s main\ndeb %s %s-updates main\ndeb http://security.debian.org/debian-security %s-security main\n' \
      "${mirror}" "${codename}" "${mirror}" "${codename}" "${codename}" >/etc/apt/sources.list
  fi
}

choose_debian_mirror
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
