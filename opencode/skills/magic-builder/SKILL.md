---
name: "magic-builder"
version: "2.0.8"
description: "统一妙笔 Magic Builder 技能：创建/更新/搜索/导出/发布妙笔网页与飞书文档 HTML Box，发布 HTTP/WSS FaaS，生成 Magic URL/飞书链接预览，上传 TOS 资源，创建云调调 API 调试文档和妙笔函数调试文档，记录妙笔用户反馈，检查/更新妙笔技能包。当用户提到妙笔、Magic Builder、Magic Page、HTML Box、妙笔空间、妙笔 FaaS、妙笔函数、云调调、API 调试文档、链接预览、妙笔资源上传、妙笔技能更新或妙笔使用反馈时触发；纯通用网页且未提妙笔时不要触发。"
---

# Magic Builder

本技能合并并替代旧技能：

- `generate-magic-page`
- `generate-magic-doc`
- `publish-magic-page`
- `publish-magic-faas`
- `magic-url-preview`
- `upload-file-to-tos`
- `magic-user-feedback`
- `check-magic-builder-update`
- `yundiaodiao-doc`

先判断用户意图，再进入对应模式。不要在一次任务里机械加载所有参考文件。

## 意图路由

| 用户意图 | 模式 |
|---|---|
| 创建妙笔网页、Magic Page、HTML/SPA、小游戏、表单、仪表盘、工具页面 | 生成妙笔网页 |
| 搜索、查找、列出、导出、拉取妙笔空间已有项目代码或已有 HTML Box 代码 | 搜索/导出妙笔空间项目 |
| 将妙笔网页写入飞书文档、创建/更新文档版妙笔应用、HTML Box | 飞书文档 HTML Box |
| 发布/部署/上线 HTML 到妙笔空间 | 发布妙笔网页 |
| 创建/更新 HTTP API、WebSocket/WSS、链接预览函数、后端逻辑 | 妙笔 FaaS |
| 为指定妙笔函数生成 API 调试飞书文档、函数调试文档、云调调调试台 | 妙笔函数调试文档 |
| 创建或更新云调调 API 调试文档、HTTP Method 调试模板、请求 JSON 后插入云调调 ISV 块 | 云调调 API 调试文档 |
| 生成妙笔链接、分享链接、`r?title`、`r?fid`、飞书链接预览 | 妙笔链接预览 |
| 上传文件获取 URL、处理 HTML 超 900000 字符的大图片/Base64/数据 | TOS 资源上传 |
| 妙笔报错、打不开、体验问题、功能建议、使用咨询，且未明确要求改代码/发布 | 用户反馈 |
| 检查/更新/安装最新版妙笔技能包 | 技能包更新 |

默认规则：

1. 用户反馈妙笔问题时，除非明确要求写代码、修仓库、发布或部署，否则先记录反馈。
2. 用户只要一个标题/一句文案的妙笔链接时，直接返回 `r?title=`，不要发布 FaaS。
3. 用户要页面、工具、小游戏、表单、仪表盘时，不要走链接预览，进入生成妙笔网页。
4. 用户要基于妙笔空间已有项目继续修改时，先搜索/导出已有项目代码，再在导出的 HTML 基础上修改。
5. 生成 HTML 后如用户要求发布，继续发布妙笔网页；如用户要求写入飞书文档，继续飞书文档 HTML Box。
6. 有后端、动态链接预览、WebSocket 或 OpenAPI 代调需求时，先生成/发布 FaaS，再把接口地址交给前端或用户。
7. 用户要“调试某个妙笔函数”时，优先生成云调调飞书文档，不要直接改函数代码；除非用户明确要求修复或发布函数。

## 生成妙笔网页

产物是完整单文件 HTML，适用于妙笔空间或飞书文档 HTML Box。

硬性约束：

- 禁止使用 `localStorage`；需要存储时用 `window.magic.store`（组件私有）或 `window.magic.redis`（文档内共享）。
- 使用 `window.lark`、`window.magic`、`DocMiniApp` 前必须判断存在；本地预览要提供轻量 Mock。
- 妙笔空间 HTML 发布上限是 900000 字符，按 10 个 `HTML 代码` 字段存储，每个字段最多写 90000 字符。不要内联大图片、Base64、长 JSON/CSV、字体文件或大量 mock 数据；先用 TOS/URL/CDN 外链化。
- 优先使用 `async/await`，只写必要注释。
- 需要输出代码时，只输出 Markdown 代码块中的完整 HTML，不加冗余解释。

允许的常用 CDN：

- `abcjs`: `https://fastly.jsdelivr.net/npm/abcjs@6.3.0/dist/abcjs-basic-min.js`
- `marked`: `https://cdn.jsdelivr.net/npm/marked/marked.min.js`
- `tailwindcss`: `https://cdn.tailwindcss.com`

