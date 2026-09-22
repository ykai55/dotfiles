---
name: task-watch
version: 1.0.0
description: "指导 agent 执行临时性的盯守/轮询任务：把等待放进脚本，agent 每次只读一行。当用户要求「盯着看」「等它跑完」「每隔几分钟查一次」「轮询直到成功」「watch / poll until done」「盯一天」等需要反复检查直到某条件达成的临时任务时使用。长期固定重复的定时任务应改用系统调度器（cron / launchd / systemd timer），不要用本 skill。"
metadata:
  requires:
    bins: ["task-watch"]
  cliHelp: "task-watch --help"
---

# Task Watch

用于**临时性、短期**的盯守任务：盯构建、盯同步、等某个状态变成目标值。核心只有一句话：**等待交给脚本，决策留给 agent**。agent 不自己 sleep，每次只读一行。

## 何时使用 / 不使用

使用：

- 用户要盯一个会自己变化的状态，直到达成条件或需要人判断。
- 任务只持续一次、几天以内，唤醒时间不精确也可以接受。

不使用：

- 长期固定重复的定时任务 → 用 `cron` / `launchd` / `systemd timer`。
- 一次性、立刻就能出结果的命令 → 直接跑。
- 需要精确到某时刻唤醒 → 用系统调度器。

## 核心原则

1. **等待写在脚本里。** 绝不让 agent 用 bash 工具挂 `sleep` 轮询：那样每轮唤醒都要重放完整上下文，token 随轮次线性增长。
2. **每次只让一行进上下文。** 脚本 stdout 只打一行结论，详细日志落文件；agent 只在需要时才 `tail` / `grep` 日志。
3. **信号走退出码。** 不让 agent 解析大段日志文本。
4. **必须有停止条件。** 每个任务都要有截止时间，不允许无限循环。
5. **job 幂等。** 中断后重跑一次即可续上，不产生重复副作用。

## 职责划分

| 脚本 | 职责 | 是否含 sleep |
| --- | --- | --- |
| `job.sh` | 做什么：检查一次 | 否 |
| `task-watch` | 等多久、多久看一次、何时放弃 | 是（循环在此） |
| agent | 读一行 + 退出码，做决定 | — |

`task-watch` 与任务无关，可跨任务复用；每个任务只需重写 `job.sh`。

## 退出码契约

`job.sh` 和 `task-watch` 共用同一套退出码：

| 码 | 含义 | task-watch 的行为 |
| --- | --- | --- |
| `0` | 目标达成 | 立即返回 0 |
| `1` | 尚未达成，继续等 | sleep 后重试；到截止时间返回 1 |
| `10` | 需要 agent 判断 | 立即返回 10 |
| `20` | 致命错误，停止 | 立即返回 20 |

`job.sh` 返回其它任何码，`task-watch` 一律按 `20` 处理（保守）。

## 关键约束：单次阻塞不能超过工具超时

`task-watch` 单次调用会阻塞一段时间，必须小于 `bash` 工具的超时，否则调用被中断。所以：

- 调用时把 bash 工具的 `timeout` 设为略大于 `--max-block`，例如 `--max-block 3300` 配 `timeout: 3600000`（毫秒）。
- 若工具拒绝该 timeout，就调小 `--max-block`。
- 到 `--max-block` 时 `task-watch` 返回 `1`，agent 再调一次即可。

**调大 `--max-block` 能显著减少 agent 轮次。** `--interval` 由脚本内部消化，改小它不会增加 agent 轮次，只影响检查粒度。

## agent 执行流程

1. 弄清**目标条件**和**总时长预算**。
2. 写 `job.sh`：幂等、无 sleep、stdout 一行、日志落文件、退出码符合契约。
3. 算一次截止时间（绝对 epoch 秒），后续每次调用都用同一个值：
   ```bash
   echo $(( $(date +%s) + 8*3600 ))   # 盯 8 小时
   ```
4. 调 `task-watch`，读它返回的那一行 + 退出码。
5. 按码分支：
   - `0` → 完成，收工。
   - `10` → 读日志定位问题，处理后决定是否继续盯。
   - `20` → 报错停止，把日志路径告诉用户。
   - `1` → 未到截止时间就再调一次；到了就收工并向用户汇报当前状态。
6. 每轮上下文只增加一行；需要细节时才 `tail -n 50 "$LOG"`。

## 停止条件与恢复

- 必须有 `--until`（绝对 epoch 秒）。它本身就是状态，agent 每次调用重复传同一个值，无需额外状态文件。
- `job.sh` 幂等，所以中断后重跑即可续。
- 任务存活依赖当前 session：session 关闭任务就停。重开会话后再调一次 `task-watch` 即可继续。

## task-watch 命令

`bin/task-watch`（已在 PATH 上）就是那个通用的等待循环，与任务无关，跨任务复用。

```text
task-watch --until <epoch秒> [--interval 60] [--max-block 540] -- <job 命令...>
```

- `--until`：绝对截止时间（epoch 秒），必填。
- `--interval`：两次检查之间的间隔秒数，默认 60。
- `--max-block`：单次调用最多阻塞的秒数，默认 540；必须小于调用方的超时。
- 它只把 job 的**最后一行** stdout 透传出来，并加上 `[task-watch] 完成 / 仍在运行 / 需要处理 / 错误` 前缀。
- `task-watch --help` 查看用法。

## 模板：job.sh

把中间那段检查逻辑替换成真实逻辑。

```bash
#!/usr/bin/env bash
# job.sh — 一次检查。幂等、无 sleep、stdout 只打一行结论。
# 退出码: 0 完成 / 1 继续等 / 10 需要 agent 判断 / 20 致命
set -uo pipefail

JOB_NAME=my-job
LOG="${JOB_LOG:-/tmp/${JOB_NAME}.log}"
log() { printf '%s %s\n' "$(date '+%F %T')" "$*" >>"$LOG"; }

# ↓↓↓ 替换成真实的一次检查 ↓↓↓
status=$(curl -sS -m 10 https://example.com/status 2>>"$LOG") \
  || { log "请求失败"; echo "请求失败，详见 $LOG"; exit 20; }

case "$status" in
  *'"state":"done"'*)    echo "完成: $(date '+%H:%M')"; exit 0 ;;
  *'"state":"running"'*) echo "运行中: $status"; exit 1 ;;
  *)                     log "未知状态: $status"; echo "异常，详见 $LOG"; exit 10 ;;
esac
```

## 示例：盯一个构建

```bash
# 1. 写 job.sh（检查构建状态，逻辑同上）
# 2. 算截止时间：盯 2 小时
until=$(( $(date +%s) + 2*3600 ))
# 3. 调用（bash 工具 timeout 设为 3600000ms）
task-watch --interval 60 --until "$until" --max-block 3300 -- ./job.sh
```

每次调用返回一行，例如 `[task-watch] 仍在运行: 运行中: {"state":"running","pct":42}`。agent 看到 `1` 就再调一次，看到 `0` 就收工，看到 `10` 才去读日志。
