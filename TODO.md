# TODO

每完成一项就把它从 TODO 移到 DONE.md（带日期），并在 IDEA.md 中记录设计变化。
每次迭代尽量小、可见、可测。

## 下一阶段：质量收口优先级（2026-05-03 代码质量评估后新增）

这些任务优先于继续堆新功能。目标是把当前 alpha 质量推进到可长期维护的状态。

- [x] Quality Gate 1：清理 Rust 格式和 lint（2026-05-03 完成 — Iter QG1）

- [x] Quality Gate 2：给 LLM tool-call loop 增加最大轮数和明确失败路径（2026-05-03 完成 — Iter QG2）

- [x] Quality Gate 3：统一手动 proactive trigger 与后台 loop 的 telemetry（2026-05-03 完成 — Iter QG3）

- [x] Quality Gate 4：补齐 prompt reinjection redaction（2026-05-03 完成 — Iter QG4）

- [x] Quality Gate 5：拆分 `src-tauri/src/proactive.rs`（2026-05-03 完成 — 7 slice
  全部完成，5500 → 3028, ~45% 缩小）。
  - AI prompt：每个 iter 抽一个完整 cohesive 子系统到 `src/proactive/<sub>.rs`，glob `pub use`
    re-export 保 public API 不变。先做行为不变的纯移动，之后行有余力再做内部清理。
  - [x] QG5a：reminders 子系统（`ReminderTarget` / `parse_reminder_prefix` / `is_reminder_due` /
    `format_target` / `is_stale_reminder` / `format_reminders_hint` + 17 测试）2026-05-03 完成
  - [x] QG5b：butler_tasks schedule 子系统（2026-05-03 完成 — `ButlerSchedule` / `parse_butler_schedule_prefix` /
    `is_butler_due` / `has_butler_error` / `is_completed_once` / `format_butler_tasks_block` +
    private `parse_updated_at_local` + `BUTLER_TASKS_HINT_*` 常量 + 24 测试）
  - [x] QG5c-prep：time helpers 子系统（`idle_tier` / `user_absence_tier` /
    `period_of_day` / `weekday_zh` / `weekday_kind_zh` / `format_day_of_week_hint` /
    `minutes_until_quiet_start` / `in_quiet_hours` + 18 测试）2026-05-03 完成
  - [x] QG5c1：rule-label 生成器 + 阈值 const + late-night-wellness 速率限制
    （`active_*_rule_labels` ×3 + `env_awareness_low` + `companionship_milestone` +
    `LATE_NIGHT_*` consts/static + `late_night_wellness_*` ×3 + `LONG_*` /
    `ENV_AWARENESS_*` consts）2026-05-03 完成
  - [x] QG5c2：prompt assembler（2026-05-03 完成 — `SILENT_MARKER` + `PromptInputs` +
    `proactive_rules` + `build_proactive_prompt` + `push_if_nonempty` +
    `format_proactive_mood_hint` + `format_plan_hint`。tests 暂留 prompt_tests 用
    `use super::*;` 通过 re-export 解析）
  - [x] QG5d：gate 子系统（`LoopAction` / `evaluate_pre_input_idle` /
    `evaluate_input_idle_gate` / `evaluate_loop_tick` / `wake_recent` /
    `WAKE_GRACE_WINDOW_SECS` + 全套 gate_tests）2026-05-03 完成。`in_quiet_hours`
    已在 QG5c-prep 时移到 time_helpers，gate 通过 `super::in_quiet_hours` 引用
  - [x] QG5e：telemetry 子系统（2026-05-03 完成 — `record_proactive_outcome` /
    `append_outcome_tag` / `chatty_mode_tag` + 5 个 `LAST_*` static stashes +
    `TurnRecord` / `ProactiveTurnMeta` + `PROACTIVE_TURN_HISTORY_CAP` + 4 个
    Tauri commands。`ProactiveTurnOutcome` 留 proactive.rs 作为 orchestrator
    return type，telemetry 通过 `super::ProactiveTurnOutcome` 引用）

