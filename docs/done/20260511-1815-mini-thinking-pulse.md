# ChatMini 流式时图标小动效

## 需求

桌面 mini chat 在 streaming 中 chunk 还没到达的窗口（用户按 Enter 后到首
chunk 落下，或 tool 调用前后），UI 完全静止 —— 用户分不清"是正在想"还
是"卡住了"。toolStatus 行只在 tool 完成时有，前置空窗仍裸露。

加一条"思考脉冲"提示：pulsing 🐾 + "思考中..."轻字。

## 实现

`src/components/ChatMini.tsx`：

- `MINI_CHAT_STYLES` 加 keyframes：
  - `pet-mini-thinking-pulse`：opacity 0.4→1→0.4 + scale 0.96→1.04→0.96，1.4s ease-in-out 循环
  - `pet-mini-thinking-dots`：CSS content 步进 4 帧 ""→"."→".."→"..."，1.4s steps(4, end) 循环
  - 配套两个 class：`.pet-mini-thinking-glyph` 给 assistantGlyph 用，
    `.pet-mini-thinking-dots::after` 给"思考中"后追加省略号
- `@media (prefers-reduced-motion: reduce)` 退化：动画停 + 替换 `…` 静态
  字符，对眩晕症 / 系统级动画偏好友好
- 插入条件：`isLoading && !showStreamingBubble && !(toolStatus 非空)`
  - first chunk 之前：showStreamingBubble false → 显
  - first chunk 之后：showStreamingBubble true → 隐（让位 streaming bubble）
  - tool 执行：toolStatus 非空 → 已有 "✅ X done" 行，不叠显
- 排版：`paddingLeft: 4`、`alignItems: center`，与左对齐 assistant bubble
  在同一边；`gap: 6` 让 glyph 与文案分得清
- aria-live="polite" + title 让屏幕阅读器 / hover 都能拿到"宠物正在思考中"
  状态

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 按 Enter 发送 → 立刻看到 🐾 脉冲 + "思考中..." 动态省略号
  - 首 chunk 到达 → 脉冲消失，streaming bubble 出
  - 流中触发 tool（chunk 暂停）→ tool done 后 toolStatus 显，思考脉冲不
    叠出（避免双行噪音）
  - 取消 streaming → isLoading 立刻 false，脉冲 / streaming bubble 同时消失
  - macOS 系统开"减弱动态效果" → 脉冲静止 + 显 "思考中…" 静态省略号

## 不在本轮范围

- 没在 panel 聊天页加同款脉冲：panel 的 streaming bubble 已有 emoji 流式
  反馈，而且窗口足够大，"卡死"误判风险低；mini 是触发面
- 没改 Live2D 角色本身的动画：那是 model.json 的 motion 调度问题，与 React
  无关；后续可以扩"streaming 时调 thinking motion"作为单独需求
- 没让 streaming bubble 自身脉冲：bubble 字本身在持续刷，已有视觉反馈

## TODO 池剩余

- PanelMemory 单条记忆 pin 置顶
- ChatMini 拖拽到面板的过渡视觉
- PanelTasks 卡片"按住拖拽改 priority"
