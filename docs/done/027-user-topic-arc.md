# 027 · user 关注主题画像 — 与 006 intent 正交的 subject 维度

006 已让 pet 知道 user "最常找我做什么"（动作维度：翻译 / 写代码 / 共情）。但「user 最关心什么」（主题维度：某项目 / 减肥 / 恋爱关系 / 学某语言）目前完全空白。023 session_distill + PanelMemory 数据齐备，没人扫提炼。GOAL「了解用户」缺这一面，且影响所有 LLM 回答的上下文质量。

需求：
- 新 proactive 子项 `topic_arc`，周期触发（周日 22:00，与 021 / 024 / 025 错开）。
- 输入：近 30d session_distill + PanelMemory entry + butler_history。
- LLM 聚类提炼 top-5 topic，每条含 keyword、提及次数、首次/最近提及 ts、相关 memory item ids。
- 写入 PanelPersona 新「最关心的事」section（与 mood / 工具 / 沟通偏好并列）。
- 注入 chat / proactive 所有 LLM prompt 头部，让答复天然贴 user 当前关注。
- TG `/topics` 查看 + `/topic_clear <id>` 撤回错识 + `/topic_pin <id>` 锁定不被下次扫覆盖。
- 30d 滚动；扫描频率 / 提炼上限常量集中，不暴露用户。
- 与 006 区分：006 = 动作维度（intent type），027 = 主题维度（subject domain），两者并存于 PanelPersona 不重复。

---
实现笔记：
- 新建 `src-tauri/src/topic_arc.rs`：JSON store `topic_arc.json` + `TopicEntry { id, keyword, mention_count, first_seen, last_seen, memory_item_titles, pinned, cleared, cleared_at }`。`replace_unpinned` 原子替换非 pinned 非 cleared 旧 list（pinned 保留，cleared 保 audit）；`clear_topic` / `set_pin` 软删 / 锁定；`inject_topic_arc_layer` 注入 system note；`format_topic_arc_intent` LLM 扫描 prompt。Tauri 命令 `topic_arc_list`。4 单测覆盖排序 / cleared 跳 / pinned chip / intent 协议。
- 新 LLM tool `set_topic_arc`（`src-tauri/src/tools/set_topic_arc_tool.rs`）注册到 ToolRegistry + BUILTIN_TOOL_NAMES；description 明示「只在 topic_arc scan 调一次」防 LLM 在日常 chat 误调清盘。
- proactive.rs `maybe_run_topic_arc_scan` + `LAST_TOPIC_ARC_WEEK` ISO 周去重；Sun 22:00 + 60min grace。喂 build_recent_session_corpus + memory_list titles 给 LLM；不论 set_topic_arc 是否被调用都 mark 本周完成（SILENT / 失败下周再说）+ butler_history `topic_arc_scan` audit。
- 11 个 `run_chat_pipeline` 站点加 `inject_topic_arc_layer`（紧跟 026 stress 注入）。topic_arc scan 自身**不**自注入 — 避免本轮 list 影响本轮聚类输入。
- TG `/topics` / `/topic_clear <id>` / `/topic_pin <id>` 三件套 wired。
- 与 006 独立：006 store / 027 store / 006 inject / 027 inject 完全分轨；PanelPersona 端只是顺序展示两份 section（GOAL「并列」）。
- **缺口**：（1）PanelPersona「最关心的事」UI section 未做（PanelPersona 2226 lines 入侵风险大；后端 `topic_arc_list` 已为前端预留入口）。（2）butler_history 未入 LLM 输入 — 噪音比偏低（事件标题信息密度 < session 文本 / memory titles）；如果实地观察到主题缺失再补。（3）`/topic_unpin` TG 命令未暴露 —— `set_pin(id, false)` 函数已支持，但走 `/topic_clear` + 下次扫描自动重出是 v1 简化路径。
