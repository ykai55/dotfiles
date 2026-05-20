FROM node:20-bookworm

ARG DEBIAN_MIRROR=auto

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

RUN set -euo pipefail; \
  codename="$(. /etc/os-release && printf '%s' "${VERSION_CODENAME}")"; \
  choose_debian_mirror() { \
    local best mirror elapsed start; \
    if [ "${DEBIAN_MIRROR}" != auto ] && [ -n "${DEBIAN_MIRROR}" ]; then \
      printf '%s' "${DEBIAN_MIRROR}"; \
      return 0; \
    fi; \
    best=""; \
    for mirror in \
      "http://deb.debian.org/debian" \
      "http://mirrors.tuna.tsinghua.edu.cn/debian" \
      "http://mirrors.ustc.edu.cn/debian" \
      "http://mirrors.aliyun.com/debian" \
      "http://mirrors.cloud.tencent.com/debian" \
      "http://mirror.sjtu.edu.cn/debian"; do \
      start="$(date +%s%3N)"; \
      if curl -fsSL --connect-timeout 2 --max-time 4 "${mirror}/dists/${codename}/Release" >/dev/null; then \
        elapsed="$(( $(date +%s%3N) - start ))"; \
        printf 'Debian mirror candidate: %s (%sms)\n' "${mirror}" "${elapsed}" >&2; \
        if [ -z "${best}" ] || [ "${elapsed}" -lt "${best%% *}" ]; then \
          best="${elapsed} ${mirror}"; \
        fi; \
      fi; \
    done; \
    if [ -n "${best}" ]; then \
      printf '%s' "${best#* }"; \
    else \
      printf '%s' "http://deb.debian.org/debian"; \
    fi; \
  }; \
  mirror="$(choose_debian_mirror)"; \
  printf 'Using Debian mirror: %s\n' "${mirror}" >&2; \
  if [ -f /etc/apt/sources.list.d/debian.sources ]; then \
    printf 'Types: deb\nURIs: %s\nSuites: %s %s-updates\nComponents: main\nSigned-By: /usr/share/keyrings/debian-archive-keyring.gpg\n\nTypes: deb\nURIs: http://security.debian.org/debian-security\nSuites: %s-security\nComponents: main\nSigned-By: /usr/share/keyrings/debian-archive-keyring.gpg\n' \
      "${mirror}" "${codename}" "${codename}" "${codename}" >/etc/apt/sources.list.d/debian.sources; \
  else \
    printf 'deb %s %s main\ndeb %s %s-updates main\ndeb http://security.debian.org/debian-security %s-security main\n' \
      "${mirror}" "${codename}" "${mirror}" "${codename}" "${codename}" >/etc/apt/sources.list; \
  fi; \
  apt-get update; \
  apt-get install -y --no-install-recommends fish openssh-client python3 ripgrep; \
  rm -rf /var/lib/apt/lists/*

RUN npm install -g opencode-ai@latest @larksuite/cli@1.0.34

EXPOSE 4096
