# ChatMini bubble 角色操作 hover hint chip（iter #573）

## Background

ChatMini 既有多个 resend / reroll 入口但 discoverability 弱：
- ⌘R 重发上句（最后一条 user message）— keyboard-only，新用户不知
- 右键 ctx menu「↺ 重发本条」— 需右键才发现
- ⌘+click bubble 复制全文 — 文档化但未在 UI 表现

owner 经常想「重发上一句」/「改一改再发」但要先记住快捷键或翻 ctx
menu。本 iter 加 hover-revealed hint chip 把既有功能挂在 bubble 上
当 discoverability nudge。

## Change

`ChatMini.tsx`：

1. CSS class `.pet-mini-row-action-hint`（opacity 0 → 0.4 on row hover；
   比 .pet-mini-row-rel 0.5 略 muted — ambient 提示级）
2. Chip 渲染在每条 bubble row：
   - **user bubble** (任意位置)：「✏️ ⌘+click 编辑重发」— 提示 ⌘+click
     复制 path + 编辑工作流
   - **assistant bubble + isLast**：「↺ ⌘R 重发」— 提示 ⌘R reroll
     最后 user message。仅 isLast 显（⌘R 只对最后 user 生效，给中间
     assistant 显误导）
3. 位置：底部偏外侧（user 左 / assistant 右 — 与 ⏱ rel chip 反向
   避免叠加噪音）
4. tooltip 完整解释 workflow（user：粘到 input 编辑后 send；assistant：
   不必复制再粘贴）

## Key design decisions

- **仅 hint 不挂 onClick**：discoverability nudge — owner 看到提示后
  自己用对应快捷键 / 右键。avoid 让 chip 自身成新 action 入口（与
  ctx menu / ⌘R 重复就乱）。tooltip 是 long-form 说明，文字是 short
  reminder
- **assistant `isLast` gate**：⌘R 只对最后一条 user 生效 — 给中间
  assistant 显「⌘R 重发」会让 owner 期待错误。仅 isLast 时 chip 才
  挂出
- **user 一律显**：右键 ctx menu「↺ 重发本条」+ ⌘+click 复制对任
  user bubble 都生效；没必要 isLast gate
- **muted italic + opacity 0.4**：信号优先级低于 ⏱ rel (0.5) — 提
  示性质更弱，不抢眼；ambient awareness 用
- **位置与 ⏱ rel chip 反向（user 左 / assistant 右）**：⏱ rel 在
  user→右 / assistant→左；本 chip 在 user→左 / assistant→右 — 不
  叠加 + 视觉对称
- **CSS class 走 pet-mini-row hover 触发**：与既有 row-rel / row-chars
  / row-time / row-copy 同 pattern — 整 row hover 时一起 reveal，
  不需各 chip 独立 hover 状态

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — 单 chip 加在熟悉 bubble-row absolute 位置（与
  ⏱ rel chip 同 family），无 layout race

## Future iters (out of scope)

- **中间 assistant bubble 加 reroll**：当前 ⌘R 仅最后 user；想让
  arbitrary 中间 assistant 触发 reroll 需 backend session-rewind 路径
  （rebuild context to point N + resend user_(N-1)）— scope 大，单独
  propose
- **chip click 触发对应 action**：当前仅 hint 不挂 click；future 可
  改成 click → 直接触发 ⌘R 等价 / 自动 ⌘+click 复制。但需评估「chip
  是 hint 还是 action」对 owner 心智模型的影响 — 防 ambiguity
- **顶 chip 整理**：bubble 顶 ⏱ ts / 📊 chars / 底 ⏱ rel / 底 action
  hint 已 4 chip — 边角密集。propose 「unified chip rack」按 hover
  expand 展示全 chip family。但当前各 chip 互不冲突，先观望
- **i18n**：中文文本（编辑重发 / 重发上句）若日后多语，需文案表。
  format 同 formatBubbleRelative pattern 抽出
