function repos --description "Select a repo with fzf and cd into it"
  set -l selected (command repos $argv)
  set -l repos_status $status

  if test $repos_status -ne 0
    return $repos_status
  end

  if test -z "$selected"
    return 0
  end

  cd "$selected"
end
