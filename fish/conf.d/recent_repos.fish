function __recent_repos_state_dir
  if set -q XDG_STATE_HOME; and test -n "$XDG_STATE_HOME"
    printf '%s\n' "$XDG_STATE_HOME"
  else
    printf '%s\n' "$HOME/.local/state"
  end
end

function __recent_repos_file
  printf '%s\n' (__recent_repos_state_dir)/recent-repos.tsv
end

function __recent_repos_limit
  if set -q recent_repos_limit; and test -n "$recent_repos_limit"
    printf '%s\n' "$recent_repos_limit"
  else
    printf '%s\n' 200
  end
end

function __recent_repos_canonical_path --argument-names path
  path resolve "$path" 2>/dev/null
end

function __recent_repos_repo_id --argument-names repo_path
  set -l common_dir (command git -C "$repo_path" rev-parse --git-common-dir 2>/dev/null)
  test $status -eq 0; or return 1
  test -n "$common_dir"; or return 1

  if not string match -q '/*' -- "$common_dir"
    set common_dir "$repo_path/$common_dir"
  end

  __recent_repos_canonical_path "$common_dir"
end

function __recent_repos_record --argument-names repo_id repo_path
  test -n "$repo_id"; or return 1
  test -n "$repo_path"; or return 1

  set -l file (__recent_repos_file)
  set -l state_dir (dirname "$file")
  command mkdir -p "$state_dir"; or return 1

  printf '%s\t%s\t%s\n' (command date +%s) "$repo_id" "$repo_path" >>"$file"

  set -l tmp "$file.tmp.$fish_pid"
  set -l limit (__recent_repos_limit)
  command sort -rn "$file" | command awk -F '\t' -v limit="$limit" '
    !seen[$2]++ {
      print
      count++
      if (count >= limit) {
        exit
      }
    }
  ' >"$tmp"

  if test $status -eq 0
    command mv "$tmp" "$file"
  else
    command rm -f "$tmp"
  end
end

function __recent_repos_remove --argument-names repo_id
  test -n "$repo_id"; or return 1

  set -l file (__recent_repos_file)
  test -f "$file"; or return 0

  set -l tmp "$file.tmp.$fish_pid"
  command awk -F '\t' -v repo_id="$repo_id" '$2 != repo_id { print }' "$file" >"$tmp"
  if test $status -eq 0
    command mv "$tmp" "$file"
  else
    command rm -f "$tmp"
  end
end

function __record_recent_repo --on-variable PWD
  status is-interactive; or return
  command -q git; or return

  set -l root (command git rev-parse --show-toplevel 2>/dev/null)
  test $status -eq 0; or return
  test -n "$root"; or return

  set -l repo_id (__recent_repos_repo_id "$root")
  test $status -eq 0; or return

  __recent_repos_record "$repo_id" "$root"
end
