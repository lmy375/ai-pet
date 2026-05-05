# PanelSettings 错误 banner 配色迁 token（Iter R151）

> 对应需求（来自 docs/TODO.md）：
> PanelSettings 错误 banner 配色迁 token：line 638 / 1452 两条 telegram
> 错误提示 banner 用 hardcoded `#fef2f2` + `#fca5a5` + `#dc2626`，dark 主题下
> 白底红字过曝。迁到 `tint-orange-{bg,fg}`，与 R150 chat error bubble
> 同款语义。

## 目标

PanelSettings 内 telegram 集成区有两处对称的"错误提示 banner"（同样的
inline 样式，line 638 / 1452）。当前 hardcoded：
- bg `#fef2f2` (red 50)
- border `#fca5a5` (red 300)
- color `#dc2626` (red 600)

dark 主题下白底贴在 #1e293b card 上，红字 + 浅红边过曝且与 R147 / R149 / R150
迁完的 orange 警示语义脱节。

## 非目标

- 不改 banner layout / radius / padding / fontSize / margin。
- 不改 banner 显示条件（status.error / status.error !== "Disabled"）。
- 不引入 red tint（theme.ts 无；R147 起统一用 orange）。
- 不动其他 PanelSettings 的颜色（保持本轮 scope）。

## 设计

### 迁移点（两处对称，整字符串一致 → 单 string Edit replace_all 即可）

| key | from | to |
| --- | --- | --- |
| background | `#fef2f2` | `var(--pet-tint-orange-bg)` |
| border | `1px solid #fca5a5` | `1px solid var(--pet-tint-orange-fg)` |
| color | `#dc2626` | `var(--pet-tint-orange-fg)` |

border 用 fg 而非另开 tint：与 R147 决策清空 armed border (orange-fg) 同款
"warning border = warning fg color"。

### 视觉保真

light：
- orange-bg #fff7ed vs 原 #fef2f2 — 浅暖底，相近
- orange-fg #9a3412 vs 原 #fca5a5 (border) / #dc2626 (text) — 棕橙取代
  红，警示语义保留

dark：
- orange-bg #2b1f10 + orange-fg #fdba74 — 暗暖底亮橙文字 / 边框，与 card
  视觉融合不刺眼

### 同字符串 replace_all 策略

line 638 与 line 1452 inline style 完全一致；用 `replace_all: true`
一次性替换，避免分两次 Edit。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | replace_all inline 样式 |
| **M2** | tsc + build |

## 复用清单

- iter 7 tints
- R147 / R149 / R150 "orange = 警示" 先例

## 进度日志

- 2026-05-11 08:00 — 创建本文档；准备 M1。
- 2026-05-11 08:20 — M1 完成：replace_all 一次性迁完两处对称 banner；
  M2 tsc + build 通过。归档。