HTML Box 宽高策略：

- HTML Box 在飞书文档里的常见展示宽度约 `820px`，不是全屏浏览器。页面根容器应按 `width: 100%`、`max-width: 100%`、`box-sizing: border-box` 和响应式布局编写；不要假设桌面全屏宽度。
- 默认使用文档流高度：文章、说明页、卡片、报表、表单、长页面、普通首屏展示都写 `<meta name="html-box-height-mode" content="auto">`。
- 普通页面即使用 `min-height: 100vh` 做首屏美观，也优先声明 `auto`；如展示不全，先检查是否误用了 `viewport`、`height: 100vh` 或根容器 `overflow: hidden`。
- 只有幻灯片、Dashboard、游戏、canvas 编辑器、单屏工具、内部滚动应用等固定视口体验，才写 `<meta name="html-box-height-mode" content="viewport">`。
- `viewport` 模式下页面要自己管理内部滚动、切页或缩放；不要把多段内容纵向全部撑开。
- 内容异步变高后可调用 `await window.magic.updateHeight()`；兼容别名 `refreshHeight()`、`resize()`。但常规 `auto` 页面应优先依赖文档流和宿主自动测量，不要把高度 API 当作布局基础。
- 不要把 `width` / `height` 塞进 `add_ons.record` 当作布局控制；文档 HTML Box 的宽高主要由 HTML 内容、`html-box-height-mode` 和宿主测量决定。

文档版 HTML 推荐骨架：

```html
<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="use-iframe" content="true">
  <meta name="html-box-height-mode" content="auto">
  <title>...</title>
</head>
<body>...</body>
</html>
```

当前用户信息优先使用：

```js
const cachedUser = window.magic?.currentUserInfo;
const user = await window.magic?.getCurrentUserInfo?.();
```

兼容旧接口：

```js
const user = await window.magic?.getCurrentUser?.();
const resp = await fetch('/api/me');
const json = await resp.json();
```

更完整的 API 见 `references/api-reference.md`。

## 飞书文档 HTML Box

用于新建文档、追加 HTML Box，或更新已有文档里的妙笔应用。

优先使用 CLI：

```bash
magic-builder doc create --html <html_file> --title <title> --summary <一句话介绍>
magic-builder doc append --html <html_file> --doc-token <docx_token>
```

关键约束：

- 新建 HTML Box 使用 `block_type: 40`。
- `add_ons.component_type_id` 固定为 `blk_6900429af84180025ce76527`。
- 新建时 `component_id` 传空字符串。
- `add_ons.record` 必须是 JSON 字符串，主入口是 `record.html`。
- 普通任务优先保持单文件 HTML；高级模板能力才使用 `record.json`、`record.js`、`record.scripts`。
- `record` 只承载运行时数据，例如 `html` / `json` / `js` / `scripts`。宽高不要写成 record 字段来驱动布局；宽度按文档容器响应式适配，高度由 HTML 内的 `html-box-height-mode` 决定。
- 创建文档和插入 HTML Box 必须使用同一身份；CLI 会用 `lark-cli auth status` 自动选择，也可传 `--as bot|user`。

更新已有 HTML Box 默认保留原 block/component 实例。优先在目标 HTML Box 正上方插入 HTML 代码块，并用自动更新标记触发运行时同步：

```html
<!-- html-box-code-auto-update -->
<!doctype html>
<html lang="zh-CN">...</html>
```

只有用户明确要求新建实例、确认旧数据可丢弃，或确认旧组件无用户数据时，才替换已有 HTML Box。不要 PATCH `add_ons.record`，该路径会返回 invalid param。

如果用户明确需要文档小组件宿主能力，例如展示用户名片，读取 `references/docminiapp.md`。

## 在 Aime 中使用

Aime 机器人可以使用妙笔技能包。安装包地址：

```text
https://magic-builder.tos-cn-beijing.volces.com/skills/magic-builder.skill.zip
```

如果 Aime 提示妙笔 `baseurl` 缺失，优先让用户把 Magic 平台服务根地址设置为：

```text
https://magic.solutionsuite.cn
```

注意：这里的 Base URL 是 Magic Builder 自身的服务根地址，用于发布 HTML、发布 FaaS、生成链接预览、上传 TOS 等平台能力；不要把具体接口路径拼进 Base URL。只有用户使用私有部署或测试环境时，才改为对应环境的根域名。

可直接让用户对 Aime/AI 这样说：

```text
请使用妙笔技能，Base URL 设置为 https://magic.solutionsuite.cn。这个 Base URL 是 Magic 平台服务根地址，不包含具体接口路径。
```

