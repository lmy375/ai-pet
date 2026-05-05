# PanelDebug 决策清空按钮配色迁 token（Iter R147）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug 决策行清空按钮配色迁 token：line 1514-1517 仍 hardcoded
> `#dc2626`/`#cbd5e1`/`#fef2f2`/`#fff`/`#b91c1c`/`#64748b`，dark 主题下白底
> 刺眼。迁到 framework token + tint-orange/red 系（armed = 警示色），跟主题
> 切换。

## 目标

R145 把 SlashCommandMenu 全迁了 token；本轮收尾紧邻区域的最后一个仍
hardcoded 的按钮 —— 决策日志「清空」chip。dark 主题下白底 + 灰文字 + 浅灰
边框在深底浮动突兀；armed 态红文 / 红边 / 浅红底也未做反相。迁到 token，
让 dark 下整个浮窗内化进主题。

## 非目标

- 不动 armed 2 阶段交互逻辑（first click → setTimeout 3s revert → second
  click 实清）。
- 不动文案 ("清空" / "确认清空 (3s 内)")。
- proactiveStatus 的 `#dc2626` / `#059669` (line 1487) 不在本轮 —— 它是
  status 文本，不是按钮，下轮再处理。

## 设计

### token 选择

theme.ts 没有 red 系 tint，但有 orange tint（既有 reminders 段使用，语义
"warning"）。用户在 TODO 中已点出 "tint-orange/red 系"，红色不可得就用 orange，
armed 态视觉从"红警示"→"橙警示"轻微弱化，可接受 —— 用户在 PanelMemory
"立即点燃"按钮 R125 也走同样的"橙 = 警示"思路。

### 迁移点表

| key | from | to |
| --- | --- | --- |
| border armed | `#dc2626` | `var(--pet-tint-orange-fg)` |
| border non-armed | `#cbd5e1` | `var(--pet-color-border)` |
| background armed | `#fef2f2` | `var(--pet-tint-orange-bg)` |
| background non-armed | `#fff` | `var(--pet-color-card)` |
| color armed | `#b91c1c` | `var(--pet-tint-orange-fg)` |
| color non-armed | `#64748b` | `var(--pet-color-muted)` |

### 视觉保真

light：
- non-armed: card #fff + border #e2e8f0 + muted #64748b → 与原 #fff +
  #cbd5e1 + #64748b 几乎一致
- armed: orange-bg #fff7ed + orange-fg #9a3412 → 原是 #fef2f2 (红 50) +
  #b91c1c (红 700)。色相红→橙，但都是"50 / 700 阶"低饱和警告，视觉强度等价

dark：
- non-armed: card #1e293b + border #334155 + muted #94a3b8 → 浮窗内化
- armed: orange-bg #2b1f10 + orange-fg #fdba74 → 暗底 + 亮橙文字，警示但不
  刺眼，与 light 保持"警示色家族"一致

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 改 button style 6 个 hardcoded |
| **M2** | tsc + build |

## 复用清单

- iter 1 (framework tokens) + iter 7 (tints)
- R125 PanelMemory "立即点燃" 橙警示语义先例

## 进度日志

- 2026-05-11 04:00 — 创建本文档；准备 M1。
- 2026-05-11 04:20 — M1 完成：6 hardcoded 替换为 framework + tint-orange
  token；M2 tsc + build 通过。归档。
