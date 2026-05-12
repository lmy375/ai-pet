# /image -s 覆盖 size

## 需求

settings.image_size 是默认尺寸；想临时画横/竖图得去改设置 → 影响后续所有生
图 → 又得改回来。`/image -s 1024x1792 dragon` 单次覆盖，settings 不动。

## 实现

### 后端

`src-tauri/src/commands/image.rs`：

- `run_image_generate` 加 `size_override: Option<&str>` 参数；处理优先级：
  override 非空 → 用它；否则 fallback 到 settings.image_size；再 fallback 到
  "1024x1024"
- `image_generate` Tauri 命令加 `size: Option<String>` 参数透传
- 前端 -s 格式 `\d+x\d+` 已粗略校验；后端不再 enforce 让 provider 自己拒绝
  不支持的尺寸（dall-e-3 强制几种），错误自然进 errors 列表

`src-tauri/src/tools/give_image_tool.rs` 同步：`run_image_generate(&prompt, n, None)` —
LLM 工具不暴露 size 控制，省得 prompt 工程加一层。

### 前端

`src/components/panel/slashCommands.ts`：

- SlashAction image 加 `sizeOverride: string | null`
- 解析循环改 `for (let i = 0; i < 3; i += 1)` 让最多剥 3 flag；加 `-s WxH` 分支
  with regex `^-s\s+(\d+x\d+)(?:\s+(.+))?$/s`，dup 检测同其它 flag
- `formatImageHelpText` 加 -s 用例 + 组合 flag 用例

`src/components/panel/panelChatBits.tsx` ChatItem 加 `imageRetrySize?: string | null`
让失败重试时一并 replay。

`src/components/panel/PanelChat.tsx`：

- `runImageGenerate` 加 `sizeOverride` 参数，pendingNote / 成功 content / 失败
  content / retry 字段全带 sizeLabel `(WxH)`
- invoke `image_generate` 传 `size: sizeOverride ?? undefined`（undefined →
  Tauri 序列化为 null → 后端 `Option<String>` 收 None）
- executeSlash case "image" 把 action.sizeOverride 透传给 runImageGenerate +
  userEcho content 也加 `-s WxH ` flag
- 重试按钮 onClick 也带 retrySize
- ImagePromptHistoryMenu Enter 路径手动构造 SlashAction 加 `sizeOverride: null`

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - `/image dragon` → 走 settings.image_size
  - `/image -s 1024x1792 dragon` → 用 1024x1792 单次生图；settings 不变
  - `/image -n 2 -s 1792x1024 dragon` / `/image -s 1024x1024 -n 2 dragon` / 任意 flag 顺序 → 都 work
  - `/image -s 999x9999 dragon` 格式合法但 dall-e-3 不支持 → API 拒 → errors 列表显原因 + 🔄 重试（重试用同 -s）
  - `/image -s abc dragon`（格式不合法）→ unknown 命令提示
  - `/image -s 1024x1024 -s 1792x1024 dragon`（dup flag）→ unknown 提示
  - `/image -h` 看到新 -s 示例

## 不在本轮范围

- 没在 give_image LLM 工具里加 size 参数：模型自己控制 size 反而让 prompt 更
  复杂，settings 默认值就好
- 没做"-s portrait / landscape"语义别名：直接 `WxH` 数字符合 OpenAI API 文档，
  更直接；要别名再加一层 mapping 没必要