如果用户在 Aime 中安装技能包、设置 Base URL 后仍发布失败或调用失败，优先判断为 Aime 侧发布/执行链路不稳定，而不是技能包缺少 Base URL。建议排查顺序：

1. 确认已安装最新技能包。
2. 确认 Base URL 为 `https://magic.solutionsuite.cn`，无尾部路径。
3. 让用户切换到本地机器或其他稳定运行环境执行同一发布/调用任务。
4. 如果本地环境可用，向用户说明 Aime 环境可能存在偶发发布失败，可先用本地环境完成发布。

第三方 API / OpenAPI 技能的 Base URL 不适用上述固定值，应按接口文档提供接口根地址。例如完整接口是 `https://example.com/api/v1/chat/completions`，Base URL 是 `https://example.com/api/v1`，path 是 `/chat/completions`。

## 安装 Magic Builder CLI

本技能的平台操作统一使用 `magic-builder` CLI。若当前环境没有 `magic-builder`、`magic-cli` 或 `miaobi` 命令，先安装：

```bash
npm install -g magic-builder
```

使用 Bun 时也可以安装：

```bash
bun add -g magic-builder
```

安装后以下三个命令等价：

```bash
magic-builder --help
magic-cli --help
miaobi --help
```

不方便全局安装时，可用 `npx` 临时运行：

```bash
npx magic-builder --help
npx magic-builder skill install
```

使用 Bun 时可用 `bunx` 临时运行：

```bash
bunx magic-builder --help
bunx magic-builder skill install
```

## 执行环境总则

进入任何需要平台操作的模式前，先判断当前 Agent 是否能执行命令行。

### 情况一：能执行 CLI

优先使用 `magic-builder` CLI。CLI 会统一处理参数校验、Token 读取、请求重试、HTML 长度限制、本地 `magic-apps.json` 记录、TOS 分片上传等细节。

认证：

```bash
magic-builder auth login
magic-builder auth login --no-open --timeout 600 --interval 2
magic-builder auth set <user-token>
magic-builder auth show
```

常用操作：

```bash
magic-builder page publish <file.html> --title <title>
magic-builder page list --title <关键词>
magic-builder page export --id <id> --out <file_or_dir>
magic-builder page delete --id <id>

magic-builder faas publish <file.js> --name <name>
magic-builder faas list --title <关键词>
magic-builder faas delete --id <id>

magic-builder file upload <file>
magic-builder file list --title <关键词>
magic-builder file delete --id <id>

magic-builder doc create --html <file.html> --title <title>
magic-builder doc append --html <file.html> --doc-token <docx_token>
```

Token 查找优先级：

1. `--token`
2. `MAGIC_TOKEN`
3. `~/.magic-builder/magic-token`
4. 旧路径 `~/.magic-token`
5. 当前项目 `.magic-token`

配置路径：

- 用户级配置目录：`~/.magic-builder/`
- Token 文件：`~/.magic-builder/magic-token`
- 页面发布记录：`~/.magic-builder/magic-apps.json`

云端 Agent 能执行 CLI 但不能打开浏览器时，使用 `magic-builder auth login --no-open --timeout 600 --interval 2`，把输出的 `auth_url` 发给用户打开授权，Agent 保持轮询。成功后 Token 写入当前环境的 `~/.magic-builder/magic-token`；该文件是否跨会话保留取决于平台。

### 情况二：不能执行 CLI

如果 Agent 不能执行命令行，但能发 HTTP 请求，则直接调用 Magic HTTP API。所有需要登录的接口都优先从平台 Secret / 环境变量读取 `MAGIC_TOKEN`，并加请求头：

```http
Authorization: Bearer <MAGIC_TOKEN>
Content-Type: application/json
```

不能把 Token 明文写进对话、代码、日志或仓库；最终答复中也不要输出明文 Token。

无 Token 时的授权流程：

1. 创建授权会话：

   ```bash
   curl -s -X POST "${magic_base_url}/api/dev-token/auth/start"
   ```

   返回 `data.request_id`、`data.poll_token`、`data.auth_url`、`data.expires_in`。
2. 把 `auth_url` 发给用户在浏览器打开授权。
3. Agent 每 2 秒轮询领取 Token，最多等待 600 秒：

   ```bash
   curl -s -X POST "${magic_base_url}/api/dev-token/auth/token" \
     -H 'Content-Type: application/json' \
     -d '{"request_id":"<start 返回的 request_id>","poll_token":"<start 返回的 poll_token>"}'
   ```

   参数：

   - `request_id`：`/api/dev-token/auth/start` 返回的授权会话 ID。
   - `poll_token`：`/api/dev-token/auth/start` 返回的轮询凭证，只给命令行/Agent 使用。

   状态：

   - `pending`：用户还没完成授权，等待后继续轮询。
   - `completed`：返回 `token` / `open_id` / `name`，保存 `token` 到 Secret 或仅用于当前任务。
   - `expired`：会话过期，重新调用 `auth/start`。
   - `consumed`：Token 已被领取过，重新调用 `auth/start`。
   - `403 invalid poll_token`：轮询凭证不匹配，重新发起授权；不要猜测或复用别的会话凭证。

