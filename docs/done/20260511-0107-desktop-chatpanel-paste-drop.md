# 桌面 ChatPanel 支持粘贴 / 拖拽图片

## 需求

paste / drop image 只在 Panel chat 页生效；桌面宠物窗的 ChatPanel 还是纯文本输入。多模态体验"半截"，用户随手把截图拖到桌面宠物上希望它识别 → 不响应。GOAL.md 写了"图片写入" 是产品定位，桌面要补齐。

## 实现

### useChat 扩 sendMessage 支持 images

`src/hooks/useChat.ts`：

- `ChatItem` 加 `images?: string[]`，与 PanelChat 同 shape，让两路径写出来的 session.items 渲染兼容
- `sendMessage(content: string, images?: string[])`：
  - hasImages 时把 ChatMessage.content 拼成 OpenAI compatible parts 数组（与 PanelChat sendMessage 同算法）
  - itemsRef.current 的 user item 携带 images 字段
- 老调用方 `sendMessage("hello")` 行为不变（images undefined → fallback 字符串路径）

### App.tsx 转发 images

`handleSend(msg, images)` → 透传给 useChat.sendMessage。

### ChatPanel 改造

`src/components/ChatPanel.tsx` 整体重写：

- props.onSend 签名扩 `(msg, images?)`
- pendingImages state + ingestImageBlobs helper（与 PanelChat 同 shape）
- onPaste：扫 clipboardData.items 拉 image/* blob
- onDrag\* + dropDepthRef：与 PanelChat 一样的 4-handler + 防抖计数 + dashed overlay 模式；overlay 文案 "📎 松开把图片加到下一条消息"
- 缩略图条：发送前在输入框上方铺 44×44 缩略图 + ✕ 单图删除按钮
- submit：先调 `is_current_model_multimodal`；非多模态 → setErrorToast 3s 自清，并丢弃 pendingImages（不发到后端）；多模态 → onSend(text, pendingImages) + 清空
- errorToast 走红 tint，与上一轮新加的红色 tint 系统对齐

容器从原来 `display: flex` 单行改为 `flexDirection: column gap: 6px position: relative`，给缩略图条 + overlay 留位置。stopPropagation 仍守在 onMouseDown，防止 textarea 被 startDragging 接走。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 截图 → 焦点桌面 textarea → Cmd+V → 缩略图条出现 → Enter → 多模态消息发出 → 桌面 ChatMini 用户气泡铺图（上一轮做的渲染）
  - 拖图 → 蓝色 overlay 出现 → 松开 → 缩略图条出现
  - 非多模态模型时粘贴/拖图 + Enter → 红色 toast "当前模型不支持图片输入" → pending 被清，没真发出去
  - 纯文本路径不变：行为与之前完全一致

## 不在本轮范围

- ChatMini / ChatPanel 都没有"输入历史"键盘上下；仅 PanelChat 有 R129 历史召回
- 桌面气泡渲染图（已经在 #42 完成）—— 现在闭环：桌面发图 → 桌面历史里看到自己发的缩略图 + 宠物多模态回复

## TODO 池剩余

- LLM 工具：give_image(prompt, n) —— 让模型自己调用生图，不用敲 /image
