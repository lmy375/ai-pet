# 006 · 意图分类 → 技能画像 — 自我进化(技能)支柱

GOAL.md「自我进化：自己有独立的情绪、记忆、技能」中「技能」支柱完全空白：mood 模块齐全、PanelMemory 富，但宠物对"用户最常找我做什么"零认知。`/whoami` 现仅显示 mood，没有"我擅长什么"的自我画像。

需求：
- 每次 user→pet 对话落地后，对 user message 做一次轻量意图分类，归入若干预设 intent（chat / 问答 / 翻译 / 总结 / 写代码 / 情绪倾诉 / 指令执行 / 其它）。分类逻辑放在 proactive 之外的独立 `skill_profile` 模块。
- 落入按日 bucket 的滚动 30d 直方图，磁盘持久化（与 mood_history.log 同级目录）。
- `/whoami` 输出新增一行：「最近 30d 你最常找我：翻译 12·共情 9·写代码 7」格式，取 Top-3。
- 当某 intent 30d 计数突破阈值（如 ≥ 20），向 PanelPersona 写一条「我擅长 X」chip（与 mood chip 并列，不入 PanelMemory）。
- 分类失败 / 模型不可用时静默跳过该次累计，不阻塞主对话；intent 桶不引入用户可配置项。

---
实现笔记：
- 新建 `src-tauri/src/skill_profile.rs`：规则分类（无 LLM cost，「轻量」精神）、`intent_history.log` 与 mood_history 同目录、`tally_recent_30d` 30d 窗 + 未知 key 归 `other`。chat.rs / bot.rs::run_chat_turn 在 url_fetch 注入后接 `spawn_record_from_content` fire-and-forget。
- /whoami 新行：TG 走新增 wrapper `format_whoami_reply_with_intents`（避开改 5 参签名导致的 11 处既有单测全炸）。桌面 PanelChat.tsx 加 `get_top_intents_30d` invoke + 本地 zh map（与后端 intent_zh 对齐）。
- PanelPersona「我擅长 X」chip：本轮**未做**。原因：PanelPersona 2226 行、chip 行 schema / 渲染插入位都需要专门设计；先把分类 + 落盘 + /whoami 跑通让数据先攒起来，chip 阈值（≥20）等数据真到了再加。