如果 Agent 不能执行 CLI、也不能发 HTTP 请求或不能保持轮询，则让用户在本地执行 `magic-builder auth login`，再把 Token 配到云端平台 Secret。不要要求用户提供飞书 `app_secret`，也不要把飞书 OAuth 回调里的 `code` 当作 Magic Token。

无 CLI 时的主要 HTTP API：

| 能力 | HTTP API |
|---|---|
| 发布新页面 | `POST /api/html-box`，body: `{ "html": "...", "title": "...", "is_open_source": true? }` |
| 更新页面 | `POST /api/html-box/{id}`，body 同发布 |
| 删除页面 | `DELETE /api/html-box/{id}` |
| 搜索应用 | `GET /api/html-box/apps?scope=mine|public&title=<关键词>` |
| 导出开源页面 | `GET /api/html-box/{id}` |
| 发布/更新 FaaS | `POST /api/faas`，body: `{ "code": "...", "name": "...", "id": "可选" }` |
| 列出 FaaS | `GET /api/faas?title=<关键词>` |
| 删除 FaaS | `DELETE /api/faas?id=<id>` |
| 小文件上传签名 | `POST /api/tos/sign`，body: `{ "filename": "...", "contentType": "...", "key": "可选" }` |
| 记录文件上传审计 | `POST /api/tos`，body: `{ "action": "record", "url": "...", "key": "...", "filename": "...", "contentType": "..." }` |
| 列出文件 | `GET /api/tos?title=<关键词>` |
| 删除文件 | `DELETE /api/tos?id=<id>` |

无 CLI 时不建议直接实现飞书文档 HTML Box 写入；除非当前 Agent 已有可用的飞书 OpenAPI 工具和用户/机器人授权。否则让用户在本地使用 `magic-builder doc create/append`，或切换到支持 `lark-cli` 的环境。

## 发布妙笔网页

使用 CLI 发布 HTML 到妙笔空间：

```bash
magic-builder page publish <file_path> [--title <title>] [--open-source] [--base-url <url>]
magic-builder auth login [--base-url <url>]
magic-builder auth set <user-token>
```

### 获取/保存开发 Token

发布私有妙笔网页、搜索/导出当前用户项目、发布妙笔 FaaS 时需要 Magic 开发 Token。优先使用网页授权流程获取，不要要求用户提供 `app_secret`。

推荐方式：

```bash
magic-builder auth login
```

流程：

1. CLI 调用 `${magic_base_url}/api/dev-token/auth/start` 创建授权会话。
2. CLI 输出并尝试打开飞书 OAuth 授权链接。
3. 用户在浏览器完成授权；回调页显示“授权成功，可以关闭这个页面”。
4. CLI 轮询 `${magic_base_url}/api/dev-token/auth/token`，领取一次性开发 Token。
5. CLI 把 Token 保存到 `~/.magic-builder/magic-token`。

非交互/远程环境无法打开浏览器时，也可以手动打开 CLI 输出的 `auth_url`。如果授权会话过期、已领取或轮询超时，重新运行 `magic-builder auth login`。

云端 Agent、不能打开浏览器、不能持久保存文件或不能执行 CLI 的环境，按“执行环境总则”选择 CLI 轮询、HTTP 轮询或本地登录后配置 Secret。不要要求用户提供飞书 `app_secret`，也不要把飞书 OAuth 回调里的 `code` 当作 Magic Token。

手动方式：

```bash
curl -s -X POST https://magic.solutionsuite.cn/api/dev-token/auth/start
# 打开返回的 auth_url 完成授权
curl -s -X POST https://magic.solutionsuite.cn/api/dev-token/auth/token \
  -H 'Content-Type: application/json' \
  -d '{"request_id":"...","poll_token":"..."}'
```

注意：

- 这里拿到的是 Magic 开发 Token，格式与在机器人私聊里发送“获取开发Token”得到的 Token 相同。
- 该 Token 不是飞书原始 `user_access_token`；不要把飞书 UAT 写入代码、日志或页面。
- `poll_token` 只给命令行使用，不要放进浏览器 URL、日志或公开消息。

Token 查找优先级：

