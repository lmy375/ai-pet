# ChatMini ts chip 双击复制「MM-DD HH:MM」友好格式（iter #283）

## Background

ChatMini bubble 上方的 ts chip click 复制 raw ISO timestamp（如
`2026-05-17T10:54:32+08:00`），适合 debug / 报错时给完整时间。但 owner
更常用场景是"贴到日程 / 发同事 / 写笔记"——这时长 ISO 是噪音，想要的是
"05-17 10:54" 短格式。

本迭代给 ts chip 加 onDoubleClick：复制 `MM-DD HH:MM` 友好串。单击仍走原
ISO 路径，二者并存让 owner 按需选。

## Changes

仅 `src/components/ChatMini.tsx`：

- ts chip `<span>` 加 onDoubleClick handler：
  - `e.stopPropagation() + preventDefault()` 防触发 onClick 二次（dblclick
    不会自动取消单击 — 这里手动拦）
  - `new Date(m.ts)` 解析；失败兜底 `formatBubbleTimestamp` 去括号后写剪贴板
  - 成功 → 复用既有 `tsCopyIdx` 1.5s ✓ 反馈通道（与 click 路径同视觉）
- title 文案更新为说明双击行为：`单击复制完整 ISO timestamp · 双击复制 "MM-DD HH:MM" 友好短格式`

## Key design decisions

- **复用 tsCopyIdx 而非新独立 state**：单击 / 双击都是"复制成功"，共用绿色
  ✓ 反馈不需要区分；省一个 state + 视觉一致。
- **`MM-DD HH:MM` 而非 `MM-DD HH:MM:SS`**：分钟精度对 owner 贴日程 / 发对话
  时间足够；秒级精度对外部沟通过头。Slack / Notion 等输入时间通常也只到
  分钟。
- **stopPropagation + preventDefault on dblclick**：浏览器双击会触发 click
  ×2 + dblclick；不阻止的话 ISO 会先复制然后被 short 覆盖（视觉上抖一下），
  preventDefault 让 dblclick 优雅独立。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.23s)
