# PanelMemory cat header「📅 N 前」cat 寿命 chip（iter #602）

## Background

PanelMemory cat header 已有：
- 📊 7d +N（活跃度 7d 视角）
- 📊 30d +N（活跃度 30d 视角）
- 📊 概览（snapshot 总量 + 最近 update ts）

缺 **cat 寿命 axis** — 「这 cat 多老」audit。新建 cat 与跟了多年的
老 cat 在「📊 概览」表面无差。本 iter 加「📅 N 前」chip 显单 cat
内 min(items.created_at) 距 now 的相对时间。

## Change

`PanelMemory.tsx` cat header 紧贴 📊 30d chip 后插「📅 N 前」chip：

```tsx
{cat.items.length > 0 && (() => {
  let earliestMs = MAX_SAFE_INTEGER;
  for (const it of cat.items) {
    if (!it.created_at) continue;
    const cMs = Date.parse(it.created_at);
    if (!isNaN(cMs) && cMs < earliestMs) earliestMs = cMs;
  }
  if (earliestMs === MAX_SAFE_INTEGER) return null;
  const ageMs = now.getTime() - earliestMs;
  const days = Math.floor(ageMs / 86_400_000);
  // 4-tier label：今日 / N 天 / N 周 / N 月 / N 年
  // click 复制「<label> · 起于 YYYY-MM-DD（N 前）」单行
})()}
```

## 4-tier label

- `days < 1` → 「今日」
- `days < 7` → 「N 天」
- `days < 30` → 「N 周」
- `days < 365` → 「N 月」
- 否则 → 「N 年」

平衡精度与简洁 — 新 cat 看精确天数；老 cat 看月/年粒度即可。

## Output

chip text:
```
📅 2 月前
```

click 复制：
```
butler_tasks · 起于 2026-03-15（2 月前）
```

tooltip：完整 ISO 日期 + 相对时间双 anchor。

## Key design decisions

- **min across items.created_at**：cat 寿命定义为「最早建立的 item
  时间点」；新建空 cat 后填第一条 item = cat 始诞。defensive：
  忽略 invalid / 空 created_at
- **相对时间 4 档**：今日精确日数 / 周内整周 / 月内整月 / 年内整年。
  比固定单位（如全显天数）更人类可读
- **不可比较 epoch time 0 兜底**：用 Number.MAX_SAFE_INTEGER sentinel；
  仍 max 时 return null 不渲
- **复用 `now` state（1s tick）**：跨自然边界（如「29 天前」→「1 月
  前」）自动 refresh
- **与既有 cat header chip family 视觉一致**：相同 `{...s.btn, marginLeft:
  4}` 样式 — chip-bar 视觉密度 consistent

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — chip 加在熟悉 cat header chip-bar 位置，无
  layout race / 额外 IO

## Future iters (out of scope)

- **「📅 oldest item」click 跳该 item**：当前 click 复制 single line；
  future 可 click 滚动 / focus 到 cat 内最早 item。需 scroll-to + flash
  highlight 路径
- **cat 内「N items / N+ years」生命表 chip**：显「156 条 · 起于
  2024-03 · 跟了 2 年」综合 chip — 信息密度高但 chip 文字偏长。按需
- **新 cat alert chip**：cat 寿命 < N 天时染色高亮「新 cat」— 与
  既有 chip 套 onboarding 路径相关；按需