- [x] Quality Gate 6：减少 panel 高频 IPC（2026-05-03 完成 — Iter QG6）

- [x] Tool Review 1：工具调用目的字段与展示（2026-05-03 完成 — Iter TR1。后端 gate +
  app.log 记录已实现；前端 ToolCallBlock 展示 purpose 留待 follow-up iter）

- [x] Tool Review 2：AI 工具调用风险审核（2026-05-03 完成 — Iter TR2。observe-only：
  分类 + 写 app.log，TR3 才会真正 block 高风险）

- [x] Tool Review 3：高风险工具调用的人类审核与 1 分钟超时（2026-05-03 完成 — Iter TR3）

## 下一迭代候选（优先级从高到低）
## 下一阶段（Iter 100 盘点后重排，2026-05-03 用户加入"宠物管家"方向）

**当前主轴是路线 F（宠物管家），其他路线退为辅助优先级。** 跨设备同步已被用户明确移除。

### 路线 G：companion register 细化（小迭代）
- [x] Iter Cμ：proactive prompt 时间行加 user_absence_tier 语气线索（2026-05-03 完成）
- [x] Iter Cν：long-absence-reunion 复合规则（≥4h 用户离开 + under_chatty + !pre_quiet
  → 触发"重逢感"提示）（2026-05-03 完成）
- [x] Iter Cξ：first-of-day 环境规则（today_speech_count == 0 → 用当下时段问候打底）
  （2026-05-03 完成）
- [x] Iter Cο：PanelPersona 加"当下心情"区（motion emoji + 文字 + 空状态）
  （2026-05-03 完成）
- [x] Iter Cρ：companionship-milestone 数据驱动规则（满 7/30/100/180 天/年/周年→
  engagement 类提示）（2026-05-03 完成）
- [x] Iter Cσ：reactive chat 的 user_profile 捕捉引导 — 闭合 Iter Cα 注入 ↔ 捕捉对称
  （2026-05-03 完成）
- [x] Iter Cτ：settings.user_name 字段 + persona_layer 称呼注入（reactive chat / Telegram）
  （2026-05-03 完成）
- [x] Iter Cυ：把 user_name 也注入 proactive prompt — 让 bubble 主动开口偶尔用名字称呼
  （2026-05-03 完成）
- [x] Iter Cφ：PanelPersona "自我画像" 空态加"立即生成画像"按钮 — 空态内嵌 consolidate
  trigger，新装用户一键 unlock（2026-05-03 完成）
- [x] Iter Cχ：butler_tasks 一键"清除失败标记" ✕ 按钮 — 跟 ❌ chip 紧贴，单击 strip
  `[error: ...]` 保留其余 description（2026-05-03 完成）
- [x] Iter Cψ：PanelStatsCard 加 "上次开口 N 前" 列 — 复用 ToneSnapshot
  since_last_proactive_minutes，五列横排（今日/本周/累计/上次/陪伴）（2026-05-03 完成）
- [x] Iter Cω：修复 LLM沉默 chip 颜色 bug（恒为紫，从未变橙）+ 加红色"失败 K" 子标签
  surface API 错误（2026-05-03 完成）

### 路线 D（series 2）：dashboard surface 与 prompt 对齐
- [x] Iter D1：ToneSnapshot 暴露 day_of_week / idle_register / idle_minutes，PanelToneStrip
  渲染 📆 / 👤 chip，让 strip 与 prompt 时间维度 1:1 对齐（2026-05-03 完成）
- [x] Iter D2：ToneSnapshot 暴露 companionship_milestone + PanelStatsCard 显 ✨ 节日 chip
  （和 Cρ rule 同源，里程碑日 user 可见）（2026-05-03 完成）
- [x] Iter D3：ToneSnapshot 暴露 focus_mode + PanelToneStrip 显 🎯 chip — proactive
  gate 路径和 panel 共享 focus_status() data source（2026-05-03 完成）
