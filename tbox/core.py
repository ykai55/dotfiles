#!/usr/bin/env python3
from __future__ import annotations

import datetime
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from typing import Any, Dict, List, Optional, Tuple


@dataclass
class Entry:
    name: str
    live: bool = False
    live_windows_count: Optional[int] = None
    archive_path: Optional[str] = None
    archive_mtime: float = 0.0
    archive_windows_count: Optional[int] = None

    def selector_key(self) -> str:
        # Selector lines are tab-delimited; keep this field tab-safe.
        return self.name.replace("\t", " ")


def repo_root() -> str:
    return os.path.abspath(os.path.join(os.path.dirname(__file__), os.pardir))


def run_cmd(argv: List[str]) -> Tuple[int, str, str]:
    proc = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    return proc.returncode, proc.stdout, proc.stderr


def tool_path(name: str) -> str:
    if os.environ.get("TBOX_PREFER_LOCAL") == "0":
        return name
    local = os.path.join(repo_root(), "bin", name)
    if os.path.isfile(local) and os.access(local, os.X_OK):
        return local
    return name


def data_dir() -> str:
    env_dir = os.environ.get("TBOX_DIR")
    if env_dir:
        return os.path.abspath(os.path.expanduser(env_dir))
    base = os.environ.get("XDG_DATA_HOME")
    if not base:
        base = os.path.expanduser("~/.local/share")
    # Keep the default compatible with existing `tbox`.
    return os.path.join(base, "tmux-box")


def in_tmux() -> bool:
    return bool(os.environ.get("TMUX"))


def current_session_name() -> Optional[str]:
    rc, out, _ = run_cmd(["tmux", "display-message", "-p", "#{session_name}"])
    if rc != 0:
        return None
    name = out.strip()
    return name or None


def session_name_from_dump(data: Dict[str, Any]) -> Optional[str]:
    if "sessions" in data and isinstance(data.get("sessions"), list):
        sessions = data.get("sessions") or []
        if not sessions:
            return None
        session = sessions[0] or {}
        return session.get("name") or session.get("session_name")
    if "windows" in data or "name" in data or "session_name" in data:
        return data.get("name") or data.get("session_name")
    return None


def windows_count_from_dump(data: Dict[str, Any]) -> Optional[int]:
    if "sessions" in data and isinstance(data.get("sessions"), list):
        sessions = data.get("sessions") or []
        if not sessions:
            return None
        session = sessions[0] or {}
        windows = session.get("windows") or []
        if isinstance(windows, list):
            return len(windows)
        return None
    windows = data.get("windows") or []
    if isinstance(windows, list):
        return len(windows)
    return None


def safe_filename(session_name: str) -> str:
    clean = "".join(ch if (ch.isalnum() or ch in "._-") else "_" for ch in session_name)
    if not clean:
        clean = "session"
    digest = hashlib.sha1(session_name.encode("utf-8")).hexdigest()[:8]
    return f"{clean}-{digest}.json"


def format_mtime(ts: float) -> str:
    if not ts:
        return ""
    return datetime.datetime.fromtimestamp(ts).strftime("%Y-%m-%d %H:%M")


def load_saved_sessions(store_dir: str) -> List[Entry]:
    if not os.path.isdir(store_dir):
        return []
    entries: List[Entry] = []
    for name in os.listdir(store_dir):
        if not name.endswith(".json"):
            continue
        path = os.path.join(store_dir, name)
        try:
            with open(path, "r", encoding="utf-8") as f:
                data = json.load(f)
        except Exception as exc:
            print(f"WARN: failed to read {path}: {exc}", file=sys.stderr)
            continue
        session_name = session_name_from_dump(data) or name
        windows_count = windows_count_from_dump(data)
        try:
            mtime = os.path.getmtime(path)
        except OSError:
            mtime = 0.0
        entries.append(
            Entry(
                name=str(session_name),
                live=False,
                archive_path=path,
                archive_mtime=float(mtime),
                archive_windows_count=windows_count,
            )
        )
    entries.sort(key=lambda e: e.archive_mtime, reverse=True)
    return entries


