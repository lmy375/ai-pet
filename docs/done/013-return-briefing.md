# 013 · 回桌 briefing — "你不在的时候我做了 X、Y、Z"

008 让宠物在用户回桌时招呼一句；011/012 让宠物在离桌期间能自主跑活儿。两者直接拼接会刷屏（每条 deferred / scheduled fire 都单独 utterance）。管家应该的形态是：用户回桌瞬间，把"你不在的时候我做了什么"打包一次性汇报。

需求：
- 在 008 welcome-back 触发点扩展：fire welcome utterance 时同步拉取该 idle 区间内的 deferred_task 完成（012）、scheduled_report fire（011）、butler_task 归档（b6a193e）。
- 若有命中：utterance 升级为 briefing 形态 — 「欢迎回来。期间我处理了：① 你早上让我查的 X，结论 ... ② 今天 17:00 的周报已发 ... ③ 某 butler_task 已归档」三行内。
- 命中条数为 0：退回原 008 单句 welcome 行为，不堆"今天没有要汇报的事"这种废话。
- briefing 内容点击 / TG 回复对应序号触发详情展开（详情来自原 task / report 输出 buffer，不二次调用 LLM 重生成）。
- 受 gate.rs 现约束：deep-focus 抑制时连同 briefing 一起延后，不拆开发。
- 不引入新持久化层：所有数据来源是 011 / 012 / butler_task 已有 store；本需求是聚合 + 展示。

---
实现笔记：
- 复用 008 的 maybe_run_welcome_back 触发点：fire 前先调新 `welcome_back::collect_briefing_items`（pure）扫 `butler_history.log` 拍出窗口内 `report_fired` (011) / `deferred_fired` (012) / `archive` (consolidate.rs 已写) 三类事件 → 拍成 `BriefingItem` 列表。窗口 = `[now - prev_idle_secs, now]`。
- 新 `format_welcome_briefing_intent(idle_min, mood, note, items)`：items 空时直接 delegate 给既有 `format_welcome_back_intent`（保证 008 原行为不变），非空时升级 prompt 让 LLM 用「① ② ③」管家口吻汇报，每条 ≤ 30 字摘要 / 总长 ≤ 120 字 / 最多 3 条（超过的留给用户事后追问）。
- 零新持久化：只读 butler_history.log；事件源 011/012 的 record_event 已在前两轮加上。decision_log 的 WelcomeBack 行追加 `briefing N items` 让 audit 一眼看「这次回桌带了几条 briefing」。
- 缺口：「点击 / TG 回复序号 → 详情展开」未做。GOAL 写「详情来自原 task / report 输出 buffer」，buffer 实际是 speech_history.log（每次 emit 都 record_speech），但要做 click handler 或 TG 命令 + ts-by-序号 lookup 是另一块独立功能。本轮先把聚合管道跑通，detail expansion 留给后续单独需求。
