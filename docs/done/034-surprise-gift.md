# 034 · pet 偶发 surprise — 把"情绪价值"从被动响应升级为主动给予

所有现有 proactive 触发线（reminder / morning_briefing / welcome_back / memory_followup / scheduled / deferred / anniversary）都是 reactive — 响应事件、时间、状态。但 pet 主动给 user 一个小惊喜（一句诗、一张自画小图、user 关心主题的 fun fact、旧 memory 的温柔 callback）完全空白。GOAL「情绪价值」最直接、却最被忽略的入口。

需求：
- 新 proactive 子项 `surprise_gift`，频率严格 ≤ 1 次/7d（避免廉价化）。
- 触发候选窗口：mood low 持续 ≥ 3d 后回升 / 完成长 chain / anniversary 当日加成 / 30d 随机偶发其一命中。
- 内容形态由 LLM 自选：① 一句诗 ② 走 003 通路画一张小图 ③ 027 topic arc 中关心主题的 fun fact ④ 旧 memory 的温柔 callback "我还记得你上周说想试咖啡店，去了吗？" ⑤ 一句感谢。
- 受 017 pet mood / 026 user stress / gate.rs deep-focus 全部 gate 抑制（焦虑 / 高 stress / 深度工作期一律不出）。
- 落地后 pet self_note（029）记一行"我今天给 user 准备了 X"，形成 surprise 的自我记忆。
- 用户对单次 surprise 反馈"别送这个 / 多余了" → 命中 019 persona 关闭整个 surprise 通道（不退化为类型黑名单细控）。
- TG `/surprise_log` 可回查最近 N 次 surprise + 用户反馈；不引入主动触发命令（surprise 必须保自发性）。

---
实现笔记：
- 新建 `src-tauri/src/surprise.rs`：持久化 store `~/.config/pet/surprise_log.json`（capped 100 条）；`SurpriseTrigger {MoodRebound, ChainCompleted, AnniversaryToday, RandomMonthly}`；pure `throttle_pass` (7d 硬上限，含负时区 offset 边界) / `in_daytime` (10-21) / `is_disabled_in_prefs` (7 关键词扫描) / `intent_for_trigger` (含 SILENT escape hatch + 反指令禁模板)。9 单测覆盖 throttle 各分支 / daytime 边界 / persona disable / intent 反指令 / label 协议稳定。
- `proactive.rs::maybe_run_surprise_gift` 复制 maybe_run_memory_follow_up 范式：5 重 gate（mute / daytime / 7d throttle 内存+硬盘双源 / persona disable / 026 stress 低分布式模式）→ pick_trigger（v1 仅 AnniversaryToday / RandomMonthly 二选一，MoodRebound + ChainCompleted variant 占位）→ run_chat_pipeline → SILENT 仍 mark throttle 防同窗反复重试 → record_surprise + emit ProactiveMessage + 落 self_note。`LAST_SURPRISE_GIFT_TIME` mutex + disk store throttle 双源——重启清内存 mutex 时仍读 store 拿 last_fired_at，"重启即失 7d 窗" 不会发生。
- self_note `TriggerKind::SurpriseEmit` 新增 variant + label "surprise_emit" + template 第一人称「今天我给主人准备了一份小 surprise」。trigger_kind_labels_are_stable 测试同步追加断言。
- TG `/surprise_log [N]` 列最近 N（默认 10、clamp 1..=50）。spec 明确"不引入主动 fire 命令"——保 surprise 自发性。
- 集成点：proactive tick 顺序在 topic_arc_scan 之后、evaluate_loop_tick 之前；与早安 / follow-up / scheduled / deferred 全部并列且自带 gate stack，不挤占常规 cooldown。
- **缺口**（this iteration 未做）：
  1. **MoodRebound 检测**：variant 占位但未实现。需扫 mood_history.log 跨日做"低→回升" pattern 识别（≥3d low + 当下 not-low）。后续加 hook 一步换 `pick_trigger` 即可。
  2. **ChainCompleted 检测**：同上占位。需扫 deferred_tasks store 整 chain Done 状态。
  3. **user reaction 回写**：`SurpriseEntry.user_reaction` 字段已就位，但 user 下次说"别送这个" 时尚无 LLM tool 写回该条 entry。可加 `record_surprise_reaction(id, reaction)` LLM tool；当下靠 persona disable 整通道关闭（spec 反指令「不退化为类型黑名单」与此自洽）。
  4. **03 image surprise**：intent ② 提到走 003 image 通路，但 ProactiveMessage emit 当下 image_url=None；让 LLM 输出图需调用 image_generate tool（已存在），但目前 emit 路径不会把 tool 输出的 url 自动回填到 payload。后续可加 ToolContext 监听 image_generate hit 并回填。
  5. **gate.rs deep-focus**：v1 仅用 026 stress (`is_in_low_distraction_mode`) 作压力 gate，未单独 deep-focus 信号——gate.rs 当下也无 `is_in_deep_focus`，需待 focus 检测层独立做。
