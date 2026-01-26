# AGENTS

修改的时候记得两个脚本同步修改。

## tmux-dump

Purpose
- Dump tmux topology as JSON to stdout.

Usage
- tmux-dump > tmux.json
- tmux-dump --pretty > tmux.pretty.json

Behavior
- If running inside tmux, only the current session is dumped.
- If not in tmux, the attached session is dumped (or the first session if none are attached).

Output structure (high level)
- session
  - windows: list of windows
    - panes: list of panes
      - processes: list of processes on the pane TTY

Important fields
- name
- windows[].name
- windows[].panes[].path
- windows[].panes[].start_command
- windows[].panes[].current_command
- windows[].panes[].processes[].command (array of tokens)

Notes
- processes[].command is an array of strings (tokenized via shell-like splitting).
- start_command reflects tmux pane start command as dumped by tmux.

## tmux-load

Purpose
- Restore tmux topology from a tmux-dump JSON file.

Usage
- tmux-load path/to/tmux.json
- tmux-load --session name path/to/tmux.json
- tmux-load -f path/to/tmux.json
- tmux-load -a path/to/tmux.json
- tmux-load --run-commands path/to/tmux.json

Behavior
- Restores windows, panes, titles, layouts, and working directories into a target session.
- Default target is the current tmux session.
- If not inside tmux, --session is required.
- If target session is not empty, use -f to clear or -a to append.
- -f clears the target session before restore.
- -a appends windows to the target session.
- --run-commands executes pane start_command.

Input expectations
- Dump can be a single session object (current output) or legacy {"sessions": [...]}.
- windows[].panes[].start_command may be a string or an array.
  - Arrays are joined into a shell command line before execution.

Notes
- start_command is executed by sending keys to the pane or by respawning the pane
  with the command attached (first pane).