- [x] Iter D4：ToneSnapshot 暴露 in_quiet_hours + PanelToneStrip 显 😴 chip — 补 D3 之后
  自检发现的盲区（pre_quiet 只在前 15min，真 quiet 里 panel 之前完全空白）（2026-05-03 完成）
- [x] Iter D5：persona_summary 加 "X 天前更新" 新鲜度标签 + 7 天 stale 红 ⚠ 警告
  （2026-05-03 完成）
- [x] Iter D6：butler 执行后 prompt 教 LLM 在 bubble 里简短提一下「我帮你做了 X」
  + contract test 钉住 phrase（2026-05-03 完成）
- [x] Iter D7：consolidate 返回 LLM summary，panel banner 显示"做了什么"而不只是
  "跑了多久"（2026-05-03 完成）
- [x] Iter D8：PanelPersona 显示 settings.user_name 当前值（"🐾 宠物称呼你为「moon」"
  或空态提示路径），让 Cτ/Cυ 设的名字有 confirmation loop（2026-05-03 完成）
- [x] Iter D9：ToneSnapshot 暴露 cooldown_remaining_seconds + PanelToneStrip 显 ⏳
  冷却 Nm chip — gate 状态全可见（2026-05-03 完成）
- [x] Iter D10：ToneSnapshot 暴露 awaiting_user_reply + PanelToneStrip 显 💭 等回应
  chip — D series 第二个"为什么静默" gate（2026-05-03 完成）
- [x] Iter D11：awaiting gate 4h auto-expire + effective_awaiting pure helper —— 修复
  长别后宠物永久 muted 的潜伏 bug；4 单测覆盖（2026-05-03 完成）
- [x] Iter D12：proactive_enabled 暴露 + 🔕 chip — 7 个 gate 全部 panel 可见
  （2026-05-03 完成）
- D series 十二连完成；从黑盒打开成 11 个 chip 维度。

### 路线 E：研发 / 高级用户工具向
- [x] Iter E1：proactive prompt 全文 panel 可看 — LAST_PROACTIVE_PROMPT static Mutex
  + "看上次 prompt" 按钮 + modal 预览（2026-05-03 完成）
- [x] Iter E2：modal 同时显示 LLM reply + 每段独立复制按钮 — 全 in/out 可见
  （2026-05-03 完成）
- [x] Iter E3：modal 头部加 ⏱ timestamp + 🔧 tools_used 元数据 — 完整 chat round
  meta 一眼可见（2026-05-03 完成）
- [x] Iter E4：proactive turn ring buffer (cap 5) + panel modal « / » 导航 — 比较
  prompt 跨 run 变化（2026-05-03 完成）

### 路线 F：用户体验回归
- [x] Iter F1：桌面 bubble 60s 自动消失 — 修复 proactive 后 bubble 永久挂屏幕的 UX bug
  （2026-05-03 完成）

### 路线 F：宠物管家（用户委托执行实际工作）— Iter Cγ 起步
- [x] Iter Cγ：butler_tasks 记忆类别 + 提示注入 + tools 描述 + panel 顺序（2026-05-03 完成）
- [x] Iter Cδ：panel 顶部 "+ 委托任务" 快捷入口 + 模态分类 placeholder（2026-05-03 完成）
- [x] Iter Cε：butler_history.log + panel "最近执行" 时间线（2026-05-03 完成）
- [x] Iter Cζ：butler_tasks 调度前缀 `[every:]` / `[once:]` + 到期标注（2026-05-03 完成）
- [x] Iter Cη：每日 butler 小结写入 butler_daily.log + panel "每日小结" 区（2026-05-03 完成。
  注：用独立文件而非 speech_history，避免污染 chatty 计数）
- [x] Iter Cθ：panel butler_tasks 调度 chip + 实时 ⏰ 到期标记（2026-05-03 完成）
- [x] Iter Cι：reactive chat 的 butler 委托引导 — 扩展 TOOL_USAGE_PROMPT 让用户从聊天
  自然委托任务（2026-05-03 完成）
