# PanelChat session preview 块 click 直接切到该 session

## 背景

iter #209 加了 session row hover 1s 浮 "最近 3 条" preview 段。owner 看 preview 后决定切到该 session 时，必须把鼠标移回上方标题区点击 —— 多一步移动。

让 preview 段本身 click → 直接调 switchSession，与上方标题区同语义。

## 改动

### `src/components/panel/PanelChat.tsx`

iter #209 加的 preview block 加 onClick + cursor / title hint：

```tsx
<div
  onClick={(e) => {
    e.stopPropagation();
    if (renamingId !== s.id) void switchSession(s.id);
  }}
  title="点击切到此 session（与点上方标题区同语义）"
  style={{
    ...
    cursor: "pointer",
  }}
>
```

- 与上方标题区 click handler 同模板（`if (renamingId !== s.id) switchSession(s.id)`）
- e.stopPropagation 防外层 column flex 容器二次收（虽外层无 onClick，守一道防回归）
- cursor: pointer 视觉提示可点

## 关键设计

- **复用 switchSession**：与上方标题区同 IPC + state 转换路径，确保一致行为（load session items + history + lastSeen 更新等）。
- **renamingId gate**：与标题区一致 —— 防止 renaming 状态下误切。
- **stopPropagation 防回归**：当前外层 column flex 容器无 onClick handler，但未来如果加了 row-level handler，本 stopPropagation 防 double-trigger。
- **不绑 dblclick**：单 click 已经清晰够用；dblclick 引入延迟（浏览器需等 300ms 确认）反而慢。
- **不绑 ⌘/Ctrl + click 复制 preview**：preview 是预览不是 source content；想复制走 switchSession 后再用 ChatMini ⌘+click 复制（iter #208）。

## 不做

- **不在 preview 块内每条消息加 onClick 跳到该消息**：preview 是"快速判断切不切"信号，jump-to-specific-message 走 search hit 更合适（既有路径）。
- **不写测试**：纯 click handler；视觉验证（开下拉 → hover 一个 session → 浮 preview → 点 preview → 应切到该 session）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.16s
- 改动 ~12 行（onClick handler + title + cursor: pointer + 注释）。既有 hover preview pipeline / load_session / cache / switchSession / 标题区 click 路径完全不动。

## TODO 状态

剩 5 条留池：
- TG bot /silenced 命令
- PanelTasks "新建任务" + ⇧Enter 创建并立即打开 detail 编辑器
- butler_task_edit LLM 工具 description 加 marker 教学示例
- PanelMemory ai_insights 类目顶 "🧠 由宠物自己写" banner
- 桌面 pet 右键加「⏰ 设倒计时 N 分钟 nudge」

## 后续

- preview 块 hover 时 ⌘/Ctrl + click "复制 preview 全文" 一键 → 让 owner 借 preview 段 quote 到其它地方而不切走。
- preview 内 emoji role glyph click → 跳到该消息（与 search hit 同源 jump pipeline）。
