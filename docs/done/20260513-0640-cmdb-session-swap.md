# PanelChat ⌘B 切到上一个 session

## 需求

power user 经常在两个 session 间来回（一个写工作笔记 / 一个写日常 /
一个跟宠物聊）。当前切 session 要鼠标点 session 列表，4-5 步。补
⌘B 快捷键"乒乓"快速来回。

## 实现

`src/components/panel/PanelChat.tsx`：

- 新 `swapTargetRef: useRef<string>("")` —— 与 `prevSessionIdRef`
  不同：后者每次 switch 立即覆盖成 new current；本 ref 在 switch 时
  截留"切换前的 session"，让 ⌘B 期间稳定指向 swap 目标
- session-switch effect 内：`prevId && prevId !== sessionId` 时把
  prevId 写入 `swapTargetRef.current`（与既有 draft toast 写入同时点）
- `handleInputKeyDown` 加 ⌘B / Ctrl+B 处理（与 ⌘K 同模式：metaKey ||
  ctrlKey + 无 shift / alt + key.toLowerCase() === "b"）：
  - preventDefault 吃浏览器粗体快捷键（textarea 内不渲染粗体，安全
    劫持）
  - 检查 swap target 非空且 != current → invoke loadSession
- 文档：chat textarea placeholder 末尾追加 "⌘B 切上一会话"

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 启动后立即 ⌘B → noop（swap target 空）
  - 在 session A 打字 → 切到 session B → swap target = A
  - 在 B 中按 ⌘B → 切回 A
  - 再按 ⌘B（现在 swap target = B）→ 切回 B
  - 反复"乒乓"两个 session 间循环
  - 第三个 session C 打开后再 ⌘B → swap 到 B（最近被切走的）
  - ⌘B 期间 placeholder 提示让用户发现快捷键
  - 与 draft reminder toast 共用 swap 记录路径，两者同时工作不互斥

## 不在本轮范围

- 没做 N 步历史栈（⌘B 多次能回 A → B → C → ...）：单步 swap 已覆盖
  "乒乓"主要 use case；栈语义太重
- 没做 ⌘B 全 panel 监听（textarea 没 focus 时也能切）：与 ⌘K 同 scope
  保持一致，避免与 button focus 等冲突
- 没做"session 列表里高亮显 swap target"：用户按一下 ⌘B 立刻看到结
  果，不需要预览
- 没改 ⌘B 与浏览器粗体冲突的兼容（如 contenteditable）：当前 chat 只
  有 textarea，无 contenteditable

## TODO 池剩余

- PanelChat "查看全部标记消息" modal
- PanelTasks 任务卡 header history 摘要小字
- PanelMemory butler_tasks "📅 今日要执行" filter chip
- PanelDebug 加 "🔄 强制 reload" 按钮
