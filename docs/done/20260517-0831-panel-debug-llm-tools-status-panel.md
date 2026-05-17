# PanelDebug「🧪 LLM tools 状态」面板（iter #270）

## Background

owner 排查"为什么宠物没用工具 X" 的常见场景：
- 内置工具 X 的 review mode 被设成 `always_review` → LLM 调时挂 review，
  自然不会主动 fire
- MCP server 未连上 → 该 server 的工具未注册，LLM tool list 里压根没出现
- 内置工具 X 的 risk level 是 high + auto mode → 在某些上下文被自动拦截

当前要 audit 这些信号要走 PanelSettings → 工具风险段 + MCP 段 + 翻 readme，
来回切多个面板。本迭代在 PanelDebug toolbar 加 🧪 按钮一键展开"全景表"：
内置工具 + MCP server 全列在一个 inline 面板里。

## Changes

仅 `src/components/panel/PanelDebug.tsx`：

- **state**：
  - `llmToolsPanelOpen: boolean` — 折叠 / 展开态
  - `llmToolsMcpStatus: McpServerStatus[] | null` — null = 还没拉过
  - `llmToolsMcpFetching: boolean` — 拉中态
  - `fetchLlmToolsMcp` useCallback — invoke `get_mcp_status`

- **toolbar 按钮**："📂 logs 目录" 之后插「🧪 LLM tools ▾/▸」按钮：
  - click → toggle `llmToolsPanelOpen` + 首次打开 lazy fetch `get_mcp_status`
  - open 时 blue tint，folded 时灰

- **inline panel**（toolbar 下方、lastManualFire 之上）：
  - **内置工具段**：toolRiskRows（已 useEffect 拉了）每条渲一个 chip：
    `工具名 · risk pill · mode pill`
    - risk pill：high → red / medium → amber / low → green
    - mode pill：auto / always_review (yellow) / always_approve (blue)
    - tooltip 含完整 note + risk + mode hint
  - **MCP 工具段**：每个 server 一行小卡：
    - 🟢 连上 / 🔴 断开 + server name + tool_count + error 提示（如有）
    - 下方列 tool_names 等宽字体 chip
  - 🔄 刷新按钮：手动 refetch get_mcp_status（owner 改完 MCP config 后用）

## Key design decisions

- **复用既有 toolRiskRows**：PanelDebug 挂载时已 useEffect 调
  `get_tool_risk_overview`；不重复 fetch。
- **MCP lazy fetch**：MCP servers 状态 PanelDebug 平时不需要，仅打开面板时
  拉一次（state null 触发，已拉过保留缓存直到手动 🔄 刷新）。
- **chip 风格 risk + mode 双 pill 而非简单文本**：扫读时颜色比文字更快被
  眼睛捕获，high red 一眼挑出来；auto + always_review + always_approve 三档
  也用不同色让 owner 立刻看到哪些工具被 owner 显式 override。
- **MCP server 卡式渲染**：每 server 独立框，方便看 connected 状态 / error
  / 子工具列表三类信息同框；error 截 60 字防超长报错撑爆面板。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.23s)
