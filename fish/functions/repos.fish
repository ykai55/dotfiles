function __repos_state_file
  if functions -q __recent_repos_file
    __recent_repos_file
  else if set -q XDG_STATE_HOME; and test -n "$XDG_STATE_HOME"
    printf '%s\n' "$XDG_STATE_HOME/recent-repos.tsv"
  else
    printf '%s\n' "$HOME/.local/state/recent-repos.tsv"
  end
end

function __repos_display_path --argument-names repo_path
  set -l escaped_home (string escape --style=regex -- $HOME)
  if test "$repo_path" = "$HOME"
    printf '~\n'
  else
    string replace -r '^'$escaped_home'(?=/)' '~' -- "$repo_path"
  end
end

function __repos_branch_name --argument-names repo_path
  set -l branch (command git -C "$repo_path" symbolic-ref --quiet --short HEAD 2>/dev/null)
  if test -n "$branch"
    printf '%s\n' "$branch"
    return
  end

  set -l revision (command git -C "$repo_path" rev-parse --short HEAD 2>/dev/null)
  if test -n "$revision"
    printf 'detached %s\n' "$revision"
  else
    printf 'unknown\n'
  end
end

function __repos_repo_id --argument-names repo_path
  if functions -q __recent_repos_repo_id
    __recent_repos_repo_id "$repo_path"
    return
  end

  set -l common_dir (command git -C "$repo_path" rev-parse --git-common-dir 2>/dev/null)
  test $status -eq 0; or return 1
  if not string match -q '/*' -- "$common_dir"
    set common_dir "$repo_path/$common_dir"
  end
  printf '%s\n' "$common_dir"
end

function __repos_worktrees --argument-names repo_path
  set -l lines (command git -C "$repo_path" worktree list --porcelain 2>/dev/null)
  if test $status -ne 0
    printf '%s\n' "$repo_path"
    return
  end

  set -l found
  for line in $lines
    if string match -q 'worktree *' -- "$line"
      set found 1
      string replace -r '^worktree ' '' -- "$line"
    end
  end

  test -n "$found"; or printf '%s\n' "$repo_path"
end

function __repos_recent_records
  set -l file (__repos_state_file)
  test -f "$file"; or return

  command sort -rn "$file" | command awk -F '\t' '
    !seen[$2]++ {
      print $2 "\t" $3
    }
  '
end

function __repos_pattern_paths
  set -l patterns $argv
  test (count $patterns) -gt 0; or return

  python3 -c '
import glob
import os
import sys

seen = set()
for pattern in sys.argv[1:]:
    expanded = os.path.expanduser(pattern)
    for path in sorted(glob.glob(expanded)):
        if not os.path.isdir(path):
            continue
        full_path = os.path.abspath(path)
        if full_path in seen:
            continue
        seen.add(full_path)
        print(full_path)
' -- $patterns
end

function __repos_append_candidate --argument-names repo_path repo_id
  command git -C "$repo_path" rev-parse --is-inside-work-tree >/dev/null 2>/dev/null; or return

  set -l branch (__repos_branch_name "$repo_path")
  set -l display_path (__repos_display_path "$repo_path")
  set -l tab (printf '\t')
  printf '%s%s%s%s%s [%s]\n' "$repo_path" "$tab" "$repo_id" "$tab" "$display_path" "$branch"
end

function __repos_candidates
  set -l patterns $argv
  set -l candidates
  set -l seen_paths

  for record in (__repos_recent_records)
    set -l fields (string split (printf '\t') -- "$record")
    set -l repo_id $fields[1]
    set -l anchor_path $fields[2]
    test -n "$repo_id"; or continue
    test -d "$anchor_path"; or continue

    for worktree_path in (__repos_worktrees "$anchor_path")
      test -d "$worktree_path"; or continue
      contains -- "$worktree_path" $seen_paths; and continue
      set -a seen_paths "$worktree_path"
      set -a candidates (__repos_append_candidate "$worktree_path" "$repo_id")
    end
  end

  if set -q patterns[1]
    for repo_path in (__repos_pattern_paths $patterns)
      contains -- "$repo_path" $seen_paths; and continue
      set -l repo_id (__repos_repo_id "$repo_path")
      test -n "$repo_id"; or continue
      set -a seen_paths "$repo_path"
      set -a candidates (__repos_append_candidate "$repo_path" "$repo_id")
    end
  end

  set -q candidates[1]; and printf '%s\n' $candidates
end

function repos --description "Select a repo with fzf and cd into it"
  if not type -q fzf
    printf 'repos: fzf is required\n' >&2
    return 1
  end

  set -l tab (printf '\t')

  while true
    set -l candidates (__repos_candidates $argv)
    if not set -q candidates[1]
      if test (count $argv) -gt 0
        printf 'No directories matched: %s\n' (string join ', ' -- $argv) >&2
      else
        printf 'No recent repositories found\n' >&2
      end
      return 1
    end

    set -l selected (printf '%s\n' $candidates | fzf \
      --delimiter $tab \
      --with-nth 3 \
      --prompt 'repo> ' \
      --header 'ctrl-d: remove repo from recents' \
      --expect ctrl-d \
      --bind 'tab:toggle-preview' \
      --preview-window 'right:70%:hidden' \
      --preview 'git -C {1} status -sb 2>/dev/null || printf "(not a git repo)\n"')

    test -n "$selected"; or return 0

    set -l key $selected[1]
    set -l selected_line $selected[2]
    test -n "$selected_line"; or return 0

    set -l fields (string split $tab -- "$selected_line")
    set -l repo_path $fields[1]
    set -l repo_id $fields[2]

    if test "$key" = ctrl-d
      if functions -q __recent_repos_remove
        __recent_repos_remove "$repo_id"
      end
      continue
    end

    cd "$repo_path"
    return
  end
end
