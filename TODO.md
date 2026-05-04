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
- [x] Iter R28：cooldown chip color-code by feedback band（2026-05-04 完成。R23 hover
  显 derivation 但 chip 始终 cyan — band 信息只在 hover 看见。R28 让 chip 颜色反映 band：
  high_negative 橙（pet 后退）/ low_negative 绿（用户活跃 / pet free）/ mid|insufficient
  保 cyan（中性）。bold 加重 non-neutral band 让"R7 adapter 在干预" 一眼可见）
- [x] Iter R29：companion_mode dropdown 进 PanelSettings（2026-05-04 完成 — R13b
  deferred 7 iter 后还债。AppSettings ProactiveConfig 加 companion_mode: string；
  PanelSettings 加 select 三选项 balanced/chatty/quiet + 文案解释 multipliers；hint
  文字解释 base=0 invariant + R7 在此模式之上叠加。用户终于不用手改 yaml 选模式了）
- [x] Iter R30：把还在 yaml-only 的 memory_consolidate 字段补 UI（2026-05-04 完成 —
  R29 codified rule 第二次 audit。stale_once_butler_hours (Cλ) + stale_daily_review_days
  (R17) 都欠 UI 债，R30 一次还完：MemoryConsolidateConfig TS 加 stale_daily_review_days；
  PanelSettings 新增第二行 twoColRow 显两个新字段；hint 文案展开解释 4 种 stale 各自含义）
- [x] Iter R31：proactive prompt size budget chip（2026-05-04 完成。LAST_PROACTIVE_PROMPT
  chars 计入 ToneSnapshot.last_prompt_chars；panel 加 📝 N字 chip with band colors（< 1500
  绿 / 1500-3000 灰 / ≥3000 橙）。R-series 累积 hint 让 prompt 越来越胖，这个 chip 是
  budget 自检 surface — 看到橙就知道该 audit 哪条 hint 该裁）
- [x] Iter R32：删除 dead-code SettingsPanel.tsx + DebugBar.tsx（2026-05-04 完成 — R29
  IDEA 标的 cleanup 债。SettingsPanel.tsx 377 行 legacy + DebugBar.tsx 94 行 self-described
  "remove when done"。两者全无 import 调用。删除 net -471 行，frontend 文件数 26 → 24。
  build/test/clippy/tsc 全 clean 验证 zero regression）
- [x] Iter R33：trailing-silent streak 检测 + prompt nudge 破沉默循环（2026-05-04 完成。
  纯函数 count_trailing_silent / format_consecutive_silent_hint 在 telemetry.rs；阈值 3
  连续沉默触发 hint "你已经连续 N 次选择沉默了..." 软提醒打破 perpetual silence 模式。
  PromptInputs.consecutive_silent_hint 新字段。9 单测覆盖 streak 计数 + threshold 边界）
- [x] Iter R34：silent streak panel chip（self-correct R33 IDEA 的"transient 不上 panel"判断）
  （2026-05-04 完成。重新审视后发现 streak 在 turn 之间 stable（≥ 5 min），panel 不会
  flicker。ToneSnapshot 加 consecutive_silent_streak; PanelToneStrip 🤐 chip ≥3 时显
  "🤐 沉默 ×N"，hover 解释 R33 阈值 + spoke 清零。复用同 count_trailing_silent helper —
  prompt + chip 不可能 drift）
- [x] Iter R35：trailing-negative streak（user 拒绝 mirror，2026-05-04 完成）。R33+R34
  做了 pet 沉默 streak。R35 是 user-side mirror — count_trailing_negative + 软 nudge
  "你最近连续 N 次开口都被忽略或主动点掉了..." prompt-side + 红色 🙉 拒绝 ×N panel chip
  ≥3 fire。dismissed 与 ignored 都计入 (R1c 同源思路)。8 单测 + chip。R26 是 20-window
  ratio，R35 是 trailing streak — 互补不重复）
- [x] Iter R36：retune R31 prompt size 📝 chip 阈值（2026-05-04 完成 — polish iter）。
  原 R31 阈值 1500/3000 calibrated 在 R31 时点；R32→R35 又加了 silent_hint /
  consecutive_negative_hint / feedback_aggregate / 等 hints 后 baseline 上移到 ~3000
  正常区。原阈值让常态显橙，warning 失去价值。R36 改 2000/4000：lean < 2000 / normal
  2000-3999 / heavy ≥4000，留 hint-all-fire 的常态在 normal 区，仅异常胖时 warn）
