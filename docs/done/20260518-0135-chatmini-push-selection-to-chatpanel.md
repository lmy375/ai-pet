# ChatMini 选区 toolbar「💬 推到 ChatPanel」按钮（iter #346）

## Background

ChatMini 选区 toolbar 已有 4 按钮：💾 转 task / 📝 记到 note / 📋 复制 /
🔄 让 AI 改写。但 owner 想「针对这段选中文字在 ChatPanel 继续问 / 评
论」时只能：📋 复制 → 切到面板 → 找 ChatPanel tab → 粘贴 → 手写 "请
评论：..." → 发。4 步。

本迭代加 💬 第 5 按钮 — 复用既有 `pet-mini-respond-to` CustomEvent
通道（ChatPanel 已 listen 此事件），dispatch 选区文本（flatten + 80 char
cap）让 ChatPanel input prefill 「关于「...」」。

## Changes

仅 `src/components/ChatMini.tsx`：

- selection toolbar 在 🔄 改写 按钮之前插 💬 按钮：
  - onClick：`flat = text.replace(/\s+/g, " ").trim()` flatten + 80
    char cap + `…` 省略
  - `window.dispatchEvent(new CustomEvent("pet-mini-respond-to",
    { detail: excerpt }))` — 复用既有事件通道
  - ChatPanel.tsx 既有 listener（line 144-167）自动 prefill input +
    设光标末尾
- tooltip 用模板字符串避免 ASCII 双引号 nested 在 JSX string-literal
  里破 parser；文案讲清与既有 4 按钮的差异化定位

## Key design decisions

- **复用 `pet-mini-respond-to` 而非新事件**：bubble row 已有"💭 针对
  这条问"按钮 dispatch 同事件 — 行为完全对偶。owner 在 chat bubble 上
  按"针对这条问"与在选区 toolbar 上按 💬 都得到同一种 prefill 体验，
  心智一致。
- **80 char cap (vs bubble 30 char)**：bubble 自动 dispatch 走 30 字
  cap（短 trim 防长 message 撑爆 prefix）；选区是 owner 主动挑选 — 给
  更宽 cap 让选段保留 more context。但仍防超长撑爆 input UI。
- **flatten whitespace**：`replace(/\s+/g, " ")` 让多行选段折叠成单
  行 — prefix in input 不该跨多行（破坏 input 视觉）。
- **位置紧贴 🔄 改写之前**：💬 / 🔄 都是"让 AI 处理选段"类语义（vs
  💾 / 📝 / 📋 是"保存 / 复制"类）；adjacency 让 owner 视觉看到"两类
  导出 / 两类 AI"四段分组。
- **不引入 unit test**：纯 dispatchEvent + DOM；既有 pet-mini-respond-
  to listener 已被 bubble respondBtn 入口覆盖；通过 vite build + 真实
  交互验证。
- **不显 toast 反馈**：ChatPanel input 立即 prefill + scroll input
  into view 已是显著视觉反馈；toast 反而扰乱。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.25s)
