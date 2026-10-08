#!/usr/bin/env bash
set -euo pipefail

script_dir="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
data_dir="${OPENCHAMBER_DATA_DIR:-$HOME/.config/openchamber}"
repo_file="$script_dir/preferences.json"
local_file="$data_dir/preferences.json"

case "${1:-}" in
  export)
    source_file="$local_file"
    target_file="$repo_file"
    ;;
  apply)
    source_file="$repo_file"
    target_file="$local_file"
    ;;
  -h|--help)
    echo "Usage: $0 export|apply"
    echo "  export  Copy local preferences into dotfiles."
    echo "  apply   Back up local preferences, then copy from dotfiles."
    echo "Close OpenChamber before apply. OPENCHAMBER_DATA_DIR overrides the local directory."
    exit 0
    ;;
  *)
    echo "Usage: $0 export|apply" >&2
    exit 1
    ;;
esac

if [[ $# -ne 1 ]]; then
  echo "Expected exactly one argument: export or apply." >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required to validate preferences.json." >&2
  exit 1
fi
if [[ ! -f "$source_file" ]]; then
  echo "Preferences file not found: $source_file" >&2
  exit 1
fi

umask 077
mkdir -p "$(dirname "$target_file")"
temp_file="$(mktemp "${target_file}.tmp.XXXXXX")"
trap 'rm -f "$temp_file"' EXIT
cp "$source_file" "$temp_file"
jq -e 'type == "object" and .version == 1 and (.fields | type == "object")' "$temp_file" >/dev/null

if [[ -f "$target_file" ]] && cmp -s "$temp_file" "$target_file"; then
  echo "Already up to date: $target_file"
  exit 0
fi
if [[ "$1" == apply && -f "$target_file" ]]; then
  backup_file="$(mktemp "${target_file}.backup.XXXXXX")"
  cp "$target_file" "$backup_file"
  chmod 600 "$backup_file"
  echo "Backup: $backup_file"
fi
chmod 600 "$temp_file"
mv -f "$temp_file" "$target_file"
echo "Copied: $source_file -> $target_file"
