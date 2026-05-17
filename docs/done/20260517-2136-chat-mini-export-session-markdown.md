# ChatMini「💾 导出本会话 markdown」按钮（iter #399）

## Background

桌面 PanelChat 已有 export session as markdown 路径
（`exportSessionAsMarkdown` in panelChatBits.tsx）— 但 ChatMini
小窗缺。owner 在 ChatMini 想"把这段对话拷给团队 / 粘到文档" 需切
panel 走 export。本 iter 加 ChatMini 顶角 💾 按钮 — 一键到剪贴板。

## Changes

### `src/components/ChatMini.tsx`

#### 1. `handleExportSessionMarkdown` handler

与 PanelChat 既有 `exportSessionAsMarkdown` 同格式：
- `# ChatMini Session` 标题
- `> 导出时间: <local> · 共 N 条消息` metadata
- 每条 `## 🧑/🐾 user/assistant` + content + 空行

过滤逻辑：仅 `role === "user" || "assistant"`，跳 system / tool。
失败走既有 `setCopyToast("err")` 反馈机制。

```ts
const handleExportSessionMarkdown = () => {
  const slice = messages.filter(m => m.role === "user" || m.role === "assistant");
  if (slice.length === 0) { setCopyToast("err"); return; }
  const lines = ["# ChatMini Session", `> 导出时间: ${...} · 共 ${slice.length} 条消息`, ""];
  for (const m of slice) {
    lines.push(`## ${glyph} ${m.role}`, "", extractText(m.content), "");
  }
  navigator.clipboard.writeText(lines.join("\n"))
    .then(() => setCopyToast("done"))
    .catch(() => setCopyToast("err"));
};
```

#### 2. 💾 按钮 UI（top-right corner，🌐 之左）

```tsx
<button
  onClick={handleExportSessionMarkdown}
  title="导出本会话为 markdown 复制到剪贴板..."
  style={{
    position: "absolute",
    top: "14px",
    right: onOpenPanel ? "104px" : "76px",
    width: 20, height: 20, borderRadius: "50%",
    // ...
  }}
>
  💾
</button>
```

按钮顺序（top-right → left）：⛶ (20px) → 📋 (48px) → 🌐 (76px) →
💾 (104px) — 每个 28px 间距（按钮 20px + 8 gap）。两 copy/export
入口（📋 = 最近 N 条片段 / 💾 = 整 session 完整 markdown）+ 时区 /
最大化辅助 chip 视觉成行。

## Key design decisions

- **复用既有 copyToast 反馈**：与 📋 复制最近 N 条同 mechanism —
  不另起 toast 系统。
- **不复用 PanelChat 的 exportSessionAsMarkdown**：那个签名是
  `(title, ChatItem[])`，ChatMini 用 `ChatMessage[]` 类型不同（has
  `role` not `type`，且 `content` 可能是 MessageContent 数组而非
  string）。内联同格式 reimplementation 更直接 — 12 行 vs 引一层
  adapter 转换。
- **过滤 user/assistant**：与 PanelChat export 同语义 — tool /
  system 行不是 owner-pet 对话内容。
- **`# ChatMini Session` 固定标题**：vs PanelChat 用 session.title。
  ChatMini 没 session title 概念（session 是 useChat 内部 ref）；
  固定 "ChatMini Session" 让导出文件有 anchor，owner 自己加 H1 都行。
- **位置 right: 104px**：避开既有 🌐 时区 chip (76px) — 否则会
  overlap。28px 间距与既有按钮节奏一致。
- **不为单 fn 引 unit test runner**：行为是 IO + clipboard；build
  pass + 手测足够（点 💾 → 粘到 markdown viewer 验格式正确）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.23s)
- 后端无改动
