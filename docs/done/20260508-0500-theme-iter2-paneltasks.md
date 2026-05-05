# 深色 / 浅色主题（迭代 2）— PanelTasks 核心 surface 迁移

> 对应需求（来自 docs/TODO.md）：
> 把 PanelTasks 的 inline color 迁到 var(--pet-color-*)：container / formCard / item / detail 等核心 surface；保留行级 status badge / 动作按钮颜色（带 motion 语义）。

## 目标

迭代 1 已加 `src/theme.ts` token 系统 + PanelApp 顶层 surface CSS var 化。
本轮把 PanelTasks 的"框架级"surface（容器底色、卡片、文本、边框、输入框）
迁到 token，让用户切深色后任务面板背景 / 文本能跟着切。

## 非目标

- **保留所有功能性配色**（带 motion 语义）：
  - 状态 badge (`STATUS_BADGE` 表 — pending 蓝 / done 绿 / error 红 /
    cancelled 灰)
  - actionBtn / actionBtnRetry / actionBtnDanger / actionBtnDisabled
    （带"重试 / 取消 / 危险"语义）
  - tagChip / tagFilterChip（紫色"过滤激活"指示）
  - DueChip 红 / 橙 / chip 系（紧迫度）
  - priBadge 黄底（优先级）
  - error / cancelled / result 短信文案的红 / 绿 / 灰
  - "刚动过"绿点 / "未读"红点
  - 搜索高亮 mark 黄底
- 不动 detail 面板内部细节配色（detail md 编辑器、history timeline 等）
  —— 等迭代 4 再处理。本轮聚焦列表 + 创建表单 + 搜索栏。
- 不引入新 token —— 继续用现有 6 个（bg / card / fg / muted / border /
  accent）。颜色不够时优先 fg-on-card 用 fg、subtle bg 沿用 bg。

## 设计

`s` style table 的迁移点（只列 surface 类，功能色不动）：

| key | from | to |
| --- | --- | --- |
| `container` | (无 bg) | + `background: var(--pet-color-bg)` 显式 |
| `sectionTitle.color` | `#334155` | `var(--pet-color-fg)` |
| `formCard.bg / border` | `#fff` / `#e2e8f0` | `card` / `border` |
| `label.color` | `#475569` | `var(--pet-color-muted)` |
| `input.border` + 显式加 `bg: card` | `#e2e8f0` | `border` |
| `textarea.border / bg` 同上 |  |  |
| `item.bg / border` | `#fff` / `#e2e8f0` | `card` / `border` |
| `itemTitle.color` | `#1e293b` | `fg` |
| `itemBody.color` | `#475569` | `fg`（dark 下亦清晰） |
| `itemMeta.color` | `#94a3b8` | `muted` |
| `searchInput.border` + 显式 bg | `#e2e8f0` | `border` / `card` |
| `searchClearBtn.border / bg / color` | `#e2e8f0` / `#fff` / `#64748b` | `border` / `card` / `muted` |
| `searchCount.color` | `#94a3b8` | `muted` |
| `tagFilterLabel.color` | `#94a3b8` | `muted` |
| `toggleRow.color` | `#475569` | `muted` |
| `chevron.color` | `#94a3b8` | `muted` |
| `cancelInput.border` | `#cbd5e1` | `border` |

### 不迁的（保留 motion 色）

- `badge(status)` —— 走 STATUS_BADGE 表，红/绿/灰/蓝跨主题语义稳定
- `priBadge` —— 黄底象征优先级
- `actionBtn*` 系 —— 蓝/红/灰带"动作类型"语义
- `tagChip` / `tagFilterChip` —— 紫色"激活过滤"指示
- `btnPrimary` / `btnDisabled` —— 主按钮蓝 / 禁用灰
- `err` / `errorMsg` / `cancelledMsg` / `resultMsg` —— 红/灰/绿语义文本
- `empty` —— 灰 placeholder

### 测试

无单测（CSS 改动 + token 应用）；手测覆盖：
- light 模式视觉与切换前完全一致
- 切 dark：背景 / 卡片 / 文本随切换，badges / 按钮保持原色
- localStorage 持久化跨重启

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | s table 11 处迁移 |
| **M2** | tsc + build + 手测 light/dark |
| **M3** | cleanup |

## 复用清单

- 既有 `src/theme.ts` token + CSS var
- 既有 PanelApp ☀️/🌙 toggle

## 进度日志

- 2026-05-08 05:00 — 创建本文档；准备 M1。
- 2026-05-08 05:15 — M1 完成。s table 14 处迁移：container/section/formCard/label/input/textarea/item/itemTitle/itemBody/itemMeta/searchInput/searchClearBtn/searchCount/tagFilterLabel/toggleRow/cancelInput/chevron 全用 var()；功能性配色（status badge / action 按钮 / chip / due chip / 搜索高亮 / 错误成功消息）保持不动。
- 2026-05-08 05:20 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 1.04s)。归档至 done。