- [x] Iter R37：feedback timeline filter buttons（2026-05-04 完成 — polish iter）。
  PanelDebug R6 反馈 timeline 之前混合显示 replied/ignored/dismissed 三种 entry。
  R37 加 4 按钮"全部 / 回复 / 忽略 / 点掉" 各带 count，点击 isolate 单一 kind retrospect。
  按钮 active state 用对应 kind 的色彩（绿/灰/红），matching pill colors。空过滤显
  "当前过滤下没有匹配条目" 兜底。R-series first 真正交互式 panel 控件，前面都是 chip 静态展示）
- [x] Iter R38：decision_log timeline 同样加 filter 按钮（2026-05-04 完成 — R37 pattern
  reuse）。4 按钮 全部 / 开口 (Spoke) / 沉默 (LlmSilent) / 跳过 (Skip)，复用 R37
  same btnStyle / 空过滤 / count-in-label pattern。9 种 kinds 中只 surface 4 个高频，
  其他 (Run/Silent/LlmError/ToolReview*) 走"全部"。R-series 首次复用 codified pattern
  到第二个 timeline，验证 R37 IDEA 提的"pattern reusable" claim）
- [x] Iter R39：抽 PanelFilterButtonRow 共享组件 + 应用到 tool_call 第三个 timeline
  （2026-05-04 完成。R38 IDEA codify "use-3+ 触发抽组件"，R39 第三 use case 立刻还。
  新 src/components/common/PanelFilterButtonRow.tsx 通用组件 generic-on-V<extends string>；
  R37 feedback + R38 decision 都重构成调用此 component；新加 R39 tool_call risk filter
  全部/低/中/高 4 按钮，色彩同 riskBadgeBg。-30 行 net 重复 + future filter 复用直接调）
- [x] Iter R40：ChatBubble 加入 fade-in 动画（2026-05-04 完成 — UX polish）。之前 bubble
  通过 React 条件渲染瞬间 pop in，感觉机械。R40 加 220ms CSS keyframes：opacity 0→1 +
  translateY(4px)→0 的"轻轻沉下来"感觉，让宠物开口更"有生命"。inline <style> 标签嵌
  组件，动画作用域不污染全局 CSS）
- [x] Iter R41：ChatBubble 点击 press feedback（2026-05-04 完成 — UX polish 续）。R40
  加 fadeIn 入场动画，R41 补 click 触觉反馈。`.pet-bubble:active` CSS pseudo-class +
  transition transform 80ms 让点击瞬间有 scale(0.97) 缩压感。dismiss 前的 80ms 触觉
  让用户知道"click 已被收到"，不再"点了瞬间 bubble 消失" 困惑）
- [x] Iter R42：ChatBubble hover lift 完成 interaction 三态（2026-05-04 完成 — bubble
  polish 第三 iter，完结 R40+R41+R42 cluster）。`.pet-bubble:hover` 加 border-color
  深化 #7dd3fc + translateY(-1px) 轻微上抬。transition 同步加 border-color 120ms。
  完整 interaction state machine：mount fadeIn / hover lift / active press。:active
  CSS 顺序后定义，press transform 自然覆盖 hover lift）
- [x] Iter R43：Tab indicator slide-in + hover widen（2026-05-04 完成 — 新 polish cluster
  起点）。auto-hide tab 之前出现是瞬间 pop。R43 加 280ms slide-in keyframe（left -16→0 +
  opacity 0→1）+ hover width 16→22px transition 120ms。复用 R40-R42 ChatBubble 的 inline
  `<style>` + className pattern。是 polish 第二个 component cluster 的第一 iter）
- [x] Iter R44：Tab arrow ambient bob（2026-05-04 完成 — tab cluster 第二 iter）。tab 内
  ▶ 箭头加 1.6s 无限 loop translateX bob (0 ↔ -2px) ease-in-out。subtle 朝左方向反复
  invite "你点这边召回 pet"。hover 时 animation-play-state: paused 让 hover state 占主
  affordance，停止 ambient 加干扰。R-series 第一次用 infinite ambient animation）
