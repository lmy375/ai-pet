# PanelTasks 行右键「💬 推到 chat ref」(iter #340)

## Background

owner 想"让 pet 评论 / 提问 / 给建议某条 task"时当前流程：
- 在 PanelTasks 选 task → ⌘C 复制 title → 切到聊天 tab → 粘贴到 textarea
  → 写 "请评论这条..." 前缀 → 发送

4-5 步太重。本迭代复用既有 `onAskLLMAbout` prop（detail editor 选段
"🧠 ask LLM" 走的同通道）— 行右键点击一次 → 自动切聊天 tab + 预填
"关于「<title>」 " → owner 续写问题即可。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 行右键 ctx menu 在「📋 复制 raw_description」之后插「💬 推到 chat
  ref」按钮：
  - 仅 `onAskLLMAbout` prop 传入时渲染（PanelApp wire；其它 caller 无
    chat panel context 不显冗余 UI）
  - click → `setTaskCtxMenu(null)` → `onAskLLMAbout(m.title)` →
    setBulkResultMsg 3s toast "💬 已切到聊天 tab + 预填..."
  - tooltip 说明完整 UX 价值 + 与 ⌘C / 切 tab / 粘贴 3 步对比

## Key design decisions

- **复用 `onAskLLMAbout` 通道**：与既有 detail editor 选段 "🧠 ask LLM
  about selection" 同 wire；上游 `requestChatPrefillFromSelection` 自动
  封装成 `关于「<excerpt>」 ` + 切 tab，不必引新事件 / 新 prop。
- **传 title 而非 ref token `「title」`**：上游 helper 内部已加 `「」`
  包装；本入口传 raw title 让两条 caller 走完全同一路径，formatter 算
  法 stable 一致。
- **位置在「📋 复制 raw_description」之后**：与既有 raw-copy 类操作相
  邻 — "复制 / 推送" 两种"导出"语义并列让 owner 视觉上看到"轻 / 重两
  个出口"。
- **toast 含具体 title**：与既有 ctx menu 其它 action 同模式 — 让 owner
  确认"我刚推送的是哪条"。
- **不引入新 ref token 复制路径**：owner 想纯 ref token 粘到任意场合
  仍走「🔗 复制 ref token」（同 ctx menu 既有项）；本入口是"一键问
  pet" 复合动作。
- **`onAskLLMAbout` 可选 prop gate**：保 PanelTasks 在其它 host 也能
  渲染（如未来 standalone test 环境）— prop 缺时安全降级。
- **不引入 unit test**：纯 prop 透传 + ctx menu JSX；底层
  requestChatPrefillFromSelection 走既有 PanelApp 路径已被现有 detail
  editor 入口 cover。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.21s)
