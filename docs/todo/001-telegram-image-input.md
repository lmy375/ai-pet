# 001 · Telegram 图片输入 — 多模态对话第一步

GOAL.md 要求「多模态支持：图片写入与图片生成」，目前 681 commit 完全未触达。先打通图片**写入**侧最轻的入口：Telegram bot。

需求：
- 用户在 TG 向宠物发送图片消息（单图或 album），宠物用多模态 LLM 看图并回复。
- 图片的 caption 作为 user text、图片作为 vision part 一起注入当前对话 turn；album 多张图合并为同一 turn。
- 无 caption 时，宠物自主决定回应（描述、提问、共情都行），不强行 fallback 模板。
- 大图按模型上限缩放后再送（保宽高比，避免 token 爆 / API 拒）。
- 对话 history 中不保存图片二进制，仅留 caption 与 `[图片]` 标记，保持现有 history 体积约束。
- TG 文本命令链路（/here_*、/audit_summary 等）不受影响。
