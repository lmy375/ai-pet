# 设置面板搜索高亮 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 设置面板搜索高亮：搜索命中 section 时把 query 子串在标题里 `<mark>` 高亮，让用户秒辨命中位置（与跨会话搜索 highlightedItemIdx 风格统一）。

## 目标

`PanelSettings` 的搜索过滤已工作；但用户输入"key"后，LLM section 与其它带
"key"关键字的 section 同时显示，标题里没视觉提示哪个字符位匹配上了。本轮
在每个 section 标题里把命中的 query 子串用 `<mark>` 浅黄高亮（与 PanelChat
跨会话搜索的 SearchResultRow 视觉同源）。

## 非目标

- 不在标题外的字段 / label 里高亮 —— 11 个 section 的子字段散在不同 JSX 里，
  全覆盖会变成无穷工程；标题级高亮已能让用户秒辨命中位置。
- 不为 keywords 命中（如搜 "api" 匹配 LLM section 的 keyword 但标题 "LLM 配置"
  不含 "api"）做"虚拟高亮" —— 标题无字面命中即不渲染 mark，与其编造高亮
  位置不如显示原文（fallback 行为符合直觉）。
- 不写 README —— 设置可寻性微调。

## 设计

### Pure 组件

新增 `<HighlightedText text query />` 子组件（同 PanelSettings 内部，靠近
`SearchableSection`）：

```tsx
function HighlightedText({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  if (q.length === 0) return <>{text}</>;
  const idx = text.toLowerCase().indexOf(q.toLowerCase());
  if (idx < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, idx)}
      <mark style={MARK_STYLE}>{text.slice(idx, idx + q.length)}</mark>
      {text.slice(idx + q.length)}
    </>
  );
}
```

`MARK_STYLE`：背景 `#fef3c7`（与 PanelChat SearchResultRow 同色 — 设置 / 聊天
两处搜索的视觉一致）；文字色 `inherit`（让父级 `<h4>` 的色继续生效，避免
mark 默认深底色覆盖）；padding 0、borderRadius 2。

### 接入

每个简单 section 的 `<h4 style={sectionTitle}>Live2D 模型</h4>` 改成：
```tsx
<h4 style={sectionTitle}>
  <HighlightedText text="Live2D 模型" query={searchQuery} />
</h4>
```

复杂 section（MCP / Telegram，h4 内有 chip span）只替换字面 title 段，保留
chip 不动。

涉及 9 + 2 共 11 个 section 标题。改动局部、机械。

### 测试

`HighlightedText` 是 pure，但项目无 vitest。极简（~10 行），靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `HighlightedText` + `MARK_STYLE` |
| **M2** | 替换 11 个 section 标题 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 上一轮 `SearchableSection` / `searchQuery` 状态
- PanelChat SearchResultRow 的 mark 配色（`#fef3c7` / `#92400e` 黄底）

## 待用户裁定的开放问题

- 仅高亮第一次出现 vs 全部出现？本轮**仅第一次**——title 通常 ≤ 10 字，多次
  出现极少，单次匹配就够清晰；多次匹配的实现也更复杂。

## 进度日志

- 2026-05-06 01:00 — 创建本文档；准备 M1。
- 2026-05-06 01:20 — 完成实现：
  - **M1**：`PanelSettings.tsx` 加 `HighlightedText` 子组件 + `HIGHLIGHT_MARK_STYLE`（黄底深棕字，与 PanelChat SearchResultRow 同色）。pure：空 query 或未命中 → 原样输出；命中 → split 三段并将中间段包 `<mark>`。
  - **M2**：替换 9 个简单 section 的 `<h4>{title}</h4>` 与 2 个复杂 section（MCP / Telegram）的字面 title 段为 `<HighlightedText text=... query={searchQuery} />`。raw mode 的 h4 不动（搜索框只在 form mode 渲染）。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 设置可寻性微调，与之前搜索框迭代同性质。
  - **设计取舍**：标题级高亮（不深入字段 / label）—— 全字段高亮的工程量与认知负担都不值；keywords-only 命中（如 query "key" 命中 keyword 但 title "LLM 配置" 不含）→ 标题不显高亮，与"如实显示原文"原则一致；mark 配色与 PanelChat 同源，让设置 / 聊天两处搜索视觉统一。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；`HighlightedText` ~10 行 pure 逻辑，由 tsc 保证。
