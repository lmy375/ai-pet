# 深色 / 浅色主题（迭代 3）— PanelDebug 核心 surface 迁移

> 对应需求（来自 docs/TODO.md）：
> 把 PanelDebug 的 inline color 迁到 var(--pet-color-*)；ring buffer / decision row / proactive prompt modal 等核心 surface。

## 目标

迭代 1（`src/theme.ts` token + PanelApp 顶层）与迭代 2（PanelTasks）已完成。
本轮把 PanelDebug 的"框架级"surface 迁到 token：

- 顶部 toolbar（按钮工具栏 + 共享 `toolBtnStyle`）
- 「看上次 prompt」modal（外框 / header / 折叠头 / PROMPT pre / REPLY pre）
- 高风险工具调用 review modal（外框 / 元信息 / pre 块）
- 决策日志容器与多选 chip / 搜索输入 / 排序按钮 / 单行复制·重跑按钮
- 宠物最近主动说过 ring buffer（恢复 framework 边框 + 正文文本）
- 工具调用历史 / 反馈记录 / 待提醒事项 的边框与正文文本

切换到 dark 后：
- 顶部 toolbar、modal 外框背景跟着切深
- 决策日志背景比 modal/card 略深一档（保留 light 模式下 `#f8fafc` < `#fff` 的层次）
- 主体文本 `#1e293b` / `#475569` 全用 `var(--pet-color-fg)`
- 弱提示文本 `#94a3b8` / `#64748b` 全用 `var(--pet-color-muted)`
- 边框 `#e2e8f0` / `#cbd5e1` 全用 `var(--pet-color-border)`

## 非目标 — 保留 motion 语义色

以下颜色带"语义信号"，跨主题保持：

- `kindColor` / `riskBadgeBg` / `reviewStatusBg` / `NATURE_META.color`：决策类型 / 风险等级 / 审核状态 / prompt-rule nature 的色族
- 状态文本：成功 `#059669` `#0d9488` / 错误 `#dc2626` / 警示 `#92400e`
- 三类 modal 的"提示色"内嵌段：高风险 modal 的粉红 `#fffafa` 边框 `#f3d7d7`、警示标题红、TOOL CALLS 黄色 (`#fffbeb` / `#fef3c7` / `#fde68a` / `#92400e`)、REPLY 绿 (`#f0fdf4` / `#166534`)
- 决策行多选 chip 的 active 态（accent 色填充）；inactive 态走 token
- 决策行"重跑"按钮的 disabled 态 `#f1f5f9` / `#94a3b8` —— disabled-state 有专门语义，跨主题保持
- 决策日志"清空"按钮 armed 态的红色 `#dc2626` / `#b91c1c` / `#fef2f2` —— 危险动作色
- "立即开口"主按钮绿 `#10b981`、"看上次 prompt" 紫 `#6366f1`、DevTools 橙 `#f59e0b` —— 工具按钮带"动作类型"语义
- 跨日 ts 的 amber `#a16207` —— "这条不在今天" 提示
- prompt 字数压力红 `#dc2626` —— token-pressure 警示
- ruleChipStyle 紫色 chip —— "软规则命中"独立色族
- 5 个 section 的 tinted 背景 + 同色系标题（紫 `#fdf4ff/#86198f`、黄 `#fefce8/#854d0e`、绿 `#f0fdf4/#065f46`、橙 `#fff7ed/#9a3412`、淡紫 `#faf5ff/#6b21a8`、警示橙 TG banner `#fff7ed/#fed7aa/#9a3412`）—— "section 类型"色块；dark 下视觉退化但功能不丢，留迭代 4 处理
- log 输出区刻意保持 terminal 风格的 `#0f172a` / `#e2e8f0` —— 与终端审美绑定，跨主题不动

## 设计

### 迁移点表

按行号区段列出（仅 framework surface）：

