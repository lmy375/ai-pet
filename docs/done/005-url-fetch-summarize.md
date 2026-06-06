# 005 · URL 抓取与摘要 — 通用任务执行第一刀

GOAL.md「通用任务：完成简单的定制化任务」目前在 reminder + morning briefing 之外完全空白。最高频的"简单任务"是用户在聊天里丢链接顺口问「这讲了什么」。当前宠物只能凭标题猜测，缺乏抓取能力。

需求：
- ChatMini / TG 聊天中检测到 URL（http/https 单条或多条），宠物在生成回复前对每条 URL 做轻量 fetch（限正文/标题，剥广告、登录墙不强求）。
- 抓回的正文与用户原文一并送入 LLM；宠物输出含该 URL 的总结/回答，而不是泛泛凭标题。
- 抓取失败（超时、4xx/5xx、二进制响应）时静默降级到"无抓取"原行为，并在回复尾部一行注明「⚠️ 链接抓取失败」。
- 单次最多抓 N=3 个 URL，单条 ≤ 1MB；超出截断并提示。
- 抓取内容不持久化到 memory；持久化路径留给后续单独需求决定。
- 用户消息含 `--no-fetch` 后缀时禁用抓取（debug / 不想被拽离话题的退出阀）。

---
实现笔记：
- 新建 `src-tauri/src/url_fetch.rs`：`extract_urls` regex 拆 + trailing 标点剥；`fetch_url_summaries` 并发 `join_all`，10s 超时 / 1MB cap / 非文本 CT 拒；`extract_title_and_body` 用正则抓 `<title>` + 干掉 script/style 块再 strip tag（不引 scraper crate，编译时间 / 二进制体积更友好，对齐 GOAL「剥广告/登录墙不强求」）。
- `inject_url_context_layer(messages)` async 注入 layer，desktop `commands/chat.rs` 在 `inject_deadline_context_layer` 后接、TG `telegram/bot.rs::run_chat_turn` 在 `inject_telegram_dispatch_layer` 后接。两条 path 都在 inject **前** 已 snapshot session，所以抓取内容天然不入持久化 —— 满足「不持久化到 memory」。
- 失败降级是软约定：system note 末尾要求 LLM 在回复尾部追加「⚠️ 链接抓取失败：…」单独一行。强制截断 LLM 输出会侵入 chat 流的太多边界，留作软约束 + 后续若发现 LLM 不照办再升级到 post-process。
