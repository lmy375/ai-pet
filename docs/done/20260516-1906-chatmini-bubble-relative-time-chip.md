# ChatMini bubble 底"⏱ N 分前"相对时间 chip

## 背景

ChatMini bubble 顶已有 hover-only `[HH:MM]` 时钟 chip。但 owner 翻历史时，常想知道"这条是多久之前的" —— 看 `[14:32]` 还得 mental math（"现在 14:55 → 23 分前"）。day-relative 信息（"今天 / 昨天 / N 天前"）只在 hover title attr 里。

加一个 bubble 底"⏱ 23 分前"相对时间 chip，与顶时钟 chip 对偶 —— 顶给绝对时刻、底给"距现在多久"。owner 不必心算或停留 hover 看 tooltip。

## 改动

### `src/components/ChatMini.tsx`

#### 1. 新 helper `formatBubbleRelative(ts, now)`

```ts
function formatBubbleRelative(ts: string | undefined, now: Date = new Date()): string {
  if (!ts) return "";
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return "";
  const ageMs = now.getTime() - d.getTime();
  if (ageMs < 60_000) return "刚刚";
  if (ageMs < 3_600_000) return `${Math.floor(ageMs / 60_000)} 分前`;
  if (ageMs < 86_400_000) return `${Math.floor(ageMs / 3_600_000)} 时前`;
  // 跨日：startOfDay 比对，相邻日历日 = "昨天"
  const startOfDay = (dt: Date) =>
    new Date(dt.getFullYear(), dt.getMonth(), dt.getDate()).getTime();
  const diffDays = Math.round((startOfDay(now) - startOfDay(d)) / 86_400_000);
  if (diffDays === 1) return "昨天";
  if (diffDays >= 2) return `${diffDays} 天前`;
  return "";  // 未来时刻：系统时钟回拨，不渲
}
```

格式：刚刚 / N 分前 / N 时前 / 昨天 / N 天前 —— 短串适合 9px chip 紧凑显示。

#### 2. CSS hover-reveal 类

```css
.pet-mini-row .pet-mini-row-rel {
  opacity: 0;
  transition: opacity 120ms ease-out;
}
.pet-mini-row:hover .pet-mini-row-rel {
  opacity: 0.5;
}
```

存在感比顶 `.pet-mini-row-time` (0.55) 还低 (0.5) —— 相对时间是 ambient 信号，比绝对时间更被动。

#### 3. Bubble 底 chip 渲染

```tsx
{hasValidTime && !hiddenTimestampIdx.has(idx) && (() => {
  const rel = formatBubbleRelative(m.ts);
  if (!rel) return null;
  return (
    <span
      className="pet-mini-row-rel"
      title={`相对时间 · ${formatFullTimestamp(m.ts)}`}
      style={{
        position: "absolute",
        bottom: -10,
        [m.role === "user" ? "right" : "left"]: 8,
        fontSize: 9, color: muted, ...,
      }}
    >
      ⏱ {rel}
    </span>
  );
})()}
```

- gate 同顶 chip：`hasValidTime && !hiddenTimestampIdx.has(idx)` —— burst 中间合并不重复显
- user 行 right: 8 / assistant 行 left: 8（与 bubble 对齐方向同侧）
- bottom: -10 让 chip 浮在 bubble 下沿外 (不抢内容)
- title attr 加 "相对时间 · " 前缀 + full ts 让 hover 仍可拿精确时刻

## 关键设计

- **底 chip 与顶 chip 信号对偶**：顶 = `[14:32]` 绝对；底 = `⏱ 23 分前` 相对。两条信息平行而不冗余 —— 想"准点 / mental math 计算时差" 看顶；想"快读一眼 ambient 时差" 看底。
- **hiddenTimestampIdx 同 gate**：burst 折叠（连续 < 60s 同 role 消息中间合并）时顶 chip 不显，底 chip 也跟随。一致行为；burst 中间相对时间高度近似（"5 分前 / 5 分前 / 5 分前"重复显意义低）。
- **opacity 0.5 < 顶 0.55**：信号优先级排序 —— 顶时钟比底相对更"准确"，更值得 attention。底 chip 是 nice-to-have ambient。
- **跨日"昨天 / N 天前"用 startOfDay 比对**：而非 ageMs / 86400000 单纯桶。23:00 发的消息现在 01:00 看应显"昨天"（startOfDay diff = 1）而非"2 时前"。
- **未来时刻空串不渲**：罕见 / 系统时钟回拨 → 不显错误信息，gate `if (!rel) return null` 直接跳过。
- **inner title attr 复用 `formatFullTimestamp`**：与顶 chip 同源；底 chip hover 也能看完整时间字符串。
- **不引新 state / event**：纯 inline 计算 + CSS hover-reveal；与既有 row hover pattern 完全一致。

## 不做

- **不做 60s 自动刷新**：相对时间随时间漂移（"5 分前" → "6 分前" → ...），技术上要 useState + setInterval 强制 React 重渲。但 ChatMini 整个组件本来已经因 messages prop 变更 + sessionTokens / nowTasks 等 poll 持续重渲；新消息追加自然触发渲染，活跃对话中 chip 实时跟着新。"静默 30 分不动" 用户也不会盯着 chip 看精确分钟数；下次 hover 时一定已重渲。
- **不在 ChatMini stream 段也渲**：streaming bubble 没 m.ts，自然 fallback gate `hasValidTime` 跳过。
- **不为 burst-collapsed 行加 "ts 已合并" hint**：burst 中间不显是有意的；hover 整 bubble 还可看 title 显完整 ts。
- **不写测试**：纯字符串 / `Date.parse` 算术；视觉验证（hover bubble → 顶 [HH:MM] + 底 ⏱ N 分前 同时浮起 0.5/0.55 opacity）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.22s
- 改动 ~75 行（helper 22 + CSS 8 + JSX IIFE 35 + 注释 10）。既有顶 `pet-mini-row-time` chip / hiddenTimestampIdx 折叠 / formatFullTimestamp / hover 双击 ref 路径完全不动。

## TODO 状态

剩 0 条 —— TODO.md 再次空。下一 cron tick 进入 auto-propose 分支自动出新需求。

## 后续

- 顶时钟 chip 与底相对 chip 加 ⌘+ 点击复制功能 —— 单击 hover-reveal 时方便 quick-copy 时间到剪贴板。
- 跨周 / 跨月分隔条：现在已有 dateKeyFromTs 分日，可扩展显 "本周一开始 / 上周"段分隔。
- ChatMini 每 60s ticking 强制重渲一次让相对 chip 自然漂移（trade-off：低优先 vs idle wakeup cost）。
