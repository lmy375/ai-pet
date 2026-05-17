# PanelChat 顶栏「📋 复制最近 N 轮」dropdown（iter #279）

## Background

ChatMini 桌面 chat 早有"📋 复制最近 N 条"dropdown 让 owner 一键拷对话发
issue / 同事 / 文档。PanelChat（panel 内的完整聊天 tab）一直缺这功能 ——
要拿对话只能逐条 hover bubble 点小复制按钮，或走 sessionList dropdown 里
的"📂 导出 MD"（一次全 session）— 两条路径都不够轻量。

本迭代把 ChatMini 的 N-rounds 复制模板移到 PanelChat：顶部 🔍 搜索 按钮旁
加 📋 dropdown，选 1/5/10/20/50，拷当前 session 末 N 条 user/assistant
消息（去 tool / error / systemNote）写剪贴板。

## Changes

仅 `src/components/panel/PanelChat.tsx`：

- **state**：
  - `copyRecentMenuOpen: boolean` — popover 显隐
  - 复用既有 `exportToast` channel 显反馈

- **outside-click + Esc 关闭 useEffect**

- **`handleCopyRecentN(n)` useCallback**：
  - filter items by `type === user|assistant && !systemNote && trim().length > 0`
  - `slice(-n)` 取末 N
  - 拼 `🧑 content` / `🐾 content`，`\n\n` 分隔
  - clipboard.writeText + 3s 反馈 toast（成功 / 失败）
  - 空选 → 友好提示"还没 user/assistant 消息可复制"

- **render**：在 🔍 button 之后插 📋 dropdown：
  - 仅 `items.length > 0` 时显（空 session 不噪音）
  - click → toggle popover；popover 含 1/5/10/20/50 五个选项
  - 选项 hover 灰底 / click → 调 handler 关 popover + 复制 + toast
  - blue tint 表"当前 popover 开"，与 🔍 / 📅 / 📌 同视觉风格

## Key design decisions

- **复用 ChatMini copy-recent 模板**：N 个 user/assistant 末 N 条 + 🧑/🐾
  glyph 前缀。owner 在两个 chat 入口（桌面 ChatMini / panel PanelChat）看
  到一致的行为，减少学习。
- **filter systemNote + tool + error**：systemNote 是宠物侧反馈消息（"📡
  ping LLM..."）；tool / error 是工具调用与失败行 — 对"对话复盘"价值低 +
  噪音多。`/silenced` / `/today` 等命令也走类似过滤策略。
- **5 个 N 档**：1（最后一条）/ 5（小段）/ 10（中段）/ 20（一节）/ 50（深度
  归档）。覆盖典型用例：贴 issue 取小段 / 复盘 session 取大段。
- **不带时间戳 / role label 元数据**：与 ChatMini "带时间戳" toggle 不同 —
  PanelChat 走精简纯文本 + glyph，owner 想要时间戳可走"导出 MD"路径。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.24s)
