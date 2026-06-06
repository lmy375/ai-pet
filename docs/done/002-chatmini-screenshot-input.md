# 002 · ChatMini「📸 看屏幕」按钮 — 桌面侧多模态输入

001 打通 TG 图片**接收**链路后，桌面侧需要对偶入口。比文件选择器更贴宠物场景的形式是直接「看屏幕」：用户工作时让宠物用视觉看一眼自己在干嘛，把现有 proactive 的「用了 VS Code 47 分钟」元数据维度补成视觉维度。

需求：
- ChatMini 输入框旁加「📸」按钮，点击后抓主显示器当前截图，作为 vision part 注入即将发出的 user turn。
- 输入框文本作 caption 与截图同发；无 caption 时仅发图，宠物自主决定回应。
- 截图按多模态 LLM 上限缩放（保宽高比），不持久化二进制；history 只留 caption + `[屏幕截图]` 标记。
- 抓屏失败（权限未授）时给清晰提示并引导授权，不静默吞错。
- 截图链路复用 001 的多模态通路；两条需求可独立交付但同期更顺。

---
实现笔记：
- 后端 `commands/screenshot.rs` shell 到 `screencapture -x -t png -D 1`，再过 001 的 `telegram::photo::resize_and_encode_jpeg`（image crate 加 png feature，函数改用 guess_format 让两 path 共用）。
- 「不持久化二进制」前端实现：`useChat.sendMessage` 加 `opts.historyText`，`done` 事件把最近 user message 的 multimodal payload 改写成纯文本占位，ChatItem 不带 images。常规 paste/drop 图未传 opts 走老路径不变。
- 坑：TCC 拒绝在新版 macOS 下不是非零退出而是 exit 0 + 空 PNG —— 加了 empty-file 兜底走同一权限引导文案，不静默吞错。
