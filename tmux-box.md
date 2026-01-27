# tbox

基于 tmux-load/tmux-dump 实现的 tmux 会话持久管理器。

## Usage

```
tbox push # save current session, must in tmux session or provider --session argument

tbox select # using a interactive selector to select which stored session to restore

tbox drop # select which stored session to drop
```

