# ChatMini Esc 取消生成（soft cancel）

## 需求

长回复进行到一半时用户想停，目前只能等到 stream 完。给 ChatMini 加 Esc 取消
快捷键。

## 设计

后端真取消需要 cancellation token 沿着 `stream_llm_request` 传到 reqwest，
重构涉及 chat.rs / tool pipeline 多处，工作量大。

**soft cancel**：前端立刻响应 + 把已 accumulated 文本 finalize 成 assistant
message + 后续 onEvent 全 noop。后端 stream 仍在跑（API quota 仍消耗），但用
户视觉立即得到反馈 —— 多数场景下"看到的文本"就是用户想保留的。

`[已取消]` 后缀显式标注让用户知道这条回复不完整。

## 实现

### useChat

`src/hooks/useChat.ts`：

- 新 refs：`cancelledRef`、`accumulatedRef`（替原 sendMessage 内部 `let
  accumulated`，让 cancel 能读到当前累积值）、`updatedMessagesRef`（cancel
  时拼新 messages 数组要用）
- sendMessage 顶部复位三个 ref；onEvent 顶部 `if (cancelledRef.current) return;`
  让取消后所有事件 noop
- 新 `cancel()` callback：检 isLoading；翻 cancelledRef；accumulated.trim()
  非空时拼成 `${text}\n\n[已取消]` 的 assistant message + save_session；
  reset state
- 返回值加 `cancel`

### App.tsx

`useChat()` 解构出 `cancel`，传给 `<ChatMini onCancel={cancel} />`。

### ChatMini

- props 加 `onCancel?: () => void`
- 新 useEffect：visible && isLoading && onCancel 三态都 true 时挂 keydown 监
  听。Esc（无 modifier）+ 焦点不在 input/textarea/contenteditable 时 →
  preventDefault + onCancel()

不挂在 ChatPanel 因为它只是 input 组件；ChatMini 是消息呈现位置，Esc 在那里
取消才符合用户预期"我在看消息生成，想停"。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 发一个长 prompt → streaming 中按 Esc → 已显文本 + `[已取消]` 后缀立刻
    finalize；后续 chunk 不再 append；isLoading=false → 按钮可点
  - 焦点在 ChatPanel typing 中 → Esc 走原生清空输入（已有 Esc handler）；不
    取消（焦点守卫保护）
  - 没有 streaming（isLoading=false）→ Esc 不响应（守卫 isLoading 才挂监听）
  - cancel 后再发新消息 → cancelledRef 自动复位，正常 stream

## 不在本轮范围

- **真后端 cancel**：要扩 `stream_llm_request` 与 tool 调用接 cancellation
  token，pipeline 多层都得加 abort 检查。soft cancel 的 token 损失是已知 trade-
  off；如用户反馈"我经常 cancel"再花精力做硬 cancel
- 没加"重发"按钮：被取消的 assistant 消息附带的 `[已取消]` 提示让用户知道这
  条不完整；想要完整版本就再发一遍。重发按钮要存 last user prompt，留给以
  后做
- PanelChat 不挂 Esc cancel：那里 input.length>0 的 Esc 已经映射到清空 input。
  panel 长回复场景下用户可以切到桌面用 Esc，或滚动浏览部分文本就够；不冲掉
  panel input 行为

## TODO 池剩余

- PanelChat 历史会话过滤"含图片"
