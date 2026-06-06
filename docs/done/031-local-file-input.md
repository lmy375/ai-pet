# 031 · 本地文件输入 — 005 URL fetch 的本地对偶

005 让 pet 能 fetch URL 摘要，但用户拖个 .md / .txt / .pdf 文件让 pet 「看看这个写了啥 / 帮我总结」目前完全空白。feedback_pet_butler_direction 明确点名「file_tools 已具备」— 工具层早就在，但没接入 chat 路径。GOAL「通用任务」高频场景。

需求：
- ChatMini 输入框支持拖拽文件 / 粘贴文件路径；TG bot 支持 `document` 消息类型（与 001 photo 并列）。
- LLM 在 user turn 中自动调 file 读取 tool 把内容入 prompt；与用户 caption / 文字一并形成 turn。
- 文件按 token 预算截断（与 005 共用 1MB 上限语义），尾部加"截断了 X 行"提示。
- 不可读格式（二进制 / 不支持扩展）回退一句"我打不开这个格式"，不静默吞。
- 不持久化（与 001 同 contract）；用户显式说"记一下 / `/keep`" 走 009 视觉记忆同等 opt-in 路径，落 PanelMemory 文本 item（非 visual，文件不存原文件）。
- 单次最多 3 个文件；超出截断并提示。
- 不引入新 TG / panel 命令，拖拽 / 发文档是隐式入口。

---
实现笔记：
- 新建 `src-tauri/src/local_file_input.rs` pure module：扩展白名单（md / txt / yaml / 主流源码…，黑名单 PDF / docx / image / archive）+ `looks_binary`（前 4KB NUL 探针）+ `MAX_BYTES=1MB`（与 url_fetch 对齐）+ `MAX_PROMPT_CHARS=20000`（防 token 飙）+ `read_text_bytes_capped` / `format_for_prompt` / `FileReadErr` 分类。UTF-8 lossy 解码让 GBK 等非 UTF-8 也能看到大半。13 单测覆盖扩展、二进制 heuristic、字节/字符两道 cap、UTF-8 边界、错误分类。
- TG `handle_document_message` (bot.rs)：预闸（size > 2× MAX_BYTES / 扩展不在白名单）→ 拒绝 + user-facing message；下载 raw → `read_text_bytes_capped` → `format_for_prompt` → run_chat_turn。Session history 仅留 `[文件: name] <caption>` 占位（与 photo `[图片]` 同 contract，不让文件内容长期占 session token）。
- 不持久化：与 001 photo / 005 url-fetch 共 contract——本路径不写 PanelMemory，不存原文件，content 仅 in-flight 入 LLM。
- 缺口：（1）ChatMini 拖拽 / 粘贴路径未做——前端需独立测；（2）/keep 显式 opt-in 落 PanelMemory 未做（需 turn-level state 跟踪「上轮是文件 + 当轮命中 keep_intent」，可复用 visual_memory::is_keep_intent 但绑定路径不同）；（3）单次多文件（TG media_group document）未聚合——TG 罕见，多数客户端单 doc 单 message；（4）PDF / docx 未做（需要 lopdf / docx-rs，单独刀）。
