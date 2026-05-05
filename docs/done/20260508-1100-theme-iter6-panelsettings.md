# 深色 / 浅色主题（迭代 6）— PanelSettings 核心 surface 迁移

> 对应需求（来自 docs/TODO.md）：
> 把 PanelSettings 的 inline color 迁到 var(--pet-color-*)；form / 输入框 / 按钮等核心 surface，保留 btnDanger / 警告色 / 状态指示色。

## 目标

迭代 1-5 已落地 PanelApp / PanelTasks / PanelDebug / PanelChat / PanelMemory。
本轮 PanelSettings：

- 文件底部 7 个共享样式常量（containerStyle / sectionTitle / labelStyle /
  inputStyle / btnStyle / mcpCardStyle / 等）
- 顶部 viewMode 切换 pill（表单 / 源码 双 tab）
- 搜索栏的清空 ✕ 按钮
- 散落在各 section 的 muted hint 文字（`#94a3b8` / `#64748b` / `#475569`
  / `#1e293b` 等通用文本）

## 非目标 — 保留 motion 语义色

- **focus ring (`#38bdf8` + rgba(56,189,248,0.18))** —— accent motion
- **btnDanger 红 `#ef4444`** —— 删除危险按钮
- **btnStyle 主色 `#0ea5e9`** → 走 accent token（dark 下变 `#38bdf8`，仍是
  primary 蓝）
- **toolBadgeStyle 蓝 (`#e0f2fe` / `#0369a1`)** —— "工具列表"标签 motion
- **status dot 三态 (绿 `#22c55e` / 红 `#ef4444` / 灰 `#94a3b8`)** —— 连接状态
- **telegram error banner (`#fef2f2` / `#fca5a5` / `#dc2626`)** —— 错误 motion
- **MCP card 错误 banner（同 telegram）** —— 同
- **save 按钮的 success / 失败 文案 (`#22c55e` / `#ef4444`)** —— 成功 / 失败
- **reconnecting 紫 (`#8b5cf6`) + disabled 灰 (`#94a3b8`)** —— 重连按钮
- **保存提示按钮蓝 (`#22c55e` / `#0ea5e9`) + disabled 灰** —— 添加 / 重连
- **mcpCard 状态字色 (`#22c55e` / `#ef4444` / `#94a3b8`)** —— 状态
- **toolBadge / mood-yellow / blue accent** 等 section motif —— 保留
- **MCP card border error 红 `#fca5a5`** —— error 凸出
- **deadline 灰 chip 字色 `#475569`** —— motion
- **success summary 黄 (`#fef3c7` / `#92400e`) / 绿 (`#dcfce7` / `#166534`)**
  —— MCP tools 健康 motion

## 设计

### 共享样式常量迁移

| key | from | to |
| --- | --- | --- |
| sectionTitle.color | `#1e293b` | fg |
| labelStyle.color | `#64748b` | muted |
| inputStyle.border / color / bg | `#e2e8f0` / `#1e293b` / `#fff` | border / fg / card |
| btnStyle.background | `#0ea5e9` | accent |
| mcpCardStyle.border / background | `#e2e8f0` / `#f8fafc` | border / bg（默认；error 态保留 `#fca5a5` border） |

### viewMode pill

| key | from | to |
| --- | --- | --- |
| pill 容器 bg | `#e2e8f0` | border（与 PanelMemory badge 同处理：light 下 `#e2e8f0`，dark 下 `#334155`，都能在主 bg 上看出 pill track） |
| 选中 tab bg | `#fff` | card |
| 选中 tab color | `#1e293b` | fg |
| 未选 tab color | `#64748b` | muted |

### 搜索栏 ✕ 清空按钮

| from | to |
| --- | --- |
| border `#e2e8f0` / bg `#fff` / color `#64748b` | border / card / muted |

### 散落的 muted hint（多处）

`#94a3b8` 文本 → muted；`#64748b` 文本 → muted；少量 `#475569` 表单标签 → fg。
Inline `color: "#1e293b"` 用于强调标题 / 卡片名 → fg。

### 测试

无单测；手测：
- light：与现状视觉一致
- dark：containerStyle 不显示底色继承自 PanelApp（已 var）；section 标题 / label / input / 按钮 / pill / 搜索清空 全部跟着切深；error banner / status dot / btnDanger / accent badge / 健康 chip 全部保留 motion

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 7 个共享样式常量 |
| **M2** | viewMode pill |
| **M3** | 散落 hint 文本（>30 处，主要是 muted） |
| **M4** | tsc + build |

## 复用清单

- 既有 token 系统 + iter1-5 的 framework / motion 切分原则

## 进度日志

- 2026-05-08 11:00 — 创建本文档；准备 M1。
- 2026-05-08 11:08 — M1 完成。共享样式常量 5 处迁 token：sectionTitle/labelStyle/inputStyle/btnStyle.bg/mcpCardStyle；inputStyle 同时显式加 bg=card；mcpCardStyle bg 用 bg token（card 上的次级表面）。
- 2026-05-08 11:11 — M2 完成。viewMode pill 容器 bg → border（与 PanelMemory badge 同 token），active tab bg/color → card/fg，inactive color → muted；搜索栏 ✕ 清空按钮 → border / card / muted。
- 2026-05-08 11:18 — M3 完成。散落 muted hint > 12 处 batch 迁 token：motion 映射 hint / MCP 已连接计数 / MCP 空状态 / 命令分隔符注解 / 自定义命令注解 + textarea 样式 / 配套 paragraph hint / companion_mode select / 早安简报 hint / 工具风险介绍 + 加载 + note / select / 提醒 hint / 上下文 hint / 搜索 empty state / McpServerCard header (title fg / TRANSPORT muted / 启用 label muted / 折叠箭头 muted / 内层 borderTop border)；motion（status dot 三态 / btnDanger 红 / telegram banner 红 / 工具风险三 chip / 错误 banner / save 成功失败文案 / 重连紫 / 添加按钮绿）全部保留。
- 2026-05-08 11:24 — M4 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (499 modules, 940ms)。归档至 done。
