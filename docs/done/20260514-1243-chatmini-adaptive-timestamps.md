# ChatMini 时间戳自适应折叠

## 背景

TODO（auto-proposed 之前）：

> 聊天消息 timestamps 自适应折叠：连续 < 60s 同方消息只保留首末 ts，密集聊天不再被时间戳切碎。

ChatMini（桌面 mini chat）每条 bubble 上方都浮一个 `[HH:MM]` 小角标。当用户与宠物快速来回 5-10 条对话时（streaming 后 follow-up，或宠物连续主动开口）这些 ts 一个挨一个全是相同分钟，反而把信息切碎、抢视觉。专业 IM（iMessage / Telegram / Slack / Discord）的 ts 都是"密集时合并、稀疏时展示"—— 复现这套行为。

## 改动

### `src/components/ChatMini.tsx`

**1. 新 useMemo `hiddenTimestampIdx: Set<number>`**

```ts
const TIMESTAMP_BURST_GAP_MS = 60_000;
const hiddenTimestampIdx = useMemo(() => {
  const out = new Set<number>();
  const ts = (i) => { /* parse visibleItems[i].ts → ms | null */ };
  for (let i = 0; i < visibleItems.length; i++) {
    const cur = visibleItems[i];
    const curTs = ts(i);
    if (curTs === null) continue;
    const prev = visibleItems[i-1];
    const next = visibleItems[i+1];
    const tightPrev = prev?.role === cur.role && ts(i-1) !== null && curTs - ts(i-1) < 60_000;
    const tightNext = next?.role === cur.role && ts(i+1) !== null && ts(i+1) - curTs < 60_000;
    if (tightPrev && tightNext) out.add(i);
  }
  return out;
}, [visibleItems]);
```

**规则**：某条消息进入"hide" 集合的条件 = 前邻 AND 后邻都是"同 role + 时差 < 60s"。这给"burst 首尾保留 ts、中间消失"的自然行为：

- 5 条 user 连发（10:30:00 / 10:30:08 / 10:30:15 / 10:30:30 / 10:30:55）：仅第 1、5 条显 ts，中间 3 条隐。
- user → assistant 来回（每条间隔 < 60s）：role 不同，prev/next 检查都不通过 → 全部保留 ts。
- 单条消息：prev / next 不存在 → 自动保留。
- ts 缺失或解析失败：本身就不会显（`hasValidTime` 早 gate），不必进 hide 集。
- burst 之间 ts > 60s 的"分界条"：tightPrev / tightNext 一端 false → 保留 ts，让用户视觉上感知"换段"。

**2. 渲染 gate**

原 `{hasValidTime && (...)} ` 改为 `{hasValidTime && !hiddenTimestampIdx.has(idx) && (...)}` —— 单行改动。

**hover tooltip 不受影响**：bubble 自身的 `title` attr 仍写 `formatBubbleTimestamp(m.ts)` —— 用户想看精确时间总能拿到，只是 visual badge 自动折叠避免视觉噪音。

## 不做

- **不动 PanelChat**。Panel 大聊天框本身没有 ts 小角标（设计选择 — 长会话回看时 bubble 上方堆 ts 会更乱）；本特性是 ChatMini 专属。
- **不做"显示完整 burst 的 ts 跨度"（如『10:30–10:35』）**。中间隐 ts 加上首末 ts 已经传达足够时序信息；再加跨度文案徒增复杂度。
- **不让用户可调阈值**。60s 是经验上 IM 类应用都用的 burst 边界；localStorage settings 暴露的价值低。
- **不缓存 hide 集**。useMemo 已绑 visibleItems；burst 不变时 React 自动短路。N 通常 < 20，每次重算 < 0.1ms。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.13s
- 改动 ~30 行（hook 25 + render gate 1）；既有 timestamp 渲染、search hit、复制按钮等逻辑不动。

## 后续

- 同款逻辑可移植到 PanelChat（若未来 panel 也加 ts 角标，复用 hiddenTimestampIdx 风格的 helper 抽到 panelChatBits）。
- 当前 60s 阈值是常量；如果用户偶有反馈"我喜欢看每条 ts" 可加 localStorage `pet-chatmini-burst-collapse = "off"`。
