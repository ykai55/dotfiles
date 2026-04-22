---
name: paste
version: 1.0.0
description: "读取当前 macOS 系统剪切板，支持文本、图片、Finder 文件/文件夹、URL 等内容，并把剪切板中的图片导出成临时文件供后续分析。当用户明确要求查看/使用当前剪切板，或说“我刚复制了...”“直接用我剪切板里的截图/文件/文本”“帮我看看 clipboard / 剪切板里是什么”时，应主动使用这个 skill；不要在用户未明确授权时读取剪切板。"
metadata:
  requires:
    bins: ["clipboard-read"]
  cliHelp: "clipboard-read --help"
---

# paste

这个 skill 用来安全地读取当前机器的系统剪切板，并把结果转换成 agent 易处理的结构化 JSON。

## 何时使用

- 用户明确要求读取当前剪切板。
- 用户表示内容已经复制好了，希望你直接使用，例如“我刚复制了一张截图”“直接分析我剪切板里的文件”。
- 用户让你粘贴、查看、总结、分析当前 clipboard / 剪切板内容。

不要在以下场景使用：

- 用户只是让你“复制”某段内容到剪切板。
- 用户没有明确同意读取当前剪切板。

## 工作流

1. 先运行 `clipboard-read --pretty`。
2. 解析返回 JSON。
3. 按 `items[].kind` 处理结果：
   - `text`: 直接引用、总结，或把文本继续用于后续任务。
   - `image`: 图片默认已经导出到 `/tmp/opencode-clipboard/` 下的固定文件；如果用户要你识别图片内容，再用 Read 工具读取返回的 `path`。
   - `files`: 返回的是 Finder 中复制的文件/文件夹路径；只有在后续任务需要时再去读取这些文件。
   - `urls`: 作为普通 URL 继续处理。
4. 如果 `empty` 为 `true`，明确告诉用户当前剪切板里没有可识别的文本、图片、文件或 URL。

## 输出结构

`clipboard-read` 返回 JSON，关键字段如下：

- `pasteboardTypes`: macOS 原始剪切板类型。
- `outputDir`: 导出图片时使用的目录；默认是 `/tmp/opencode-clipboard`，没有图片时通常为 `null`。
- `items[].kind`: `text` / `image` / `files` / `urls`。
- `items[].path`: `image` 类型下导出的本地图片路径。
- `items[].files`: `files` 类型下的文件列表，包含路径、名称、目录标记、大小。
- `items[].text`: `text` 类型下的文本内容。

## 响应原则

- 默认先简洁汇报识别到的类型，再给出具体内容或下一步结果。
- 剪切板可能包含敏感信息；除非用户需要完整内容，否则优先摘要而不是整段复述。
- 当剪切板里同时包含多种表示形式时，优先保留全部检测结果，不要擅自丢弃其中一种。

## 示例

用户说：`我刚复制了一张截图，帮我看一下上面报的什么错`

建议流程：

1. 运行 `clipboard-read --pretty`
2. 找到 `kind == "image"` 的条目
3. 用 Read 工具读取 `path`
4. 基于图片内容继续回答

用户说：`我刚在 Finder 里复制了两个文件，把路径告诉我`

建议流程：

1. 运行 `clipboard-read --pretty`
2. 找到 `kind == "files"` 的条目
3. 直接返回 `items[].files[].path`