1. `MAGIC_TOKEN`
2. `~/.magic-builder/magic-token`
3. 旧路径 `~/.magic-token`
4. 当前项目 `.magic-token`

如果没有 Token，先运行 `magic-builder auth login`；只有用户已经有 Token 字符串时，才使用 `magic-builder auth set <user-token>` 手动保存。

域名优先级：用户参数 `--base-url` / `--magic-base-url` -> `MAGIC_BASE_URL` -> `https://magic.solutionsuite.cn`。

发布前检查 HTML 长度。超过 900000 字符时停止发布，先把大资源用 TOS 上传并改为 URL 引用，再重新发布。

## 搜索/导出妙笔空间项目

用于查找当前用户或公开开源的妙笔空间 / Magic Space HTML Box 应用，并导出已有项目 HTML 代码，常见触发词包括“搜索已有项目”“导出项目代码”“拉取妙笔空间里的页面”“基于已有妙笔项目改”。

优先使用 CLI：

```bash
magic-builder page list --title <关键词>
magic-builder page list --scope public --title <关键词>
magic-builder page export --id <id> --out <html_file>
magic-builder page export --title <关键词> --out <directory> [--all]
magic-builder page delete --id <id>
```

Token 查找优先级与发布 CLI 一致：

1. `MAGIC_TOKEN`
2. `~/.magic-builder/magic-token`
3. 旧路径 `~/.magic-token`
4. 当前项目 `.magic-token`

如果导出当前用户私有项目时缺少 Token，先运行：

```bash
magic-builder auth login
```

规则：

- 默认 `scope=mine`，搜索当前登录用户发布的应用；需要查公开开源应用时传 `--scope public`。
- `list` 输出应用 ID、标题、开源状态、更新时间和访问 URL；需要机器可读结果时传 `--format json`。
- `export --id` 导出指定应用；`export --title` 导出标题匹配的第一个应用，传 `--all` 可导出全部匹配项。
- `delete --id` 删除当前登录用户自己的妙笔空间页面，并清理本地 `~/.magic-builder/magic-apps.json` 中匹配记录。
- 导出当前用户私有项目需要有效 `MAGIC_TOKEN`。公开开源项目可用 `--scope public` 导出。
- 导出的 HTML 可作为后续修改基础；修改完成后如用户要求发布，再进入“发布妙笔网页”模式。

## 妙笔 FaaS

用于 HTTP、WSS、HTTP+WSS、动态链接预览、服务端 OpenAPI 代调。

代码规范：

- 只能使用 CommonJS，禁止 `import/export`。
- HTTP handler 必须导出 `module.exports = async function (request, context) { ... }`，返回 `Response`。
- WSS handler 必须导出 `module.exports.ws = async function (ws, req, context) { ... }`。
- 可用模块：`http`、`https`、`crypto`、`fs`、`path`、`buffer`、`zlib`、`uuid`、`ws`。
- 禁止输出或记录密钥、token、cookie。
- 飞书 OpenAPI 调用优先使用运行时注入的 `magic.*` 快捷方法；复杂 OpenAPI 再读取 `references/larkclient.md`。

类型判断：

| 用户描述 | 类型 |
|---|---|
| 返回 JSON、处理请求、调用 API、定时任务 | HTTP |
| 实时通信、WebSocket、WSS、长连接、推送 | WSS |
| 同时需要接口和长连接 | HTTP+WSS |
| 动态链接标题/图片、`url.preview.get`、飞书链接预览 | HTTP + link preview |
| 只有标题/一句文案的妙笔链接 | 不发布，直接 `r?title=` |

发布 CLI：

```bash
magic-builder faas publish <CODE_FILE> --name <NAME> [--id <RECORD_ID>] [--base-url <URL>]
magic-builder faas list [--title <关键词>] [--base-url <URL>]
magic-builder faas delete --id <RECORD_ID> [--base-url <URL>]
```

默认从 `--token`、`MAGIC_TOKEN`、`~/.magic-builder/magic-token` 等位置读取 Magic 开发 Token。缺少 Token 时先运行：

```bash
magic-builder auth login
```

用户未提供 `name` 时，根据需求自动生成 2-5 个关键词的 snake_case 名称，不要停下来询问。

发布成功后返回：

- `record_id`
- `faas_url`
- 链接预览场景返回 `preview_url`: `${magic_base_url}/r?fid={id}`
- WSS 场景返回 `wss_url`: `${magic_ws_base_url}/api/faas/{id}`

## 妙笔链接预览

用于生成飞书里可展开预览的 URL。

执行策略：

