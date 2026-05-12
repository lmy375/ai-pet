# ChatMini 渲染用户图片

## 需求

PanelChat 已能粘图、`/image` 已能生图，但 useChat / ChatMini 的 ChatMessage.content 仍只识别 string。当 PanelChat 在某个 session 里发了一条带图片的消息（content 是 OpenAI compatible parts 数组），桌面 ChatMini 加载同一 session 时：
1. TS 类型不允许 array 形态
2. parseMarkdown 把数组当 string 处理 → 渲染崩 / 显空

修这条最后一公里，让多模态消息在桌面气泡里也能看到。

## 实现

### 共享拆解工具

新建 `src/utils/messageContent.ts`：

```ts
type ContentPart = { type: "text", text: string } | { type: "image_url", image_url: { url: string } };
type MessageContent = string | ContentPart[];
extractText(content): string   // 数组 → text parts join "\n"；string 原样返回
extractImages(content): string[]   // image_url parts 的 url 列表
```

后续任何路径需要从消息里拿"显示用文本 / 图片"都走这一处。

### useChat / ChatMini 类型扩展

- `useChat.ts` 的 ChatMessage.content 改 `string | ContentPart[]`，import `MessageContent` 与 `extractText`
- `useChat` 的 `displayMessage` 用 `extractText(lastAssistantMsg.content)` 拿桌面气泡的纯文本
- `ChatMini.tsx` 同步换成 `MessageContent`，渲染分支：
  - `imgs = extractImages(content)`，非空时在 bubble 顶部铺 max 96px 缩略图条
  - `text = extractText(content)`，非空时走原 `parseMarkdown` 路径
  - key 改 `${role}-${idx}-${textLen}-${imgsLen}` 防数组形态下 length 抓到的是 array length 误差
- `App.tsx` 的 👍 反馈 `record_bubble_liked` 走 `extractText` 拿纯文本（assistant 实时路径恒 string，但联合类型让 TS 要 narrowing；同时防御未来流程把数组形态给到 assistant 角色）

## 验证

- `npx tsc --noEmit` clean
- 路径检查：从 PanelChat 粘图发消息 → 切到桌面（同一个 session 是 useChat 加载的最新会话）→ ChatMini 用户气泡顶部铺出缩略图

## 已完成多模态全部目标

GOAL.md "多模态支持：要支持图片写入与图片生成" 这条完整闭环：
- PanelChat 粘贴图片（#31）
- 多模态识别守门（#30）
- PanelChat 渲染（#32）
- /image 生图（#33）
- /image 失败重试按钮（#41）
- 设置页 image_model 字段（#39）
- 设置页多模态 chip（#40）
- 桌面气泡渲染图片（#42 — 本轮）

## TODO 池清空 → 自主提案

按 TODO.md 规则 #1，自主提出新需求：
- PanelChat 拖拽图片（扩 paste handler 到 onDrop）
- PanelTasks due date 颜色等级
- 桌面气泡走主题色 token（深色模式适配）
- /clear 二次确认
- /image -n N 多图生成