- [x] Iter R45：Tab 显示 unread badge 当 pet hidden 期间主动开口（2026-05-04 完成 — tab
  cluster 第三 iter，新功能不只装饰）。useEffect 监听 proactive-message 事件，hidden=true
  时计数+1，hidden 翻 false 时清零。Tab 右上角 -4/-4 偏移红色圆角 badge，9+ 截断显示
  "9+"。hover title 解释"pet 隐藏期间主动开口 N 次，mouse-enter 让 pet 回来后会自动消失"。
  R-series 首次"在 polish iter 里加新功能"）
- [x] Iter R46：ChatPanel ⚙ CSS hover + textarea 加 focus 环（2026-05-04 完成 — ChatPanel
  cluster 起点）。⚙ 按钮去掉 onMouseEnter/Leave JS handlers，改用 :hover CSS（R41 codified
  pattern）。textarea 之前 `outline: none` 后无 focus 视觉，是 accessibility 漏洞 ——
  R46 加 .pet-chat-input:focus { border-color: #38bdf8; box-shadow: 0 0 0 2px rgba(... 0.18) } 
  让聚焦时有专业焦点环。inline `<style>` 复用 R40-R44 pattern）
- [x] Iter R47：focus ring audit 应用到 PanelSettings + PanelChat（2026-05-04 完成 —
  R46 IDEA 提的 audit pass）。grep 'outline: none' 找到另两处 accessibility 漏洞。
  用 descendant selector 模式（`.pet-settings-root input:focus, ...textarea:focus,
  ...select:focus`）— 一处 inline `<style>` 覆盖 component 内所有 input，不需要给每个
  input 加 className。同样模式应用到 PanelChat。三处 input 现在都有 focus ring）
- [x] Iter R48：ChatPanel isLoading 三 dots pulsing 指示器（2026-05-04 完成 — ChatPanel
  cluster 第三 iter，完成 cluster）。之前 isLoading=true 时 textarea 视觉无变化，user 不
  知 AI 在思考。R48 加 .pet-loading-dot 三圆点 staggered animation（0 / 0.18s / 0.36s
  delay）+ pet-loading-dot-pulse 1.2s ease-in-out 上升 2px + opacity 0.25↔1。industry
  standard "thinking" 视觉，在 textarea 和 ⚙ 之间）
- [x] Iter R49：Live2D loading status 改友好文案 + fade-in（2026-05-04 完成 — Live2D
  cluster 起点）。原状态 "importing pixi.js" / "checking cubism core" 等 dev 文案直接
  显给 user，他们不知道什么意思。R49 加 derived displayStatus：非 Error 时统一显
  "正在唤醒…"，Error 保留原始 detail。状态 div 加 240ms fadeIn keyframe（opacity 0→1
  + translateY 4px→0）让 loading 出现柔和。setStatus 内部值不变，dev 仍可 inspect）
- [x] Iter R50：PanelStatsCard 加 avg-per-day 派生统计（2026-05-04 完成）。stats card
  之前缺"平均每日主动开口次数"——揭示长期 engagement 强度。R50 加新 column：
  lifetime / max(1, companionshipDays)，< 10 显小数 1 位，≥ 10 取整数。companionshipDays=0
  时隐藏（避免 1 天分母无意义）。色用 teal (#0d9488) 跟陪伴天数同色 family 暗示"陪伴
  derived 信号"，区别于其他柱状统计的紫色）
- [x] Iter R51：PanelStatsCard 加 /周日均 trend 列（2026-05-04 完成 — R50 续）。R50 是
  lifetime 平均（长期性格），R51 是 7-day rolling 平均（最近趋势）。week / min(7, days+1)
  让首周 user 分母合理。hover title 自动判断 weekAvg 跟 lifetimeAvg 比例 ±30%，显示
  "(最近比长期均值更健谈/更安静)" 文案。两列并排让用户一眼看 trend：lifetime 2 /
  week 5 = 趋势上升）
- [x] Iter R52：transient mute 按钮 + chip + gate（2026-05-04 完成 — 真实功能 iter）。
  之前 user 想"静一下" 只能去 settings 改 enabled flag。R52 新 MUTE_UNTIL static
  + mute_remaining_seconds 纯 helper + gate 第一道检查跳过被 mute 的 tick。新两
  Tauri command set_mute_minutes / get_mute_until。ChatPanel 加红色 🔇 toggle
  按钮（无→30min→clear 循环）。PanelToneStrip 加紫色"🔇 静音 Nm" chip。reactive
  chat 不受影响 —— 只跳 proactive。focus session friendly）
- [x] Iter R53：抽 compute_mute_remaining 纯函数 + 5 单测（2026-05-04 完成 — R52 配套
  test debt 还）。R52 ship 时 mute_remaining_seconds 依赖 chrono::Local::now() 全局
  时钟，不可测。R53 抽 `compute_mute_remaining(until, now) -> Option<i64>` 纯函数，
  原 wrapper 一行调用。5 单测覆盖：none / past / now boundary / future / 1s before
  expiry。500 tests milestone）
- [x] Iter R54：mute 按钮加右键 preset 菜单（2026-05-04 完成 — R52 follow-up）。
  左键保留 R52 的 30min toggle 快速路径；右键打开浮层菜单 15/30/60/120 min/解除静音
  五选项。stopPropagation + window click outside-close handler 防止 menu 一开就关。
  hover state 用 CSS .pet-mute-menu-item:hover（R41 codified pattern），不再用
  React onMouseEnter handlers。fast path + flexible path 双轨）
- [x] Iter R55：transient instruction note 完整 stack 功能（2026-05-04 完成 —— R52 之
  后第二个真实 feature iter）。用户可留一段 context 文本（如"开会到 14:00"）+ 时长，
  pet 下次 proactive 看到 "[临时指示]" header + 文本 + "尊重 / 配合，不追问" 指令。
  与 mute 区别：mute 阻塞 / note 不阻塞但加 context。Backend：TransientNote struct +
  compute_transient_note_active 纯函数（5 单测）+ 3 Tauri 命令。Frontend：📝 button
  popover with textarea + 4 preset durations (30/60/120/240) + 解除按钮 + cyan 状态色
  + "📝 {text}" panel chip。复用 R52/R54 popover idiom。505 tests）
- [x] Iter R56：transient note remaining 时长 surface（2026-05-04 完成 — R55 follow-up）。
  R55 chip 之前只显文本，user 不知"还有多久到期"。R56 加 compute_transient_note_remaining
  纯函数 + 4 单测，对称 R52 compute_mute_remaining。ToneSnapshot 加 transient_note_
  remaining_seconds 字段。chip 改"📝 {text} · 剩 Nm"，hover title 同步显精确分钟数。
  509 tests）
- [x] Iter R57：note popover 打开时 refresh state（2026-05-04 完成 — R55 stale-state bug fix）。
  R55 popover open 不 fetch backend，note 自动到期后仍显 stale text + noteActive=true。
  R57 加 handleNoteToggle async — open 时 invoke get_transient_note。preserve draft：if
  backend 有 note → load 进 textarea + 标 active；if 无 → 仅标 inactive，**不清空 noteText**
  保用户未保存的 draft）
- [x] Iter R58：mute 按钮也加 refresh-on-click（2026-05-04 完成 — R57 IDEA codified rule
  audit）。R52 mute 同 R55 latent bug：mute 自动到期后 frontend `muted=true` 仍 stale，按钮
  仍红色。新 refreshMuteState() async helper，左键 click + 右键 menu open 都先 fetch
  fresh state。Returns Promise<boolean> 让 caller 拿到 truth 不依赖 React state 异步。
  R57 codified pattern 第二次 audit-and-backfill）
- [x] Iter R59：抽 R52/R55 setter 纯函数 + 9 单测（2026-05-04 完成 — R53/R56 pattern 续）。
  set_mute_minutes / set_transient_note 之前 logic 跟 mutex IO 耦合，仅 boundary 行为
  tested through helper 的"读" 函数（R53/R56），"写" 函数本身没测。R59 抽
  compute_new_mute_until + compute_new_transient_note 两 pure 函数，Tauri 命令变 thin
  wrapper。9 单测覆盖 0/负数 / whitespace / trim / valid 多 case。518 tests）
- [x] Iter R60：format_feedback_hint excerpt 加 redaction（2026-05-04 完成 — privacy audit
  catch）。R1 format_feedback_hint 把 latest excerpt 直接 inject 进 prompt 没 redact。
  pet 自己的 reply 可能含用户 redaction 模式的私人词（LLM 可能 weave 进 reply text）。
  R60 改 signature 加 redact closure（同 format_reminders_hint 模式），applied 在
  excerpt 注入前。新 test 验证 closure actually called。519 tests）
- [x] Iter R61：tool outputs 切到 redact_with_settings（2026-05-04 完成 — R60 audit 续）。
  audit 发现 system_tools.rs (Cx) 和 calendar_tool.rs (Cx) 两处用 `redact_text` 仅 substring，
  bypass regex patterns。switch 到 `redact_with_settings` 让 substring + regex 都 apply。
  减少 set_pattern 的手动 fetch，简化 callsite。privacy boundary 完整性提升）
- [x] Iter R62：deep-focus 90min+ 硬阻塞 gate（2026-05-04 完成 — R15→R27→R62 escalation
  第二台阶）。新 const HARD_FOCUS_BLOCK_MINUTES = 90 + 纯函数 compute_deep_focus_block +
  生产 wrapper deep_focus_block_minutes + refresh_active_app_snapshot helper。
  evaluate_loop_tick 加新 gate（mute 后 pre_input_idle 前）：refresh snapshot →
  if hard-block → Skip。PanelToneStrip 4 段色 < 15 / 15-59 / 60-89 / ≥90 + 🔒🛑
  hard-block 视觉。R27 60m soft directive 留 30min 自觉机会；90m 仍未切 app 才硬阻塞。
  8 单测；527 tests pass; clippy/fmt/tsc 全 clean）
- [x] Iter R63：deep-focus recovery hint（2026-05-04 完成 — R62 配对补全）。R62 让 gate
  skip 但 skip 不留 trace，用户真切出来时 pet 像"什么都没发生"一样开口。R63 加 LastHardBlock
  static + record_hard_block writer + compute_recovery_hint / format_deep_focus_recovery_hint
  纯函数 + take_recovery_hint take-on-use wrapper（10 min grace）。PromptInputs 新字段
  deep_focus_recovery_hint 注入 prompt：第一句变成"你刚从「X」的 N 分钟专注里切出来..."
  attentive 反馈。10 单测；537 tests pass）
- [x] Iter R64：companion_mode-aware hard-block threshold（2026-05-04 完成 — R62 magic
  number 接进 mode 系统）。新 apply_companion_mode_hard_block 纯函数：chatty=135 /
  balanced=90 / quiet=60。ProactiveConfig::effective_hard_block_minutes(base) 方法。
  gate 用此值替代 const-hardcoded wrapper。删除 deep_focus_block_minutes 单 caller wrapper
  作为 codify。ToneSnapshot.effective_hard_block_minutes 新字段让 PanelToneStrip chip
  阈值跟 gate 同步（chatty 90-134min 不再误显 deep-red）。5 新单测；542 tests pass）
- [x] Iter R65：今日深度专注 stretch 累计 + PanelStatsCard 显示（2026-05-04 完成 —
  R62/R63/R64 cluster 第 4 iter，hard-block 事件沉淀成 stat）。新 DailyBlockStats struct +
  DAILY_BLOCK_STATS static + compute_finalize_stats 纯函数 + finalize_stretch wrapper +
  current_daily_block_stats reader。record_hard_block 加 transition-finalize（prev > 120s
  视为 stretch 中断）；take_recovery_hint 加 clean-end finalize；两路径互斥。
  PanelStatsCard "🛑 N 次/Xm" 列，count > 0 才显（empty state 不展示）。5 新单测；547 tests pass）
- [x] Iter R66：deep-focus history vec + 昨日深度专注 first-of-day recap hint（2026-05-04
  完成 — R62~R65 cluster 第 5 iter）。R65 single-Option 升级 Vec<DailyBlockStats> + cap=7。
  删 compute_finalize_stats 替换为 compute_history_after_finalize 一站式（increment/append/
  sort/cap）。新 yesterday_block_stats reader + format_yesterday_focus_recap_hint 纯函数。
  PromptInputs.yesterday_focus_hint 字段 + assembler push；run_proactive_turn 在
  today_speech_count == 0 时注入。三 hint 互补：cross_day(continuity) + yesterday_recap
  (review summary) + yesterday_focus(activity intensity)。9 新单测；552 tests pass）
- [x] Iter R67：deep-focus history 持久化到磁盘（2026-05-04 完成 — R66 IDEA 标的"R67+ 候选"
  立刻还）。新 block_history_path / save_block_history / load_block_history /
  load_block_history_into_memory 四 fn。finalize_stretch 写后调 save。lib.rs 在
  proactive::spawn 前调 load。DailyBlockStats 加 Deserialize derive。新 TEST_LOCK 串行化
  mutate-static 测试。错误三层 fallback（path None / file missing / parse fail 均 empty
  Vec）。4 新单测；556 tests pass）
- [x] Iter R68：本周 deep-focus 聚合 + PanelStatsCard 新列（2026-05-04 完成 — R66 cap=7
  future-proof 兑现）。新 WeeklyBlockSummary struct + compute_weekly_block_summary 纯函数
  （filter today-6..=today 双闭区间，saturating sum）+ current_weekly_block_summary wrapper。
  ToneSnapshot.weekly_block_stats 字段；PanelStatsCard 新列 "🛑 N 本周/Xm/Y天" #9f1239 浅红
  与今日列 #7f1d1d 深红区分。total_count > 0 才显，沿 R65 stat-as-confirmation UX。
  6 新单测；562 tests pass）
- [x] Iter R69：deep-focus week-over-week trend 指示（2026-05-04 完成 — cluster 第 8 iter
  收尾）。cap 7→14 让 prior week 数据可用。新 WeekOverWeekTrend struct + compute_week_over_week_trend
  纯函数（filter 两 7-day 窗口 → saturating sum → i128 % delta → ±15% threshold direction +
  ±999 clamp）。ToneSnapshot.week_trend 字段；PanelStatsCard 加 inline ↑/=/↓ icon (up=green
  肯定，down/flat=gray 中性)。tooltip 拼基础 + trend math。7 新单测；569 tests pass）
- [x] Iter R70：reactive chat 注入今日/本周 deep-focus 上下文（2026-05-04 完成 —
  R69 cluster 闭合后首个 cross-domain iter）。新 format_focus_context_layer 纯函数 +
  inject_focus_context_layer wrapper 沿 R9 recent_speech layer idiom。chat() pipeline
  加 layer 在 recent_speech 之后。AI 回答"今天怎么样"准确；mid-focus chat 自动简短。
  layer 链 4 层：mood / persona / recent_speech / focus_context。6 新单测；575 tests pass）
- [x] Iter R71：focus context 加"正在专注"信号 + telegram parity（2026-05-04 完成 —
  R70 follow-up）。format_focus_context_layer 第三参数 in_progress: Option<(&str, u64)>，
  新加 "用户当前正在「X」专注 N 分钟（进行中）"line 排首。IN_PROGRESS_FOCUS_MIN_MINUTES=30
  阈值 gate 在 wrapper。telegram bot 也 inject_focus_context_layer 拿到 stat 上下文。
  recent_speech 故意 NOT inject telegram (bubbles 是 desktop-only)。3 新单测；578 tests pass）
- [x] Iter R72：今日单次最长 deep-focus 跟踪 + panel surface（2026-05-04 完成 — depth
  维度补齐）。DailyBlockStats 加 max_single_stretch_minutes 字段 + `#[serde(default)]`
  schema 演进。compute_history_after_finalize 用 max() 维护，第一次 finalize 用 peak_minutes
  初值。三维度 count(频度)/total(量)/peak(深度)各不替代。panel "次/Xm/峰 Ym" 多 stretch
  时才显（单 stretch 冗余不显）。20+ 测试 fixtures sed 自动补；4 新单测；582 tests pass）
- [x] Iter R73：weekly summary 加 peak_single_stretch 维度 + panel tooltip（2026-05-04
  完成 — R72 IDEA 标的 candidate 立刻兑现）。WeeklyBlockSummary 加 peak_single_stretch_minutes
  字段；compute_weekly_block_summary 跨 7-day 窗口 .max().unwrap_or(0)。stat granularity
  hierarchy 完整（count/total/peak × day/week）。panel weekly tooltip 拼接"本周最长一次 X
  分钟"，不加新 chip（信息密度够用 tooltip 足）。3 新单测；585 tests pass）
- [x] Iter R74：personal-record prompt nudge "[今日破纪录]"（2026-05-04 完成 —
  R72/R73 data → surface 后第三阶 narrate）。新 compute_personal_record_hint 纯函数（strict
  > only，避免 tied / no-baseline 误 fire）+ current_personal_record_hint wrapper。
  PromptInputs.personal_record_hint 字段；run_proactive_turn 每次 turn 调（非 first-of-day
  gated，record 应当 fire 立即）。prompt 文案"不必每次都提"留 LLM context-aware judgment。
  6 新单测；591 tests pass）
- [x] Iter R75：focus_context layer 也注入 record（chat + telegram parity）（2026-05-04
  完成 — R74 cross-modality）。format_focus_context_layer 第 4 参数 record:
  Option<(u64, u64)>，内部 strict-> 同 R74 logic。inject wrapper 读 DAILY_BLOCK_HISTORY
  计算 today_peak + prior_week_peak。R71 共享 helper 让 telegram 自动同步。
  9 处已有测试加 None 第 4 arg；4 新测试 strict_higher / tied / no_baseline / record_alone。
  595 tests pass）
- [x] Iter R76：PanelStatsCard ⭐ 破纪录视觉指示（2026-05-04 完成 — record cluster 三
  surface 全部对齐）。ToneSnapshot.is_personal_record_today 字段复用 R74 helper
  的 emptiness 判定。Panel daily chip 后追加 ⭐ + tooltip "破纪录" 详解。R62-R76
  deep-focus cluster 第一阶段收官（14 iter）。无新单测——纯 wiring；595 tests pass）
- [x] Iter R77：butler_tasks `[deadline:]` 前缀 + 紧迫度分级 prompt nudge（2026-05-04
  完成 — R76 后 first cross-domain iter，user → pet 委托方向）。新
  parse_butler_deadline_prefix + DeadlineUrgency enum (Distant/Approaching/Imminent/Overdue)
  + compute_deadline_urgency 4 段分级（>6h/1-6h/<1h/过期）+ format_butler_deadlines_hint
  纯函数（filter Distant）+ build_butler_deadlines_hint IO wrapper + PromptInputs.deadline_hint。
  butler_tasks 三语义正交：every/once/deadline。urgency-aware 打扰策略：deadline
  imminent/overdue 才 override deep-focus 静默。11 新单测；606 tests pass）
- [x] Iter R78：教 LLM 用 `[deadline:]` + PanelToneStrip ⏳ deadline chip（2026-05-04 完成 —
  R77 闭合 cluster）。TOOL_USAGE_PROMPT 加教学段（pin test 锁）+ count_urgent_butler_deadlines
  纯函数（仅 Imminent + Overdue 计数）+ ToneSnapshot.urgent_deadline_count 字段 + ⏳ chip
  红色 #b91c1c。R77 + R78 闭合：data + prompt + 教学 + panel surface 全到位。3 新单测；
  609 tests pass）
- [x] Iter R79：deadline 信息 cross-modality 到 reactive chat + telegram（2026-05-04 完成 —
  deadline cluster cross-surface 完整）。新 format_deadline_chat_layer 纯函数（chat-specific
  framing："user 可能问起" vs proactive "你 might bring up"）+ inject_deadline_context_layer
  wrapper。chat() + telegram bot 都 inject。三 surface 三 fidelity：chat 含 Approaching，
  proactive 含 Approaching，chip 仅 imminent+overdue。3 新单测；612 tests pass）
- [x] Iter R80：PanelMemory butler_tasks 加 `[deadline:]` chip + placeholder 教学（2026-05-04
  完成 — deadline cluster 8 surface 全闭合）。parseButlerSchedule TS 扩展第三 kind +
  computeDeadlineUrgency TS 镜像 Rust。chip 4-way styling：every 蓝循环/once amber/deadline
  按 urgency 4 段。placeholder 加 deadline 例句教 user。R77-R80 deadline cluster 完整。
  无新单测——纯 frontend，612 tests pass）
- [x] Iter R81：deadline 紧迫度驱动 cooldown 缩半（2026-05-04 完成 — R77-R80 cluster 真正 closure）。
  之前 deadline chip / prompt 都告诉用户"逼近"了，但 cooldown gate 不 react，pet 仍按之前 cadence
  说话。R81 加 `deadline_urgency_factor`：urgent_count >= 1 时 effective_cooldown × 0.5。
  CooldownBreakdown 加 urgent_deadline_count + deadline_factor 字段；evaluate_loop_tick 读 butler_tasks
  + count_urgent_butler_deadlines + multiply；build_tone_snapshot 把 urgent_count lift 到 shared scope；
  PanelToneStrip hover "× 0.5 (deadline 紧迫 N)" 段。3 新单测；615 tests pass）

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
