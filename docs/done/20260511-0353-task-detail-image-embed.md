# 任务详情图片附件渲染

## 需求

任务详情 detail.md 里贴的截图（用户粘贴时大模型 / 自己用 markdown image 语法
`![alt](data:image/png;base64,...)` 写进去）现在只能看到一行难懂的 base64 字
符，不渲染。要切到外部 markdown 预览才看到图。让 panel 自带渲染：扫 markdown
image 语法 → 用 ImageThumb 显出来 + 点开 lightbox + hover 📋 复制。

## 实现

### 解析助手

`src/components/panel/PanelTasks.tsx` 顶部加两个 helper：

```ts
function isImageUrl(url: string): boolean {
  if (url.startsWith("data:image/")) return true;
  return /^https?:\/\/.+\.(png|jpe?g|gif|webp|svg|bmp)(\?|#|$)/i.test(url);
}

function parseDetailMdWithImages(md, onOpenImage): ReactNode[] {
  // 正则切 ![alt](url)；text 段交给 parseMarkdown，image 段渲 ImageThumb
  const re = /!\[([^\]]*)\]\(([^)\s]+)\)/g;
  // ... 标准 split + map ...
}
```

URL 限制 `[^)\s]+` 防止吃过界（贪婪匹配会把后面的右括号 / 多行内容都吃进
URL）。非图链接（比如 `![logo](https://docs.example.com/page)` 这种文档链
接误用 image 语法）走 `parseMarkdown(m[0])` 字面回退，避免渲一个 broken
img。

不识别带 title 的形式 `![alt](url "title")` —— 大模型 / 用户实际几乎不写。

### 接入

PanelTasks 组件：

- import `ImageLightbox` + `ImageThumb`
- 加 `detailLightboxSrc: string | null` state
- detail.md 渲染分支：`parseMarkdown(...)` → `parseDetailMdWithImages(detail.detail_md, setDetailLightboxSrc)`
- 根 `</div>` 前挂一次 `<ImageLightbox src={detailLightboxSrc} onClose={...} />`

source 模式（用户切到 raw view）保持 markdown 原文显示，不动。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 任务 detail.md 含 `![](data:image/png;base64,...)` → 渲一张 160px 缩略图
  - hover 缩略图 → 📋 复制；click 缩略图 → lightbox 弹出（自带复制 + Esc 关）
  - 切 source 模式 → 字面 markdown 文本
  - `![logo](https://docs.example.com/page)` 这种非图链接 → 字面渲，不 broken img
  - 多张图 → 多个缩略图依顺序渲，互不影响

## TODO 池清空 → 自主提案

按 TODO.md 规则 #1，提出 5 条新需求（已写入 TODO.md）：

1. detail.md 编辑器支持粘贴 / 拖入图片（自动插 markdown image 行）
2. 任务列表"全选导出 markdown"按钮
3. ChatMini 用户气泡解析 message text 里的 markdown image
4. 设置页 raw YAML 模式加搜索 / 跳转
5. 桌面气泡⌘+C 快捷复制聚焦的 assistant 文本
