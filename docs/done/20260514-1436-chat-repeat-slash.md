# PanelChat `/repeat` slash + TODO 清理

## 背景

TODO 本批 6 条（上一轮 auto-proposed），盘后 4 条状态：

- ✅ Settings 「🔌 测试模型」: 已实现 —— `chat_test` Tauri 命令 + `handleTestChat` UI (PanelSettings.tsx:194-216) 早已存在。
- ✅ 任务行 due hover 显剩余时间: 已实现 —— PanelTasks.tsx:5320-5346 通过 `formatDueRelative(t.due, nowMs)` + tooltip 三态文案（已过期 / 24h 内 / normal）覆盖。
- ✅ 任务详情编辑器 ⌘S 保存: 已实现 —— PanelTasks.tsx 多处引用（line 5802, 5997, 6017, 6157, 6182）证明 ⌘S keydown 已挂。
- ❌ ChatPanel 桌面输入框 `/reset`: 架构上不合理 —— 桌面 ChatMini 直接渲 useChat 的 `messages` 数组，没有 PanelChat 那种 `items`（display）vs `messagesRef`（LLM context）的分层。砍 messages 就等于砍可见历史，与 `/reset` 的"保留可见"语义冲突。**主动放弃**这条 TODO，从 list 移除。

剩两条真正未做：
- 任务行 due hover 显剩余时间（其实已做）
- `/repeat` slash 命令（本轮实现）

## 改动（frontend only）

### `src/components/panel/slashCommands.ts`

```ts
{ name: "repeat", description: "再发一遍上一条 user 消息（IM 风便利）", parametric: false }
```

SlashAction 加 `| { kind: "repeat" }`；parser switch 加 `case "repeat": return { kind: "repeat" }`。位置紧贴 `clear / reset` —— 三者都是"对历史做点什么"的语义群。

### `src/components/panel/PanelChat.tsx`

`executeSlash` 新增 `case "repeat"`：

```ts
case "repeat": {
  if (isLoading) {
    pushLocalAssistantNote("⚠️ 正在流式回复中；先等完成或 Esc 取消，再 /repeat。");
    break;
  }
  let lastUser: ChatItem | null = null;
  for (let i = items.length - 1; i >= 0; i--) {
    const it = items[i];
    if (it.type === "user") { lastUser = it; break; }
  }
  if (!lastUser) {
    pushLocalAssistantNote("⚠️ 当前会话还没有 user 消息可以 /repeat。");
    break;
  }
  void sendMessage(lastUser.content, lastUser.images);
  break;
}
```

**设计取舍**：

- **不复用 messagesRef 找 user**：items 是渲染态 source-of-truth，含 images 字段；messagesRef 是 LLM-facing（user 项可能是 multipart 数组）。从 items 拿 (content, images) 与 input bar 提交路径完全对偶。
- **不去重 / 不缓存**：sendMessage 自动 push 新 user item + LLM stream 跟原 user 是独立一轮。即使内容相同，新 turn 让 LLM 看到的上下文是「user X / asst Y / user X」 — 这正是用户期望的"再试一次"。
- **流式中拒绝**：与 /reset 同语义边界（不与正在跑的 LLM stream race）。
- **空 user 兜底**：fresh session 还没人说过话就敲 /repeat 给明确反馈而非沉默。

## 不做

- **不让 /repeat 跨 session 召回**：仅本 session 的最近 user。要召回历史 session 的消息走跨会话搜索 `/search` 再手动复制。
- **不挂键盘快捷键**（如 ⌥R）：slash 入口足够；占快捷键池要权衡。
- **不动 ChatPanel 桌面输入框**：见背景章节 — 架构上 desktop 没有 display/LLM 分层，/repeat 在桌面也意义不大（用户能直接按 ↑ 召回上条然后 Enter，等价于 /repeat 桌面侧）。
- **不写测试**：前端无 vitest；逻辑是 items 反向扫 + sendMessage，与既有 /reset / Alt+↑ 同模式。

## TODO 状态

- 移除 4 条 stale（3 已实现 + 1 主动放弃）
- 本轮实现 1 条
- 当前 TODO empty —— 下次启动 TODO 流程会进入 auto-propose 分支提新需求。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.19s
- 改动 ~35 行（slashCommands 3 + handler 30 + README 1）；既有 /clear /reset /new /title 等 slash 路径不动。

## 后续

- 桌面 ChatPanel 接 `/repeat`（与 /image 同模式，需要把 useChat 暴露 `lastUserMessage` 或让 App.tsx 自己 walk messages）—— 留作 follow-up。
- `/repeat N` 形态：再发上 N-th 条（用户回看旧消息时点 /repeat 3 召回三轮前那条）。
- `/repeat --tweak` 形态：把上一条 user 内容 prefill 到 input 让用户改了再发（与 Alt+↑ 编辑等价但显式 slash 入口）。
