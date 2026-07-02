# LarkClient 使用说明

本文档说明妙笔云函数内置的 `LarkClient`如何配置与调用飞书开放平台 API，并给出在本项目中常用的调用范式（多维表格、IM 等）。

## Table of Contents

- [1. LarkClient 是什么](#1-larkclient-是什么)
- [2. 最常见用法（在 API Route 里）](#2-最常见用法在-api-route-里)
- [3. 构造参数说明](#3-构造参数说明)
- [4. Token 获取（updateToken）](#4-token-获取updatetoken)
- [5. 常用场景示例](#5-常用场景示例)
- [6. 代理模式（无 app_secret）](#6-代理模式无-app_secret)
- [7. 注意事项（强烈建议）](#7-注意事项强烈建议)

## 1. LarkClient 是什么

`LarkClient` 是一个轻量封装：

- 通过 `app_id + app_secret` 自动获取 tenant_access_token（内部应用）并携带 `Authorization: Bearer <token>` 调用飞书 OpenAPI
- 通过 “apis 配置表” 的形式，把 `id` 映射到某个 OpenAPI 的 `{url, method, body/params 默认值}`，再用统一的 `lark.api({ id, path_params, params, body })` 调用
- 内置了一些分页/重试逻辑：当 `GET` 或传了 `params.page_size` 时会自动循环拉取分页并合并 items
- 支持一种“代理模式”：当没有 `app_secret` 但传了 `app`（pass_ticket）时，会把请求转发到 `${magic_base_url}/api/lark`（默认 `https://magic.solutionsuite.cn/api/lark`，用于某些场景的中转）

## 2. 最常见用法（在 API Route 里）

项目里最常见的使用方式是：

1) 设置 `LARK_APP_ID / LARK_APP_SECRET`  
2) new 一个 `LarkClient`，并在 `apis` 里声明你要用的 API  
3) 用 `(lark.api as any)({ id, path_params, params, body } as any, undefined)` 调用

示例（以多维表格创建记录为例）：

```ts
const lark = new LarkClient({
  app_id: LARK_APP_ID,
  app_secret: LARK_APP_SECRET,
  is_isv: false,
  tenant_key: "",
  token: "",
  open_id: "",
  headers: "",
  context: {},
  apis: [
    {
      id: "base_record_create",
      url: "https://open.larkoffice.com/open-apis/bitable/v1/apps/:app_token/tables/:table_id/records",
      method: "POST",
      body: [{ param: "fields" }],
    },
  ],
});

const app_token = process.env.BITABLE_APP_TOKEN;
const table_id = process.env.BITABLE_TABLE_ID;

if (!app_token || !table_id) {
  throw new Error("Missing BITABLE_APP_TOKEN or BITABLE_TABLE_ID");
}

const resp = await (lark.api as any)({
  id: "base_record_create",
  path_params: [app_token, table_id],
  body: { fields: { 名称: "test" } },
} as any, undefined);
```

## 3. 构造参数说明

### 3.1 new LarkClient({ ... })

常用字段：

- `app_id`：飞书应用 app_id
- `app_secret`：飞书应用 app_secret（内部应用用）
- `is_isv`：是否 ISV（项目里大多数路由传 `false`）
- `tenant_key`：ISV 场景使用
- `token`：可显式指定 token（一般不传，让它自动 `updateToken()`）
- `open_id`：部分用户 token 刷新逻辑会用到（项目里多数不依赖）
- `headers`：额外透传的 headers（会合并到请求头里）
- `context`：代理模式会把它透传给 `/api/lark`
- `apis`：API 配置数组（核心）

### 3.2 apis 配置项结构

每个 API 配置最少包含：

- `id`：你自己定义的唯一标识，后续调用 `lark.api({ id: "xxx" })`
- `url`：OpenAPI 绝对地址（允许 `:param` 形式占位）
- `method`：`GET/POST/PUT/DELETE/PATCH`

可选字段：

- `body`：用于声明 body 中允许/需要的字段，并支持默认值/分片上限：
  - `[{ param: "fields" }]` 表示 body 需要 `fields`
  - `[{ param: "record_ids" }]` 表示 body 需要 `record_ids`
  - `[{ param: "xxx", default: <value> }]` 可填默认值
  - `[{ param: "xxx", max_length: 500 }]` 会自动按 max_length 分批调用（见 [api](file:///Users/amoblin/MyDocuments/Lark/docai/app/lib/lark_client.ts#L138-L183)）
- `params`：用于声明 query 参数的默认值（会与 `api()` 传入的 `params` 合并）

### 3.3 lark.api({ ... })

签名（核心字段）见 [api](file:///Users/amoblin/MyDocuments/Lark/docai/app/lib/lark_client.ts#L138-L183)：

- `id`：匹配 `apis` 里的 `id`
- `path_params`：用于替换 URL 里的 `:xxx` 或 `%s` 占位符（按顺序替换）
- `params`：query 参数对象
- `body`：POST/PUT/PATCH/DELETE 的请求体
- `headers`：临时覆盖/追加 headers（会写回 `this.headers`）
- `token`：显式指定 token（可选）
- `retry`：重试次数（默认 3）

返回值：

- 基本是飞书 OpenAPI 的标准返回：`{ code, msg, data }`
- 分页拉取时可能返回聚合后的结构（items 合并）

## 4. Token 获取（updateToken）

内部应用最常见的 token 获取方式在 [updateToken](file:///Users/amoblin/MyDocuments/Lark/docai/app/lib/lark_client.ts#L1033-L1106)：

- 调用 `POST /open-apis/auth/v3/tenant_access_token/internal/`
- 成功后会把 `this.token` 设为 `tenant_access_token`

你通常不需要手动调用 `updateToken()`：`sendRequest()` 在缺 token 时会自动触发。

## 5. 常用场景示例

### 5.1 多维表格：创建记录（base_record_create）

```ts
const resp = await (lark.api as any)({
  id: "base_record_create",
  path_params: [app_token, table_id],
  body: { fields: { 标题: "Hello", 开发者: [{ id: open_id }] } },
} as any, undefined);
```

### 5.2 多维表格：更新记录（base_record_update）

```ts
const resp = await (lark.api as any)({
  id: "base_record_update",
  path_params: [app_token, table_id, record_id],
  body: { fields: { 标题: "Updated" } },
} as any, undefined);
```

### 5.3 多维表格：查询记录（base_records_search）

`base_records_search` 通常需要 `view_id/field_names` 等字段；项目里也会用 `page_size` 拉分页。

```ts
const resp = await (lark.api as any)({
  id: "base_records_search",
  path_params: [app_token, table_id],
  params: { page_size: 500 },
  body: { view_id, field_names },
} as any, undefined);
```

### 5.4 IM：发消息（im_message_send）

你需要先在 `apis` 里声明 `im_message_send`，再按飞书接口格式传 body。

```ts
const resp = await (lark.api as any)({
  id: "im_message_send",
  params: { receive_id_type: "open_id" },
  body: {
    receive_id: open_id,
    msg_type: "text",
    content: JSON.stringify({ text: "hello" }),
  },
} as any, undefined);
```

## 6. 代理模式（无 app_secret）

当 `this.app_secret` 为空且传入了 `app`（pass_ticket）时，`sendRequest()` 会把请求签名后转发到：

- `${magic_base_url}/api/lark`（默认 `https://magic.solutionsuite.cn/api/lark`）

实现见 [sendRequest](file:///Users/amoblin/MyDocuments/Lark/docai/app/lib/lark_client.ts#L626-L659)。

你一般在服务端路由中使用 `app_id/app_secret` 的内部应用模式；只有在特定环境/历史兼容时才会走代理模式。

## 7. 注意事项（强烈建议）

- 不要在日志中输出 `app_secret`、token、cookie、用户敏感信息
- `apis` 的 `id` 需要全局唯一（在同一个 LarkClient 实例内）
- `path_params` 替换是按顺序进行的，传错顺序会导致 URL 拼错
- 对于可能超长的字段（如批量 id、长文本），可用 `body: [{ param, max_length }]` 让 LarkClient 自动分片调用
