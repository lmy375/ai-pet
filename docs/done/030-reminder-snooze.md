# 030 · reminder snooze / reschedule — 015 + 022 之外缺的最后一块对偶

015 (conversational cancel) + 022 (time ambiguity confirm) 已让 user 在对话里"取消"和"创建时不确定澄清"通畅。但最高频的"再等 10 分钟提醒"、"挪到明天 9 点"、"晚 30 分"目前没对偶 — user 只能先 cancel 再重设，两步反而比 GUI 难用。

需求：
- LLM 在 user→pet turn 检测 snooze / reschedule intent（短词："再等 / 晚 N 分 / 挪到 / 改到 / 推迟 / 还是 X 时再来"）。
- 候选来源：未触发的 reminder / 已触发但 user 标 snooze 的 reminder；候选 ≤ 3，与 015 共用 disambiguation 模块。
- 时间表达走 022 ambiguity confirm 模块（模糊词反问，精确词直接落）。
- 短期 snooze（≤ 24h）原 reminder 更新 ts；跨日 reschedule 同路径不分裂出新 entry，保 audit trail 在 butler_history 加 `snoozed`/`rescheduled` event。
- 同一 reminder 短期 snooze 上限 3 次 / 24h，超出后 pet 主动反问「这条要不要直接改日子或删了？」。
- 不引入新 TG / Panel 命令；纯 LLM tool 层补 `snooze_reminder(id, new_ts)`，对话路径自动调用。
- 020 chain 节点是 reminder 时同样适用；snooze 一个 chain 节点不影响后续节点的依赖关系。

---
实现笔记：
- 新 tool `snooze_reminder(title, new_ts, reason?)` 在 `src-tauri/src/tools/snooze_reminder_tool.rs`。in-place 更新 todo memory item 的 `[remind: …]` 前缀，topic 保留；`new_ts` 仅接精确 `HH:MM` 或 `YYYY-MM-DD HH:MM`，模糊词由既有 022 inject_time_ambiguity_layer 在 prompt 层拦截。
- action 区分：TodayHour → `snooze`（短期），Absolute → `reschedule`（跨日）。butler_history record_event 同 015 协议，snippet `"<old> -> <new> :: <reason|"(no reason)">"`。
- 24h 上限：`count_recent_snoozes` 扫 butler_history `action="snooze"` + title 精确匹配 + 24h 窗；>=3 时返 `{status:"refused", reason:"snooze_cap_24h", message}` 让 LLM 自然反问 owner。Reschedule 不计入。
- 拒绝路径：`[cancelled:]` 已存在 / `[recur-daily:` 前缀 / 非 reminder 前缀 / new_ts 格式错 → 全部 `{error}` 让 LLM 回退到 cancel + 新建。
- 020 chain：tool 只改 `[remind: …]`，不动 `[blockedBy:]` → 单节点 snooze 天然不破坏后续依赖。
- 6 单测：parse_new_ts × 3、classify × 1、canonical prefix × 1、count_recent_snoozes × 3（filter / 窗外 / 负时区 offset 不被 rfind('-') 误判）。
- 缺口：015 disambiguation 未抽共用 Rust 模块——当前 tool description 软约束 LLM「ambiguous → 列 3 候选反问」，多候选靠 LLM 自觉；后续可抽 `find_candidate_reminders(text)` 给 cancel/snooze 两 tool 共用。