- [x] Iter Cκ：butler_tasks "等了 Nh" 过期指示 + 一键"立即处理"逃生口（2026-05-03 完成）
- [x] Iter Cλ：completed `[once]` butler_tasks 自动清理（48h grace）+ settings 配置项
  （2026-05-03 完成）
- [x] Iter Cπ：butler_tasks 执行失败回退 — `[error: 原因]` description 标记 + 红 chip
  （2026-05-03 完成）
- 路线 F 真闭环。后续看使用数据再加（任务依赖 / 监督模式 / batch 操作 etc）。

### 路线 A：长期人格演化（Iter 101-107 全部完成，路线 A 真正收官）

### 路线 R 后续候选（gap analysis 后写入，2026-05-03）
- [x] R10：tone strip 加反馈率 chip（2026-05-03 完成 — Iter R10）
- [x] R11：speech topic redundancy 检测器（2026-05-03 完成 — Iter R11）
- [x] R12：daily review 自动生成（2026-05-03 完成 — Iter R12 deterministic 版。22:00 后第一次
  proactive tick 写 ai_insights/daily_review_YYYY-MM-DD：今日计划 + 今日开口 bullet list。
  双重 idempotency：进程内 LAST_DAILY_REVIEW_DATE + 跨重启 index 存在性检查。11 单测）
- [x] R12b：daily review description 加入 plan progress 解析（2026-05-03 完成 — Iter R12b
  deterministic 版本。`[N/M]` 标记从 daily_plan 拉到 description："今天主动开口 7 次，计划 3/5"
  替代笼统的"有计划"。LLM 一句话总结另列为 R12c follow-up）
- [ ] R12c：LLM 一句话总结升级（路由 AppHandle + chat pipeline 进 daily_review，把
  description 改成"[review] 今天我们一起..."自然语言。需要把 maybe_run_daily_review
  从 clock-pure 升级到 app-aware）
- [x] R16：yesterday review description 注入 first-of-day prompt 作 "[昨日总览]" hint
  （2026-05-04 完成 — Iter R16。闭合 R12 review 的 write→read 循环。format_yesterday_recap_hint
  把 "[review] 今天主动开口 N 次，计划 X/Y" 改写成 "[昨日总览] 我们昨天主动开口 N 次，计划 X/Y。"
  与 R14 cross_day_hint 共同形成"高层总览 + 具体尾声"两层早起 callback。7 单测）
- [x] R13：companion mode setting（2026-05-03 完成 — Iter R13。3 模式 balanced/chatty/quiet
  调 cooldown + chatty_threshold；前端 settings UI 留 R13b follow-up）
- [x] R14：跨日记忆线（2026-05-03 完成 — Iter R14）
- [x] R15：active app 时长追踪（2026-05-03 完成 — Iter R15。proactive loop 每 tick 通过
  `current_active_window` 拉取前台 app，与上次快照比对计算停留分钟数；≥15 分钟才注入
  "用户在「X」里已经待了 N 分钟" 提示。粒度=interval_seconds，redaction 在 hint 格式
  化时应用，snapshot 留原文以稳定 transition 比较。7 单测）

### 路线 B：表情系统升级
- [ ] Iter 8b：扩展 mood 解析支持 expression 字段（如 [motion: Tap, expression: smile]），
  前端读到后切 Live2D expression 而不只是 motion group。

### 路线 C：隐私 filter
- [ ] Iter 12b：实机跑一次 proactive 看 LLM 是否守 `[motion: X]` 格式，配合 Iter 40 的
  Tag 统计观察实际命中率，决定是否要再加强 prompt。

### 路线 D：记忆 surface
- [x] Iter Dx：panel 加 Memory tab — 实际由 Iter Cε / Cη / Cθ / Cπ 等连续 iter 完成，
  PanelMemory.tsx 已 835 行（categories / butler / schedule / history 全在），
  专门 iter 不需要再做（2026-05-03 确认收官）

## 下一阶段：companion-grade 体验补全（2026-05-03 TR3 收口后新增）

