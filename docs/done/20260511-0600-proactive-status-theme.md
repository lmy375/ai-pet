# PanelDebug proactiveStatus 状态文本配色迁 token（Iter R149）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug proactiveStatus 状态文本配色迁 token：line 1487 仍 hardcoded
> `#dc2626` (失败) / `#059669` (成功)，dark 主题下深底浅色对比度退化。
> 迁到 `tint-orange-fg` / `tint-green-fg`，与 R147 决策清空按钮同款
> tint 警示语义。

## 目标

R147 把决策清空按钮迁 token；本轮收尾紧邻区域的另一处 hardcoded —— header
内 `proactiveStatus` 状态文本。当前规则 `proactiveStatus.startsWith("触发失败")
? "#dc2626" : "#059669"`：

- light：红 700 / 绿 600，醒目但 fine
- dark：深 slate 底叠 #dc2626 红 / #059669 绿，对比度近 WCAG AA 边缘且与
  浮窗主体 (orange tint / green tint) 视觉脱节

## 非目标

- 不动 `startsWith("触发失败")` 判定逻辑（这是 status 来源约定）。
- 不动 maxWidth / ellipsis / title fallback。
- proactiveStatus 8s 自清空策略不动（在父 hooks 层）。

## 设计

### 迁移点

| state | from | to |
| --- | --- | --- |
| 触发失败 | `#dc2626` | `var(--pet-tint-orange-fg)` |
| 成功 | `#059669` | `var(--pet-tint-green-fg)` |

为何 orange 而不是 red：theme.ts 无 red tint，R147 已确立"orange = 警示"
约定（PanelMemory 立即点燃 R125 + 决策清空 armed R147 同款）。

### 视觉保真

- light: orange-fg #9a3412 vs 原 #dc2626 → 红 → 棕橙，色相轻微变化但仍是
  警告语义；green-fg #065f46 vs 原 #059669 → 都是绿 600/700 阶
- dark: orange-fg #fdba74 + green-fg #86efac → 暗底亮饱和反相，对比度强但
  不刺眼，跟决策清空 armed 态视觉对齐

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 替换 inline ternary |
| **M2** | tsc + build |

## 复用清单

- iter 7 tints
- R125 / R147 "orange = 警示" 语义先例

## 进度日志

- 2026-05-11 06:00 — 创建本文档；准备 M1。
- 2026-05-11 06:20 — M1 完成：ternary 替换为 tint-orange-fg / tint-green-fg；
  M2 tsc + build 通过。归档。