def find_entry_by_name(entries: List[Entry], name: str) -> Optional[Entry]:
    for entry in entries:
        if entry.name == name:
            return entry
    return None


def tmux_has_session(name: str) -> bool:
    rc, _, _ = run_cmd(["tmux", "has-session", "-t", name])
    return rc == 0


def unique_session_name(base: str) -> str:
    if not tmux_has_session(base):
        return base
    idx = 1
    while True:
        candidate = f"{base}({idx})"
        if not tmux_has_session(candidate):
            return candidate
        idx += 1


def list_live_sessions() -> List[Entry]:
    # Use a minimal format that is stable across tmux versions.
    rc, out, err = run_cmd(["tmux", "list-sessions", "-F", "#{session_name}\t#{session_windows}"])
    if rc != 0:
        msg = err.strip() or "tmux list-sessions failed"
        raise RuntimeError(msg)
    entries: List[Entry] = []
    for line in out.splitlines():
        if not line.strip():
            continue
        parts = line.split("\t")
        name = (parts[0] if parts else "").strip()
        if not name:
            continue
        windows_count: Optional[int] = None
        if len(parts) > 1:
            wc = parts[1].strip()
            if wc.isdigit():
                windows_count = int(wc)
        entries.append(Entry(name=name, live=True, live_windows_count=windows_count))
    return entries


def merge_sessions(live: List[Entry], archived: List[Entry]) -> List[Entry]:
    by_name: Dict[str, Entry] = {}

    # Prefer the most-recent archive when there are duplicates.
    for entry in archived:
        if not entry.name:
            continue
        existing = by_name.get(entry.name)
        if not existing or entry.archive_mtime >= existing.archive_mtime:
            by_name[entry.name] = entry

    for entry in live:
        if not entry.name:
            continue
        existing = by_name.get(entry.name)
        if not existing:
            by_name[entry.name] = entry
            continue
        by_name[entry.name] = Entry(
            name=entry.name,
            live=True,
            live_windows_count=entry.live_windows_count,
            archive_path=existing.archive_path,
            archive_mtime=existing.archive_mtime,
            archive_windows_count=existing.archive_windows_count,
        )

    # Stable ordering: live sessions first, then by archive mtime desc.
    def sort_key(entry: Entry) -> Tuple[int, float, str]:
        live_rank = 0 if entry.live else 1
        return (live_rank, -float(entry.archive_mtime), entry.name)

    return sorted(by_name.values(), key=sort_key)


def is_named_session(name: str) -> bool:
    stripped = name.strip()
    if not stripped:
        return False
    return not stripped.isdigit()


def select_selector() -> Optional[str]:
    selector = os.environ.get("TBOX_SELECTOR")
    if not selector:
        return shutil.which("fzf") or shutil.which("sk")
    selector = selector.strip()
    if selector in {"none", "prompt", "builtin"}:
        return None
    if selector in {"fzf", "sk"}:
        path = shutil.which(selector)
        if not path:
            print(f"WARN: selector {selector} not found, using prompt", file=sys.stderr)
        return path
    print(f"WARN: unsupported selector {selector}, using prompt", file=sys.stderr)
    return None


