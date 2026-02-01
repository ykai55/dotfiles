# tbox

基于 `tmux-dump` / `tmux-load` 的 tmux 会话持久化与会话切换器。

一句话：把 tmux session 切换变成“无负担操作”——不管 session 现在是否还在内存里，你都用同一个入口 `tbox select` 来切换；如果不在内存里，自动从存档恢复。

## 设计理念

### 1) 无负担切换

- tmux 的 session 生命周期本质上是“可丢失”的：服务重启、误关窗口、断电都会让内存态消失。
- tbox 的目标是把“切换 session”从高风险操作变成低成本操作：
  - 看到的列表里同时包含 live（内存中的）和 archived（仅存档的）session。
  - 选择 live：直接 switch/attach。
  - 选择 archived：自动恢复并 switch/attach。

### 2) 以 tmux 为真相，以 dump/load 为接口

- tbox 不自己维护“会话结构”的权威模型，而是复用：
  - `tmux-dump` 负责把 tmux 拓扑保存成 JSON。
  - `tmux-load` 负责把 JSON 还原回 tmux。
- tbox 只做编排：存档目录、选择器、合并 live+archived 列表、以及 tmux hooks 的自动保存策略。

### 3) 自动保存是 best-effort（不做守护进程）

- 自动保存通过 tmux hooks 触发，配合节流（throttle）避免频繁写盘。
- 不引入后台 daemon：一切逻辑都在 `tbox autosave` 里完成，触发点交给 tmux。

### 4) 最少依赖，随时可回退

- 依赖：`tmux` 必须；`fzf`/`sk` 可选。
- 存档是普通 JSON 文件；不绑数据库/服务。

## 快速开始

### 1) 手动保存/恢复

保存当前会话（仅在 tmux 内可用）：

```bash
tbox save
```

保存指定会话（tmux 内/外均可）：

```bash
tbox save work
```

切换（或恢复）会话：

```bash
tbox select
```

### 2) 推荐：集成到 tmux（自动保存 + 一键切换）

运行下面命令生成 tmux 配置片段，并粘贴到 `~/.tmux.conf` 或你的 `tmux/tmux.conf`：

```bash
~/dotfiles/bin/tbox tmux-snippet
```

如果你的 tmux server 环境里 `PATH` 能直接找到 `tbox`，可以用命令名（更方便迁移机器）：

```bash
tbox tmux-snippet --tbox-command tbox
```

该 snippet 默认提供：

- hooks：在“合适时机”自动触发 `tbox autosave`
- `prefix + W`：弹窗选择 session（live + archived）
- `prefix + X`：确认后执行“保存当前 session -> kill-session”

## 命令说明

### 保存：`tbox save [name]`

```bash
tbox save
tbox save <session-name>
```

- tmux 内：不传 `name` 时保存当前 session。
- tmux 外：必须显式传入 `name`。
- 同名存档会被原子覆盖（先写临时文件，再 `rename` 替换）。

### 自动保存：`tbox autosave`

```bash
tbox autosave
tbox autosave --quiet
tbox autosave --throttle-seconds 3
```

- 保存所有“已命名”的 live session（规则：session 名不是纯数字）。
- `--throttle-seconds`：节流窗口内重复触发会直接跳过（默认 3 秒）。
- best-effort：单个 session 保存失败不会中断整个 autosave。

### 选择/恢复：`tbox select`

```bash
tbox select
tbox select <session-name>
tbox select --new
tbox select -n
tbox select --no-run-commands
```

- 默认进入交互选择器，列表中同时包含：
  - `LIVE`：当前 tmux server 内存中的 session
  - `ARCH`：仅存档的 session
- 传入 `name` 时跳过交互选择。
- 选择器优先使用 `fzf`/`sk`；若不可用则回退到数字输入。
- 交互选择时：
  - Enter：switch（LIVE）或 restore（ARCH）
  - Ctrl-D：删除该 session 的“存档”（不会 kill live session）
- `--new/-n`：只对 ARCH 生效。会把存档恢复到一个新 session（自动避开冲突，使用 `name(1)` 形式）。
- `--no-run-commands`：恢复时不启动 pane 命令（透传到 `tmux-load --no-run-commands`）。

注意：如果某个 session 当前是 `LIVE`，即使你传了 `--new`，tbox 也会优先切换到 live session，而不会从存档恢复。

### 删除存档：`tbox drop [name]`

```bash
tbox drop
tbox drop <session-name>
```

- 删除“存档文件”，不会影响 tmux 里的 live session。
- tmux 内：不传 `name` 时删除当前 session 的存档。
- tmux 外：必须显式传入 `name`。

### 列表：`tbox list`

```bash
tbox list
tbox list -v
tbox list --all
```

- 默认列出存档（ARCH）。
- `--all` 会把 live sessions 也一起列出来（需要 tmux server 可用）。
- `-v/--verbose` 会额外显示存档文件路径。

### 预览：`tbox preview <name>`

```bash
tbox preview <session-name>
```

读取该 session 的存档并打印简要结构（windows/panes）。

### 生成 tmux 配置：`tbox tmux-snippet`

```bash
tbox tmux-snippet
tbox tmux-snippet --throttle-seconds 3
tbox tmux-snippet --tbox-command tbox
```

- 输出一段可直接粘贴进 tmux 配置的 snippet。
- 默认使用解析到的可执行路径；通过 `--tbox-command` 可指定命令名或自定义路径。

## 存储

### 存储目录

- 默认：`~/.local/share/tmux-box`
- 覆盖：`TBOX_DIR=/path/to/dir`
- 也支持 `XDG_DATA_HOME`：默认目录为 `$XDG_DATA_HOME/tmux-box`

### 文件命名与格式

- 每个 session 一个 JSON 文件。
- 文件名形如：`<sanitized-name>-<sha1prefix>.json`（避免特殊字符 & 同名冲突）。
- 文件内容就是 `tmux-dump` 的输出 JSON；tbox 不改 schema。

## 选择器配置

- 默认优先使用：`fzf`，否则 `sk`，否则进入 prompt。
- 可通过 `TBOX_SELECTOR` 强制选择器：
  - `fzf` / `sk`: 强制使用该选择器（找不到时回退到输入）。
  - `none` / `prompt` / `builtin`: 强制使用输入模式。

## tmux 集成建议

### 推荐绑定（snippet 默认会生成）

- `prefix + W`：弹窗选择 session（live + archived）
- `prefix + X`：确认后执行：
  - `tbox save #{session_name}`
  - `tmux kill-session -t #{session_name}`

### hooks 触发点（snippet 默认会生成）

当前默认 hook 集合偏保守，配合 `--throttle-seconds` 使用：

- `client-session-changed`
- `client-detached`
- `session-renamed`
- `window-renamed`
- `window-layout-changed`

这些 hook 的目标不是“每次细节变化都保存”，而是尽量在用户可感知的切换/调整点把存档向内存态靠拢。

## 已知约束

- autosave 只保存“已命名”session（非纯数字）；如果你习惯用数字 session 当临时工作区，这是刻意的。
- `tbox select` 对 live session 永远优先切换，不会覆盖正在运行的会话。
- `tmux popup` 需要较新的 tmux 版本；如果你的 tmux 不支持 popup，可以改成绑定直接跑 `tbox select`（不弹窗）。

## 开发与测试

```bash
python -m unittest \
  bin/tests/test_tmux_load.py \
  bin/tests/test_tmux_dump.py \
  bin/tests/test_tbox.py \
  bin/tests/test_tbox_integration.py
```
