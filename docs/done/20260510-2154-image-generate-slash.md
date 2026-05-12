# `/image <prompt>` 图片生成入口

## 需求

TODO.md：图片生成入口。`/image <prompt>` slash 命令 + 后端 `image_generate` 调 OpenAI compatible images API，结果作为 assistant 消息的图片附件渲染。

## 设计

OpenAI compatible 的 `POST {base_url}/images/generations` 是个标准接口，请求体形如：

```json
{ "model": "dall-e-3", "prompt": "...", "n": 1, "size": "1024x1024", "response_format": "b64_json" }
```

返回 `data: [{ b64_json: "..." }]` 或 `data: [{ url: "..." }]`（部分代理回 url 而非 b64）。这个入口和 chat 完全独立 —— chat 用文本/多模态模型，images 用 dall-e-3 / sd-xl 这种纯图模型 —— 所以 settings 里要有独立的 `image_model` 字段。

## 实现

### 后端

1. `commands/settings.rs`：`AppSettings` 加 `image_model: String`，默认 `"dall-e-3"`，空串等价"禁用 `/image`"。
2. `commands/image.rs`（新文件）：`#[tauri::command] async fn image_generate(prompt: String) -> Result<Vec<String>, String>`
   - 读 settings 拿 api_key / api_base / image_model
   - reqwest::Client 120s timeout（dall-e-3 经常 30+s）
   - 失败时透传 server 响应文本，让用户看到 quota / policy / model 名错原因
   - b64 → 拼成 `data:image/png;base64,…` data URL；url → 透传
3. `lib.rs`：注册 `commands::image::image_generate`
4. `commands/mod.rs`：`pub mod image;`

### 前端

1. `slashCommands.ts`：登记 `image` 为 parametric 命令，加 `kind: "image"; prompt: string` 分支。空 arg 走 unknown 弹错提示用法。
2. `PanelChat.tsx` `executeSlash`：
   - push 一条 user echo（`/image <prompt>`）+ 一条 pending assistant 占位（`🎨 正在生成图片：…`）让用户立刻看到反馈
   - 异步 invoke `image_generate`，拿到 urls 后用 setItems updater 找到 pending 占位行替换为带 `images: urls` 的 assistant 行；失败则替换为错误说明
   - 替换后立即 `saveCurrentSession(next)` 让用户重启 panel 也能看回
3. `panelChatBits.tsx` `CopyableMessage`：上轮已支持 `images?: string[]`，本轮无需改 —— assistant 行也走同一渲染路径。`PanelChat` 现在 assistant 渲染分支会把 `item.images` 透传过去。

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean（6 个 pre-existing 死代码 warning 与本改动无关）
- `/help` 文案自动包含新命令一行（`SLASH_COMMANDS` 是源 of truth）

## 用户行为

```
/                       → 命令面板 5 行（含 image）
/image                  → 选中后只回填 `/image `，等用户敲 prompt
/image 一只在月亮上跳舞的兔子   → 立即 push 用户回声 + 🎨 pending 占位
                            → 30~90s 后 pending 替换为渲染图片的 assistant 行
                            → save_session 一次，刷盘
```

## 后续

如果用户想要"会话内自然语言生图"（不用敲 `/image`），后续可以：
- 给 LLM tool 工具表加一个 `image_generate(prompt)` 工具
- LLM 决定何时调用（用户说"画一张..."就触发）
- 工具结果直接挂到 assistant message 的 images 字段

但这需要工具接口能携带二进制结果，比当前的工具协议（result: string）扩展量更大，先看用户对 `/image` 显式入口反馈再说。
