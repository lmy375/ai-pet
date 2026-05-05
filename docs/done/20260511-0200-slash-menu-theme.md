# SlashCommandMenu 配色迁 token（Iter R145）

> 对应需求（来自 docs/TODO.md）：
> SlashCommandMenu 配色迁 token：现 menuContainerStyle / 行内 `#fff` / `#e2e8f0` / `#e0f2fe` / `#94a3b8` 等都 hardcoded，dark 主题下不跟随；迁到 `var(--pet-color-*)` framework token + tint / accent，与 iter 4 PanelChat 主体已迁主题保持一致。

## 目标

PanelChat 主体已 R104 (iter4) 走 token 系统。SlashCommandMenu 子组件
（line 1248 渲染入口）仍用 hardcoded：dark 模式下浮窗白底刺眼、selected
浅蓝在深底上格外突兀。

迁到 token：让浮窗 / 选中态 / 文字色都跟主题切换。

## 非目标

- 不动 R144 hover overlay（rgba 跨主题安全，已生效）
- 不引入新 token —— 复用 R7 增的 6 framework tokens + 6 tint tokens
- 不动 boxShadow 数值（rgba 半透明跨主题大致可接受，dark 下稍强但不刺眼）

## 设计

### 迁移点表

| key | from | to |
| --- | --- | --- |
| menuContainerStyle.background | `#fff` | `var(--pet-color-card)` |
| menuContainerStyle.border | `1px solid #e2e8f0` | `1px solid var(--pet-color-border)` |
| empty placeholder color | `#94a3b8` | `var(--pet-color-muted)` |
| selected row background | `#e0f2fe` | `var(--pet-tint-blue-bg)` |
| selected row borderLeft | `2px solid #0ea5e9` | `2px solid var(--pet-color-accent)` |
| 命令名 selected color | `#0c4a6e` | `var(--pet-tint-blue-fg)` |
| 命令名 unselected color | `#1e293b` | `var(--pet-color-fg)` |
| 参数 hint color | `#94a3b8` | `var(--pet-color-muted)` |
| 描述 color | `#475569` | `var(--pet-color-fg)` |

### 关于 selected 蓝 tint 选择

`--pet-tint-blue-bg` / `--pet-tint-blue-fg` 是 R7 加的 section tint，原意
"butler 最近执行 section 颜色 tint"。这里用作"选中行背景"是语义复用 ——
都是"蓝色家族的低饱和高亮"，跨主题一致。语义上 OK：
- light: tint-blue-bg ≈ #f0f9ff（与原 #e0f2fe 接近）
- dark: tint-blue-bg = 暗 slate-blue（与原 #e0f2fe 完全不同，但符合 dark
  主题"高亮稍突出但不刺眼"的预期）

borderLeft 用 accent token 保持"蓝边框 = 选中"语义。

### 测试

无单测；手测：
- light：与切换前完全一致（tint 值精确匹配旧 hex 范围）
- dark：浮窗、选中行、文字色全跟切换；selected 仍蓝色家族突出但不刺眼
- 命令补全键盘 ↑/↓ 切 selected：tint-blue-bg + accent borderLeft + tint-blue-fg
  组合让用户清楚当前选中

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | menuContainerStyle / empty / selected / 文本色全迁 |
| **M2** | tsc + build |

## 复用清单

- iter 1-7 token 系统（framework + tint）
- 既有 R144 hover overlay

## 进度日志

- 2026-05-11 02:00 — 创建本文档；准备 M1。
- 2026-05-11 02:20 — M1 完成：menuContainerStyle / 行内 selected / hint /
  desc 全迁 token；M2 tsc + build 通过。归档。
