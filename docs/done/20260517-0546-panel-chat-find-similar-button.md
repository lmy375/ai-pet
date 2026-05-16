# PanelChat 历史消息「🔍 在当前 session 找类似」hover 按钮（iter #255）

## Background

PanelChat 顶部已有跨 / 本会话 search bar（🔍 按钮唤起），但 owner 要从某条历
史消息出发"找本 session 内类似讨论"必须：手动点 🔍 → 切 scope 到 current →
手敲 query。

更直观的入口是：直接在该消息 hover 时弹一个 🔍 按钮，click 即把这条消息文
本作 query 触发 search bar（scope 强制 current）。一步到位，与已有的 📌
mark / 复制按钮同 hover-only 显隐模板。

## Changes

- `src/components/panel/panelChatBits.tsx`：
  - `CopyableMessage` 加新 prop `onFindSimilar?: (text: string) => void`
  - 新增 `findSimilarButton`（与 markButton / copy button 同 `.pet-copy-btn`
    hover-only 类）：仅 content 非空 + 传 callback 时挂；🔍 图标；click 调
    `onFindSimilar(content)`
  - user 行按钮序：`markButton / findSimilarButton / copyButton / bubble`
  - assistant 行按钮序：`bubble / reactionRow / copyButton / findSimilarButton
    / markButton`（与 user 行对偶 — 内侧 reaction 与外侧 markButton 不动，新
    按钮挂复制按钮外侧）

- `src/components/panel/PanelChat.tsx`：
  - 新增 `handleFindSimilarInSession(text)` useCallback：
    - 把消息文本 normalize（`replace(/\s+/g, " ").trim().slice(0, 30)`）作
      query — 30 字够触发匹配且不挤爆 input
    - `setSearchMode(true) / setSearchScope("current") / setSearchQuery(q)`
  - 两条 CopyableMessage（user + assistant）调用都传 `onFindSimilar={
    handleFindSimilarInSession }`

## Key design decisions

- **截短到 30 字 + 折掉换行**：长消息整段做 search query 会让 input 撑爆 +
  匹配过严（query 太长几乎不可能精确命中其它消息）。30 字平衡"独特性"
  vs "可视性"，与 ChatMini 既有 "💭 针对这条问" 的 excerpt 30 字裁切习惯
  一致。
- **强制 scope = current**：这条按钮的核心语义是"找本会话内类似"；如果 scope
  保留 owner 上次值（all / current），用户体验有歧义。强制 current 让按钮
  含义稳定；owner 想跨会话搜可手切 scope 或用顶部 🔍 入口。
- **复用 CopyableMessage 而不是新 wrapper 组件**：CopyableMessage 已是
  message hover-button 的标准载体（mark / copy / reaction 都在内）；新增
  prop 路径轻量，不破坏既有调用方。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)
