function __tmux_command_tracker_preexec --on-event fish_preexec --argument-names commandline
  status is-interactive; or return
  set -q TMUX TMUX_PANE; or return
  set -q TMUX_COMMAND_TRACKER_DISABLE; and return
  test "$fish_private_mode" = 1; and return
  string match -qr '^\s' -- "$commandline"; and return
  string match -qr '^\s*(command\s+)?([^\s]*/)?tmux-dump(\s|$)' -- "$commandline"; and return
  string match -qr '^\s*(command\s+)?([^\s]*/)?tbox\s+(save|autosave)(\s|$)' -- "$commandline"; and return

  set -l encoded (command printf '%s' "$commandline" | command base64 | string join '')
  command tmux set-option -p -t "$TMUX_PANE" @tmux_command_tracker_command "$encoded" 2>/dev/null
end


function __tmux_command_tracker_postexec --on-event fish_postexec
  status is-interactive; or return
  set -q TMUX TMUX_PANE; or return

  command tmux set-option -p -u -t "$TMUX_PANE" @tmux_command_tracker_command 2>/dev/null
end
