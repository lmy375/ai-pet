# 019 · pet 沟通风格 self-tune — 自我进化的第四维

PanelPersona 有「当下心情」+「最近常用工具」(b6a193e)，但 user 对 pet 说话风格的反馈（"以后简短点 / 别用太多 emoji / 直接给结论 / 用 markdown"）没机制持久化 — 下次仍按旧风格回答。GOAL「自我进化」中情绪/记忆/技能三柱已覆盖，沟通方式是该被补的第四维。

需求：
- 在 user→pet turn 处理中，LLM 检测当条 message 是否含对 pet 沟通风格的反馈（短指令式："简短点 / 别 emoji / 别那么活泼 / 给结论 / markdown"等）。
- 命中后写入 PanelPersona 新「沟通偏好」section（与"当下心情"、"最近常用工具"并列），每条 entry 含 ts + 偏好语 + 原始 user message 引用。
- 风格 record 注入所有 LLM prompt 头部（chat / proactive / morning_briefing / 011 报告 / 012 deferred 输出 全部共用）。
- TG `/persona` 展示当前所有偏好；`/persona_clear <id>` 删除单条；`/persona_reset` 清空。
- User 撤回反馈（"算了你正常说话 / 还是按之前的来"）→ LLM 识别后将对应 record 标 cleared，不物理删除（保 audit）。
- 偏好之间冲突时（前后矛盾）按时间倒序优先；不引入冲突解决面板。

---
实现笔记：
- 新建 `src-tauri/src/communication_prefs.rs`：JSON store `communication_prefs.json` + `PreferenceEntry { id, created_at, preference, source, cleared, cleared_at }` + `add_preference` / `clear_preference`（软删保 audit）/ `reset_all`。pure `active_preferences_desc`（按 created_at desc + 跳 cleared）、`format_for_injection`（cap 8 条防 prompt 涨 + 末段说「时间倒序前者优先」让 LLM 自处理冲突）。
- 新 LLM tool `set_communication_preference`（`src-tauri/src/tools/communication_pref_tool.rs`）注册到 ToolRegistry + BUILTIN_TOOL_NAMES。三 action：add / clear / clear_all。clear 接受 `id_or_match`，substring 匹配多条时取最近 active 一条（与冲突解决「时间倒序」一致）。
- 注入层 `inject_communication_prefs_layer(messages)` 在**所有** 8 个 `run_chat_pipeline` 站点接入：desktop chat / TG chat / 5 个 proactive wrappers (regular turn / morning_briefing / memory_follow_up / welcome_back / scheduled_report / deferred_task)。
- TG 命令 `/persona` / `/persona_clear <id>` / `/persona_reset` 三件套全部 wired；handler 用既有 `format_missing_argument` / `format_command_error` 模板。
- 缺口：「PanelPersona 新「沟通偏好」section」**未做** UI（PanelPersona 2226 行入侵风险大）。后端 Tauri 命令 `communication_prefs_list` 已为前端预留入口；TG 路径完整可用让 owner 现在就能看 + 操作。后续 iter 单独做 PanelPersona section。
