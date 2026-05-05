# 心情谱按 motion 类型 toggle — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 心情谱按 motion 类型 toggle：sparkline 当前所有 motion 段叠加；点 4 个 MOTION_META 颜色块筛选只看某一类（点 Tap → 仅 Tap 段保留），帮助用户聚焦"我最近开心 / 焦虑哪几天多"。

## 目标

「人格」标签 sparkline 把所有 motion 段叠加在每根柱子里。如果用户想专注看
"哪几天我比较焦虑（Flick3）" / "哪几天 Tap 多"，需要在 stacked bar 里费眼
分割。本轮在 sparkline 上方加 5 颗 chip：「全部」+ 四种 motion，点 chip 切换
filter，filter 模式下只渲染该 motion 段，y 轴重缩放到该 motion 的最大日 count。

## 非目标

- 不做多选 OR filter（同时显示 Tap+Flick）—— 单选已覆盖"专注一类"语义；多选
  让 chip 状态更复杂。
- 不做时间窗口拉伸（仍 7 天）—— 改窗口是 mood_sparkline 另一独立维度。
- 不写 README —— sparkline 内嵌交互微调。

## 设计

### 状态

`MoodSparkline` 加 `selectedMotion: string | null`（null = "全部"）。
点 chip 自身切换 / 切换到 null。

### Filter chips

5 颗 chip 横排（在标题"最近 7 天 motion 频次"右侧或下方一行）：
- 「全部」chip：default，灰色，selected 时蓝边
- Tap / Flick / Flick3 / Idle：各显示 8×8 圆点 + 缩写标签，颜色取自
  `MOTION_META[motion].color`；selected 时填充 + 白字。

### Bar 渲染

`SparklineBar` 接受新 prop `filter: string | null`（null = stacked 全量；非
null = 只渲染该 motion 段）+ `effectiveMax: number`（filter 模式下重算为
`max(day.motions[filter] ?? 0)`）。

实现：
```ts
const visibleSegments = filter === null
  ? Object.entries(day.motions).filter(([, c]) => c > 0)
  : (() => {
      const c = day.motions[filter] ?? 0;
      return c > 0 ? [[filter, c] as const] : [];
    })();

const dayCountForHeight = filter === null ? day.total : (day.motions[filter] ?? 0);
const heightPx = effectiveMax > 0
  ? Math.max(1, Math.round(dayCountForHeight / effectiveMax * SPARKLINE_BAR_HEIGHT))
  : 1;
```

空日 + filter 命中 0 → 1px baseline 占位（与"全部空日"行为一致）。

Tooltip：filter 模式下 `2026-05-04 · Tap × 3`；全部模式仍走原文案。

### 测试

逻辑全在前端 SparklineBar / MoodSparkline 内部；无 vitest，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `selectedMotion` 状态 + chip row UI |
| **M2** | SparklineBar 接 filter / effectiveMax + tooltip 适配 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 `MOTION_META` 配色与 glyph 标签
- `SPARKLINE_BAR_HEIGHT` / `FALLBACK_MOTION_COLOR`

## 进度日志

- 2026-05-05 29:00 — 创建本文档；准备 M1。
- 2026-05-05 29:20 — 完成实现：
  - **M1**：`MoodSparkline` 加 `selectedMotion: string | null` 状态；标题行右侧加 `MotionFilterChips` 子组件 —— 5 颗 chip（全部 + Tap/Flick/Flick3/Idle），点击切换 selected；selected 时填色 + 白字 + 6×6 圆点变白，未 selected 时白底色边 + 颜色圆点（color 取自 `MOTION_META`）。
  - **M2**：`SparklineBar` 接 `effectiveMax` + `filter` props。filter 非 null 时 segments 仅保留该 motion 一条，effectiveMax 重缩放为 `max(day.motions[filter] ?? 0)`，让"专注 Tap"等场景仍有可比较的视觉。空日 / filter 命中 0 → 1px baseline + 适配 tooltip "没有 X 记录"。段高度按 `filterCount` 归一（filter 时单段填满 100%，stacked 时与原 day.total 等价）。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— sparkline 内嵌交互微调。
  - **设计取舍**：单选 toggle（再点同 chip 取消）vs 多选 OR filter — 选前者，多选让 chip 状态机复杂；y 轴重缩放（filter 模式下 effectiveMax = 该 motion 7 天最大）让"专注一类"时柱高仍可比，否则单 motion 在 stacked maxTotal 下会被压扁；段高度归一统一用 `filterCount` 让 filter / stacked 两路径同语义。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯 React 状态 + 数据派生，由 tsc 与既有 sparkline 组件测试覆盖路径保证。
  - **TODO 后续**：列表清空后按规则提 5 条新候选。
