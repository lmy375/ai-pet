# Telegram 机器人

让你在手机上通过 Telegram 和宠物聊天。它使用一个独立的会话（`telegram-bot`），
与桌面共享同一套人设和长期记忆。

## 配置

1. 在 [@BotFather](https://t.me/BotFather) 用 `/newbot` 建一个机器人，拿到 token。
2. 面板「设置」里填 `bot_token`、`allowed_username`（只有这个用户名能和它对话；留空则
   不限制），勾选 `enabled`。
3. 保存后机器人自动启动；也可用 `reconnect_telegram` 重连。对应 `config.yaml` 的
   `telegram` 段。

## 图片

- **收**：你发的照片会取最大尺寸、转成多模态内容喂给模型（需视觉模型）；照片说明
  (caption) 作为文字一并带上。
- **发**：宠物在对话中产生的图片（如截图）会作为照片发回给你。

## 说明

- 没有斜杠命令 —— 所有消息（含 `/xxx`）都直接当对话发给宠物。
- 上下文取该会话最近 50 条消息 + 人设。
- 回复超过 Telegram 单条上限会自动按句/词边界拆分发送。