def format_entry_lines(entries: List[Entry]) -> Tuple[List[str], Dict[str, Entry], int, int, int]:
    entry_map = {entry.selector_key(): entry for entry in entries}
    name_width = max(len(entry.selector_key()) for entry in entries)
    status_width = 4
    windows_width = max(
        len(f"{effective_windows_count(entry)}w") if effective_windows_count(entry) is not None else 2
        for entry in entries
    )
    saved_width = max(len(format_mtime(float(entry.archive_mtime))) for entry in entries)

    lines: List[str] = []
    for entry in entries:
        raw_name = entry.selector_key()
        name = raw_name.ljust(name_width)
        status = ("LIVE" if entry.live else "ARCH").ljust(status_width)
        wc = effective_windows_count(entry)
        windows_label = f"{wc}w" if wc is not None else ""
        windows_pad = windows_label.ljust(windows_width)
        saved = format_mtime(float(entry.archive_mtime)).ljust(saved_width)
        # 1=name 2=status 3=windows 4=saved 5=raw_name
        lines.append(f"{name}\t{status}\t{windows_pad}\t{saved}\t{raw_name}")

    return lines, entry_map, name_width, windows_width, saved_width


def effective_windows_count(entry: Entry) -> Optional[int]:
    if entry.live and entry.live_windows_count is not None:
        return int(entry.live_windows_count)
    if entry.archive_windows_count is not None:
        return int(entry.archive_windows_count)
    return None


def choose_entry_action(entries: List[Entry], prompt: str) -> Tuple[Optional[Entry], str]:
    if not entries:
        return None, ""

    selector = select_selector()
    lines, entry_map, _, _, _ = format_entry_lines(entries)
    if selector:
        preview_cmd = f"{tool_path('tbox')} preview " + "{5}"
        cmd = [
            selector,
            "--prompt",
            f"{prompt}: ",
            "--header",
            "enter=switch/restore, ctrl-d=drop-archive",
            "--expect",
            "ctrl-d",
            "--with-nth",
            "1,2,3,4",
            "--delimiter",
            "\t",
            "--preview",
            preview_cmd,
            "--preview-window",
            "up,50%",
        ]
        proc = subprocess.run(
            cmd,
            input="\n".join(lines) + "\n",
            text=True,
            stdout=subprocess.PIPE,
        )
        if proc.returncode != 0:
            return None, ""
        out_lines = [line for line in proc.stdout.splitlines() if line.strip()]
        if not out_lines:
            return None, ""
        key = ""
        selected = out_lines[0]
        if len(out_lines) > 1:
            key = out_lines[0].strip()
            selected = out_lines[1].strip()
        if not selected:
            return None, ""
        # raw_name is field 5.
        parts = selected.split("\t")
        raw_name = parts[4].strip() if len(parts) > 4 else ""
        action = "drop" if key == "ctrl-d" else "select"
        return entry_map.get(raw_name), action

    for idx, entry in enumerate(entries, start=1):
        status = "LIVE" if entry.live else "ARCH"
        wc = effective_windows_count(entry)
        windows_label = f"{wc}w" if wc is not None else "?w"
        saved = format_mtime(float(entry.archive_mtime))
        line = f"{idx}) {entry.name}  {status}  {windows_label}  {saved}".rstrip()
        print(line)
    selection = input(f"{prompt} (number): ").strip()
    if not selection:
        return None, ""
    try:
        idx = int(selection)
    except ValueError:
        return None, ""
    if idx < 1 or idx > len(entries):
        return None, ""
    entry = entries[idx - 1]
    action = input("Action ([s]elect/[d]rop-archive, default s): ").strip().lower()
    if action.startswith("d"):
        return entry, "drop"
    return entry, "select"


def ensure_store_dir() -> str:
    store = data_dir()
    os.makedirs(store, exist_ok=True)
    return store


def save_session_dump(session_name: str, store: str) -> str:
    entries = load_saved_sessions(store)
    existing = find_entry_by_name(entries, session_name)
    dest_path = (
        str(existing.archive_path)
        if existing and existing.archive_path
        else os.path.join(store, safe_filename(session_name))
    )

    tmp_fd, tmp_path = tempfile.mkstemp(prefix=".tbox-", suffix=".json", dir=store)
    os.close(tmp_fd)

    cmd = [tool_path("tmux-dump"), "--session", session_name, tmp_path]
    rc, _, err = run_cmd(cmd)
    if rc != 0:
        if err:
            raise RuntimeError(err.strip())
        raise RuntimeError("tmux-dump failed")
    os.replace(tmp_path, dest_path)
    return dest_path


