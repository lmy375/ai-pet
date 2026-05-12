# LLM 工具 give_image

## 需求

`/image <prompt>` 是显式入口 —— 用户得记得"我想生图就敲 slash"。但自然语言里"画一张兔子" / "做张图给我看看" / "draw a sunset" 才是更自然的请求方式。把生图能力暴露成 LLM tool，让模型自己决定何时调，符合 GOAL.md "通用任务" + "多模态" 的双重定位。

## 协议设计 (核心问题)

工具结果是字符串，会进下一轮 LLM 上下文。data URL 一张约 1MB / 1.3M token-ish，给 LLM 看是浪费 + 没有信息（模型刚生成了它，自然知道）。但前端要看到图。

**解法：双 payload + 后端 strip。**

`give_image` 工具返回：

```json
{ "ok": true, "count": N, "_attachments": ["data:image/png;base64,...", ...] }
```

- 前端 `send_tool_result` 收完整字符串 → ToolCallBlock 解析 `_attachments` → 渲染缩略图
- chat.rs 在塞回 `conv_messages`（次轮 LLM 上下文）前调 `strip_tool_attachments` 把 `_attachments` 字段删掉，只剩 `{ok, count}` 给模型

`_attachments` 是约定字段名（前缀 `_` 表示"前端附件，不进 LLM 上下文"），未来其它工具想返二进制也走同一路径。

## 实现

### 后端

`src-tauri/src/commands/image.rs`：

- 把原 `image_generate` 的核心拆出 `pub async fn run_image_generate(prompt: &str, n: u32) -> Result<Vec<String>, String>`。Tauri 命令变薄，工具走同一函数 —— settings 来源、错误透传一致。
- `IMAGE_HARD_MAX_N` 改 `pub`，让工具复用同 cap。

`src-tauri/src/tools/give_image_tool.rs`（新文件）：

- `Tool` 实现 + OpenAI function calling definition：description 明确"用户说画/draw/做图时调；不要在 memory recall 上滥用；不要 fabricate URL"
- args schema：prompt 必填，n 可选（1-8）
- execute：从 args 读 prompt + n，调 run_image_generate，组装双 payload JSON

`src-tauri/src/tools/mod.rs` + `registry.rs`：

- 注册 GiveImageTool 到 ToolRegistry::new
- BUILTIN_TOOL_NAMES 加 "give_image"

`src-tauri/src/commands/chat.rs`：

- 工具结果塞 conv_messages 前调 `strip_tool_attachments`。函数 pub(crate) 让后续测试 / 其它路径可以引。
- `send_tool_result` 给前端的 result 不动 —— 前端拿到完整 _attachments 才能渲染。

### 前端

`src/components/panel/ToolCallBlock.tsx`：

- 加 `extractAttachments(result)` helper
- header 行末加 `(N 张图)` 计数
- 折叠态外层就铺图 grid（不强制展开 args/result）—— 工具产出第一眼可见

## 验证

- `cargo check` clean
- `npx tsc --noEmit` clean
- 行为：
  - 用户说"画一张山水画" → LLM 调 `give_image({prompt:"a chinese ink wash painting of mountains and water"})` → ToolCallBlock 渲一张图
  - 模型再下一轮看到 `{"ok":true,"count":1}`（不带 base64），可以自然说"画好啦~喜欢吗？"
  - 多张：模型可以传 `n: 4` → 4 张缩略图平铺
  - 失败：返回 `{"ok":false,"error":"..."}`，模型可以告诉用户原因

## 不在本轮范围

- `_attachments` 字段没有从 itemsRef 持久化的 ChatItem 里加单独字段 —— 当前 PanelChat 和 useChat 都不把 tool item 的图片单独存在 ChatItem.images 上；ToolCallBlock 解析的是 result string，开 panel 重启后 result 字符串是 saved 的，仍能渲。够用。
- 后续若想"图片附在 assistant 消息上而非 tool 块里"（视觉更顺），需要新的 channel event 或 ChatItem 字段，留给反馈再扩。

## TODO 池清空 → 自主提案

按规则 #1 提出 5 条新需求（已写入 TODO.md）：

1. 桌面气泡 ⛶ 按钮加 hover 反馈
2. ChatMini 历史气泡图片支持点开放大 (lightbox)
3. PanelChat slash menu 加 history pin
4. 设置页加"测试 image_model"按钮
5. 图片消息支持复制单图到剪贴板
