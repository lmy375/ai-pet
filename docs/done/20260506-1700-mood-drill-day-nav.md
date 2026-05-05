# mood_history drill 跨日导航 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> mood_history drill 跨日导航：当日详情面板加 ‹ › 按钮，无需回 sparkline 即可看前一/后一天的 entries，连续复盘"这一周哪天最躁"。

## 目标

mood sparkline 已支持点格子展开当日 entries + motion filter + 复制为 MD。
但用户连续复盘"这一周每天发生了什么"时，要点开周一 → 看完 → 关闭 →
点周二柱 → ……来回切。本轮在当日详情头部加 ‹ › 按钮，直接前/后翻一天，
无需回 sparkline。

## 非目标

- 不超出 7 天窗口 —— 后端 `get_mood_daily_motions(days=7)` 是 sparkline
  的固定窗口；扩窗需要单独需求重做后端 API + 性能评估。
- 不做空日跳过 —— 用户可能想看到"周三完全没记录"这件事本身（也是一种
  情绪信号：那天可能彻底沉默），所以 ‹ › 走每一天，而不是只跳有 entry
  的天。
- 不做键盘快捷键 ←/→ —— 多个面板挂全局 keydown 已有键盘 nav 冲突风险；
  先做按钮，必要时再加。

## 设计

### 数据来源

`daily: DailyMotion[]` 是后端返回的 7 天升序数组（最旧 → 最新，与
`mood_history.rs::summarize_motions_by_day` 一致）。所以：
- prev (‹) = idx - 1（更早一天）
- next (›) = idx + 1（更晚一天）

### 边界

- selectedDate 命中第 0 个 → prev 按钮 disabled
- selectedDate 命中末尾 → next 按钮 disabled
- selectedDate 不在 daily 里（不该发生 — selectedDate 只能由点 sparkline
  产生）→ 两个按钮都 disabled

### UI

在头部行 `{selectedDate} · 当日 N 条` 之前插入 `‹ ›` 两个小按钮。视觉与既
有「关闭」按钮 / 「复制为 MD」按钮统一（10px 字、灰底白字、相同 padding /
border-radius）。

### state 影响

`selectedDate` 改变 → 既有 `useEffect([selectedDate])` 自动重新 fetch
当日 entries + reset entryFilter / copiedDayMd。**复用既有 effect，不改
其它行为**。

### 纯函数

`adjacentDate(daily: DailyMotion[], current: string, delta: -1 | 1):
string | null` 放在 `formatDayEntriesAsMarkdown` 旁。返回相邻日期或 null。

## 测试

`adjacentDate` 是纯函数；前端无 vitest，靠 tsc + 手测。逻辑足够小，类型
和边界已被 `?? null` 覆盖。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `adjacentDate` 纯函数 |
| **M2** | header 行加 ‹ › 按钮 + disabled 边界 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 `daily` prop 数据
- 既有 `selectedDate` state + useEffect fetch 链路
- 既有 header 按钮视觉

## 进度日志

- 2026-05-06 17:00 — 创建本文档；准备 M1。
- 2026-05-06 17:10 — M1 完成。`adjacentDate(daily, current, delta)` 纯函数加在 `formatDayEntriesAsMarkdown` 旁；越界 / not-found 返回 null 让按钮 disable。
- 2026-05-06 17:20 — M2 完成。当日详情头部行最前方插入 ‹ › 两个小按钮，IIFE 内联渲染避免拆组件；disabled 时灰字 + cursor:not-allowed + tooltip 解释"已是窗口最早/最晚一天"。点击 setSelectedDate 直接复用既有 useEffect 重新 fetch 流。
- 2026-05-06 17:25 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 976ms)。归档至 done。
