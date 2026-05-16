# 桌面 mini chat ts label hover 显完整时间戳 tooltip

## 背景

TODO 上 auto-proposed 一条："桌面 mini chat 消息 ts label hover 显完整时间戳 tooltip：当前折叠后只看到相对时间，hover 应给精确时刻。"

ChatMini 消息上方的 ts label（绿框小角标 `[14:35]`）只显时 / 分。这有两个不足：

1. 跨日 / 跨年场景下没日期，owner 看不出"这条是今天的还是昨天的"。
2. 60s burst 折叠规则下，密集对话中间 ts 标签会隐藏 —— 只剩首末显，但 owner 想精确知道"那条消息几点几分几秒"。

bubble 自身 hover 已有 title attr 但内容是 `[HH:MM]` + 双击 hint，仍不够精确。ts label 自身因 `pointerEvents: "none"` 不接收 hover，无 tooltip。

补 `formatFullTimestamp(ts)` helper + 给 ts label 加 title attr + 取消 pointerEvents:none，让 owner hover 小角标即得到 `2026-05-16 14:35:42 周五 · 今天` 这样的完整时刻。

## 改动

### `src/components/ChatMini.tsx`

#### 新 `formatFullTimestamp` helper

```ts
function formatFullTimestamp(ts: string | undefined, now: Date = new Date()): string {
  if (!ts) return "";
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return "";
  const y = d.getFullYear();
  const mo = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  const weekdays = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];
  const wd = weekdays[d.getDay()];
  // 相对天数：0=今天 / 1=昨天 / -1=明天（罕见） / 2..29=N 天前 / -2..-29=N 天后
  const startOfDay = (dt: Date) =>
    new Date(dt.getFullYear(), dt.getMonth(), dt.getDate()).getTime();
  const diffDays = Math.round(
    (startOfDay(now) - startOfDay(d)) / (24 * 60 * 60 * 1000),
  );
  let rel: string;
  if (diffDays === 0) rel = "今天";
  else if (diffDays === 1) rel = "昨天";
  else if (diffDays === -1) rel = "明天";
  else if (diffDays > 1 && diffDays < 30) rel = `${diffDays} 天前`;
  else if (diffDays < -1 && diffDays > -30) rel = `${-diffDays} 天后`;
  else rel = "";
  return `${y}-${mo}-${day} ${hh}:${mm}:${ss} ${wd}${rel ? ` · ${rel}` : ""}`;
}
```

放在 `formatBubbleTimestamp` 之后（同字符串 helpers cluster）。

#### ts label 加 title + 删 pointerEvents

```tsx
<span
  className="pet-mini-row-time"
  title={formatFullTimestamp(m.ts)}
  style={{
    // ...
    // pointerEvents 默认 auto —— hover 触发 title native tooltip
    background: "var(--pet-color-card)",
    // ...
  }}
>
  {timeLabel}
</span>
```

去掉 `pointerEvents: "none"` 让 hover 注册到 span。

## 关键设计

- **`formatFullTimestamp` 取代 bubble title 不必要**：bubble 自身 title 仍是 `[HH:MM] + 双击 panel hint` —— bubble 是 message body 容器，title 与 message 主信息（双击行为）相关；ts label 自身 hover 才显完整时刻，分工清晰。
- **去 pointerEvents: "none"**：ts label 是 absolute 浮在 bubble 上方 `top: -12`，物理面积只覆盖 bubble 顶部 ~12px。去掉 `none` 后 hover label 物理区域 → 触发 title。bubble 主体面积仍可正常 hover 自身 title。
- **完整 timestamp 含 `周X` + 相对天数**：日期单看 `2026-05-16` 数字感强，加 `周五` 让 owner 立刻知道"周末发的"。相对天数 `今天 / 昨天 / N 天前` 让 owner 跨日浏览时不必心算。30 天 cap 防 too-old 消息显荒诞值（`152 天前`）。
- **`now` 参数注入**：纯函数 + 显式 wall clock 让将来加单测时不依赖系统时钟（虽然当前未引入 vitest）。
- **空串 fallback 而非 `[?]`**：title attr 空串 = 浏览器不显 tooltip。owner hover 无 ts 的 row 不应弹空 tooltip。
- **不影响 burst 折叠规则**：60s 内同 role 中间 ts label 仍隐藏（hiddenTimestampIdx 判断）。本 iter 只让"显出来的那些" ts label 多一个 hover tooltip。

## 不做

- **不去 pointerEvents: "none" 影响 bubble click**：ts label 物理面积只 ~12px 高 + 数十 px 宽，不挡 bubble 主区域（即便挡了，浏览器仍把 click 派给最上层元素，bubble click 行为不受影响）。
- **不写测试**：纯字符串拼接 + Date 算术，逻辑 ~20 行；既有 formatBubbleTimestamp / dateKeyFromTs 等同类 helper 都无单测。视觉验证（hover ts → tooltip 显完整时刻）足够。
- **不接 PanelChat 消息**：那里有更丰富的元数据 chip + 自身 title attr，不需要补 ts label tooltip。本 iter 专注 ChatMini ts label。
- **不加更多语言**：当前 `周一` `周五` 中文硬编码；i18n 不在本 iter 范围。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.14s
- 改动 ~50 行（formatFullTimestamp helper 30 + ts label title 5 + 去 pointerEvents 1 + 注释）；既有 formatBubbleTimestamp / 折叠规则 / pet-mini-row-time CSS / bubble title 路径完全不动。

## TODO 状态

6 条 auto-proposed 已完成 2 条，余 4 条留池：
- PanelMemory 类目内 items > 20 时按 updated_at 月份分组
- ChatPanel session tab 右键加「📋 复制会话 ID」
- detail.md preview「📑 大纲」浮窗
- 任务 detail.md 中文配对引号 / 括号

## 后续

- 同款 `formatFullTimestamp` 扩到 PanelChat 消息（如果用户反馈那边的精度也不够）。
- ts label 显"刚刚"等更柔和的相对词：< 1 分钟 / < 5 分钟 / < 1 小时 各档不同语义文本。当前 `[HH:MM]` 已经精确到分钟，过度软化没意义。
- tooltip 多行显示（不同信息分行）—— native browser tooltip 不支持多行，要 custom popover 才能实现。HTML title 单行足够本 iter 范围。
