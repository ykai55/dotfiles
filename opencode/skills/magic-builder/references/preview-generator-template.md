# Magic URL Preview Generator Template

## Table of Contents

- [Purpose](#purpose)
- [Template](#template)

## Purpose

This reference stores the full executable generator template used by the `magic-url-preview` skill. It was moved out of `SKILL.md` so the skill instructions stay concise while the implementation details remain available on demand.

Use this reference only when a request needs generated FaaS code for icon matching, built-in oncall / byteworks / bitable previews, or custom link-preview logic. Title-only links should be handled directly with `r?title=` and do not need this template.

The template reads only named runtime configuration values such as `MAGIC_BASE_URL`, data-source tokens, and preview table IDs. It must not serialize or forward arbitrary `process.env` contents.

## Template

```js
module.exports = async function (request, context) {
  const DEFAULT_ICON_TABLE_ID = "";
  const ICON_PAGE_SIZE = 200;
  const ICON_MATCH_THRESHOLD = 2.5;
  const DEFAULT_EXPIRE = "1h";
  const ALLOWED_EXPIRE = new Set(["60s", "1h", "24h", "1day"]);
  const DEFAULT_TIME_FIELD = "创建时间";
  const DEFAULT_MAGIC_BASE_URL = "https://magic.solutionsuite.cn";

  const ICON_INTENT = ["图标", "icon", "图片", "image", "封面", "avatar", "logo", "cover", "带图", "有图"];
  const NO_ICON_INTENT = ["无图", "不带图片", "不需要图片", "纯文本", "text only", "no image", "no_picture", "no icon"];
  const ONCALL_HINTS = ["oncall", "值班", "值班表", "值班人", "on call", "on-call", "班次", "值班时段", "oncall_flow"];
  const BYTEWORKS_HINTS = ["byteworks", "work list", "byteworks_schedule", "我的班表", "我的排班", "byte works", "byteworks_schedule"];
  const BITABLE_HINTS = ["多维表", "bitable", "base_records", "多维表格", "表格", "记录", "查记录", "记录数", "统计", "汇总", "最近", "本周"];
  const COMPLEX_HINTS = ["聚合", "关联", "条件", "筛选", "过滤", "去重", "排序", "趋势", "同比", "环比", "矩阵", "复杂", "自定义", "高级"];
  const COUNT_HINTS = ["统计", "总数", "数量", "count", "几条", "有多少", "多少条", "record count"];
  const WINDOW_HINTS = ["最近", "最近7天", "近7天", "7天", "上周", "本周", "周"];
  const DIRECT_TITLE_BLOCKERS = [
    "摘要", "summary", "desc", "description", "图标", "icon", "图片", "image", "封面", "cover",
    "数据源", "多维表", "bitable", "base", "oncall", "byteworks", "统计", "查询", "动态", "函数", "faas", "api"
  ];

  const magic = typeof window !== "undefined" && window.magic ? window.magic : null;
  const lark = typeof window !== "undefined" && window.lark ? window.lark : null;
  const headers = { "Content-Type": "application/json; charset=utf-8" };

  function safeText(value) {
    if (value == null) return "";
    if (typeof value === "string") return value.trim();
    if (typeof value === "number") return String(value);
    if (typeof value === "boolean") return value ? "true" : "false";
    if (Array.isArray(value)) {
      for (const v of value) {
        const t = safeText(v);
        if (t) return t;
      }
      return "";
    }
    if (typeof value === "object") {
      if (typeof value.file_token === "string") return value.file_token.trim();
      if (typeof value.token === "string") return value.token.trim();
      if (typeof value.key === "string") return value.key.trim();
      if (typeof value.image_key === "string") return value.image_key.trim();
    }
    return "";
  }

  function normalize(value) {
    return safeText(value)
      .toLowerCase()
      .replace(/[^\p{L}\p{N}\u4e00-\u9fa5]+/gu, " ")
      .replace(/\s+/g, " ")
      .trim();
  }

  function splitTokens(value) {
    const n = normalize(value);
    if (!n) return [];
    return Array.from(new Set(n.split(" ").filter(Boolean)));
  }

  function hasAnyIntent(text, words) {
    const n = normalize(text);
    return words.some((w) => n.includes(normalize(w)));
  }

  function toBool(req, keys) {
    for (const key of keys) {
      const raw = req[key];
      if (raw === true || raw === false) return !!raw;
      if (typeof raw === "string") {
        const v = raw.toLowerCase().trim();
        if (["1", "true", "yes", "y", "on"].includes(v)) return true;
        if (["0", "false", "no", "n", "off"].includes(v)) return false;
      }
    }
    return false;
  }

  function getField(req, keys, fallback = "") {
    for (const key of keys) {
      const v = req[key];
      if (v != null && String(v).trim()) return String(v).trim();
    }
    return fallback;
  }

  function normalizeMagicBaseUrl(value) {
    const raw = safeText(value || DEFAULT_MAGIC_BASE_URL).replace(/\/+$/, "");
    if (!raw) return DEFAULT_MAGIC_BASE_URL;
    return /^https?:\/\//i.test(raw) ? raw : `https://${raw}`;
  }

  function extractTitleCandidate(req) {
    const explicit = getField(req, ["title", "name", "subject"]);
    if (explicit) return explicit;
    const raw = getField(req, ["requirements", "text", "msg", "content"]);
    if (!raw) return "";
    const patterns = [
      /(?:标题|题目|文案|内容)\s*(?:是|为)?\s*[:：]?\s*([\s\S]+)$/i,
      /(?:title|text|content)\s*(?:is|=|:)?\s*([\s\S]+)$/i,
    ];
    for (const pattern of patterns) {
      const hit = raw.match(pattern);
      if (hit && hit[1]) return hit[1].trim().replace(/^[\s:："'“”‘’]+|[\s"'“”‘’]+$/g, "");
    }
    return raw.trim();
  }

  function buildDirectTitleUrl(title, magicBaseUrl) {
    const url = new URL(`${magicBaseUrl}/r`);
    url.searchParams.set("title", title);
    return url.toString();
  }

  function isDirectTitleOnly(req, title) {
    if (!title) return false;
    if (toBool(req, ["force_fid", "force_icon", "need_icon"])) return false;
    if (getField(req, ["summary", "desc", "description"])) return false;
    if (getField(req, ["icon_key", "image_key", "source", "data_app_token", "app_token", "data_table_id", "table_id", "data_view_id", "view_id"])) return false;
    if (getField(req, ["expire_strategy", "expire", "cache"])) return false;
    const raw = [
      getField(req, ["requirements", "text", "msg", "content"]),
      getField(req, ["scene"]),
      getField(req, ["keyword"]),
    ].filter(Boolean).join(" ");
    if (hasAnyIntent(raw, DIRECT_TITLE_BLOCKERS)) return false;
    return true;
  }

  async function parseRequest() {
    const req = {};
    const method = (request.method || "GET").toUpperCase();
    const url = new URL(request.url);
    for (const [k, v] of url.searchParams.entries()) req[k] = v;
    if (method !== "GET") {
      const ct = request.headers.get("content-type") || "";
      if (ct.includes("application/json")) {
        const body = await request.json().catch(() => ({}));
        Object.assign(req, body || {});
      } else {
        const text = await request.text().catch(() => "");
        if (!req.text && text) req.text = text;
      }
    }
    return req;
  }

  function pickField(fields, keys, fallbackKeys = []) {
    for (const k of keys) {
      const v = safeText(fields[k]);
      if (v) return v;
    }
    for (const k of fallbackKeys) {
      const v = safeText(fields[k]);
      if (v) return v;
    }
    for (const [k, v] of Object.entries(fields)) {
      if (safeText(v)) return safeText(v);
    }
    return "";
  }

  function buildContext(record) {
    const fields = (record && record.fields) || {};
    return {
      iconKey: pickField(fields, ["image_key", "icon", "icon_key", "ICON", "图标"]),
      title: pickField(fields, ["name", "title", "场景", "用途", "名称"]),
      summary: pickField(fields, ["summary", "描述", "说明", "description", "摘要"]),
      tags: pickField(fields, ["tags", "关键词", "关键字", "tag"]),
    };
  }

  function scoreContext(ctx, tokens) {
    let score = 0;
    const allText = [ctx.title, ctx.summary, ctx.tags].map(safeText).join(" ");
    const normAll = normalize(allText);
    for (const token of tokens) {
      if (!token || token.length < 2) continue;
      if (normAll.includes(token)) score += 2;
      if (normalize(ctx.title).split(" ").includes(token)) score += 1.5;
      if (normalize(ctx.summary).split(" ").includes(token)) score += 1;
      if (normalize(ctx.tags).split(" ").includes(token)) score += 2.5;
    }
    return score;
  }

  async function loadAllIconContexts({ appToken, tableId, viewId }) {
    const contexts = [];
    if (!magic && !lark) throw new Error("当前环境未注入 window.magic / window.lark");
    const hasMagic = magic && typeof magic.base_records_search === "function";
    const hasLark = lark && typeof lark.api === "function";
    if (!hasMagic && !hasLark) throw new Error("缺少 base_records_search 能力（magic / lark.api）");

    let pageToken = "";
    while (true) {
      if (hasMagic) {
        const resp = await magic.base_records_search(
          appToken,
          tableId,
          viewId || undefined,
          undefined,
          undefined,
          pageToken || undefined,
          ICON_PAGE_SIZE
        );
        if (resp?.code && resp.code !== 0) {
          throw new Error(resp.msg || "base_records_search 失败");
        }
        const data = resp?.data || {};
        for (const rec of data.records || []) {
          const ctx = buildContext(rec);
          if (ctx.iconKey) contexts.push(ctx);
        }
        if (!data.has_more) break;
        if (!data.page_token) break;
        pageToken = String(data.page_token || "");
      } else {
        const params = { page_size: ICON_PAGE_SIZE };
        if (pageToken) params.page_token = pageToken;
        const resp = await lark.api({
          id: "base_records_search",
          path_params: [appToken, tableId],
          body: { view_id: viewId || undefined },
          params,
        });
        if (resp?.code && resp.code !== 0) {
          throw new Error(resp.msg || "base_records_search 失败");
        }
        const data = resp?.data || {};
        for (const rec of data.items || data.records || []) {
          const ctx = buildContext(rec);
          if (ctx.iconKey) contexts.push(ctx);
        }
        if (!data.has_more) break;
        if (!data.page_token) break;
        pageToken = String(data.page_token || "");
      }
    }
    return contexts;
  }

  function detectScenario(req) {
    const forcedSource = getField(req, ["source"]).toLowerCase();
    const query = [
      getField(req, ["requirements", "text", "msg", "content"]),
      getField(req, ["title"]),
      getField(req, ["summary", "desc", "description"]),
      getField(req, ["scene"]),
      getField(req, ["keyword"]),
    ].filter(Boolean).join(" ");
    const n = normalize(query);

    if (forcedSource === "custom") return "custom";
    if (forcedSource === "builtin_oncall") return "oncall";
    if (forcedSource === "builtin_byteworks") return "byteworks";
    if (forcedSource === "builtin_bitable") return "bitable";

    if (hasAnyIntent(n, COMPLEX_HINTS)) return "custom";
    if (hasAnyIntent(n, ONCALL_HINTS)) return "oncall";
    if (hasAnyIntent(n, BYTEWORKS_HINTS)) return "byteworks";
    if (hasAnyIntent(n, BITABLE_HINTS)) return "bitable";
    return "custom";
  }

  async function pickIconOrNoIcon({ queryText, forceIcon, forceFid, explicitIconKey, appToken, tableId, viewId }) {
    const explicitIcon = safeText(explicitIconKey);
    if (!forceIcon && (forceFid || !queryText)) {
      return { iconKey: "", useIcon: false, matched: null, score: 0, needIcon: false };
    }

    if (!forceFid && (hasAnyIntent(queryText, ICON_INTENT) || forceIcon || explicitIcon)) {
      const contexts = await loadAllIconContexts({ appToken, tableId, viewId });
      if (!contexts.length) {
        return { iconKey: "", useIcon: false, matched: null, score: 0, needIcon: true };
      }

      if (explicitIcon) {
        const hit = contexts.find((ctx) => normalize(ctx.iconKey) === normalize(explicitIcon));
        if (hit) return { iconKey: hit.iconKey, useIcon: true, matched: hit, score: Number.MAX_SAFE_INTEGER, needIcon: true };
        return { iconKey: "", useIcon: false, matched: null, score: 0, needIcon: true };
      }

      const tokens = splitTokens(queryText);
      let best = { score: -1, matched: null };
      for (const ctx of contexts) {
        const s = scoreContext(ctx, tokens);
        if (s > best.score) best = { score: s, matched: ctx };
      }

      if (!best.matched || best.score < ICON_MATCH_THRESHOLD) {
        return { iconKey: "", useIcon: false, matched: best.matched, score: best.score, needIcon: true };
      }
      return { iconKey: best.matched.iconKey, useIcon: true, matched: best.matched, score: best.score, needIcon: true };
    }

    return { iconKey: "", useIcon: false, matched: null, score: 0, needIcon: false };
  }

  function normalizeExpire(value) {
    const v = String(value || "").trim();
    return ALLOWED_EXPIRE.has(v) ? v : DEFAULT_EXPIRE;
  }

  function jsLiteral(value) {
    return JSON.stringify(String(value || ""));
  }

  function makeCommonOk(expire) {
    return `function ok(title, summary, imageKey) {
      return new Response(JSON.stringify({
        inline: {
          i18n_title: { zh_cn: title },
          ...(summary ? { i18n_summary: { zh_cn: summary } } : {}),
          ...(imageKey ? { image_key: imageKey } : {}),
        },
        expire_strategy: "${expire}"
      }), { status: 200, headers: ${JSON.stringify(headers)} });
    }`;
  }

  function makeErrorTemplate(expire = "60s") {
    return `function fail(msg) {
      return new Response(JSON.stringify({
        inline: {
          i18n_title: { zh_cn: "链接预览生成失败" },
          i18n_summary: { zh_cn: String(msg || "未知错误") }
        },
        expire_strategy: "${expire}"
      }), { status: 200, headers: ${JSON.stringify(headers)} });
    }`;
  }

  function makeBitableCode({ mode, imageKey, expire, appTokenFromReq, tableIdFromReq, viewIdFromReq, timeFieldFromReq }) {
    const ok = makeCommonOk(expire);
    const fail = makeErrorTemplate();
    const appToken = appTokenFromReq || "";
    const tableId = tableIdFromReq || "";
    const viewId = viewIdFromReq || "";
    const timeField = timeFieldFromReq || DEFAULT_TIME_FIELD;

    const appTokenJs = jsLiteral(appToken);
    const tableIdJs = jsLiteral(tableId);
    const viewIdJs = jsLiteral(viewId);
    const timeFieldJs = jsLiteral(timeField);

    if (mode === "recent7d") {
      return `module.exports = async function (request, context) {
        const magic = (typeof window !== "undefined" && window.magic) ? window.magic : null;
        ${ok}
        ${fail}
        try {
          if (!magic || typeof magic.base_records_search !== "function") return fail("当前环境未注入 window.magic.base_records_search");
          const appToken = ${appTokenJs} || process.env.PREVIEW_APP_TOKEN || "";
          const tableId = ${tableIdJs} || process.env.PREVIEW_TABLE_ID || "";
          const viewId = ${viewIdJs} || process.env.PREVIEW_TABLE_VIEW_ID;
          if (!appToken || !tableId) return fail("请配置 PREVIEW_APP_TOKEN、PREVIEW_TABLE_ID");

          const now = Date.now();
          const weekStart = now - 7 * 24 * 60 * 60 * 1000;
          let pageToken = "";
          let total = 0;
          while (true) {
            const resp = await magic.base_records_search(appToken, tableId, viewId, undefined, undefined, pageToken || undefined, 200);
            if (resp?.code && resp.code !== 0) return fail(resp.msg || "base_records_search 失败");
            const data = resp?.data || {};
            for (const rec of (data.records || [])) {
              const fields = rec.fields || {};
              const rawTime = fields[${timeFieldJs}];
              const ts = Date.parse(String(rawTime || ""));
              if (Number.isFinite(ts) && ts >= weekStart && ts <= now) total += 1;
            }
            if (!data.has_more) break;
            pageToken = String(data.page_token || "");
            if (!pageToken) break;
          }
          return ok("最近7天共 " + total + " 条", "数据源：多维表查询（bitable）", ${JSON.stringify(imageKey || "")});
        } catch (e) {
          return fail(e?.message || e);
        }
      };`;
    }

    if (mode === "count") {
      return `module.exports = async function (request, context) {
        const magic = (typeof window !== "undefined" && window.magic) ? window.magic : null;
        ${ok}
        ${fail}
        try {
          if (!magic || typeof magic.base_records_search !== "function") return fail("当前环境未注入 window.magic.base_records_search");
          const appToken = ${appTokenJs} || process.env.PREVIEW_APP_TOKEN || "";
          const tableId = ${tableIdJs} || process.env.PREVIEW_TABLE_ID || "";
          const viewId = ${viewIdJs} || process.env.PREVIEW_TABLE_VIEW_ID;
          if (!appToken || !tableId) return fail("请配置 PREVIEW_APP_TOKEN、PREVIEW_TABLE_ID");

          let pageToken = "";
          let count = 0;
          while (true) {
            const resp = await magic.base_records_search(appToken, tableId, viewId, undefined, undefined, pageToken || undefined, 200);
            if (resp?.code && resp.code !== 0) return fail(resp.msg || "base_records_search 失败");
            const data = resp?.data || {};
            count += Array.isArray(data.records) ? data.records.length : 0;
            if (!data.has_more) break;
            pageToken = String(data.page_token || "");
            if (!pageToken) break;
          }
          return ok("当前记录总数：" + count + " 条", "数据源：多维表查询（bitable）", ${JSON.stringify(imageKey || "")});
        } catch (e) {
          return fail(e?.message || e);
        }
      };`;
    }

    return `module.exports = async function (request, context) {
      const magic = (typeof window !== "undefined" && window.magic) ? window.magic : null;
      ${ok}
      ${fail}
      try {
        if (!magic || typeof magic.base_records_search !== "function") return fail("当前环境未注入 window.magic.base_records_search");
        const appToken = ${appTokenJs} || process.env.PREVIEW_APP_TOKEN || "";
        const tableId = ${tableIdJs} || process.env.PREVIEW_TABLE_ID || "";
        const viewId = ${viewIdJs} || process.env.PREVIEW_TABLE_VIEW_ID;
        if (!appToken || !tableId) return fail("请配置 PREVIEW_APP_TOKEN、PREVIEW_TABLE_ID");

        const resp = await magic.base_records_search(appToken, tableId, viewId, undefined, undefined, undefined, 1);
        if (resp?.code && resp.code !== 0) return fail(resp.msg || "base_records_search 失败");
        const total = Array.isArray(resp?.data?.records) ? resp.data.records.length : 0;
        return ok("多维表记录查询成功", "检索到 " + total + " 条样例（按实际场景可二次定制）", ${JSON.stringify(imageKey || "")});
      } catch (e) {
        return fail(e?.message || e);
      }
    };`;
  }

  function makeOncallCode(imageKey, expire) {
    const ok = makeCommonOk(expire);
    const fail = makeErrorTemplate();
    return `module.exports = async function (request, context) {
      ${ok}
      ${fail}
      try {
        const oncallToken = process.env.ONCALL_TENANT_TOKEN || "";
        const oncallUrl = process.env.ONCALL_LIST_URL || "https://oncall-backend.bytedance.net/api/inf/v1/oncall_flow/list";
        if (!oncallToken) return fail("请配置 ONCALL_TENANT_TOKEN（x-notenant-token）");

        const resp = await fetch("/api/proxy", {
          method: "POST",
          headers: { "Content-Type": "application/json; charset=utf-8" },
          body: JSON.stringify({
            _action: "proxy",
            _url: oncallUrl,
            _method: "GET",
            _headers: { "x-notenant-token": oncallToken },
            current_page: 1,
            page_size: 1,
            filter_fields: []
          })
        });
        const data = await resp.json().catch(() => ({}));
        const flowCount = Array.isArray(data?.data?.flowList || data?.data || data?.items) ? (data.data.flowList || data.data || data.items).length : 0;
        return ok("oncall 视图", "待办流程数：" + flowCount + "（数据来自 oncall 数据源）", ${JSON.stringify(imageKey || "")});
      } catch (e) {
        return fail(e?.message || e);
      }
    };`;
  }

  function makeByteworksCode(imageKey, expire) {
    const ok = makeCommonOk(expire);
    const fail = makeErrorTemplate();
    return `module.exports = async function (request, context) {
      ${ok}
      ${fail}
      try {
        const unionId = process.env.BYTEWORKS_UNION_ID || process.env.UNION_ID || "";
        const byteworksApi = process.env.BYTEWORKS_API || "https://apaas.feishuapp.cn/ae/public/ai__c/api";
        if (!unionId) return fail("请配置 BYTEWORKS_UNION_ID");

        const resp = await fetch("/api/proxy", {
          method: "POST",
          headers: { "Content-Type": "application/json; charset=utf-8" },
          body: JSON.stringify({
            _action: "proxy",
            _url: byteworksApi,
            _method: "POST",
            action: "byteworks_schedule",
            user_id: unionId
          })
        });
        const data = await resp.json().catch(() => ({}));
        const tasks = Array.isArray(data?.data?.tasks || data?.tasks) ? (data.data.tasks || data.tasks).length : 0;
        return ok("byteworks 排班预览", "排班/任务数：" + tasks + "（数据来自 byteworks）", ${JSON.stringify(imageKey || "")});
      } catch (e) {
        return fail(e?.message || e);
      }
    };`;
  }

  function makeCustomCode({ imageKey, expire, title, summary }) {
    const ok = makeCommonOk(expire);
    const fail = makeErrorTemplate();
    const rawTitle = JSON.stringify(title || "");
    const rawSummary = JSON.stringify(summary || "");
    return `module.exports = async function (request, context) {
      const magic = (typeof window !== "undefined" && window.magic) ? window.magic : null;
      const contentType = request.headers.get("content-type") || "";
      const req = {};
      const url = new URL(request.url);
      for (const [k, v] of url.searchParams.entries()) req[k] = v;
      if (contentType.includes("application/json")) {
        const body = await request.json().catch(() => ({}));
        Object.assign(req, body || {});
      }
      ${ok}
      ${fail}
      try {
        const inputTitle = req.title || ${rawTitle};
        const inputSummary = req.summary || ${rawSummary};
        let title = String(inputTitle || "妙笔链接预览");
        if (!inputTitle && magic && typeof magic.ai === "function") {
          const aiReq = [
            "你是一个短标题生成器。",
            "请基于以下用户需求输出一句简短、可用于链接预览的标题。",
            "要求：不超过20字，中文优先。",
            "需求：" + (req.requirements || req.text || req.content || "${String("").replace(/"/g, '\\"')}")
          ].filter(Boolean).join("\\n");
          const ai = await magic.ai({ system: aiReq, user: "请输出标题", temperature: 0.2, reasoning_effort: "minimal" });
          const fromAi = String(ai?.data?.result || "").trim();
          if (fromAi) title = fromAi;
        }
        return ok(title, inputSummary || "由通用预览逻辑生成（可后续接入数据源）", ${JSON.stringify(imageKey || "")});
      } catch (e) {
        return fail(e?.message || e);
      }
    };`;
  }

  let magicBaseUrl = DEFAULT_MAGIC_BASE_URL;
  try {
    const req = await parseRequest();
    magicBaseUrl = normalizeMagicBaseUrl(
      getField(req, ["magic_base_url", "base_url", "domain"], process.env.MAGIC_BASE_URL || "")
    );
    const titleCandidate = extractTitleCandidate(req);
    if (isDirectTitleOnly(req, titleCandidate)) {
      return new Response(
        JSON.stringify({
          code: "",
          route: "direct_title",
          scenario: "direct_title",
          direct_url: buildDirectTitleUrl(titleCandidate, magicBaseUrl),
          skip_publish: true,
          preview_url_tpl: `${magicBaseUrl}/r?title={encoded_title}`,
          icon_strategy: {
            force_fid: false,
            force_icon: false,
            icon_checked: false,
            icon_matched: false,
            matched_score: 0,
            selected_icon_key: "",
          },
          icon_match_note: "只有标题，已直接拼接 r?title 链接",
        }),
        { status: 200, headers }
      );
    }

    const forceFid = toBool(req, ["force_fid", "no_image", "pure_text"]);
    const forceIcon = toBool(req, ["force_icon", "need_icon"]);
    const iconNeedText = [
      getField(req, ["requirements", "text", "msg", "content"]),
      getField(req, ["title", "name", "subject"]),
      getField(req, ["summary", "desc", "description"]),
      getField(req, ["scene"]),
      getField(req, ["keyword"]),
    ]
      .filter(Boolean)
      .join(" ");

    const explicitIconKey = getField(req, ["icon_key", "image_key"]);
    const iconAppToken = getField(req, ["icon_app_token"], process.env.PREVIEW_ICON_APP_TOKEN || "");
    const iconTableId = getField(req, ["icon_table_id"], process.env.PREVIEW_ICON_TABLE_ID || DEFAULT_ICON_TABLE_ID);
    const iconViewId = getField(req, ["icon_view_id"], "");
    const expire = normalizeExpire(req.expire_strategy || req.expire || req.cache);
    const scenario = detectScenario(req);
    const route = forceFid ? "fid_only" : scenario;

    const iconResult = await pickIconOrNoIcon({
      queryText: iconNeedText,
      forceIcon,
      forceFid,
      explicitIconKey,
      appToken: iconAppToken,
      tableId: iconTableId,
      viewId: iconViewId,
    });
    const title = getField(req, ["title", "name", "subject"]) || titleCandidate;
    const summary = getField(req, ["summary", "desc", "description", "content", "text"]);
    const imageKey = iconResult.useIcon ? iconResult.iconKey : "";
    const finalSummary = summary || (iconResult.matched && iconResult.matched.summary) || "";

    const codeMode = scenario === "bitable"
      ? (hasAnyIntent(iconNeedText, WINDOW_HINTS)
          ? "bitable_recent7d"
          : hasAnyIntent(iconNeedText, COUNT_HINTS)
            ? "bitable_count"
            : "bitable_latest")
      : scenario;

    const dataAppToken = getField(req, ["data_app_token", "app_token", "dataAppToken"]);
    const dataTableId = getField(req, ["data_table_id", "table_id", "dataTableId"]);
    const dataViewId = getField(req, ["data_view_id", "view_id", "dataViewId"]);
    const timeField = getField(req, ["time_field", "timeField"]) || DEFAULT_TIME_FIELD;

    let code = "";
    if (route === "fid_only" || route === "custom") {
      code = makeCustomCode({
        imageKey,
        expire,
        title,
        summary: finalSummary,
      });
    } else if (codeMode === "bitable_recent7d") {
      code = makeBitableCode({
        mode: "recent7d",
        imageKey,
        expire,
        appTokenFromReq: dataAppToken,
        tableIdFromReq: dataTableId,
        viewIdFromReq: dataViewId,
        timeFieldFromReq: timeField,
      });
    } else if (codeMode === "bitable_count") {
      code = makeBitableCode({
        mode: "count",
        imageKey,
        expire,
        appTokenFromReq: dataAppToken,
        tableIdFromReq: dataTableId,
        viewIdFromReq: dataViewId,
      });
    } else if (codeMode === "bitable_latest") {
      code = makeBitableCode({
        mode: "latest",
        imageKey,
        expire,
        appTokenFromReq: dataAppToken,
        tableIdFromReq: dataTableId,
        viewIdFromReq: dataViewId,
      });
    } else if (codeMode === "oncall") {
      code = makeOncallCode(imageKey, expire);
    } else if (codeMode === "byteworks") {
      code = makeByteworksCode(imageKey, expire);
    } else {
      code = makeCustomCode({
        imageKey,
        expire,
        title,
        summary: finalSummary,
      });
    }

    return new Response(
      JSON.stringify({
        code,
        route,
        scenario: codeMode,
        preview_url_tpl: `${magicBaseUrl}/r?fid={fid}`,
        icon_strategy: {
          force_fid: forceFid,
          force_icon: forceIcon,
          icon_checked: iconResult.needIcon,
          icon_matched: iconResult.useIcon,
          matched_score: iconResult.score || 0,
          selected_icon_key: imageKey || "",
        },
        icon_match_note: !iconResult.useIcon ? "未匹配到图标，已回退为 fid" : "已匹配图标",
      }),
      { status: 200, headers }
    );
  } catch (e) {
    return new Response(
      JSON.stringify({
        code: makeCustomCode({
          imageKey: "",
          expire: "60s",
          title: "链接预览生成失败",
          summary: String(e?.message || e || "未知错误"),
        }),
        preview_url_tpl: `${magicBaseUrl}/r?fid={fid}`,
        route: "error",
        icon_matched: false,
      }),
      { status: 200, headers }
    );
  }
};
```