1. 确定 `magic_base_url`：用户参数 -> `MAGIC_BASE_URL` -> `https://magic.solutionsuite.cn`。无协议域名补 `https://`，去掉末尾 `/`。
2. 只有标题/一句文案，且没有摘要、图标、数据源、动态计算、缓存策略时，直接返回：

   ```text
   ${magic_base_url}/r?title=${encodeURIComponent(title)}
   ```

3. 需要摘要、图标、数据源、动态逻辑或明确要求 FaaS 时，生成链接预览 FaaS，再用发布 FaaS 模式获取 `fid`。
4. 需要图标库匹配、内置 oncall / byteworks / bitable 数据源或通用预览模板时，读取 `references/preview-generator-template.md`。

最终答复：

- 直链场景只返回 `direct_url`，不要编造 `record_id`。
- FaaS 场景返回最终可发送链接：`${magic_base_url}/r?fid={fid}`，并附 `record_id` / `faas_url`。

## 云调调 API 调试文档

用于生成飞书文档里的 API 调试说明，常见触发词包括“云调调”“API 调试文档”“接口调试模板”“HTTP Method 调试”“在请求 JSON 后插入云调调 ISV 块”。

必须配合 `lark-cli` / `lark-doc` / `lark-shared` 能力使用；创建或编辑飞书文档时显式使用 docx v2 API，默认使用当前用户身份。如果当前环境不能执行 `lark-cli` 或缺少飞书文档写权限，不要伪造 ISV 块；说明需要可执行 `lark-cli api` 且有 docx 写入权限。

文档结构：

1. 模板引用 blockquote。
2. 文档标题和说明。
3. 每个接口按 `Method + URL`、请求 JSON、云调调 ISV 块、调用结果、响应 JSON 排列。
4. 文末追加“HTTP Method 语义配色方案”表。

### 固定云调调 ISV 块

云调调调试台是飞书 ISV 块，不要用普通文本或占位文案替代。

固定参数：

- `block_type`: `40`
- `add_ons.component_id`: `""`
- `add_ons.component_type_id`: `blk_62ba7256b241c0012c919542`
- `add_ons.record`: `"{}"`

通过普通 XML 写 `<readonly-block type="isv">` 可能会被飞书忽略；创建真实云调调块时必须调用 docx block create API：

```bash
bun -e 'process.stdout.write(JSON.stringify({
  children: [{
    block_type: 40,
    add_ons: {
      component_id: "",
      component_type_id: "blk_62ba7256b241c0012c919542",
      record: "{}"
    }
  }],
  index: INSERT_INDEX
}))' | lark-cli api POST /open-apis/docx/v1/documents/DOCUMENT_ID/blocks/PARENT_BLOCK_ID/children \
  --as user --data - --format json
```

`PARENT_BLOCK_ID` 通常是文档根 block，也就是 `DOCUMENT_ID`。`INSERT_INDEX` 是根 block children 数组里的插入位置。

### 请求 JSON 规则

每个请求 JSON 都写成代码块，内容必须包含顶层字段：

- `path_params`
- `params`
- `headers`
- `output_id`：默认为空字符串；为空时云调调小组件会使用随机 6 位字符串作为响应代码块 caption
- `body`

示例：

```json
{
  "path_params": {},
  "params": {},
  "headers": {
    "Accept": "application/json"
  },
  "output_id": "",
  "body": {}
}
```

不要把响应 JSON 误判为请求 JSON；只有包含 `path_params`、`params`、`headers` 和 `body` 四个字段的代码块后需要插入云调调 ISV 块。

### Method 按钮

Method 标签必须使用 OpenDocVerse 按钮，不要用普通 `<span>` 标签。

按钮 payload 规则：

- `blockTypeID`: `blk_6900429af84180025cda8cc1`
- `id`: `random_` 开头的随机值
- `button_id`: 与 `id` 相同
- `button_type`: HTTP Method 值，例如 `GET`、`POST`

示例：

```xml
<button action="OpenDocVerse" background-color="rgb(225,234,255)" src="{&quot;blockTypeID&quot;:&quot;blk_6900429af84180025cda8cc1&quot;,&quot;id&quot;:&quot;random_0123abcd&quot;,&quot;button_id&quot;:&quot;random_0123abcd&quot;,&quot;button_type&quot;:&quot;GET&quot;}">GET</button>
```

统一使用以下语义配色：

| Method | 按钮背景色 | 语义 |
|---|---|---|
| `GET` | `rgb(225,234,255)` | 读取资源，低风险查询 |
| `HEAD` | `rgb(217,243,253)` | 读取响应头，轻量探测 |
| `OPTIONS` | `rgb(236,226,254)` | 发现服务能力 |
| `POST` | `rgb(217,245,214)` | 创建或提交资源 |
| `PUT` | `rgb(254,234,210)` | 整体替换资源 |
| `PATCH` | `rgb(236,226,254)` | 局部更新资源 |
| `DELETE` | `rgb(253,226,226)` | 删除资源，高风险 |
| `CONNECT` | `rgb(217,243,253)` | 建立隧道，普通 API 不常开放 |
| `TRACE` | `rgb(253,221,239)` | 诊断回环，公网通常禁用 |