QG1-6 + TR1-3 把质量基线和工具审计闭环了。现在的差距在"真实伙伴感"上 — 不是
更多功能，而是把现有信号闭回宠物的判断里。优先级从高到低：

- [x] Iter R1：用户反馈信号采集 + 注入 proactive prompt（2026-05-03 完成 — 实现
  ignored / replied 二分，dismiss <5s 留作 R1b 后续 — 需 ChatBubble UI 改动）
- [x] Iter R1b：ChatBubble 5 秒内点击 = active dismiss 信号（2026-05-04 完成 — Iter R1b。
  新 FeedbackKind::Dismissed + record_bubble_dismissed Tauri 命令；ratio 计算改名
  ignore_ratio → negative_signal_ratio 同时计 Ignored + Dismissed；前端 ChatBubble
  接 onClick + App.tsx 跟踪 bubbleShownAt 仅在 < 5s 内 fire。format_feedback_hint
  Dismissed 用更强语气）
- [x] Iter R1c：panel UI 区分 Dismissed vs Ignored（2026-05-04 完成 — Iter R1c。
  FeedbackSummary 加 dismissed: u64；PanelToneStrip chip 加 "👋N" 后缀和 hover
  "回复 X 被动忽略 Y 主动点掉 Z" 三段；PanelDebug timeline pill 红色"点掉" 区别
  灰色"忽略"，hover 解释信号强度差异。R1b 的强反馈信号现在 panel 全可见）
- [x] Iter R17：consolidate 阶段清理 30 天前的 daily_review 条目（2026-05-04 完成 — Iter R17。
  新 settings 字段 stale_daily_review_days（默认 30，0 = 关闭）；纯函数 parse_daily_review_date /
  is_stale_daily_review；consolidate.sweep_stale_daily_reviews 复用 reminder/plan/butler
  sweep 模式。protected items mood/plan/persona 不会被误删 — 只匹配 daily_review_YYYY-MM-DD
  title prefix。9 单测覆盖各种边界）
- [x] Iter R18：抽取 read_ai_insights_item 共享 helper（2026-05-04 完成 — Iter R18。
  从 proactive.rs / consolidate.rs 7 处重复 boilerplate 抽到 commands/memory.rs：
  `pub fn read_ai_insights_item(title: &str) -> Option<MemoryItem>`。复用消除 ~30 行
  重复 + 对齐 ai_insights 单条读取的语义模型。R16 IDEA 标的债务还清）
- [x] Iter R19：speech length register variance hint（2026-05-04 完成 — Iter R19。
  纯函数 format_speech_length_hint 扫最近 5 句平均字数：全 ≥25 → "偏长" + 提示更短，
  全 ≤8 → "偏短" + 提示多花两句，混合 → 静默。chars().count() 处理中文。8 单测）
- [x] Iter R20：speech register 在 PanelToneStrip 加 📏 chip（2026-05-04 完成 — Iter R20。
  抽 classify_speech_register 纯函数同时服 R19 prompt-hint + R20 panel chip。
  ToneSnapshot 加 speech_register: Option<SpeechRegisterSummary>。chip 显 长/短/混 + avg
  字数；长/短橙色（"卡 register"），混合绿色（"自然变化"）。4 单测）
- [x] Iter R21：repeated topic 在 PanelToneStrip 加 🔁 chip（2026-05-04 完成 — Iter R21。
  把 R11 的 detect_repeated_topic 信号 surface 到 panel：ToneSnapshot 加
  repeated_topic: Option<String>，redact 后传 panel；橙色 chip "🔁 {topic}"
  + hover 解释 4-char ngram + 跨 3 句阈值。复用 R20 的 5-line single fetch 给两个
  signal 共享。**继续践行"prompt 信号 = 同 iter 加 panel surface" 原则**）
- [x] Iter R22：active app 在 PanelToneStrip 加 🪟 chip（2026-05-04 完成 — Iter R22。
  R15 active app duration 信号 surface 到 panel：新 snapshot_active_app() read-only
  helper（不 mutate LAST_ACTIVE_APP since 时钟 — panel poll 安全）+ ActiveAppSummary。
  ToneSnapshot 加 active_app 字段。chip "🪟 {app}（{m}m）"，≥15m 橙色（R15 阈值
  fire），< 15m 灰色（observability only）。R20 codified 原则的第二次 audit-and-backfill）
