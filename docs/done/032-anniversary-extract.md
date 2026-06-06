# 032 · 纪念日 / 特殊日期自动识别 — 从 memory 里"挖出"用户的重要日子

reminder / 011 / 020 都要 user 显式设。但用户聊天里偶然提的「我生日 5 月 8 日」「我们 9 月 3 日结婚周年」「这个 deadline 是月底」目前 pet 完全错过 — 到日子 pet 一无所知，错失"宠物记得我的日子"这个最直接的情绪价值瞬间。

需求：
- 新 proactive 子项 `anniversary_extract`，每周一次扫近 90d PanelMemory item + session_distill（023）。
- LLM 提取 `(日期表达, 事件 desc, source memory id, 类型: 生日/纪念日/deadline/其它)` triplet；模糊只到月不到日的不入。
- 落入新 anniversaries store（persisted），不与 reminder / 011 混用。
- 触发：当日前 1 天晚上 21:00 + 当日早上（嵌入 016 morning_briefing 的 enrich 行）pet 主动祝福 / 提醒，文案围绕事件 desc 自由生成（不用「您今天是...日」模板）。
- TG `/anniversaries` 列已识别 + `/anniversary_del <id>` 撤回错识 + `/anniversary_add` 直接添加（保 user 显式入口）。
- 误识别保护：item 入库前若类型为生日 / 纪念日 → pet 主动 confirm 一次（"你 X 月 Y 日是生日吧？我记一下"），避免静默错记。deadline 类型不 confirm（高频且短期）。
- 受 017 / 026 情绪 gate：mood / stress 低时跳过提醒该日的「庆祝向」祝福，仅保留 deadline 类。

---
实现笔记：
- 新建 `src-tauri/src/anniversaries.rs`：persisted store `~/.config/pet/anniversaries.json`，`AnniversaryKind {Birthday, Anniversary, Deadline, Other}`，`date` 双形（`MM-DD` recurring / `YYYY-MM-DD` absolute；`parse_stored_date` 识别）。Pure：`entry_matches_date` / `entries_matching_date` / `filter_by_mood_gate`（仅过滤 celebratory）/ `format_for_briefing`（含「不要用『您今天是...』模板」反指令）。10 单测覆盖双形 / 闰年 2/29 / unconfirmed 跳过 / mood gate / 中英 kind 解析 / icon 完整性。
- 016 morning_briefing 早上集成：`proactive.rs::maybe_run_morning_briefing` 在 intent 拼装末尾 append `format_for_briefing(matches_today)`，复用既有 `is_in_low_distraction_mode` 作 celebratory gate；空命中不污染 intent。
- TG triad：`/anniversaries` 列（含 confirmed chip）/ `/anniversary_add <date> <kind> <event...>`（kind 接中英；三参缺一给用法）/ `/anniversary_del <id>`（短码删）。3 Tauri 命令 `anniversary_list/add/delete` 给将来 Panel 用。
- **缺口**（this iteration 未做）：
  1. **LLM 周扫抽取**：proactive 子项 `anniversary_extract` 未做——需 prompt 工程让 LLM 从 ai_insights / user_profile / session_distill 抽 (date, event, kind) 落 unconfirmed entry；本刀仅手动 add 路径走通。
  2. **未 confirmed 流**：`confirmed` 字段 + Birthday/Anniversary 默认 false 已就位；但 pet 主动 confirm 反问（"你 X 月 Y 日是生日吧？"）+ `confirm_anniversary` LLM tool 未实现。LLM 抽取上线前不影响。
  3. **当日前一晚 21:00 提醒**：`format_for_eve_intent` 已 pub 备用，但 `maybe_run_anniversary_eve` proactive hook 未接（emit boilerplate 较重，留 follow-up）。当下 morning 当日已覆盖最关键的「记得日子」体验。
  4. **mood gate**：本刀仅用 026 stress 低分布式模式作 celebratory gate（一个 bool）；017 mood-text 多档分类（焦虑 / 沮丧 / 累 / 烦）未单独接——后续可加 keyword list。
