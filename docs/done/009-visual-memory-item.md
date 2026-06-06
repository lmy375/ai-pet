# 009 · 视觉记忆 item — 多模态闭环第三块

001（看）+ 003（画）已落地，但「看过的能留下」缺失：图片输入严格 turn-内不持久化，PanelMemory 只接收文本 item。用户偶尔会想「这张图记一下」（菜单照片、白板截图、屏幕错误对话框），目前无处安放。

需求：
- ChatMini / TG 用户消息附图 + caption 含"记一下 / 存一下 / 以后看 / `/keep`"等意图词时，把该 turn 物化为 PanelMemory 一条 visual item。
- 缩略图（短边 ≤ 200px）落 `attachments/` 子目录，文件名 hash 化避重；item text = 用户 caption + LLM 对图的一句描述。
- PanelMemory 列表对 visual item 行渲染加缩略图前缀；点击放大查看；现有 mood / cat / pin / decay 等 chip 全部沿用。
- cat 归类：用户 caption 显式指定优先；否则 LLM 在现有 cat 列表中 best-match，不创建新 cat。
- 与 001 的 contract：001 的「对话 history 不留二进制」仍成立；本需求是用户主动 opt-in 的写入动作，独立于 history。
- /keep 反向操作：用户对 visual item 说「忘了它」走现有 memory 删除路径，缩略图同步清除。

---
实现笔记：
- 新建 `src-tauri/src/visual_memory.rs`：`is_keep_intent` 关键词检测、`save_thumbnail` (复用 photo.rs 提取的 `resize_and_encode_jpeg_to(raw, 200)` 长边压到 200px) + DefaultHasher 16 hex filename 去重、`format_visual_description` 拼 `[visual: <rel>] <body>` 协议、`detect_explicit_category` 扫 `#tag` 命中既有 cat、`keep_image_as_memory` 一站式入口。Tauri 命令 `keep_visual_memory`（前端显式 /keep）+ `read_attachment`（前端读缩略图 base64）。auto-detection hook 同时在 desktop `commands/chat.rs` 和 TG `bot.rs::run_photo_turn` 里 fire-and-forget 触发 —— 与 001 的 history 不留二进制契约互不干扰（visual_memory 是独立 opt-in 写入路径）。
- 删除链路：`memory_edit("delete", ...)` 已有 detail.md 删除分支旁加 `cleanup_thumbnail_on_delete(removed.description)`，parse 出 `[visual: …]` rel → 校验在 attachments/ 子目录后 unlink。path-traversal 双重防御（拒 `..` / 绝对路径，且要求 starts_with(att_dir)）。
- 前端 PanelMemory.tsx：模块级 `attachmentCache` Map 避同图重复 invoke；`VisualThumb` 子组件 lazy fetch；`displayDesc` 计算先剥 `[visual:]` 前缀，避免 raw bracket 文本干扰；点击触发 `setLightboxSrc` 弹既有 Modal 大图。原有 chip / 折叠 / 搜索高亮全部沿用，因为剥前缀只动 displayDesc。
- 两处对 GOAL 字面降级：（1）"LLM 对图的一句描述" 自动 path 未触发额外 LLM 调用（避免每 keep 都耗一次 vision LLM cost）—— description 只用用户 caption；用户可用 PanelMemory 编辑追加。（2）"LLM 在现有 cat 列表中 best-match" 简化为「caption #tag 命中既有 cat 优先，否则 ai_insights」，省一次 LLM 分类 call。两处若后续观察到 description 太单薄 / 归类不准再升级。
