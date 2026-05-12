# PanelDebug hardcoded hex 大扫除（UI 美化 迭代 11）

## 背景

PanelDebug 是调试窗口的主页（4185 行，重度内容），充满早期开发遗留的 hardcoded hex。许多 muted 灰 / 错误红 / 成功绿在 dark 主题下完全错位。

## 改动（`PanelDebug.tsx` 全文 bulk replace）

### 文字 / 通用色 token 化

| 旧 hex | 新 | 含义 | 替换数 |
|--------|----|----|----|
| `#94a3b8` | `var(--pet-color-muted)` | slate-400 muted | 18 |
| `#dc2626` | `var(--pet-tint-red-fg)` | 错误 / armed 红 | 10+ |
| `#7c2d12` | `var(--pet-tint-red-fg)` | 深红文字 | 1 |
| `#b91c1c` | `var(--pet-tint-red-fg)` | armed 文字红 | 2 |
| `#16a34a` | `var(--pet-tint-green-fg)` | 成功 / spoke 绿 | 4 |
| `#059669` | `var(--pet-tint-green-fg)` | 状态成功绿 | 1 |
| `#0369a1` | `var(--pet-tint-blue-fg)` | 信息蓝文字 | 1 |
| `#a16207` | `var(--pet-tint-yellow-fg)` | 警告黄文字 | 2 |
| `#64748b` | `var(--pet-color-muted)` | slate-500 | 1 |
| `#e2e8f0` | `var(--pet-color-border)` | slate-200 | 1 |
| `#0ea5e9` | `var(--pet-color-accent)` | sky-500 | 3 |

### Tint bg / 边框

| 旧 | 新 |
|----|----|
| `#fef2f2` | `var(--pet-tint-red-bg)` (armed 浅红底) |
| `#fffafa` | `var(--pet-tint-red-bg)` (modal review 浅底) |
| `#f3d7d7` | `color-mix(in srgb, var(--pet-tint-red-fg) 30%, transparent)` (review 边框) |
| `#fde68a` 边框 | `var(--pet-tint-yellow-fg)` (实线 + 虚线两处) |

## 保留

- `#10b981` (emerald-500 火 / 跑 trigger 按钮)
- `#6366f1` (indigo-500 临时 prompt / 看 prompt 按钮)
- `#f59e0b` (amber-500 DevTools 按钮)
- `#7dd3fc` (sky-300 hover 边框)
- `#fff` (白字在彩色按钮上)

这几个是**deliberate "action button pop colors"** —— 不同色暗示不同动作语义（绿=触发 / 紫=临时 / 橙=devtools / 蓝=查看）。统一成 accent 反而失去信息分层；tint vars 走"低饱和深底"路线，作为按钮 bg 不够 pop。

## 不做

- 不改动 NATURE_META / Spoke/Skip etc 内的 accent 字段（数据驱动配色，本身是分类色，与"action 按钮 pop"同思路）。
- 不写测试。

## 验收

- dark 主题切到调试窗口：muted 文字 / 错误红 / 成功绿 / 警告黄全部跟随；不再有"白字硬切灰底"或"浅红 hardcoded 在深底刺眼"。
- 浅主题视觉与原版基本一致（tint vars light 值与原 hex 接近）。
- 多色 action 按钮（立即开口 / 临时 prompt / DevTools 等）仍保留各自鲜艳色，区分动作语义。
- `npx tsc --noEmit` 通过。

## 完成

- [x] PanelDebug.tsx 11 种 hex 全量 token 化
- [x] 移到 docs/done/
