# detail.md bare URL 渲染为「📎 hostname」link card

## 背景

TODO 上 auto-proposed 一条："任务详情 detail.md 内嵌 https 链接预览：parseDetailMdWithImages 扩到 https url 显占位『📎 url 域名』卡片，让链接看起来不是纯文本。"

detail.md 是 owner / 宠物轮流写笔记的载体；其中常含外部引用 URL（PR / Linear / Figma / 文档 / 博客等）。当前 parseDetailMdWithImages 已识别 `![alt](url)` 图片块；bare 文本 URL 走 parseMarkdown 渲为蓝下划线 anchor，与正文文字视觉混在一起。

把 bare URL 升级为「📎 hostname」chip 让它们像附件而非散文 URL，更切合 owner "我贴这个链接是要单独引用" 的意图。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 新增 `LinkCard` 组件

```tsx
function LinkCard({ url }: { url: string }) {
  let label = url;
  try {
    label = new URL(url).hostname;
  } catch {
    // 不合法 URL → 退化用原文做 label
  }
  return (
    <a
      href={url}
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        openUrl(url).catch(console.error);
      }}
      title={url}
      style={{
        display: "inline-flex",
        ...chip-like styles...
        maxWidth: 240, overflow: "hidden", textOverflow: "ellipsis",
      }}
    >
      📎 {label}
    </a>
  );
}
```

#### 新增 `renderDetailTextWithLinkCards` helper

把文本段切成 URL 和非 URL 子段。URL 渲 LinkCard，其它走 parseMarkdown：

```ts
const URL_RE = /(?<!\]\()https?:\/\/[^\s)\]<>"']+/g;
```

**negative lookbehind `(?<!\]\()`** 是关键 —— 排除 markdown 链接 `[text](url)` 里的 url，让显式锚文本路径不被 LinkCard 抢走。owner 写 `[Linear ticket](https://...)` 仍渲为带 "Linear ticket" 锚的标准超链接；只有 bare URL `https://...` 才升级为 📎 chip。

剥 trailing 标点（与 parseUrls 同思路）让 "看这里 https://a.com。" 不把"。"吃进 URL。

#### `parseDetailMdWithImages` 接入

文本段（图片块之间和末尾）从 `parseMarkdown(...)` 改为 `renderDetailTextWithLinkCards(text, keyPrefix)`。图片块 / 非图 markdown 链接路径不变。

## 关键设计

- **仅 detail.md，不动 PanelChat / ChatMini**：mini chat 是流水对话场景，URL chip 化容易显得突兀（聊天里贴的链接通常是即时引用而非"附件"语义）。detail.md 是文档化场景，URL 多为"参考资料"角色，chip 化贴合心智。
- **`(?<!\]\()` negative lookbehind 保 markdown link 不被抢**：parseMarkdown 内部已经把 `[text](url)` 渲成 anchor with custom text。我们的 URL 扫描必须跳过这种 url 否则会双重处理。lookbehind 是 V8 / WebKit 标准 ES2018，Tauri WKWebView 全支持。
- **URL 后 trailing punctuation 剥到尾巴**：与既有 inlineMarkdown.parseUrls 同模式 —— 让句末"。" / "，" / ")" 不被吃进 URL，render 时仍是正确的中文标点。
- **`hostname` 显示而非完整 URL**：chip 视觉简洁。完整 URL 在 `title` attr 让 owner hover 验证。`new URL(url)` 解析失败时退化全文，不渲染空字符串。
- **maxWidth 240 + ellipsis**：长 url（含 query string / hash）的 hostname 也可能拉长（如 docs.google.com/spreadsheets/...）；chip 给个尺寸 cap 让长 URL 不撑爆 detail.md 单行。
- **stopPropagation 防冒泡**：detail.md preview 容器可能挂 onClick（展开收起 / 滚动等），LinkCard 点击不应被父级吞掉。`preventDefault` 防 Tauri WebView 自身尝试导航（webview 无新标签页语义，会导致 webview 内空白）。
- **out.length === 0 fallback**：理论上 URL_RE 至少能 fall through 到末尾 text 段，但保险 fallback 让"全无 text 的边界"也能正常渲。

## 不做

- **不做"OG meta 抓取"的真实卡片预览**：抓 hostname 标题 / favicon / 描述需要后端 fetch + 缓存 + 安全审计，复杂度大且 detail.md 工作流不要求那么富文本。`📎 hostname` 是轻量低成本的"附件"暗示。
- **不动 PanelChat 内 URL 渲染**：chat 流里 URL 多是实时引用，蓝下划线 anchor 更轻；chip 化会让消息气泡变臃肿。
- **不动 ChatMini**：同上理由 + ChatMini 极简化原则（不在小窗口里塞 chip 噪音）。
- **不写测试**：纯字符串 split + ReactNode 构造，逻辑 50 行；既有 parseDetailMdWithImages 路径无单测（同模式）。视觉验证（detail.md 含 bare https URL → 显 📎 chip → 点击 openUrl）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.17s
- 改动 ~140 行（LinkCard 30 + renderDetailTextWithLinkCards 50 + openUrl import 1 + parseDetailMdWithImages 两处文本段调用 8 + 注释）；既有图片块 / markdown link `[text](url)` / parseMarkdown 路径完全不动。

## TODO 状态

6 条候选 auto-proposed 已完成 4 条（其中 1 条 stale 移除），余 1 条留池：
- 跨会话搜索结果按月份分组

## 后续

- LinkCard 右键菜单："📋 复制链接" / "🔄 重新打开" / "📌 设为参考资料" 等。当前单击即打开够用。
- 检测特殊域名做差异化 emoji：GitHub → 🐙、Linear → 📐、Figma → 🎨、Notion → 📓 等；当前统一 📎 简洁但缺信息密度。
- 链接历史：detail.md 引用过的 URL 写入跨任务 "recent links" 列表，让其它任务可快速引用。
