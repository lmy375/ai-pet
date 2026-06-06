# 037 · 长期目标追踪 — 带进度的 trajectory 维度

memory item 是 episodic（一件事一条）、reminder / 011 / 020 是 time-anchored single fire。但用户的「今年想读 24 本书 / 减重 10 斤 / 跑步 100 公里 / 学完某课程」这类长期带进度目标完全无承接。GOAL「了解用户 / 自我进化 / 通用任务」三柱在长期 trajectory 维度合流缺位。

需求：
- 新 store `goals` 与 reminder / scheduled_report / butler_task 平级；持久化磁盘。
- 用户 turn 中 LLM 检测 goal-shape intent（"我想 / 计划 / 决定 / 今年 / 本月要 ... N 个/N 次/N 公斤"等可量化句式）→ 创建 goal entry：text、target value、unit、horizon (3m/6m/12m)、created_at、progress=0。
- pet 周期 check-in（默认每周一 21:00）主动询问当前进度："你今年想读 24 本书，已经读了几本？"；用户回复 → 更新 progress；跳过 → 不追问，下周一再问。
- LLM 在 user turn 中识别隐式进度更新（"刚读完一本"、"今天跑了 5 公里"）→ 自动累加 progress，不需 user 显式 /goal_progress。
- TG `/goals` 列表 + `/goal_progress <id> <delta>` 显式更新 + `/goal_del <id>` 删除 + `/goal_done <id>` 标完成。
- 与 027 topic arc 关联：goal text 进入 topic arc 作为强信号（user 长期关注什么 → 自动是 top topic）。
- 011 built-in：可加一条 "monthly goal review" scheduled_report（不在本需求交付，留待 021 通路延伸）。

---
实现笔记：
- 新建 `src-tauri/src/goals.rs`：`GoalEntry {id, text, target, unit, horizon, progress, status, last_check_in_at, last_progress_at, completed_at, deleted_at}` + `GoalHorizon {Quarter, HalfYear, Year, Other}` 中英 parse + `GoalStatus {Active, Completed, Deleted}`。常量集中：`CHECK_IN_INTERVAL_DAYS=7`。
- 操作：`create_goal`（拒空 text / target ≤ 0 / 空 unit—spec 「可量化」门槛）/ `update_progress`（delta 正负皆可；累至 target 自动 mark Completed）/ `complete_goal` / `delete_goal`（软删保 audit）。
- Pure：`active_goals_oldest_progress`（按 last_progress_at 升序，最久在前）/ `pick_check_in_candidate`（active + 距 last_check_in_at ≥ 7d + 最久 progress 优先；含含负时区 offset 解析）/ `format_for_inject`（inject 列表，按 active oldest）/ `format_check_in_intent`（含 SILENT 退出口 + 反指令不施压 + tool call hint）。
- 新建 `src-tauri/src/tools/goal_edit_tool.rs`：统一 LLM tool action 路由 `create / update_progress / complete / delete`，工具描述明示「不要 double-count；vague statement 不可量化 → decline」+ 「completed/deleted 拒绝 update」。
- inject 集成：11 个 chat pipeline 站点（与 040 user_lexicon / 027 topic_arc 同 11 站，replace_all 命中 8 + 3 单独 Edit），紧跟 user_lexicon 之后。`maybe_run_goal_check_in` 自身**不**自注入避循环。
- 周扫 `proactive.rs::maybe_run_goal_check_in`：Mon 21:00 ± 60min grace + per-ISO-week dedup。Mon 21:00 与现有 6 周扫（Sun 21:00 mood / Sun 22:00 topic / Mon 04:00 routine / Mon 18:00 forget / Tue 22:00 lexicon / Wed 18:00 consolidate）错峰。流程：mute → weekday/time gate → ISO-week dedup → pick_check_in_candidate → SILENT 仍 mark 周不 mark last_check_in_at → 非 SILENT emit ProactiveMessage + 刷 last_check_in_at。
- TG 四件套：`/goals`（列含 % chip + horizon + id）/ `/goal_progress <id> <delta>`（含负值；命中 100% 显 🎉）/ `/goal_done <id>`/`/goal_del <id>`。4 Tauri 命令 Panel 备用。
- 12 单测：horizon parse 中英 + label 协议 / active oldest 排序 / pick 三档（never checked → cand / recent → none / oldest progress 优先 / completed-deleted skip）/ inject 空 / inject 含 % + 反指令 / check_in_intent 含目标原文 + SILENT 退出口 + tool call hint + 反指令不施压。
- **缺口**（this iteration 未做）：
  1. **topic_arc 强信号关联**：spec 写「goal text 进入 topic arc 作为强信号」，本刀未做。需在 027 topic_arc weekly scan 的 intent / corpus builder 里加入 active goal text；改 027 涉及 prompt 调优 + replace_unpinned 协同，留单独刀。
  2. **011 monthly goal review built-in**：spec 显式标「不在本需求交付」。
  3. **隐式进度 double-count 防护**：tool description 软约束 LLM「same delta one turn = double-count」，无 backend 强校验（即使存了 last_progress_at，相同 ts 内连续 delta 也可能被 LLM 重复调）。可后续加 turn-id 去重。
  4. **完成时庆祝 utterance**：当下仅 TG /goal_progress 命中 100% 显 🎉，但 LLM update_progress 命中 100% 自动 mark Completed 时**无主动 emit 庆祝**——目标完成是情绪价值高峰，应触发 surprise（034）或独立 emit。后续可加 hook：goal completed → spawn surprise (kind=ChainCompleted 占位变体可复用) 或 self_note。
  5. **store 增长**：cap 未设；spec 未要求。预期目标条数低，可忽略。
