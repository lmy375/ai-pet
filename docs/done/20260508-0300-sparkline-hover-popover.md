# mood sparkline 鼠标悬停浮窗 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> mood sparkline 鼠标悬停浮窗：当前柱体 hover 只有 title tooltip 显文字；做一个 inline 浮窗显示当日最 dominant motion 颜色 + 计数 + "点击展开"提示。

## 目标

sparkline 柱体 hover 现在只有原生 `title` tooltip（无颜色 / 无强调）。
本轮加 inline 浮窗：hover 时柱子上方弹一个小气泡显示
- 日期
- dominant motion（颜色块 + 名字 + count）
- 共 N 次
- "点击展开" 提示

让快速 skim 时不用先看到 tooltip 才知道"今天最常见的情绪是哪个"。

## 非目标

- 不替换 `title` tooltip —— 系统级 tooltip 在键盘 focus / accessibility
  路径上仍有用（用户禁用 hover 浮窗 / 屏幕阅读器场景）。两者并存，按
  实现成本最低。
- 不做 popover 拖动 / pin —— 鼠标移开自动消失即可，简化交互。
- 不在 split (早晚分段) / filter 模式下做特殊浮窗内容 —— dominant motion
  / total 在所有模式下语义稳定（dominant 永远基于 day.motions 全量；
  filter 模式下"点击展开"仍合理）。

## 设计

### state

每个 SparklineBar 内 `hover: boolean` 局部 state（不上提；只这一处用）。
onMouseEnter/Leave 切换。

### 浮窗结构

外层 bar div 加 `position: relative`，浮窗 `position: absolute, bottom: 100%`
+ `transform: translateX(-50%) + left: 50%` 居中柱顶。

内容：
```
2026-05-04
🟧 Flick3 × 5
共 7 次 · 点击展开
```

### dominant motion

`Object.entries(day.motions).sort(([, a], [, b]) => b - a)[0]`。空 bar
（filterCount === 0 分支）的 hover 浮窗显 "当日没有记录" + "点击展开
仍然有效"（不阻拦 user 进 entry 列表）。

### 视觉

浮窗：白底、1px 浅灰边、4px 圆角、阴影 0 1px 4px rgba(0,0,0,0.1)、
zIndex 10、`pointer-events: none`（防止浮窗自身吃掉 mouse leave）。

### 复用

颜色用 MOTION_META，未知 motion 走 FALLBACK_MOTION_COLOR。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | hover state + onMouseEnter/Leave |
| **M2** | 空 bar 分支：浮窗显"当日无记录" |
| **M3** | 非空分支：dominant motion + count + 共 N + 提示 |
| **M4** | tsc + build + cleanup |

## 复用清单

- 既有 MOTION_META / FALLBACK_MOTION_COLOR
- 既有 day.motions / day.total

## 进度日志

- 2026-05-08 03:00 — 创建本文档；准备 M1。
- 2026-05-08 03:10 — M1 完成。SparklineBar 内 `hover: boolean` state；dominant motion 计算：filter 模式下取该 motion，否则按 count desc + MOTION_META 顺序破 tie；空 day 返 null。
- 2026-05-08 03:15 — M2 + M3 完成。`popover` 浮窗：position absolute, bottom 100% + translate -50% 居中柱顶；空 day 显 "当日无记录 · 点击仍可展开"；非空显 date + 颜色块 dominant + count + 共 N + 点击展开。pointer-events none 防吃 mouseLeave。
- 2026-05-08 03:25 — 三个 return 分支（empty / halfDay split / regular）全部包到 wrapper：外层 flex relative + onMouseEnter/Leave；内层保持原 bar 视觉（width: 100% 替换原 flex:1）。
- 2026-05-08 03:30 — M4 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 997ms)。归档至 done。