| 区段 | 行号近似 | from | to |
| --- | --- | --- | --- |
| toolbar 容器 | 1020 | borderBottom #e2e8f0 / bg #fff | border / card |
| toolBtnStyle | 2214-2223 | border #e2e8f0 / bg #fff / color #475569 | border / card / fg |
| review modal 外框 | 406 | bg #fff | card |
| review modal 元信息 | 430-448 | color #475569 / #0f172a / #1e293b / #475569 | fg |
| review modal timeout 提示 | 506 | color #94a3b8 | muted |
| 「看上次」modal 外框 | 532 | bg #fff | card |
| 「看上次」modal header | 545-694 | borderBottom #e2e8f0 + 文本 / 按钮 | border / fg / muted / card |
| 折叠 PROMPT header | 701 | bg #f8fafc / borderBottom #e2e8f0 / 文本 | bg / border / fg / muted |
| 折叠 PROMPT pre | 752-760 | color #1e293b / borderBottom #e2e8f0 | fg / border |
| 折叠 REPLY pre | 983 | color #1e293b | fg |
| copy 按钮（PROMPT/REPLY 各一） | 740/967 | border #cbd5e1 / bg #fff / color #475569 | border / card / fg |
| 决策日志容器 | 1263 | bg #f8fafc / borderBottom #e2e8f0 | bg / border |
| 决策日志 header | 1271 | color #64748b | muted |
| 决策日志 chipStyle inactive | 1348-1350 | border #cbd5e1 / bg #fff / color #475569 | border / card / fg |
| 决策日志 reason 搜索 | 1408-1411 / 1424-1427 | border / bg / color (#cbd5e1/#fff/#475569 + 清空 #64748b) | border / card / fg / muted |
| 决策日志 排序切换 | 1449-1452 | 同上 | 同上 |
| 决策日志 buffer 计数 | 1464-1475 | color #94a3b8 | muted |
| 决策日志 empty hint | 1490 | color #94a3b8 | muted |
| 决策行 ts (same day) | 1548 | color #94a3b8 | muted |
| 决策行 reason 文本 | 1562 | color #475569 | fg |
| 决策行 单行复制 | 1598-1600 | border / bg / color | border / card / fg |
| 决策行 重跑 active | 1616-1618 | (active) bg #fff / color #475569 + border #cbd5e1 | card / fg / border |
| 宠物最近说过 容器 | 1638 | borderBottom #e2e8f0 | border（保留 #fdf4ff 紫色 tint） |
| 宠物最近说过 正文 | 1658 | color #475569 | fg |
| 工具历史 容器 | 1672 | borderBottom #e2e8f0 | border（保留 #fefce8 黄色 tint） |
| 反馈记录 容器 | 1808 | borderBottom #e2e8f0 | border（保留 #f0fdf4 绿色 tint） |
| 反馈记录 正文 | 1919 | color #1e293b | fg |
| 反馈记录 empty | 1925 | color #94a3b8 | muted |
| 提醒事项 容器 | 1940 | borderBottom #e2e8f0 | border（保留 #fff7ed 橙色 tint） |
| 提醒事项 正文 | 1962 | color #475569 | fg |
| 提醒事项 title 括注 | 1964 | color #94a3b8 | muted |

### 测试

无单测（CSS 改动 + token 应用）；手测覆盖：
- light 模式视觉与切换前完全一致
- 切 dark：toolbar / modal / 决策日志背景 + 文本随切换；motion 段（badges、按钮 disabled 态、tinted section）保持原色

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | toolbar + toolBtnStyle + 两个 modal 外框 |
| **M2** | 「看上次」modal 内部（header / 折叠头 / pre / copy 按钮） |
| **M3** | 决策日志（容器 / chip / 搜索 / 排序 / 行内按钮） |
| **M4** | ring buffer / 工具历史 / 反馈 / 提醒 各 section 框架色 |
| **M5** | tsc + build + 手测 light/dark |

## 复用清单

- 既有 `src/theme.ts` token + CSS var
- 既有 PanelApp ☀️/🌙 toggle
- 模式与迭代 2 PanelTasks 完全对齐：surface 迁 token，motion 色保留

## 进度日志

- 2026-05-08 06:00 — 创建本文档；准备 M1。
- 2026-05-08 06:10 — M1 完成。toolBtnStyle / 顶部 toolbar 容器 / 高风险审核 modal 外框 + 元信息文本 / 「看上次」prompt modal 外框迁 token；motion 段（modal 警示色 / 危险按钮 / 工具按钮 accent）保留。
- 2026-05-08 06:18 — M2 完成。「看上次」modal 内部 header（prev/next 导航 active 态、turn-counter、no-turns、char-pressure 灰色态、ts、close ✕、copyMsg）+ PROMPT 折叠头 + PROMPT/REPLY pre + 两个复制按钮迁 token；TOOL CALLS 包裹 div 与 borderBottom 也迁；motion 段（绿黄段 motif、disabled #f1f5f9、char-pressure 红、tools_used 青、outcome 开口/沉默 badge）保留。
- 2026-05-08 06:25 — M3 完成。决策日志容器 bg / borderBottom + header muted + chipStyle inactive 三件套（border/bg/color）+ reason 搜索框 + 清空 ✕ + 排序按钮 + 计数 / buffer 容量灰 + empty 提示 + 单行 ts (same-day) / reason / 复制 / 重跑 active 态全部迁 token；motion 段（kindColor 色条 + kind 标签、跨日 ts amber、armed 清空红、buffer 满 amber、active accent 填充、disabled 重跑 #f1f5f9）保留；ruleChipStyle 紫色 chip 保留（独立色族）。
- 2026-05-08 06:32 — M4 完成。recentSpeeches / 工具历史 / 反馈记录 / 提醒事项 4 段的 borderBottom 迁 border；正文文本（#475569 / #1e293b → fg；#94a3b8 → muted）迁 token；section tinted bg + 同色系 header（紫 #fdf4ff/#86198f、黄 #fefce8/#854d0e、绿 #f0fdf4/#065f46、橙 #fff7ed/#9a3412、淡紫 #faf5ff prompt-hints）+ 反馈 badge 颜色 + ts ring buffer 紫 #a78bfa + 提醒到时橙 #ea580c 全部保留（dark 下视觉退化是迭代 4 polish 的范围）。
- 2026-05-08 06:36 — M5 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (499 modules, 941ms)。归档至 done。
