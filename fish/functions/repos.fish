function repos --description "Select a repo with fzf and cd into it"
  if not type -q fzf
    printf 'repos: fzf is required\n' >&2
    return 1
  end

  set -l patterns $argv
  if not set -q patterns[1]
    set patterns '~/src/template-assembler*'
  end

  set -l repo_paths (
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
  )

  if not set -q repo_paths[1]
    printf 'No directories matched: %s\n' (string join ', ' -- $patterns) >&2
    return 1
  end

  set -l escaped_home (string escape --style=regex -- $HOME)
  set -l tab (printf '\t')
  set -l candidates
  for repo_path in $repo_paths
    set -l display_path
    if test $repo_path = $HOME
      set display_path '~'
    else
      set display_path (string replace -r '^'$escaped_home'(?=/)' '~' -- $repo_path)
    end

    set -l branch '(not a git repo)'
    if git -C $repo_path rev-parse --is-inside-work-tree >/dev/null 2>/dev/null
      set branch (git -C $repo_path symbolic-ref --quiet --short HEAD 2>/dev/null)
      if test -z "$branch"
        set -l revision (git -C $repo_path rev-parse --short HEAD 2>/dev/null)
        if test -n "$revision"
          set branch "(detached $revision)"
        else
          set branch '(unknown)'
        end
      end
    end

    set candidates $candidates "$repo_path$tab$display_path [$branch]"
  end

  set -l selected (printf '%s\n' $candidates | fzf \
    --delimiter $tab \
    --with-nth 2 \
    --prompt 'repo> ' \
    --bind 'tab:toggle-preview' \
    --preview-window 'right:70%:hidden' \
    --preview 'git -C {1} status -sb 2>/dev/null || printf "(not a git repo)\n"')
  if test -z "$selected"
    return 0
  end

  cd (string split -f 1 $tab -- $selected)
end
