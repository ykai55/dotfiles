# DocMiniApp 文档端能力

本参考只用于 `magic-builder` 的飞书文档 HTML Box 模式。优先使用 `window.magic.*`；只有用户明确需要文档小组件宿主能力，或需求点无法由 `window.magic` 覆盖时，才使用 `DocMiniApp.*`。

`DocMiniApp` 不是普通浏览器 API，也不是 Magic Page/FaaS 的通用运行时能力。生成 HTML 时必须做可用性判断；本地预览、独立 `/html-box/[id]`、非文档端 iframe 中可能不存在。

## 用户名片

参考飞书云文档小组件开发手册：`View.Action.showUserProfile` / `View.Action.hideUserProfile`。

### showUserProfile

展示用户卡片，异步调用。

可用性：

- 权限要求：可读
- 视图：所有视图
- 平台：PC、移动端
- 场景：演示模式

参数：

| 名称 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `userId` | `string` | 是 | 要展示名片的用户 ID |
| `placement` | `string` | 否 | 卡片位置，默认 `bottom` |
| `boundingRect` | `object` | 是 | 目标元素相对容器 iframe 的位置信息 |
| `boundingRect.x` | `number` | 是 | 目标相对容器 iframe 的 x 偏移 |
| `boundingRect.y` | `number` | 是 | 目标相对容器 iframe 的 y 偏移 |
| `boundingRect.width` | `number` | 是 | 目标宽度 |
| `boundingRect.height` | `number` | 是 | 目标高度 |

`placement` 可选值：

`top`、`bottom`、`left`、`right`、`top-left`、`top-right`、`bottom-left`、`bottom-right`、`left-top`、`left-bottom`、`right-top`、`right-bottom`。

直接调用示例：

```js
await DocMiniApp.View.Action.showUserProfile({
  userId: '6677348378390577411',
  placement: 'right-bottom',
  boundingRect: {
    x: 0,
    y: 0,
    width: 100,
    height: 180,
  },
});
```

生成页面推荐封装，避免非文档环境报错：

```js
function getDocMiniApp() {
  if (window.DocMiniApp) return window.DocMiniApp;
  if (window.BlockitClient) return new window.BlockitClient().initAPI();
  return null;
}

async function showUserProfileFromElement(userId, element, placement = 'right-bottom') {
  const docMiniApp = getDocMiniApp();
  const showUserProfile = docMiniApp?.View?.Action?.showUserProfile;
  if (!showUserProfile || !element) return false;

  const rect = element.getBoundingClientRect();
  await showUserProfile({
    userId: String(userId),
    placement,
    boundingRect: {
      x: rect.left,
      y: rect.top,
      width: rect.width,
      height: rect.height,
    },
  });
  return true;
}
```

使用建议：

- 只在用户头像、姓名、人员字段等明确可点击的人物 UI 上触发。
- `boundingRect` 用触发元素的 `getBoundingClientRect()` 计算；该坐标在 iframe 内相对当前视口，符合“相对容器 iframe”的定位要求。
- 不要把 `open_id`、`user_id`、数字用户 ID 混用；除非上游数据已明确就是该接口需要的 `userId`。
- 调用失败或能力不可用时，静默降级为不展示名片，或显示普通文本信息。

### hideUserProfile

隐藏用户卡片，异步调用，无参数、无返回值。

```js
const docMiniApp = getDocMiniApp();
await docMiniApp?.View?.Action?.hideUserProfile?.();
```

在弹层关闭、列表刷新、鼠标离开触发区域，或页面切换时可调用它收起名片。
