#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  ""|export) ;;
  -h|--help)
    echo "Usage: $0 [export]"
    echo "Copy local OpenChamber preferences into dotfiles."
    echo "OPENCHAMBER_DATA_DIR overrides the local directory."
    exit 0
    ;;
  *)
    echo "Usage: $0 [export]. Restore with bin/dotfiles-apply --apply." >&2
    exit 1
    ;;
esac
if [[ $# -gt 1 ]]; then
  echo "Usage: $0 [export]" >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required to validate preferences.json." >&2
  exit 1
fi

script_dir="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
source_file="${OPENCHAMBER_DATA_DIR:-$HOME/.config/openchamber}/preferences.json"
target_file="$script_dir/preferences.json"
if [[ ! -f "$source_file" ]]; then
  echo "Preferences file not found: $source_file" >&2
  exit 1
fi

umask 077
temp_file="$(mktemp "${target_file}.tmp.XXXXXX")"
trap 'rm -f "$temp_file"' EXIT
cp "$source_file" "$temp_file"
jq -e 'type == "object" and .version == 1 and (.fields | type == "object")' "$temp_file" >/dev/null

if [[ -f "$target_file" ]] && cmp -s "$temp_file" "$target_file"; then
  echo "Already up to date: $target_file"
  exit 0
fi
chmod 600 "$temp_file"
mv -f "$temp_file" "$target_file"
echo "Copied: $source_file -> $target_file"
