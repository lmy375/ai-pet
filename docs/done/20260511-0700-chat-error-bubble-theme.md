# PanelChat 错误 bubble 配色迁 token（Iter R150）

> 对应需求（来自 docs/TODO.md）：
> PanelChat 错误 bubble 配色迁 token：line 1162 assistant 错误 bubble 用
> hardcoded `#fef2f2` + `#dc2626`，dark 主题下白底红字过曝。迁到
> `tint-orange-bg` / `tint-orange-fg`，与 R147 / R145 全局 token 化方向一致。

## 目标

PanelChat 主体已 R104 (iter 4) 全迁 token，唯独 error 分支这条 bubble 还在用
`{ ...bubbleStyle("assistant"), background: "#fef2f2", color: "#dc2626" }` —
overlay 红 50 / 红 700 在 light 下 OK，dark 下白底贴在 #1e293b 卡片上又是
红字，刺眼且与浮窗 / debug status 不一致（R149 已迁 orange）。

## 非目标

- 不改 bubbleStyle("assistant") 基本结构（padding / borderRadius / 字号）—
  这是父函数返回的样式 spread，仅在最末覆盖 bg / color，本轮也只改这两个。
- 不引入 red tint（theme.ts 无；R147 / R149 都用 orange）。
- 不改 bubble 文字内容、不改 layout。

## 设计

### 迁移点

| key | from | to |
| --- | --- | --- |
| background | `#fef2f2` | `var(--pet-tint-orange-bg)` |
| color | `#dc2626` | `var(--pet-tint-orange-fg)` |

### 视觉保真

light：
- orange-bg #fff7ed (暖白) vs 原 #fef2f2 (粉白) — 极相近的浅警示底
- orange-fg #9a3412 (深棕橙) vs 原 #dc2626 (红) — 色相红→棕，但都是高对比
  深警示文字

dark：
- orange-bg #2b1f10 (暗暖底) — 与 card #1e293b 区分但不抢戏
- orange-fg #fdba74 (亮橙) — 暗底高对比，可读性远优于原 #dc2626

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 替换 inline 2 hardcoded |
| **M2** | tsc + build |

## 复用清单

- iter 4 PanelChat 主体 token migration
- iter 7 tints
- R147 / R149 "orange = 警示"先例

## 进度日志

- 2026-05-11 07:00 — 创建本文档；准备 M1。
- 2026-05-11 07:20 — M1 完成：bg / color 2 hardcoded 替换为 tint-orange；
  M2 tsc + build 通过。归档。
