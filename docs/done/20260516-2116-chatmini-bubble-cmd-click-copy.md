# ChatMini bubble ⌘+click 复制单条消息

## 背景

ChatMini 现有复制路径：
1. ⌘C / Ctrl+C 全局复制最近 1 条消息（user / assistant 不限）
2. 角标 copy 按钮 hover 显（每条 bubble 一个）
3. 右键 ctx menu 含"复制"项

但键盘党想精准复制对话历史中段某一条时，⌘C 只能给最近一条；角标 copy 需鼠标移过去；ctx menu 需先右键打开。缺一条"键盘 + 单击" 精准复制路径。

加 ⌘/Ctrl + click bubble 触发复制本条 —— 与既有 ⌘C 同 modifier 风格 + 不抢用普通 click（保留选区 / drag 行为）。

## 改动

### `src/components/ChatMini.tsx`

#### bubble div 加 onClick

```tsx
onClick={(e) => {
  if (!(e.metaKey || e.ctrlKey)) return;
  if (e.altKey || e.shiftKey) return;
  if (!text) return;
  e.preventDefault();
  e.stopPropagation();
  handleBubbleCopy(idx, text);
}}
```

- 无 ⌘/Ctrl modifier → 早 return，不抢普通 click
- ⌥/⇧ 同按时不触发（防与系统级 ⌥⌘C / ⌘⇧C 等 shortcut 冲突）
- 复用既有 `handleBubbleCopy(idx, text)` —— 写剪贴板 + setBubbleCopyIdx 视觉反馈 + 1.5s 自清

#### title attr hint

```tsx
title={`${formatBubbleTimestamp(m.ts)}${text ? " · ⌘+点击 复制本条文本" : ""}...`}
```

让 hover 自然发现新快捷键。

## 关键设计

- **复用 handleBubbleCopy**：与既有角标 copy 按钮 / ctx menu 复制项同 pipeline —— writeText + bubbleCopyIdx 状态 + 1.5s 自动还原。视觉反馈一致。
- **modifier-gated onClick**：普通 click 完全不抢用，保留 owner 在 bubble 上拖选 / 单击聚焦等既有 UX。仅 ⌘/Ctrl 同按时触发。
- **过滤 ⌥/⇧ 同按**：避免与 ⌘⇧C / ⌘⌥C 等系统级 / 浏览器级快捷键冲突。owner 想"清干净修饰键 + ⌘+click" 才触发。
- **preventDefault + stopPropagation**：避免触发 bubble 内的 selection 副作用 / row 级 click 冒泡（虽实际上 bubble 是终点）。安全冗余。
- **title attr 列多 hint**：⌘+点击 / 双击「title」/ 双击空白处 三条 hint 用 ` · ` 分隔，原生 tooltip 单行显（macOS 浏览器会自动换行长 tooltip）。
- **不写新 toast/visual feedback**：handleBubbleCopy 已经设 bubbleCopyIdx → 视觉态切到"✓ 已复制"（角标 copy 按钮已用同状态）。⌘+click 触发同状态，无需新 feedback。

## 不做

- **不绑 ⌘⇧C 复制全部 visible items**：已有"复制最近 N 条"按钮 + ⌘C copy last 两条路径，新加 ⌘⇧C 多 path 混乱。
- **不写 keyboard-only nav 到 bubble + Enter 复制**：当前 ChatMini 无 keyboard-nav focused-bubble 概念；引入会扰乱既有 textarea/list 焦点 flow。
- **不在 PanelChat session 列表也加 ⌘+click**：PanelChat 已有完整 ctx menu + 复制按钮；scope creep。
- **不写测试**：纯 inline 事件 handler；视觉验证（任何 bubble 上按住 ⌘ click → 显 "✓ 已复制" 状态 + 粘贴板含本条文本）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.19s
- 改动 ~25 行（onClick handler 13 + title attr hint 一行 + 注释）。既有 onDoubleClick / handleBubbleCopy / bubbleStyle / search highlight / ts label / 角标 copy 按钮 / ctx menu 路径完全不动。

## TODO 状态

剩 2 条留池：
- butler_task edit-schedule modal 扩支 every_weekdays
- PanelChat session bar item hover 1s 浮 "最近 3 条" preview

## 后续

- ⌘⇧+click 复制本条 + 上下文 N 条（让 owner 选"复制本段 mini-context 喂另一个 LLM"用）。
- ⌥+click 弹 ctx menu 在该 bubble 位置（macOS Touchbar / 无右键鼠标的 owner 触发 ctx menu）。
- 拓展到 PanelChat user/assistant bubble 同款 ⌘+click 复制 —— 与桌面 ChatMini 体验对偶。
