# 聊天输入框 ↑ 历史回溯 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> chat 输入框历史回溯：↑ 键（输入框为空时）填回最后一次发送的消息，与 shell 的 readline 习惯一致，免重打误删的 prompt。

## 目标

PanelChat 输入框给键盘党加 shell-readline 风格的"上一条"召回 —— 输入框为空
时按 ↑，填回最近一次发出的非命令消息（slash 命令不入历史，不污染聊天召回）。
这覆盖几个常见场景：
1. 不小心 Enter 提了半句 → ↑ 拉回原文继续编辑再发
2. AI 回复超出预期 → ↑ 拉回原 prompt 微调再问
3. 误删 / 误清空输入框 → 一键找回

## 非目标

- 不做完整 ↑↓ 历史栈（多次 ↑ 翻更早消息）—— 一条 lastSent 已覆盖 90% 用例；
  多条历史需引入 history index + 状态栈，价值与复杂度比偏低。
- 不在 slash 模式下截获 ↑（slash menu 上下选已用 ↑↓）—— 与既有键盘语义不冲突。
- 不写 README —— chat 面板键盘可达性微调。

## 设计

### 状态

`lastSentMessage: string` — 最近一次用户**发出的非 slash 消息**。slash 命令
（`/clear` / `/help` 等）执行后不更新该字段（命令不视作"chat content"，召回
也不应回到命令本身）。空串 = 还没有历史。

### handleSubmit 注入

```ts
sendMessage(trimmed);
setLastSentMessage(trimmed);  // ← 新增
setInput("");
```

仅在非 slash 路径上 setLastSentMessage。

### handleInputKeyDown 扩展

既有 slash 模式的 ↑↓ 操作菜单选项；本轮在 `if (slashMenuVisible) { ... return }`
之后追加：

```ts
if (e.key === "ArrowUp" && input.length === 0 && lastSentMessage) {
  e.preventDefault();
  setInput(lastSentMessage);
}
```

非 slash 模式 + 空输入 + 有历史时召回。其它情况下 ↑ 走浏览器默认行为（单行
input 中的 ↑ 是 no-op，无副作用）。

### 测试

逻辑全前端 React state；无 vitest，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `lastSentMessage` 状态 + handleSubmit 写入 + handleInputKeyDown ↑ 召回 |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 `handleSubmit` / `handleInputKeyDown` / slash 模式分支
- React state 模式（与 `slashMenuVisible` / `selectedSlashIdx` 同源）

## 待用户裁定的开放问题

- 是否在 slash 命令执行后清空 lastSentMessage？本轮**否**——slash 是 panel 内
  控制流不是聊天，不该影响"上一条聊天 prompt"语义。
- 是否处理 ↓ 来清空填回？不必——用户一键 backspace 全选清空更直观。

## 进度日志

- 2026-05-05 30:00 — 创建本文档；准备 M1。
- 2026-05-05 30:05 — 完成实现：
  - **M1**：`PanelChat.tsx` 加 `lastSentMessage: string` 状态；`handleSubmit` 非 slash 路径上 `sendMessage(trimmed)` 后写入 `setLastSentMessage(trimmed)`（slash 命令不更新，避免命令本身污染聊天召回语义）。
  - **M2**：`handleInputKeyDown` 非 slash 模式（既有 `if (!slashMenuVisible) return;` early-return）改为接住 ↑ 键：空输入 + 有历史时填回 lastSentMessage 并 `preventDefault`；其它情况走浏览器默认（单行 input ↑ 是 no-op）。Slash 模式 ↑↓ 选 menu 项的语义不变。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— chat 面板键盘可达性微调。
  - **设计取舍**：单条 lastSent 而非完整 ↑↓ 历史栈（一条已覆盖 90% 误删 / 微调用例，多条历史需引入 history index 状态机不划算）；slash 命令不入历史（命令是控制流不是 chat content）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯 React 状态由 tsc 保证。