文末“HTTP Method 语义配色方案”表中的 Method 也使用同一套按钮。

### 推荐工作流

1. 用 `lark-cli docs +create --api-version v2 --as user --content - --format json` 创建正文骨架，或用 `lark-cli docs +patch` 更新已有文档。
2. 正文中先创建 Method 按钮、说明、请求 JSON、调用结果和响应 JSON。
3. 创建后用 docx block list 拉取块结构：

   ```bash
   lark-cli api GET /open-apis/docx/v1/documents/DOCUMENT_ID/blocks \
     --params '{"page_size":500}' --as user --format json
   ```

4. 找出根 block 直属 children 中所有请求 JSON 代码块：`block_type == 14`，且 `code.elements` 文本包含 `"path_params"`、`"params"`、`"headers"` 和 `"body"`。
5. 按 children index 从大到小，在每个请求 JSON 代码块后插入云调调 ISV 块。倒序插入可以避免前面的插入操作改变后续 index。
6. 插入后再次 fetch 或 list，确认每个请求 JSON 后都有 `block_type: 40` 且 `add_ons.component_type_id` 为 `blk_62ba7256b241c0012c919542`。

Endpoint 选择：

- 优先选择公开、稳定、可在线测试的 endpoint。
- 常规 REST 方法优先覆盖：`GET`、`POST`、`PUT`、`PATCH`、`DELETE`、`HEAD`、`OPTIONS`。
- `TRACE` 常被公网服务禁用，通常记录为 `405` 或受限说明。
- `CONNECT` 是代理隧道语义，不适合作为普通 REST API endpoint，通常放入受限说明。
- 生成文档前尽量用 `curl` 简单验证 endpoint 可访问，并把实际状态码写入示例结果。

## 妙笔函数调试文档

用于根据指定妙笔函数生成一篇可直接调试该函数 API 的云调调飞书文档。常见触发词包括“给这个妙笔函数生成调试文档”“调试某个 FaaS”“为指定函数生成云调调文档”“妙笔函数 API 调试”。

输入可以是：

- 妙笔函数 `record_id`
- 妙笔 FaaS URL，例如 `${magic_base_url}/api/faas/{id}`
- 链接预览 URL，例如 `${magic_base_url}/r?fid={id}`
- 函数名称或标题关键词

处理步骤：

1. 解析函数 ID。`/api/faas/{id}` 直接取路径 ID；`/r?fid={id}` 取 `fid`；只有标题或关键词时先 `magic-builder faas list --title <关键词> --format json` 查找候选。
2. 读取函数信息。能执行 CLI 时优先：

   ```bash
   magic-builder faas list --title <关键词> --format json
   ```

   如果已知 ID 但 CLI 没有详情命令，优先通过 Magic HTTP API 拉取同一条 FaaS 记录；缺少详情接口时至少使用已知 URL 生成通用调试模板，并说明需要用户补充请求参数。
3. 推断接口形态：
   - HTTP 函数：生成 `GET`、`POST`、`PUT`、`PATCH`、`DELETE`、`OPTIONS` 等调试小节；默认 URL 是 `${magic_base_url}/api/faas/{id}`。
   - 链接预览函数：额外生成 `${magic_base_url}/r?fid={id}` 预览调试说明；API 调试仍以 `/api/faas/{id}` 为主。
   - WSS 函数：说明云调调主要调试 HTTP API；WSS 给出连接地址 `${magic_ws_base_url}/api/faas/{id}` 和握手参数说明，不强行插入 HTTP 调试块。
4. 生成云调调文档骨架。每个请求 JSON 后必须插入 `blk_62ba7256b241c0012c919542` 云调调 ISV 块；Method 标签使用 `blk_6900429af84180025cda8cc1` 语义色按钮。
5. 如果能从函数代码或描述中识别参数、headers、鉴权方式、body schema，就写入对应请求 JSON；不能识别时使用空对象，并在说明中标注“待补充”。
6. 创建或更新飞书文档后，用 docx block list 验证请求 JSON 后的 ISV 块没有遗漏。

默认请求 JSON 模板：

```json
{
  "path_params": {},
  "params": {},
  "headers": {
    "Accept": "application/json"
  },
  "output_id": "",
  "body": {}
}
```

POST/PUT/PATCH 请求如果无法推断业务字段，`body` 先写 `{}`，不要编造敏感参数、token、cookie 或真实用户数据。

