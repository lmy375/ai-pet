# 设置页加 chat model 测试按钮

## 需求

image_model 有"🧪 测试生图"按钮验链路，chat model 没有。新用户配完 base_url / api_key / model 后想知道"我配对了吗"，得真发一句聊天才能验。失败时常常分不清是"我说错话了"还是"接口根本不通"。chat 测试按钮提供一个秒级反馈点。

## 实现

### 后端

`src-tauri/src/commands/chat.rs` 加 `chat_test() -> Result<String, String>`：

- 非流式（`stream: false`）POST `/chat/completions`
- 固定 prompt `Reply with the single word: pong`、`max_tokens: 20`，最大限速保持秒级响应
- 不走 tool / 不写 session / 不动 mood / 不走 SOUL 注入 —— 纯连通性测试
- 失败透传 status code + 响应体前 200 字（同 image_test 套路）
- 成功返回 `choices[0].message.content` 的文本

`src-tauri/src/lib.rs` 注册 `commands::chat::chat_test`。

### 前端

`src/components/panel/PanelSettings.tsx`：

- 三个新 state：`chatTesting / chatTestReply / chatTestError`
- `handleTestChat`：performance.now 计时 → invoke chat_test → 写 state → finally 关 testing
- Model 字段 `<datalist>` 下方加 row：
  - 🧪 测试 chat 按钮（accent 色 / 灰态 / model 空 disable）
  - 旁边：成功显 `✓ X.Xs · <reply 截 80 字>`；失败显 `✗ <错误透传>`
- 与 image_test 完全对称的 UX，复用 btnSmallStyle + tint-green-fg / tint-red-fg

## 与 image_test 对比

| 维度 | chat_test | image_test |
|------|-----------|------------|
| 接口 | /chat/completions | /images/generations |
| stream | false | n/a |
| max_tokens | 20 | n |
| 响应 | 短文本 | 1 张 data URL |
| UI 展示 | 文本截 80 字 | 200px 缩略图 |
| 用时 | < 5s | 10-30s（dall-e-3 慢） |

## 验证

- `npx tsc --noEmit` clean
- `cargo check` clean
- 行为：
  - 配 gpt-4o-mini + valid key → 点测试 → 1-2s 显 `✓ 1.2s · pong`
  - 配错 model 名 → 红色 `✗ chat API 返回 404：...`
  - 配空 key → 红色 `✗ API Key 未配置...`
  - 配错 base_url → 红色 `✗ 请求 chat API 失败：...`

## 不在本轮范围

- 没做"测试 multimodal model"按钮（拖一张本地图给模型问"这是什么"）—— 需要 file picker / 本地静态图，比文本测试链路复杂；先观察是否真有需求
- 没做"一键全测"按钮（chat + image 一起）—— 用户分别点两次本就足够明确

## 剩余 TODO

- PanelTasks 任务详情解析 image markdown
- ChatMini 96px 缩略图 hover 📋
- /image 历史 prompt 召回
