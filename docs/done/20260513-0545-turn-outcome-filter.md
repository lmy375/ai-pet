# PanelDebug 看上次 prompt modal — 加 outcome filter chips

## 需求

ring buffer 保留最近 5 turns，但调 prompt 时常想"只看 silent 的几
条"或"只看 spoke 的几条"。当前 prev/next nav 没法跳过不相关 outcome，
得手动翻过去。补三段 filter chips（全部 / 开口 / 沉默），与 chip 内
计数让用户一眼看到子集大小。

## 实现

`src/components/panel/PanelDebug.tsx`：

- 新 state `turnOutcomeFilter: "all" | "spoke" | "silent"`
- 新派生 `filteredTurns = recentTurns.filter(...)`：
  - "all" → 全集
  - "silent" → outcome === "silent"
  - "spoke" → outcome === "spoke" 或 undefined（老 ring 项升级前没字段
    时按 spoke 兜底，与 R25 二态对齐）
- `currentTurn` 改读 `filteredTurns[turnIndex]`
- prev/next nav button + counter 全部 from `recentTurns` → `filteredTurns`
- 在 modal header（"proactive 的 prompt + reply" 标题之后、prev/next
  导航之前）加 chip 行：
  - 三 chip：全部 / 开口 / 沉默，每个带计数 `(N)`
  - active chip：accent border + accent fg + bg-tint + bold + "✓ " prefix
  - 切换 filter → reset turnIndex=0（防越界）
  - 仅在 recentTurns.length > 0 时浮（无数据 noop）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 打开 modal：默认 "全部 (5)" 状态，prev/next 翻全 5 条
  - 点 "沉默 (3)" → 仅显 silent turn，counter 变 1/3 → 翻 prev/next 仅跳
    silent 子集
  - 点 "开口 (2)" → 切到 spoke 子集
  - 点 "全部" → 回到 5 条全集
  - 任何切换都把 turnIndex 重置 0 → 首条命中
  - filteredTurns 为空时（如选 silent 但没有 silent turn）→ "全部" chip
    仍可用切回，counter 显 0
  - tool calls / outcome chip / 全文复制 / issue 模板等所有 modal 内功能
    都基于 currentTurn → 自然跟随 filter

## 不在本轮范围

- 没做 multi-select filter（同时开口 + 沉默 = 全部，逻辑等价）：三段
  互斥 enum 已经覆盖；多选语义模糊
- 没做按 tool 用过滤（仅显含某工具的 turn）：tool 维度多 + 命中疏，
  scope 翻倍；future 可加 secondary chip
- 没做"高亮 outcome 标记"：现 outcome chip 自然显在 modal header（既
  有 spoke/silent 标签）

## TODO 池剩余

- PanelChat 消息加 "📌 标记" 按钮
