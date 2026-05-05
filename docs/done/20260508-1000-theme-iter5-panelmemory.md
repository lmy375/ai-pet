# 深色 / 浅色主题（迭代 5）— PanelMemory 核心 surface 迁移

> 对应需求（来自 docs/TODO.md）：
> 把 PanelMemory 的 inline color 迁到 var(--pet-color-*)；s style 表 / 弹窗 / item 列表等核心 surface，保留 btnDanger 红、btnPrimary 蓝等语义色。

## 目标

延续 iter1-4：把 PanelMemory 的"框架级"surface 迁到 token：

- `s` style 表里 13 处通用样式（容器 / sectionTitle / badge / item / itemTitle/Desc/Meta / btn / input / textarea / msg）
- 加载中 placeholder
- 委托任务 / 立即整理 / 立即处理 三个特殊主按钮（accent / 紫 / 红，主按钮 bg 走 token，danger / 紫保留 motion）
- 编辑/新建记忆 modal 外框 + 表单标签
- 搜索结果"未找到匹配项" 与各分类 "暂无记忆" placeholder

切换到 dark：
- 容器、卡片、按钮、modal、输入框跟着切深
- 紫色"立即整理"按钮 / 红色"立即处理"按钮 / 黄色"每日小结"section / 蓝色"最近执行"section / 4 类 schedule chip / 失败红 chip / 到期 ⏰ chip / 已过 chip / 成功 msg 绿条 全部保留 motion

## 非目标 — 保留 motion 语义色

- **btnDanger 红 (`#fecaca` / `#ef4444`)** —— 删除按钮
- **btnPrimary 蓝**：bg 走 accent token；color 保留 `#fff`（on-accent 白字）
- **"+ 委托任务" 蓝 `#0ea5e9` / "立即整理" 紫 `#8b5cf6` / "立即处理" 红 `#ef4444`** —— 主操作按钮，前者迁 accent，后两者保留 motion；disabled 灰 `#94a3b8` 保留
- **每日小结 黄 section (`#fefce8` / `#fde68a` / `#a16207` / `#374151`)** —— "section 类型"色块
- **最近执行 蓝 section (`#f0f9ff` / `#bae6fd` / `#0369a1` / `#475569` / `#94a3b8` / `#64748b`)** —— "section 类型"色块；内部 actionColor delete `#dc2626` / update `#0d9488` 保留
- **schedule chip 4 类**：every 蓝 (`#dbeafe` / `#1e40af`) / once 黄 (`#fef3c7` / `#92400e`) / deadline 4 urgency tier (overdue/imminent 红、approaching 黄、distant 灰) —— 全保留
- **失败 chip + 清除 ✕ (`#fef2f2` / `#991b1b` / `#fecaca` / `#fff`)** —— 错误 motion
- **到期 ⏰ chip 红 (`#fee2e2` / `#b91c1c`)** —— due motion
- **已过 chip 黄 (`#fef3c7` / `#92400e`)** —— overdue motion
- **success msg 绿条 (`#f0fdf4` / `#166534`)** —— 操作成功反馈

## 设计

### s style 表迁移点

| key | from | to |
| --- | --- | --- |
| sectionTitle.color | `#334155` | fg |
| badge.background | `#e2e8f0` | border（token；light 下与现状一致 `#e2e8f0`，dark 下 `#334155` 在 `#1e293b` 卡上仍可见） |
| badge.color | `#64748b` | muted |
| item.background | `#fff` | card |
| item.border | `#e2e8f0` | border |
| itemTitle.color | `#1e293b` | fg |
| itemDesc.color | `#64748b` | muted |
| itemMeta.color | `#94a3b8` | muted |
| btn.border | `#e2e8f0` | border |
| btn.background | `#fff` | card |
| btn.color | `#64748b` | muted |
| btnDanger.background | `#fff` | card（border / color 保留 motion 红） |
| btnPrimary.background | `#0ea5e9` | accent |
| input.border | `#e2e8f0` | border（+ 显式 bg=card / color=fg） |
| textarea.border | `#e2e8f0` | border（+ 显式 bg=card / color=fg） |

### 其它迁移点

| 区段 | from | to |
| --- | --- | --- |
| 加载 placeholder color | `#64748b` | muted |
| "+ 委托任务" bg | `#0ea5e9` | accent |
| 搜索结果"未找到匹配项" color | `#94a3b8` | muted |
| 各分类 "暂无记忆" color | `#94a3b8` | muted |
| modal 内层 card bg | `#fff` | card |
| modal 表单 `<label>` color (3 处) | `#64748b` | muted |

### 测试

无单测；手测：
- light：与现状视觉一致
- dark：item 卡片底色变深、文字与边框对比度可读；btnDanger 红、btnPrimary 蓝、立即整理紫、立即处理红、schedule 4 chip、failure / 到期 chip、msg 绿条、yellow / blue butler section 全部色相不变

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | s table 13 处 + loading placeholder |
| **M2** | "+ 委托任务" / 搜索 placeholder / 每分类 placeholder |
| **M3** | modal 外框 + 表单 label |
| **M4** | tsc + build |

## 复用清单

- 既有 token 系统
- 模式与 iter2/3/4 一致：framework 走 token，motion 保留 hex

## 进度日志

- 2026-05-08 10:00 — 创建本文档；准备 M1。
- 2026-05-08 10:08 — M1 完成。loading + s table 13 处迁 token：sectionTitle/badge/item/itemTitle/itemDesc/itemMeta/btn/btnDanger.bg/btnPrimary.bg/input/textarea；btnDanger 红边框+红字保留；input/textarea 同时显式加 bg=card + color=fg（之前依赖浏览器默认）。
- 2026-05-08 10:11 — M2 完成。"+ 委托任务" bg → accent；搜索结果"未找到匹配项" + 各分类"暂无记忆" placeholder → muted。
- 2026-05-08 10:14 — M3 完成。modal 内层 card bg → card；3 个表单 label color → muted；modal backdrop rgba 保留；立即整理紫 / 立即处理红 / butler 黄黄 + 蓝蓝 section / 4 chip / 错误失败 / 到期 / msg 绿条 / 删除按钮红 全部保留 motion。
- 2026-05-08 10:18 — M4 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (499 modules, 960ms)。归档至 done。
