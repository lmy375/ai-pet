# 多模态：聊天页粘贴图片输入 + 渲染

## 需求

TODO.md 中第一条："图片支持（多模态）：聊天页允许粘贴图片、走多模态大模型识别…用户配置的非多模态模型下粘贴图片时报错说明不支持。前端需要 paste handler、图片在 ChatMini / PanelChat 中的渲染、以及生图入口。"

本轮聚焦 **粘贴 + 渲染**（图生图入口拆到 #33 单独做）。

## 设计

后端 `chat` 命令的 `messages[i].content` 字段类型已经是 `serde_json::Value`（见 `src-tauri/src/commands/chat.rs:377`），直接转发给 OpenAI compatible API；所以 multipart 内容数组不需要 schema 改动。

前端要做的是：

1. **多模态能力检测命令** —— 上一轮已加 `is_current_model_multimodal()`（`src-tauri/src/commands/settings.rs:585`），按当前 settings.model 名做关键字匹配（`gpt-4o`、`claude-3`、`gemini`、`vision`、`qwen-vl`、…）。
2. **PanelChat 粘贴**：textarea 上挂 `onPaste`，扫 `clipboardData.items` 拉 `image/*` blob → FileReader → base64 data URL → `pendingImages` state。preventDefault 防图片"路径文本"被一并粘到 textarea。
3. **缩略图条**：`pendingImages` 非空时浮在 input bar 上方左侧，每图右上角带 ✕ 单独移除。
4. **发送守门**：submit 时若 `pendingImages.length > 0`，先调 `is_current_model_multimodal`，false → 弹本地 assistant 提示"当前模型 X 不支持图片输入"，不发；true → 把消息内容拼成 OpenAI compatible 数组 `[{type:"text",text},{type:"image_url",image_url:{url}}, …]` 推给后端，本地 items 同时记录 `images: string[]` 字段。
5. **渲染**：`ChatItem` 加可选 `images?: string[]`，`CopyableMessage` 在 user bubble 内首部铺一行缩略图（max 160px，wrap）；assistant 消息暂不携带图片（图生图阶段再扩）。
6. **空文本 + 图片**也允许发送（"图说一切"），原 `if (!trimmed) return` 放宽为 `if (!hasImages && !trimmed) return`。

## 实现细节

- 多图同帧粘贴（截图 + 应用复制）按时序异步推入 — 每个 blob 一个 FileReader.onload。
- 缩略图条用 `position: absolute; bottom: calc(100% - 4px)` 浮在 form 上方，不挤占 textarea 高度；`zIndex: 5` 高于 SlashCommandMenu 的层叠不冲突（slash menu 在 `bottom: 100%` 不是 calc）。
- send 后 `setPendingImages([])` 立即释放 base64 内存（一张 1MB 截图 base64 ≈ 1.3MB string）。
- 用户 message 历史召回 (`messageHistory`) **不**带图片，只 push 文本。下次 ↑ 召回是文本编辑场景，不应自动带回图。
- PanelChat 路径完整覆盖。桌面 ChatPanel（宠物窗输入条）+ useChat 暂未接入图片粘贴 —— useChat 对 content: string 的假设较多（item count、title 截断…），改造与本轮目标对比 ROI 偏低，先把核心通路打穿。

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean（6 个 pre-existing dead-code warning，与本改动无关）
- 视觉：缩略图条在 input bar 上方浮起，✕ 删图 / send 后清空；user bubble 首部铺图，文本紧随其后
- 守门：非多模态模型（如 gpt-3.5-turbo）粘图后 send → 本地 assistant 行提示，图被丢弃，不发给 LLM

## 后续 (#33)

`/image <prompt>` slash 命令 + 后端 image_generate 命令调 OpenAI images API，结果作为 assistant 消息的图片附件返回。届时 `ChatItem.images` 字段可以扩到 assistant 角色，`CopyableMessage` 已经按 role 无关方式渲染，免改。
