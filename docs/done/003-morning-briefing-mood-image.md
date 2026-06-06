# 003 · 早安播报附心情图 — 多模态生成第一刀

001/002 打通了图片**输入**两侧；GOAL.md「图片生成」完全未触达。生成侧最稳的首刀不是 `/draw` 工具命令，而是嫁接到已有的 `morning_briefing.rs` 日常节奏：宠物每天早安播报时，按当下 mood 生成一张视觉问候图随文字一起推。

需求：
- morning_briefing 触发时，除现有文字外，额外按 current_mood（mood.rs / mood_history.log）生成一张小尺寸视觉问候图。
- 图片提示词由 mood tag + 简短风格约束拼出，固定可爱风格基调（与 GOAL「UI 要美观可爱」一致）。
- 一天一次硬上限；生成失败时回退到「仅文字」原行为，不阻塞早安推送。
- 图片随早安一同在 ChatMini 与 TG 渲染（依赖 001/002 已落地的多模态通路）。
- 不持久化二进制；history 中只留 mood tag + `[早安图]` marker。

---
实现笔记：
- 后端 `proactive.rs::generate_morning_image` 在 briefing LLM 完成后用 `mood_after` 拼水彩 / Ghibli 风格 prompt，调既有 `run_image_generate`，1 张 / 用户 image_size。`ProactiveMessage` 加 `image_url: Option<String>` 同步推。一天一次由现有 `LAST_MORNING_BRIEFING_DATE` + `morning_briefing_last.txt` 已守好，复用。
- 不入二进制的 trick：useChat 加 `proactiveImages: Record<ts, url>` transient state（绕开 `messages` content 与 `itemsRef`），ChatMini render 时按 `m.ts` 查表叠到 imgs。重启即失效；`messages` / disk 只保留文字 `... [早安图]` marker，给未来 LLM context 提示「之前发过图」。失败时不挂 marker，避免误导。
- TG 渲染未做：proactive→TG 桥在当前 codebase 不存在（morning_briefing 当前只 emit 桌面 `proactive-message` 事件），属于另一块独立改造，本轮不引入。
