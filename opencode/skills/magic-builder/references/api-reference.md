# 妙笔 API 参考文档

## Table of Contents

- [运行时说明（HTML Box / FaaS）](#运行时说明html-box--faas)
- [HTML Box 高度与刷新](#html-box-高度与刷新)
- [多维表格插件能力](#多维表格插件能力)
- [当前用户信息](#当前用户信息)
- [查询当前用户发布的妙笔应用](#查询当前用户发布的妙笔应用)
- [根据 ID 获取用户信息](#根据-id-获取用户信息)
- [通用飞书 OpenAPI 调用](#通用飞书-openapi-调用)
- [获取文档基本信息](#获取文档基本信息)
- [获取文档全文评论](#获取文档全文评论)
- [文档存储（替代 localStorage）](#文档存储替代-localstorage)
- [运行时 URL 与页面控制](#运行时-url-与页面控制)
- [Magic Bar 顶部栏](#magic-bar-顶部栏)
- [获取文档内容（Markdown 格式）](#获取文档内容markdown-格式)
- [查询多维表记录](#查询多维表记录)
- [根据记录 ID 批量获取多维表记录](#根据记录-id-批量获取多维表记录)
- [写多维表](#写多维表)
- [更新多维表记录](#更新多维表记录)
- [AI 能力调用](#ai-能力调用)
- [FaaS 消息回复](#faas-消息回复)
- [TOS 分片上传](#tos-分片上传)

## 运行时说明（HTML Box / FaaS）

- HTML Box：可直接使用 `window.magic.*`。
- FaaS：运行时同样已注入 `window.magic` / `window.lark`（并提供全局 `magic` / `lark` 别名）。
- 建议：业务代码优先用 `window.magic.*`，跨环境调试时可保留 OpenAPI/LarkClient 兜底调用。

## HTML Box 高度与刷新

HTML Box 读取自定义 meta `html-box-height-mode` 决定宿主块高度策略；这是 HTML Box 协议，不是浏览器标准 meta。

HTML Box 在飞书文档中的常见展示宽度约 `820px`。页面应按文档容器适配，而不是按全屏浏览器适配：

- 根容器使用 `width: 100%`、`max-width: 100%`、`box-sizing: border-box`。
- 内容区可以设置 `max-width` 居中，但不能让正文、按钮、表格或图表依赖超宽屏幕。
- 避免用固定大宽度、负 margin、全屏绝对定位破坏文档流。
- 移动端和窄容器下，表格/工具栏要能换行、横向滚动或压缩列宽。

```html
<meta name="html-box-height-mode" content="auto">
```

- `auto`：文档流模式，适合文章、说明页、卡片、报表、表单、长页面。内容应进入普通文档流，根容器不要用 `overflow: hidden` 截断长内容。
- `viewport`：视口应用模式，适合幻灯片、Dashboard、游戏、画布、全屏工具和内部滚动应用。页面内部自行处理切页、滚动或缩放。

默认优先 `auto`。普通页面即使用 `min-height: 100vh` 做首屏美观，也应显式声明 `auto`；如果页面显示不全，先检查是否误用了 `viewport`、`height: 100vh` 或根容器 `overflow: hidden`。

全屏应用推荐显式声明：

```html
<meta name="html-box-height-mode" content="viewport">
```

初始化后 HTML Box 不会持续监听页面内部高度变化。动态加载图片、展开折叠、异步追加列表后，调用：

```JavaScript
await window.magic.updateHeight();
```

等价别名：

```JavaScript
await window.magic.refreshHeight();
await window.magic.resize();
```

如果页面使用 `vh/dvh/lvh/svh` 且根容器有 `overflow: hidden`，或多段 `100vh` 全屏 section 加 sticky/锚点/reveal 交互，HTML Box 可能自动按视口应用处理；有歧义时显式声明 `auto` 或 `viewport`。

不要把 `width` / `height` 写进 `add_ons.record` 并期待宿主按这些字段布局。HTML Box 的宽高由文档容器、HTML 文档流、`html-box-height-mode` 和宿主测量共同决定。

## 多维表格插件能力

> 文档：`https://lark-base-team.github.io/js-sdk-docs/`

获取 bitable 对象：`window.bitable`（等效于 `import { bitable } from '@lark-base-open/js-sdk'`）

## 当前用户信息

HTML Box 运行时会注入当前用户缓存：

```JavaScript
window.magic.currentUserInfo
window.magic.user
```

结构示例：

```JSON
{
  "open_id": "ou_xxx",
  "name": "用户名",
  "en_name": "English Name",
  "avatar_url": "https://..."
}
```

需要刷新登录态用户信息时，优先使用：

```JavaScript
const user = await window.magic.getCurrentUserInfo();
```

`getCurrentUserInfo()` 会通过父页面登录态请求当前用户，并回写 `window.magic.currentUserInfo` 和 `window.magic.user`。兼容方法 `window.magic.getCurrentUser()` 行为相同。

也支持兼容已有代码：

```JavaScript
const resp = await fetch('/api/me');
const json = await resp.json();
```

在 sandbox iframe 内，运行时会把 `/api/me` 请求代理到父页面执行，避免 iframe 请求无法携带登录 cookie。未登录时返回 401。`/api/me` 不返回 user access token；前端不要要求或保存用户 token。

## 查询/导出妙笔空间应用

用于列出当前登录用户或公开开源的妙笔空间 / Magic Space HTML Box 应用，支持按标题搜索，并可导出应用 HTML 代码。

CLI 优先使用：

```bash
magic-builder page list --title 项目看板
magic-builder page list --scope public --title 项目看板
magic-builder page export --id <id> --out ./project.html
magic-builder page export --title 项目看板 --out ./exports --all
```

HTTP 查询当前用户应用：

```HTTP
GET /api/html-box/apps?scope=mine&title=项目看板
```

HTTP 查询公开开源应用：

```HTTP
GET /api/html-box/apps?scope=public&title=项目看板
```

兼容参数：

- `scope=mine`、`scope=my`、`scope=private`、`scope=current_user`：查询当前用户应用。
- 标题搜索参数支持 `title`、`q`、`query`、`search`、`keyword`。
- 登录态优先使用 `session_id` cookie；脚本或外部调用可带 `Authorization: Bearer <token>`。
- `scope=mine` 返回当前用户有权导出的项目代码；`scope=public` 只返回公开开源项目的代码。

返回示例：

```JSON
{
  "code": 0,
  "data": {
    "open_id": "ou_xxx",
    "count": 1,
    "title_search": "项目看板",
    "records": [
      {
        "id": "xxx",
        "title": "项目看板",
        "html_preview": "<!doctype html>...",
        "isOpenSource": false,
        "modify_time": "2026-06-08T12:00:00.000Z",
        "modifier": "用户名"
      }
    ]
  }
}
```

当前接口每次最多返回 500 条；如果要保证超过 500 条的全量历史，需要在服务端补多维表分页。

## 根据 ID 获取用户信息

```TypeScript
async function window.magic.getUserInfoById(open_id: string);
```

返回结果：

```JSON
{
    "code": 0,
    "msg": "success",
    "data": {
        "user": {
            "open_id": "ou_7dab8a3d3cdcc9da365777c7ad535d62",
            "name": "张三",
            "en_name": "San Zhang",
            "nickname": "Alex Zhang",
            "avatar": {
                "avatar_72": "https://foo.icon.com/xxxx",
                "avatar_240": "https://foo.icon.com/xxxx",
                "avatar_640": "https://foo.icon.com/xxxx",
                "avatar_origin": "https://foo.icon.com/xxxx"
            }
        }
    }
}
```

## 通用飞书 OpenAPI 调用

HTML Box 中可以通过 `window.magic.api` 调用运行时已配置的 LarkClient API：

```JavaScript
const resp = await window.magic.api({
    id: "base_records_search",
    path_params: [app_token, table_id],
    body: { view_id, filter, sort },
    params: { page_size: 100 },
});
```

FaaS 中也注入了 `magic.api` / `lark.api`，当前快捷支持：

- `base_records_search`
- `base_records_get`
- `base_record_create`
- `base_record_update`

更通用或未配置的飞书 OpenAPI 调用，FaaS 中优先使用 `LarkClient`。

## 获取文档基本信息

```TypeScript
async function window.magic.getPageMeta();
```

> 注意：文档内 HTML Box runtime 提供 `window.magic.getPageMeta()`；独立 `/html-box/[id]` 页面或 FaaS 环境不一定提供，生成跨环境页面时要先判断方法是否存在。

返回数据示例：

```JSON
{
    "comments_count": 0,
    "create_timestamp": 1673426766,
    "pv": 1,
    "uv": 1,
    "owner_user": {
        "open_id": "ou_xxx",
        "cn_name": "名字",
        "avatar_url": "https://xxx"
    },
    "doc_token": "doccnfYZzTlvXqZIGTdAHKabcef",
    "title": "sampletitle",
    "owner_id": "ou_b13d41c02edc52ce66aaae67bf1abcef",
    "create_time": "1652066345",
    "latest_modify_user": "ou_b13d41c02edc52ce66aaae67bf1abcef",
    "latest_modify_time": "1652066345",
    "url": "https://sample.feishu.cn/docs/doccnfYZzTlvXqZIGTdAHKabcef",
    "sec_label_name": "L2-内部"
}
```

## 获取文档全文评论

```TypeScript
async function window.magic.doc_comments_get(doc_token);
```

> 注意：文档内 HTML Box runtime 提供该方法；独立 `/html-box/[id]` 页面或 FaaS 环境不一定提供，生成跨环境页面时要先判断方法是否存在。

返回数据结构包含 `items` 数组，每项含 `comment_id`、`user_id`、`quote`、`reply_list` 等字段。

## 文档存储（替代 localStorage）

> **严禁使用 `localStorage`**，必须使用以下接口。

| 作用域 | 私有数据（用户独享） | 共有数据（用户共享） | 权限要求 |
| --- | --- | --- | --- |
| 当前小组件独享 | `window.magic.store.get/set` | `window.magic.store.global_get/global_set` | 阅读权限 |
| 复制后共享 | `window.magic.redis.get/set` | `window.magic.redis.global_get/global_set` | 编辑权限 |

- **私有数据**：用户的数据读写仅自己可见，其他人无感知
- **共有数据**：所有用户共享，当前用户的操作会被其他浏览页面的用户看到
- 需要显示"有哪些人参与"类逻辑时，使用 `global_get/global_set` 接口

示例：

```JavaScript
// 私有数据
await window.magic.redis.set(key, value);
await window.magic.redis.get(key);

// 共享数据
await window.magic.redis.global_set(key, value);
await window.magic.redis.global_get(key);
```

## 运行时 URL 与页面控制

HTML Box runtime 会注入外层页面 URL 和 query 参数：

```JavaScript
window.magic.parentHref     // 外层 /html-box 页面 URL
window.magic.iframeHref     // iframe 自身真实 URL
window.magic.params         // Object.fromEntries(new URLSearchParams(search))
window.magic.searchParams   // 原始 query 字符串
```

常用页面控制：

```JavaScript
await window.magic.setFavicon("/favicon.png");
await window.magic.navigate("https://example.com");
await window.magic.reload();
```

`navigate` / `reload` 会尽量作用到外层页面，适合 sandbox iframe 中使用。

## Magic Bar 顶部栏

启用 `magic_bar=1` 或 `magic_bar=true` 后，页面可以控制顶部栏右侧自定义操作和主题：

```JavaScript
await window.magic.topbar.setActions([
    { id: "save", type: "button", label: "保存", variant: "primary" },
]);

await window.magic.topbar.setDarkMode(true);
await window.magic.topbar.setTheme("dark");

const off = window.magic.topbar.onAction((detail) => {
    if (detail.id === "save" && detail.event === "click") {
        save();
    }
});
```

顶层兼容别名：

```JavaScript
await window.magic.setTopbarItems(items);
await window.magic.clearTopbarItems();
await window.magic.setTopbarDarkMode(true);
await window.magic.setTopbarTheme("dark");
const off = window.magic.onTopbarAction(handler);
```

如果当前页面没有启用 Magic Bar，设置类方法会返回 `{ available: false }`。

## 获取文档内容（Markdown 格式）

```JavaScript
await window.magic.getDocAsMarkdown();
```

> 注意：文档内 HTML Box runtime 提供该方法；独立 `/html-box/[id]` 页面或 FaaS 环境不一定提供，生成跨环境页面时要先判断方法是否存在。

## 查询多维表记录

```TypeScript
await window.magic.base_records_search(app_token, table_id, view_id, filter, sort, page_token, page_size, field_names);
```

filter 字段说明见：`https://open.larkoffice.com/document/docs/bitable-v1/app-table-record/record-filter-guide`

分页说明：
- `page_token`：分页游标。第一页可传 `undefined`、`null` 或空字符串；后续请求传上一次返回的 `data.page_token`。
- `page_size`：每页记录数，例如 `100`、`500`。
- `field_names`：可选字段名数组。只传需要展示或计算的字段，能减少返回体积并提升大表查询速度。
- `data.has_more`：是否还有下一页。
- `data.page_token`：下一页分页游标，只有在继续请求下一页时使用。

请求示例：

```JavaScript
await window.magic.base_records_search("HaVFwEUN8iUUyVk4v6Nc7dbenOf", "tblYlY0M6sngLJjc", "vewaIuHxPl", {
    "conjunction": "and",
    "conditions": [{
        "field_name": "字段1",
        "operator": "is",
        "value": ["文本内容"]
    }]
}, [{
    "desc": true,
    "field_name": "多行文本"
}], undefined, 100, ["字段1", "多行文本"]);
```

分页拉取全部记录示例：

```JavaScript
async function fetchAllBaseRecords(appToken, tableId, viewId, filter, sort) {
    const all = [];
    let pageToken = undefined;

    while (true) {
        const resp = await window.magic.base_records_search(
            appToken,
            tableId,
            viewId,
            filter,
            sort,
            pageToken,
            500
        );

        if (resp.code !== 0) {
            throw new Error(resp.msg || "base_records_search failed");
        }

        all.push(...(resp.data.items || []));

        if (!resp.data.has_more) {
            break;
        }
        pageToken = resp.data.page_token;
    }

    return all;
}
```

返回数据结构：

```JSON
{
    "code": 0,
    "data": {
        "has_more": false,
        "items": [{
            "fields": {
                "数字字段": 96,
                "文本字段": [{ "text": "内容", "type": "text" }]
            }
        }],
        "page_token": "",
        "total": 1
    }
}
```

## 根据记录 ID 批量获取多维表记录

```TypeScript
await window.magic.base_records_get(app_token, table_id, record_ids);
```

返回结构：

```JSON
{
    "code": 0,
    "data": {
        "absent_record_ids": [],
        "forbidden_record_ids": [],
        "records": [{
            "fields": { "字段名称": "字段值" },
            "record_id": "recv5IqcPLqCR7"
        }]
    }
}
```

## 写多维表

```JavaScript
await window.magic.base_record_create(app_token, table_id, fields);
```

调用举例：

```JavaScript
await window.magic.base_record_create("MUPpbjdeRaOcF1sa1FkcqT1Tnpg", "tbl0OMllgriuo6Pt", {
    "中奖人": [{ id: "open_id" }],
    "奖品": "充电宝",
    "收件人": "阿毛",
    "联系电话": "12345678910",
    "邮寄地址": "北京市海淀区xxx",
});
```

## 更新多维表记录

```JavaScript
await window.magic.base_record_update(app_token, table_id, record_id, fields);
```

在 FaaS 中同样可以使用：

```JavaScript
await magic.base_record_update(app_token, table_id, record_id, fields);
```

调用举例：

```JavaScript
const resp = await window.magic.base_record_update(
    "MUPpbjdeRaOcF1sa1FkcqT1Tnpg",
    "tbl0OMllgriuo6Pt",
    "recv5IqcPLqCR7",
    {
        "状态": "已处理",
        "备注": "通过 window.magic 更新",
    }
);
```

`base_record_update` 只更新单条记录。`fields` 使用飞书多维表记录更新接口的字段格式，例如人员字段使用 `[{ id: "open_id" }]`。在 HTML Box 中调用时需要当前页面对目标多维表具备写权限；在 FaaS 中调用时需要服务端应用具备对应多维表权限。

## AI 能力调用

```JavaScript
await window.magic.ai({
    system: '请用简洁、准确的中文回答用户问题。',
    user: '请介绍 window.magic.ai 的用途。',
    temperature: 0.7,
    thinking: { type: 'disabled' },
    reasoning_effort: 'minimal',
})
// 返回: { code: 0, data: { result: 'AI返回的文本内容' } }
```

## FaaS 消息回复

FaaS runtime 提供 `magic.msg_reply(messageId, content)`，用于在消息事件或卡片回调链路里回复文本消息：

```JavaScript
await magic.msg_reply(messageId, "处理完成");
```

`messageId` 需要来自事件上下文或用户传入参数。只生成 HTTP 接口、链接预览或普通数据处理函数时，不要默认发送消息。

## TOS 分片上传

通过服务端代理 TOS 分片上传，绕过 16MB 请求体限制。

### 1) 初始化：POST `/api/tos/multipart/init`

```json
{ "filename": "video.mp4", "contentType": "video/mp4" }
```

返回 `uploadId`、`key`、`url`。

### 2) 上传分片：POST `/api/tos/multipart/part`

multipart/form-data，字段：`file`（Blob）、`uploadId`、`key`、`partNumber`（从 1 开始）。

返回 `partNumber`、`etag`。

### 3) 合并分片：POST `/api/tos/multipart/complete`

```json
{
    "uploadId": "xxxxx",
    "key": "uploads/1700000000000_video.mp4",
    "parts": [
        { "partNumber": 1, "etag": "\"9b2cf535...\"" },
        { "partNumber": 2, "etag": "\"0a6e4a1b...\"" }
    ]
}
```

### 4) 终止分片（可选）：POST `/api/tos/multipart/abort`

```json
{ "uploadId": "xxxxx", "key": "uploads/..." }
```

### 前端示例

```js
async function uploadMultipart(file, partSize = 10 * 1024 * 1024) {
    // 1) init
    const initResp = await fetch('/api/tos/multipart/init', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ filename: file.name, contentType: file.type })
    });
    const { data: { uploadId, key } } = await initResp.json();
    const parts = [];

    // 2) upload parts
    let partNumber = 1;
    for (let start = 0; start < file.size; start += partSize, partNumber++) {
        const fd = new FormData();
        fd.append('file', file.slice(start, start + partSize));
        fd.append('uploadId', uploadId);
        fd.append('key', key);
        fd.append('partNumber', String(partNumber));
        const partResp = await fetch('/api/tos/multipart/part', { method: 'POST', body: fd });
        const partJson = await partResp.json();
        parts.push({ partNumber, etag: partJson.data.etag });
    }

    // 3) complete
    const completeResp = await fetch('/api/tos/multipart/complete', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ uploadId, key, parts })
    });
    return (await completeResp.json()).data.url;
}
```

**注意事项**：
- `partNumber` 必须从 1 开始递增
- `parts` 必须包含所有分片的 `etag`，否则合并会失败
- 建议单片大小 5–10MB，避免超出 16MB 限制