无 CLI 时：

- 能发 HTTP 请求且有 `MAGIC_TOKEN`：调用 Magic HTTP API 查询 FaaS 列表或详情，再按云调调文档流程生成飞书文档。
- 不能发 HTTP 请求：让用户提供函数 ID、函数 URL、请求参数和目标飞书文档；不要声称已查询到函数详情。

验证清单：

1. 文档标题能明确对应函数名称或 ID。
2. 所有请求 JSON 后都有同级云调调 ISV 块，`component_type_id` 是 `blk_62ba7256b241c0012c919542`。
3. 所有 Method 标签都是 OpenDocVerse 按钮，payload 里的 `button_type` 等于该 Method。
4. 响应代码块标题设置为 `Response`；如果请求 JSON 的 `output_id` 为空，由云调调小组件生成响应 caption。
5. 不把 Method 按钮组件 `blk_6900429af84180025cda8cc1` 误用于请求 JSON 后的调试台 ISV 块。

## TOS 资源上传

用于两类任务：

- 生成一个妙笔文件上传页面。
- CLI 上传本地文件，获取公开 URL，用于 HTML 超限资源外链化。

CLI 上传：

```bash
magic-builder file upload <file_path> [--key <tos-key>] [--content-type <mime>] [--base-url <url>] [-q]
magic-builder file list [--title <关键词>] [--base-url <url>]
magic-builder file delete --id <id> [--base-url <url>]
```

上传流程：

- 文件 <= 16MB：`/api/tos/sign` 获取预签名 URL，再 PUT 上传。
- 文件 > 16MB：`/api/tos/multipart/init`、`part`、`complete` 分片上传，每片 10MB。
- `list/delete` 管理当前登录用户通过 CLI 上传的文件记录；删除会删除 TOS 对象，并在审计记录中标记 `已删除=是`。

生成上传页面时，页面必须支持拖拽/选择文件、批量上传、进度条、上传成功链接和复制按钮；文档嵌入使用 `html-box-height-mode: auto`，队列变化后刷新高度。

## 用户反馈

用户反馈妙笔问题且没有明确要求实现时，整理并写入反馈表。

目标表：

`https://bytedance.larkoffice.com/wiki/Ia5xwTdUmiQu8skc3C3cSQgEnmb?table=tblyfZ7z5J0Ujqs4&view=vewql5Ifjo`

整理字段：

- 标题：40 字以内。
- 整理后问题：问题、影响场景、用户期望。
- 分类：`Bug/报错`、`功能建议`、`体验问题`、`使用咨询`、`权限/登录`、`性能/稳定性`、`其他`。
- 优先级：默认 `P2`；主链路不可用/数据丢失/发布失败用 `P1`；轻微体验或咨询用 `P3`。
- 状态：默认 `待处理`。
- 来源：默认 `飞书用户反馈`。
- 产品：默认 `妙笔`。

写入 CLI：

```bash
magic-builder feedback create \
  --feedback-file /tmp/magic-feedback.txt \
  --title "一句话标题" \
  --summary "整理后的问题、影响和期望" \
  --category "Bug/报错" \
  --priority "P2" \
  --reporter-name "<发送者姓名>" \
  --reporter-open-id "<发送者 open_id>" \
  --session-id "<botmux session_id>"
```

如果用户说“上面那个问题”等依赖上下文表达，先用 `botmux history --session-id <session_id> --limit 20` 补齐上下文。写入失败时不要伪造成功，报告失败原因并说明已整理好待补录。

## 技能包更新

固定地址：

- 更新包：`https://magic-builder.tos-cn-beijing.volces.com/skills/magic-builder.skill.zip`

妙笔技能包不再发布独立 `index.json`。检查更新时直接下载 zip，并读取包内 `magic-builder/SKILL.md` 的 `version`。

检查：

```bash
magic-builder skill check-update --environment auto --format json
```

安装：

```bash
magic-builder skill install --environment local
```

本地更新：

```bash
magic-builder skill update --environment local
```

云端环境不要复制或覆盖 `~/.codex/skills` 或仓库 `skills/`。使用云端托管 skill update/install 机制；若当前运行时没有暴露该能力，只报告 cloud update descriptor，不要声称已更新。

更新完成后告知用户重新触发妙笔技能，以便会话加载新版元数据。

## 参考资料

- `references/api-reference.md`：Magic runtime、TOS、HTML Box API。
- `references/docminiapp.md`：仅在需要飞书文档小组件宿主能力时读取。
- `references/larkclient.md`：仅在 FaaS 需要直接调用飞书 OpenAPI 时读取。
- `references/preview-generator-template.md`：仅在生成复杂链接预览 FaaS 时读取。