def attach_or_switch_session(session_name: str) -> int:
    cmd = ["tmux", "switch-client" if in_tmux() else "attach-session", "-t", session_name]
    return subprocess.run(cmd).returncode


def restore_from_archive(
    session_name: str,
    archive_path: str,
    run_commands: bool,
    new_session: bool,
) -> int:
    target = session_name
    if new_session:
        target = unique_session_name(session_name)
    cmd = [tool_path("tmux-load"), archive_path, "--session", target]
    if not run_commands:
        cmd.append("--no-run-commands")
    return subprocess.run(cmd).returncode


def cmd_save(name: Optional[str]) -> int:
    target = name
    if not target:
        if not in_tmux():
            print("ERROR: name is required when not inside tmux", file=sys.stderr)
            return 2
        target = current_session_name()
    if not target:
        print("ERROR: name is required when not inside tmux", file=sys.stderr)
        return 2

    store = ensure_store_dir()
    try:
        save_session_dump(target, store)
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    print(f"Saved session '{target}'")
    return 0


def cmd_autosave(throttle_seconds: float, quiet: bool) -> int:
    if os.environ.get("TBOX_AUTOSAVE_IN_PROGRESS") == "1":
        return 0
    os.environ["TBOX_AUTOSAVE_IN_PROGRESS"] = "1"

    store = ensure_store_dir()
    stamp_path = os.path.join(store, ".tbox-autosave.stamp")
    now = time.time()
    try:
        last = os.path.getmtime(stamp_path)
        if throttle_seconds > 0 and now - last < throttle_seconds:
            return 0
    except OSError:
        pass

    try:
        live = list_live_sessions()
    except Exception as exc:
        if not quiet:
            print(f"ERROR: {exc}", file=sys.stderr)
        return 1

    saved = 0
    for entry in live:
        name = entry.name
        if not is_named_session(name):
            continue
        try:
            save_session_dump(name, store)
            saved += 1
        except Exception:
            # Best-effort autosave; keep going.
            continue
    try:
        with open(stamp_path, "w", encoding="utf-8") as f:
            f.write(f"{now}\n")
    except Exception:
        pass
    if not quiet:
        print(f"Autosaved {saved} session(s)")
    return 0


def cmd_drop(name: Optional[str]) -> int:
    if not name:
        if not in_tmux():
            print("ERROR: name is required when not inside tmux", file=sys.stderr)
            return 2
        name = current_session_name()
    if not name:
        print("ERROR: name is required when not inside tmux", file=sys.stderr)
        return 2

    store = data_dir()
    entries = load_saved_sessions(store)
    entry = find_entry_by_name(entries, name)
    if not entry or not entry.archive_path:
        print(f"ERROR: no stored session named '{name}'", file=sys.stderr)
        return 1
    try:
        os.remove(str(entry.archive_path))
    except OSError as exc:
        print(f"ERROR: failed to remove {entry.archive_path}: {exc}", file=sys.stderr)
        return 1
    print(f"Removed session '{name}'")
    return 0


def cmd_list(verbose: bool, include_live: bool) -> int:
    store = data_dir()
    archived = load_saved_sessions(store)
    if include_live:
        try:
            live = list_live_sessions()
        except Exception as exc:
            print(f"ERROR: {exc}", file=sys.stderr)
            return 1
        entries = merge_sessions(live, archived)
    else:
        entries = archived

    if not entries:
        print("No sessions")
        return 1

    name_width = max(len(entry.name) for entry in entries)
    for entry in entries:
        name = entry.name.ljust(name_width)
        status = "LIVE" if entry.live else "ARCH"
        wc = effective_windows_count(entry)
        wc_s = f"{wc}w" if wc is not None else ""
        saved = format_mtime(float(entry.archive_mtime))
        line = f"{name}  {status}  {wc_s}  {saved}".rstrip()
        if verbose and entry.archive_path:
            line = f"{line}  {entry.archive_path}"
        print(line)
    return 0


