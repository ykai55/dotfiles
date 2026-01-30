# tbox

基于 tmux-load/tmux-dump 实现的 tmux 会话持久管理器。

## 使用说明

### 保存会话

```
tbox save
tbox save <session-name>
```

- 在 tmux 内：不传 name 时会保存当前会话。
- 在 tmux 外：必须显式传入 name。
- 如果已有同名会话，会直接覆盖更新。

### 恢复会话

```
tbox select
tbox select <session-name>
tbox select --new
tbox select -n
```

- 使用交互式选择器选择要恢复的会话。
- 传入 name 时跳过交互选择，直接恢复对应会话。
- 优先使用 fzf/sk；若不可用则回退到数字输入。
- `--new/-n` 会在新会话中恢复（自动选择未占用的会话名）。

### 删除会话

```
tbox drop
tbox drop <session-name>
```

- 在 tmux 内：不传 name 时删除当前会话的存档。
- 在 tmux 外：必须显式传入 name。

### 列出会话

```
tbox list
tbox list -v
```

- 默认只显示会话名、窗口数、更新时间。
- `-v/--verbose` 会额外显示存档文件路径。

## 存储位置

- 默认：`~/.local/share/tmux-box`
- 可通过 `TBOX_DIR` 覆盖存储目录。
