# 决策日志按 kind 多选 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 决策日志按 kind 多选：现 chip 互斥单选；改成 multi-select（点 chip 加入 / 再点取消），让"看 Spoke + LlmSilent 但忽略 Skip"场景成立。

## 目标

`PanelDebug` 决策日志现 chip 是互斥 4 档（全部 / Spoke / LlmSilent / Skip）。
但常见复盘场景是"我想看开口和沉默两类来对比，但屏蔽 Skip 的噪音" —— 互
斥过滤逼用户在两次切换间来回扫视。本轮把决策日志的 chip 行改成 multi-
select：点 chip 加入选中集合，再点取消；空集合 = "全部"。

## 非目标

- 不动其它两个 timeline（feedback / tool_call risk_level）—— 它们的 chip
  行复用 `PanelFilterButtonRow`，单选语义符合各自需求；硬把组件抽象成
  multi-mode 反而让简单的两条受 API 干扰。
- 不持久化选中集合 —— 与既有 PanelDebug 切换型 state（reason search /
  newestFirst / dueFilter 等）一致。

## 设计

### state 类型变更

`decisionFilter: "all" | "Spoke" | "LlmSilent" | "Skip"` →
`decisionKinds: Set<string>`，空 Set = 全部。

### chip 行：脱离 PanelFilterButtonRow 内联实现

PanelFilterButtonRow 是单选 abstraction（active V vs onChange(V)），多选语
义不同（点同一 chip 是 toggle）。强行扩展会让其它复用方接 surprise。本轮
在 PanelDebug 内联 mini-chip 行，与 `MotionFilterChips` / `DueChip` 同模式。

chips：
- "全部 N"：active 当 `decisionKinds.size === 0`；点 → 清空 Set
- "开口 N" / "沉默 N" / "跳过 N"：active 当 Set 包含；点 → toggle in/out

### 过滤逻辑

```ts
const kindFiltered = decisionKinds.size === 0
  ? decisions
  : decisions.filter((d) => decisionKinds.has(d.kind));
```

替换原 `decisionFilter === "all" ? ... : decisions.filter(d => d.kind === decisionFilter)`。

### tooltip 文案

"全部" 的 title：依然 "显示全部决策..."。
其它 chip：active 时 "再次点击移出过滤集合"，inactive 时 "加入到只看的 kind
集合（多选）"。

## 测试

PanelDebug 是 IO 重容器，前端无 vitest。Set 操作纯 React state；靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state 类型替换 + 过滤逻辑改 Set.has |
| **M2** | chip 行内联实现（4 chip + toggle 行为） |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 chip 视觉（fontSize 10 / 圆角 / accent 配色）—— 内联时按既有
  PanelFilterButtonRow 的样式 spec 直抄
- 既有 reason search / newestFirst / 排序 / 渲染 不动

## 进度日志

- 2026-05-07 12:00 — 创建本文档；准备 M1。
- 2026-05-07 12:10 — M1 完成。`decisionFilter` enum 替换为 `decisionKinds: Set<string>` (empty=全部)；filter 链改 `decisionKinds.has(d.kind)`。
- 2026-05-07 12:20 — M2 完成。脱离单选 PanelFilterButtonRow（保留给其它两 timeline），内联实现 multi-select chip 行：「全部」点击清空，其它 chip 点击 toggle in/out；视觉规格抄自原组件保持一致；title hover 文案区分 active "再次点击移出" 与 inactive "加入到只看的 kind 集合"。
- 2026-05-07 12:25 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 920ms)。归档至 done。