def cmd_preview(name: str) -> int:
    store = data_dir()
    entries = load_saved_sessions(store)
    entry = find_entry_by_name(entries, name)
    if not entry or not entry.archive_path:
        print(f"No archive for session: {name}")
        return 0

    try:
        with open(str(entry.archive_path), "r", encoding="utf-8") as f:
            data = json.load(f)
    except Exception as exc:
        print(f"Preview error: {exc}")
        return 0

    sess: Dict[str, Any] = {}
    if isinstance(data, dict):
        sessions = data.get("sessions")
        if isinstance(sessions, list) and sessions:
            sess = sessions[0] or {}
        else:
            sess = data

    sname = sess.get("name") or sess.get("session_name") or entry.name or name
    wins = sess.get("windows") or []
    print(f"Session: {sname}")
    print(f"Windows: {len(wins)}")
    for win in wins:
        idx = win.get("index", "")
        wname = win.get("name", "")
        panes = win.get("panes") or []
        print(f"- [{idx}] {wname} ({len(panes)} panes)")
        for pane in panes:
            pidx = pane.get("index", "")
            title = pane.get("title") or ""
            path_val = pane.get("path") or ""
            if title:
                print(f"  - {pidx}: {title}")
            elif path_val:
                print(f"  - {pidx}: {path_val}")
    return 0


def cmd_select(run_commands: bool, new_session: bool, name: Optional[str]) -> int:
    store = data_dir()
    archived = load_saved_sessions(store)
    try:
        live = list_live_sessions()
    except Exception:
        live = []
    entries = merge_sessions(live, archived)
    if not entries:
        print("ERROR: no sessions (live or stored)", file=sys.stderr)
        return 1

    entry: Optional[Entry] = None
    action = "select"
    if name:
        entry = find_entry_by_name(entries, name)
        if not entry:
            print(f"ERROR: no session named '{name}'", file=sys.stderr)
            return 1
    else:
        entry, action = choose_entry_action(entries, "Select session")
    if not entry:
        return 1
    if action == "drop":
        # Drop archive only.
        return cmd_drop(entry.name)

    sess_name = entry.name
    if entry.live:
        return attach_or_switch_session(sess_name)
    archive_path = entry.archive_path
    if not archive_path:
        print(f"ERROR: no archive for session '{sess_name}'", file=sys.stderr)
        return 1
    return restore_from_archive(sess_name, str(archive_path), run_commands, new_session)


def tmux_snippet(script_path: str, throttle_seconds: float) -> str:
    quoted = script_path.replace('"', '\\"')
    # Keep hook set minimal and rely on throttling in autosave.
    hooks = [
        "client-session-changed",
        "client-detached",
        "session-renamed",
        "window-renamed",
        "window-layout-changed",
    ]
    lines = [
        "# tbox session persistence",
        f"# Requires: {quoted}",
        f"set -g @tbox_autosave \"{quoted} autosave --quiet --throttle-seconds {throttle_seconds}\"",
    ]
    for hook in hooks:
        lines.append(f"set-hook -g {hook} \"run-shell -b \\\"#{{@tbox_autosave}}\\\"\"")
    lines.append(f"bind W popup -E \"{quoted} select\"")
    lines.append(
        "bind X confirm-before -p \"kill-session #{session_name}? (y/n)\" "
        f"\"run-shell -b \\\"{quoted} save #{{session_name}} >/dev/null 2>&1; tmux kill-session -t #{{session_name}}\\\"\""
    )
    return "\n".join(lines) + "\n"


def cmd_tmux_snippet(throttle_seconds: float, tbox_command: Optional[str]) -> int:
    script = (tbox_command or "").strip() or tool_path("tbox")
    print(tmux_snippet(script, throttle_seconds), end="")
    return 0