- [x] Iter R23：cooldown chip 显 derivation breakdown + 修 D9 bug（2026-05-04 完成。
  D9 chip 之前用 base cooldown_seconds 计算 remaining，gate 用 effective — 不一致。
  R23 修这个 bug：CooldownBreakdown struct 含 configured / mode_factor / after_mode /
  feedback_band / feedback_factor / effective_seconds；新 classify_feedback_band 纯函数
  + 5 单测；hover 显 "1800s × 1.0 (balanced) × 2.0 (high_negative) = 3600s"）
- [x] Iter R24：ChatBubble 加 ✕ 角标提示可点（2026-05-04 完成 — Iter R24。R1b 的
  click-to-dismiss 之前完全无 affordance — 用户不知道 bubble 可点。✕ 半透明角标
  让 dismiss 行为可发现；click 通过 event bubbling 上传到父 onClick，所以点 ✕ 还是
  bubble 任意处效果一致。tooltip 解释 5s 内点 = 强反馈信号）
- [x] Iter R25：TurnRecord 加 outcome 字段（spoke / silent）+ panel modal 显示 badge
  （2026-05-04 完成 — Iter R25。E4 ring buffer 之前只存 prompt/reply/timestamp/tools，
  没有"这轮 LLM 是开口还是沉默" 的明确标签 —— silent 是 reply 为空隐式，user 在 modal 翻
  prev/next 看不出。R25 加 outcome: String 字段；构造点同 SILENT_MARKER 检测点判断；
  modal 显绿色"开口"/灰色"沉默" pill，hover 解释判定 logic）
- [x] Iter R26：feedback aggregate hint 注入 prompt（trend 信号补 R1 latest 信号）
  （2026-05-04 完成 — Iter R26。format_feedback_aggregate_hint 纯函数：≥5 条样本 fire，
  统计 replied/ignored/dismissed 三计数 —— "你最近 N 次主动开口里，X 回复 / Y 静默忽略 /
  Z 主动点掉。"。dismissed=0 时缩 segment。run_proactive_turn 把 recent_feedback(1) 升级
  到 (20)，feedback_hint + feedback_aggregate_hint 复用同一 fetch。5 单测）
- [x] Iter R27：active_app deep-focus directive + 3-band panel chip（2026-05-04 完成。
  ≥60min 同 app = 深度专注，prompt hint 升级到指令形式 "极简或选择沉默"；< 60min 保
  R15 描述形式。新 const DEEP_FOCUS_MINUTES = 60。panel chip 升 3 段色：< 15m 灰 /
  15-59m 橙 / ≥60m 红 + 🔒 锁标。4 新单测覆盖边界 + boundary）

- [x] Iter R2：TR3 review 结果写入 decision_log（2026-05-03 完成 — Iter R2）

- [x] Iter R3：late-night wellness nudge 复合规则（2026-05-03 完成 — Iter R3）

- [x] Iter R4：PanelDebug 显示 tool call purpose / risk / review status（2026-05-03 完成 — Iter R4）

- [x] Iter R5：SOUL.md hot reload（2026-05-03 完成 — Iter R5。审计后发现 proactive /
  telegram 已自动 hot-reload；本 iter 修补 reactive 会话 SOUL 烘焙的盲点）

- [x] Iter R8：late-night-wellness rate limit（2026-05-03 完成 — Iter R8）

- [x] Iter R6：feedback_history 在 panel timeline 可见（2026-05-03 完成 — Iter R6）

- [x] Iter R7：feedback signal 驱动 cooldown 调整（2026-05-03 完成 — Iter R7）

### 历史保留候选
- [x] Iter 74：speech_daily.json 扩展 panel stats 卡为"今日 / 本周 / 累计"三列（2026-05-03 完成）
