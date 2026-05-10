# 桌面宠物迷你聊天窗

> 对应需求（来自 docs/TODO.md）：
> 宠物首页放一个小的聊天窗展示聊天记录，而不是只展示一个气泡。这个聊天记录应该
> 与 Panel Chat 页中的样式一样，尽量复用。右侧有滚动条，每次宠物说话都自动滚动到最底。

## 设计

替换 `App.tsx` 里的 `<ChatBubble />` 为新组件 `<ChatMini />`：

- 新文件 `src/components/ChatMini.tsx`：接收 `messages` (ChatMessage[]) /
  `currentResponse` (string) / `isLoading` (boolean) / `visible` (boolean)，
  以及保留既有的反馈钩子 `onLike` / `onDismiss` / `historyControls` 给最新
  那一条 assistant 消息（保持 R1 / R7 的反馈采集语义不丢）。
- 内部渲染规则：
  - 过滤 system / tool 留 user / assistant，截最近 N (=20) 条防过长。
  - 每条按 `panelChatBits` 的 `bubbleStyle` 渲染气泡（user 右、assistant 左）。
  - 流式中追加一条「正在打字」的 ghost bubble（content = currentResponse）。
  - 容器 max-height ≈ 50% 窗口高度，`overflow-y: auto`，原生右侧滚动条。
- 自动滚到底：useRef 持 scroll 容器，useEffect 依赖 `messages.length` 与
  `currentResponse`；每次更新就 `el.scrollTop = el.scrollHeight`。
- 反馈按钮（✕ / 👍）只挂在「最新一条 assistant」之上，与原 ChatBubble 同语义；
  history 模式（指定了 historyControls）下整体不显反馈，沿用既有规则。

`ChatBubble` 不删除，留作可能的兼容入口（短期 dead code，可以下一轮删）。

## App.tsx 改动

- `<ChatBubble … />` 换成 `<ChatMini messages={…} currentResponse={…} … />`。
- `useChat` 已经暴露 `messages` 和 `currentResponse`，直接消费。
- `bubbleHistory.displayed` 历史回看暂时不接到 ChatMini —— history 模式
  的语义是"看过去单条"，跟"最近聊天列表"重合度高，等下一轮决定怎么融合。

## 验证

- tsc / vite build 干净。
- 没有现成 e2e；手动开 `pnpm tauri dev` → 主动消息进来时窗口最底自动滚出新一行。
