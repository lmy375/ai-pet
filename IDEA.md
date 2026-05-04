# IDEA — 实时陪伴型 AI 桌面宠物的设计思考

## Iter R80 设计要点（已实现）
- **PanelMemory butler_tasks 列表是 user-author 主入口**：user 手动加 / 改 butler_tasks 主要在 PanelMemory 列表里。R77/R78 让 prompt + chip + LLM 教学全 cover 了，但用户翻 PanelMemory 添加任务时 placeholder 还没教 `[deadline:]`，列表里 deadline-prefixed item 也没特殊视觉。**user-author 入口不教 = 死循环**——LLM 自己创建 deadline 但 user 看不到示范。
- **解析器扩展 vs 新写一个**：parseButlerSchedule 已经处理 every/once。R80 选择扩展同函数加 deadline 第三 kind，因为 once + deadline 共享 YYYY-MM-DD HH:MM body shape，正则一处复用 `(every|once|deadline)`。**新功能跟旧 share grammar 时 extend 而非 fork**——避免重复 parse 逻辑。
- **TS 镜像 Rust 4 段 urgency**：computeDeadlineUrgency 完全镜像 R77 compute_deadline_urgency 的 `≥6h distant / ≥1h approaching / >0 imminent / <=0 overdue` tier。**前后端 urgency 阈值同步**——避免 panel 显 Approaching 但 prompt 已 Imminent 之类不一致。
- **chip 4-way styling**：every (#dbeafe 蓝循环) / once (#fef3c7 amber 一次性) / deadline 按 urgency 4 段（distant 灰, approaching amber, imminent 红, overdue 深红）。**视觉密度反映 urgency 密度**——同 chip family 但深度递进。
- **deadline 不参与 isButlerDue check**：每 / 一次性 schedule 的 due 语义是 "pet 应该执行了吗"——deadline 不是 pet 执行，所以 due check skip。`due = parsed.kind !== "deadline" && isButlerDue(...)`。**语义过滤在使用点而非 helper 内部**——清晰分离 "schedule" 与 "deadline" 概念。
- **placeholder 教 user 用 [deadline:]**：R78 教了 LLM；R80 教 user。两层教学不冗余——user 直接编辑 / LLM auto-create 是两路径。**input affordance 跟 LLM prompt 平行覆盖**。
- **R80 完整闭合 deadline cluster**：data (R77) + prompt nudge (R77) + LLM 教学 (R78) + panel chip (R78) + chat layer (R79) + telegram (R79) + PanelMemory chip + placeholder (R80)。**4 iter 8 surface**——每个 surface 都让 user 在不同 affordance 看到 deadline 概念。
- **下一 iter 该换方向**：deadline cluster 完整，跟之前 deep-focus cluster 收官时同样的 closure 信号——下一 surface 不在同一概念域。

## Iter R79 设计要点（已实现）
- **chat layer 跟 proactive prompt 文案 framing 不同**：proactive 是 "你 might bring it up" (pet initiates)，chat 是 "user 可能问起，这是 ground truth" (pet responds)。R79 写独立 format_deadline_chat_layer 而非 reuse format_butler_deadlines_hint。**modality 决定 framing**——shared data, separate prose。
- **chat 层包含 Approaching 而 proactive 不也包**：proactive R77 format 已含 Approaching（"约 N 小时后"）。chat R79 跟它保持一致——user 可能问起 4 小时后的 deadline。但 panel chip R78 仅 imminent + overdue（"act now" 焦点）。**三个 surface 三个 fidelity 阈值**：chat = "可问"（包含 Approaching），proactive = "可提"（包含 Approaching），chip = "必须看"（仅 imminent + overdue）。
- **R77 → R78 → R79 deadline cluster 三 surface**：proactive prompt (R77) → LLM 教学 + panel chip (R78) → chat layer (R79)。**单 cluster 三 iter 完成 cross-surface coverage**——之前 deep-focus cluster 用了 14 iter 才到这个状态，deadline cluster 紧凑得多。**学习效率：第二个同类 cluster 比第一个快**。
- **redact_with_settings 在 wrapper 内**：format_deadline_chat_layer 是 pure helper（不读 settings），inject_deadline_context_layer wrapper 在 inject 前 redact 整段。**沿 R20 codified pattern**：pure 不知 settings，wrapper 知。
- **layer 链长到 5**：mood / persona / recent_speech / focus_context / deadline_context。每层独立可 toggle，互不重叠。**"添加新 layer" 是高 leverage 操作**——一次实现 chat + telegram 都得到。
- **不全 reuse R77 build_butler_deadlines_hint**：考虑过让 chat 直接 inject 同一字符串，但 framing 不同（proactive 用 "如果用户当前没在专注" 暗示 pet 主导，chat 应让 user 主导）。**copy-paste-with-edit 比 reuse-with-flag 更直白**——加 flag 的 helper 更难读。
- **R79 也可作"R77 reuse 没问题就 ship 简单版"反例**：理论上 R79 用 build_butler_deadlines_hint 一行也能 work，少 30 行新 helper。但 framing 不一致会让 LLM 偶尔在 reactive chat 主动 brag deadline——bad UX。**framing 是 fidelity 决定，不是 LOC 优化**。
- **测试覆盖 chat-specific 的 distinguishing tail**：测试 assert "不必主动列举" 这句 chat 特有 tail。**测试不只验数据正确，也锁 framing 正确**。

## Iter R78 设计要点（已实现）
- **教 LLM 用 [deadline:] 是 R77 的 unblocking 必要条件**：R77 加了 parser + classifier + prompt nudge，但没 LLM 教学，自然对话里"周五前发出去"的 phrasing 不会创建 deadline-prefixed butler task。**R77 + R78 是必然 paired** —— 数据结构 + 教学语料缺一不可。
- **TOOL_USAGE_PROMPT 是 leverage 最高的修改点**：每次 reactive chat / telegram 都会注入这段。一处文字改动，所有路径自动 LLM 学到。**修改 prompt 比修改代码 reach 更广**——好的 prompt 比代码更易演化。
- **Pin tests 强制保留教学**：assert TOOL_USAGE_PROMPT.contains("[deadline:") + 同对比文字。**Pin test 是 prompt 演化的版本控制**——避免未来重构 prompt 时无意删 critical 段。
- **chip 不重复 prompt 信号**：⏳ deadline chip 只显 urgent_deadline_count（imminent + overdue 数）。Approaching 不显 chip 因为 prompt 已 inject "[逼近的 deadline]" 段，user 看到 pet 自己提就够了。**多 surface 同信号不必 redundant**——chip 是最 imminent 的 standalone 信号。
- **count helper 跟 format helper 拆开**：format_butler_deadlines_hint 给 prompt 用（详细文本 + filter Distant），count_urgent_butler_deadlines 给 panel chip 用（数字 + 严格 imminent/overdue only）。**两个 surface 不同 fidelity 应有不同 helper**——共用一个会导致一方过滤太多另一方不够。
- **chip 颜色 #b91c1c 红色**：跟 deep-focus deep-red (#7f1d1d) 区分但同 family。red = "需要注意"，多 chip 同色家族但深度 distinguishable。
- **build_tone_snapshot inline IO 是 acceptable**：考虑过抽 wrapper，但 build_tone_snapshot 已经 IO-heavy（多次 memory_list / disk read），inline `memory::memory_list("butler_tasks")` 不是新概念。**抽函数的标准是"复用 ≥ 2"**——R78 panel only 一处用，不抽。R74 / R77 各自抽 wrapper 因为 prompt + chat layer 两处用。
- **R78 是 R77 的 surface 闭合 iter**：R77 数据 → prompt; R78 data → panel + LLM 教学。**单 cluster 内 data → prompt → panel + teaching 三阶段**——跟 deep-focus cluster 的 data → surface → narrate 节奏对称但更紧凑（2 iter 而非 3+ iter）。

## Iter R77 设计要点（已实现）
- **R76 IDEA 标"换方向"立刻执行**：deep-focus cluster 闭合后第一个 fresh-direction iter。选 butler_tasks deadline 因为 (a) 直接 user-action 相关（不是 "pet 注意 user 的状态" 而是 "user 委托给 pet 的事"）(b) 用现有 butler memory 基础设施 (c) cluster 关联度低，避免新 cluster 立刻 over-investing。
- **`[deadline:]` 跟 `[once:]` 语义区分**：`[once: 14:00]` = "pet 在 14:00 自动执行此任务"，`[deadline: 14:00]` = "user 必须在 14:00 之前完成"。**前者 pet 是 actor, 后者 user 是 actor**。同样的时间 prefix，行为不同：once → is_butler_due → 自动 fire 执行；deadline → urgency classifier → 提醒 user。
- **不复用 ButlerSchedule enum 加 Deadline 变体**：考虑过 `ButlerSchedule::Deadline(NaiveDateTime)`，但 is_butler_due / is_completed_once 都不适用 deadline 语义。**新概念用新类型**——独立的 `parse_butler_deadline_prefix` + `DeadlineUrgency` enum。
- **4 段 urgency tier**：Distant(>6h) / Approaching(1-6h) / Imminent(<1h) / Overdue(过期)。**threshold 分布反映 actionable density**：6h 是工作日里"差不多还有时间但要规划"的阈值，1h 是"必须立刻处理"的阈值。Distant 不进 prompt（不是 actionable signal）。
- **format_butler_deadlines_hint 内部 filter Distant**：纯 helper 内部跳过 Distant 而非 caller 提前过滤。**让 helper "知道什么不该 render"**——caller 只 fetch 全 list，helper 决定显示哪些。这种 inversion 让 caller 简单。
- **整数 hour 计数 max(1)**：`(*deadline - now).num_hours().max(1)`——防止 1 小时刚好的 boundary case 显 "约 0 小时" (整数除法)。**stat 显示要给"看着合理"，不是数学严格**——0 小时不如 1 小时来得实际。
- **prompt 文案区分"专注 vs 不专注"上下文**：tail "如果用户当前没在专注其他事，可以提一下；如果在专注中，仅在 imminent / overdue 时才打断"。**urgency tier 决定打扰许可**——相当于 deep-focus cluster R71 in_progress 信号反向利用：urgent enough 才 override deep_focus 静默原则。
- **R-series 单 iter 多 helper + 完整 wiring**：R77 一 iter 包含 parse + classify + format + 3 个 wrapper + run_proactive_turn 接入 + 11 单测。**bigger iter when helpers are tightly coupled**——split 成 R77/R78 反而 R78 是孤儿数据没用。

## Iter R76 设计要点（已实现）
- **R74→R75→R76 closes the record cluster across 3 surfaces**：proactive prompt（R74） / chat layer（R75） / panel chip（R76）。同 strict-> 语义共享 single source of truth = `current_personal_record_hint()` 是否非空。**三个 surface 不重新实现 record check**，调一次 helper 看 emptiness。这种"empty-string-as-bool"的 reuse 比单加 bool helper 更省一个函数，但 readability 略低——R76 加了清晰注释解释 "non-empty hint == record fired"。
- **panel side 不重做 record 计算**：本来可以让 frontend 把 today.peak vs week.peak 比一下自己判定。但那样 (a) frontend 需要再做一次 strict-> check，(b) 三 surface 的语义边界容易 drift。**让 backend single-source-of-truth + bool flag 出来**，frontend 只 render，三处永远一致。
- **⭐ icon 而非文字**：panel 已经 busy 多 chip，再加文字 "破纪录" 浪费空间。**emoji 是高密度 visual cue**——一颗星表达"今天特别"，hover tooltip 详细解释。
- **⭐ 跟在 chip 数字之后**：放 chip 内部紧邻 "次/Xm" stat 文字。视觉上"宠物盖了个章"——right-of-data 的位置比 left-of-data 更像 "approval mark"。
- **iter 没新单测**：R76 是 surface 加 chip，所有 record-detection 逻辑已 R74 测过。没有新逻辑加测。**纯 wiring iter 不必凑测试**——595 testable surfaces 不必每 iter 都加。这一点跟 R-series cluster 通常 3-6 测/iter 不同；reflect 这是 thin-passthrough 性质 iter。
- **R76 完成 deep-focus cluster 的 cross-surface 对齐**：R62 起 14 iter。从 gate (R62) → recovery (R63) → mode (R64) → today stat (R65) → history vec (R66) → persist (R67) → weekly (R68) → trend (R69) → chat layer (R70) → in-progress (R71) → day peak (R72) → week peak (R73) → record proactive (R74) → record chat (R75) → record panel (R76)。**深度链条第一阶段收官**。
- **下个 iter 该换方向**：cluster 长到 14 iter 不健康，开始有 over-investment 风险。R77 候选: butler_tasks deadline / settings reorganization / persona_summary auto-derived focus pattern / panel layout density audit。

## Iter R75 设计要点（已实现）
- **R74 IDEA 标的"cross modality"立刻执行**：R74 personal-record 只 inject proactive prompt。R75 把 record 信息也嵌进 reactive chat / telegram 共享的 focus_context layer。**modality parity 是 cross-domain bridge 的标准 follow-up**——R70-R71 已建立 layer 机制，R75 让 record 数据穿过去。
- **同 strict-> 阈值在 chat 和 proactive 共享**：R74 compute_personal_record_hint 用 strict > 0 双条件；R75 format_focus_context_layer 内部也是同 logic。**两路径不能 drift**——单 source DAILY_BLOCK_HISTORY，单语义 strict-> only。
- **chat layer 文案 vs proactive 文案差异**：proactive R74 写 "可以温和肯定一下 / 替他高兴"（directive 给 LLM 表达建议）。R75 chat layer 写 "可以为他高兴一下，不必夸张"（context primer 不是 directive）。**reactive chat AI 是被叫起来回答，proactive AI 是主动开口** —— 二者文案 register 不同。
- **wrapper 在 chat.rs 直接读 DAILY_BLOCK_HISTORY static**：跟 R74 wrapper 在 active_app.rs 形成对称。chat.rs 是消费者，access pattern 跟 R72 chat.rs 引用 DailyBlockStats 一样（through glob re-export）。**static access 不必 always 走 helper wrapper**——多 caller 不同语义时直接 inline 计算更直接。
- **测试 fixture 4-arg 升级用 5-line python 自动加 None**：R72/R73 codified pattern 第三次复用（DailyBlockStats / WeeklyBlockSummary / format_focus_context_layer）。**function signature evolution 的 batch fixup 已成模式**——签名增项→sed 加默认值→测试通过。
- **layer 行序：current → today → week → record → guidance**：reading order 时效与 narrative 强度递进。in-progress 是"现在"，today 是"今天"，week 是"本周"，record 是"特别"——celebration 该在 retrospect 信息之后才有上下文支撑。**信息架构有逻辑序，不是 dump-list**。
- **record 单独 fire 也渲染整个 layer**：测试 case `focus_context_record_alone_renders_layer` 显式让 only-record signal 也产出 layer。**信号触发 layer 的策略是 inclusive**——任何一个分支有 data 就 render，反映 has_record 进 has_anything 检查。
- **chat / telegram 共享 inject 函数 = 单点改动**：R71 已让 telegram 共享 inject_focus_context_layer。R75 改 wrapper 一处，telegram 自动得到 record 信息。**cross-modality consistency 通过共享 helper 而非 duplicate code**——R70-R71 投入的 layer abstraction 这次回报。

## Iter R74 设计要点（已实现）
- **R72 IDEA "先 surface 后 inject" 节奏到第三阶**：R72 加 day-level peak 字段（data） → R73 weekly peak（surface） → R74 prompt nudge（inject）。**三阶递进让 user 先 retrospect 看 stat，再让 LLM weave 进对话**。如果 R72 直接做 prompt nudge，user 还没建立 stat 信任就被宠物提醒"今天最长 X 分钟"——感觉算计；先 panel 让用户 retrospect 后注入是 conviction-building 顺序。
- **strict > 阈值的选择**：tied 不算 record（"break" 字面意思就是超过）。也避免 user 重复触发 — 同一个 peak 多次 finalize（理论不发生但防御）不会重复 fire。第一次有 peak（prior=0）也不算 record（无 baseline）。**stat-celebration 的标准要 strict**，否则贬值快。
- **prior_week_peak 排除今天**：使用 `< today` 而非 `<= today`。weekly_block_summary 的 peak_single_stretch_minutes (R73) 是包含今天的，对 panel 显示有用，但对"今日 vs 此前 7 天" 比较是 useless（"今天比包括今天的最大值高" 是不可能的）。**两个 peak 字段语义不同，分开计算**。
- **运行时机：每次 proactive turn 都尝试 inject**：不像 R66 yesterday recap 仅 first-of-day，personal_record 只要数据满足就 fire。**好的 record celebration 应该在 record 发生后的下一次 turn 立即出现**，不必等到第二天。但用户能多次看到吗？只要今日 peak 没被新 finalize 超越就一直可见，turn-to-turn stable。
- **prompt 文案"不必每次都提（如果用户已经很累就别强调）"**：让 LLM 自己 judge。**避免成 spam**——如果每个 turn 都 robotically 提 record，user 反感。LLM 做"context-aware celebration"。同 R66 yesterday focus 的 "自然带过即可，不必非提" 模式一致。
- **没加 panel celebrate visual**：用 prompt-only 注入。**panel chip 已经显 max_single_stretch（R72） + week peak (R73 tooltip）**，再加"破纪录"指示 visual 冗余。celebration 是 LLM 的工作 (用语言)，不是 chip (用视觉)。**职责分离: panel = data, prompt = narrative**。
- **测试 case 6 个覆盖 truth table**：today=0 / prior=0 / tied / lower / strictly higher / +1 boundary。**boundary case (+1) 单独测**避免 off-by-one；strict-only 阈值用 `<=` 实现，所以 today_peak == prior + 1 是首个 fire 点。
- **R74 闭合 R72-R73 微 cluster**：data → surface → narrate 三 iter 后 deep-focus stat 完整覆盖。**未来扩展候选**：(a) 跨 modality (telegram chat 也注入 record hint 当 user 问起)；(b) 月度 peak（cap=14 不够，需扩 30+）；(c) "你的最长一次专注是 N 分钟" 历史最高记录（all-time）需要 separate persistence。

## Iter R73 设计要点（已实现）
- **R72 day-level 镜像扩展到 week-level**：R72 加 day-level `max_single_stretch_minutes`，R73 自然延展加 weekly `peak_single_stretch_minutes` —— 同一 depth 维度在 day + week 两 granularity 都可见。**stat 系统的 granularity hierarchy 应该 mirror 而非碎片化**。
- **R72 IDEA 标的"R73 候选"立刻还**：R72 IDEA 写"R73+ 候选：weekly 也加 peak 字段"，R73 兑现。**candidate list 兑现节奏 = 下一 iter 立即取**，避免堆积变 backlog 黑洞。
- **不加 prompt nudge**：R72 IDEA 提"先 surface 后 inject"——data ship 后让 user 在 panel 看到，pattern 验证再注入 LLM。R73 仍 only data + panel surface，prompt nudge 留 R74 候选。**功能演进的"先观察后行动"节奏**: 数据 → panel → prompt nudge 三阶递进。
- **WeeklyBlockSummary 没加 #[serde(default)]**：跟 DailyBlockStats 不同，WeeklyBlockSummary 不写盘（只 in-memory ToneSnapshot 用），所以不需要 schema migration 标记。**serde default 只在 persistent 类型上 matter**。
- **iter().max() 的 None 边界 unwrap_or(0)**：filter 后空 entries 已在更早 return None，理论上 iter().max() 不会 None。但 unwrap_or(0) 是防御性，避免假设传播；同时保证 zero-fallback 在测试中 explicit。**unwrap 在永不发生的 case 也写 default 值**——测试 / 静态分析能交叉验证。
- **WeeklyBlockSummary 测试构造的批量 sed 修复**：跟 R72 同 pattern，3 处 chat.rs test fixtures 通过 5-line python 自动补 `peak_single_stretch_minutes: 0`。**schema 演进的工程化批改是 R72 codified 的 pattern**——R73 验证可重复。
- **panel 用 tooltip 而非新 chip**：weekly chip 不再加新视觉元素（已显 N 次/Xm/Y天 + 趋势 ↑↓）。**信息进 tooltip 而非 chip** = 信息密度足够而 visual surface 不再扩张。chip 是 attention-grabber，tooltip 是 reference-on-demand，二者职责不同。

## Iter R72 设计要点（已实现）
- **schema migration with `#[serde(default)]`**：DailyBlockStats 加新字段 `max_single_stretch_minutes`。R67 写盘的 JSON 没有此字段，原本会让 from_str 失败 → load_block_history 返回空 Vec → 用户丢历史。`#[serde(default)]` 让旧 JSON 自动用 0 fill，**演进式 schema 不破历史**。这是 R67 持久化写在 IDEA 里的 trade-off "坏数据不会永久 freeze" 的具体兑现。
- **三个数据维度的语义区分**：`count` = 次数（频次 / 频度），`total_minutes` = 累计时长（量），`max_single_stretch_minutes` = 单次峰值（深度）。**三个维度互不替代**：5 次 30m 各 vs 1 次 150m 在 count + total 看似差不多，但深度差很多。R72 把"深度"维度也 surface。
- **`max(prev, current_peak)` 而非 just last peak**：用户某天可能先做 90m 然后做 60m。最长一次仍是 90m，不是最后那次。**保 max 而非 latest** 是 stat 性质决定的。同 R65 codified "stat as confirmation" UX 原则——不让晚的小数字盖住早的大数字。
- **fresh-day 用 `peak_minutes` 而非 0 初值**：第一次 finalize 时 max = peak。如果用 0 初值 + `max(0, peak)`，结果一样。但**显式写 `peak_minutes` 更易读**——读者一眼看出"第一次就是最大值"，不用想 max(0, x) = x 的代数。
- **panel chip 显示 conditional**：`stats.count > 1 && peak > 0` 才显"/峰 Xm"。**单 stretch 的"峰" = "总" = 一回事**，重复显示是冗余。多 stretch 时"峰 vs 总" 区分才有意义。**信息密度 = 区别度**，没区别就不显。
- **批量改 test fixtures 的 sed-based 自动化**：R72 加字段后 20+ 处 test 构造 fail。手改 20 次太烦；写 5-line python 自动找 `DailyBlockStats {` block 后插入 `max_single_stretch_minutes: 0,` 一次解决。**测试 fixture 的 schema 演进可批量自动化**——用 Default impl 也行，但 NaiveDate 没 Default 方便（虽然能用 1970-01-01）。
- **R72 没改 weekly summary**：只 day-level。R73+ 候选：weekly 也加 `peak_single_stretch_minutes` 字段，用于"本周最长一次专注 X min" panel chip / "今天破纪录"prompt nudge。**保持单 iter 单 concern**，不一并扩散。
- **没加 prompt nudge**：data 已有但暂不 inject prompt。R73 可以加 "[今日破纪录]" hint —— 先 ship data 让用户在 panel 看到，pattern 验证再注入 LLM。**先 surface 后 inject**，让 user UX 主导 LLM 行为设计。

## Iter R71 设计要点（已实现）
- **R70 layer 缺 in-progress 状态**：R70 把今日 + 本周聚合放进反应式 chat，但没说"用户当前正在专注 X 分钟"。如果 user mid-focus 开 chat 问 "现在能聊一会儿吗"，AI 不知用户其实正卡 45 分钟连续工作。R71 补这层。
- **30min threshold 选择**：R15=15min（informational），R27=60min（directive），R71=30min。**30 是 "yes user is focused, but not deeply yet"** 的中点。低于 30 是 casual browsing。这种"分级阈值各服一目的" 模式跟 R62-R69 cluster 的多 threshold 同源。
- **gate 在 wrapper 而非 pure helper**：format_focus_context_layer 接 in_progress: Option<(&str, u64)>。**不在 pure 内部判断 ≥30**，让 caller 决定阈值（test 可注入任意值）。**gate 决策属于"业务规则"**，pure formatter 只该做 rendering。
- **layer 内部行序：current → today → week**：阅读顺序"现在 → 今天 → 本周"递减时间近度。in-progress 是"right now"，最优先；today 是"已完成"，retrospective；week 是 broader retrospective。**信息架构倒序：从最高时效→低时效**。
- **新加 1 个测试 fn 测 in-progress order**：明确 assert in-progress.find_index < today.find_index。**顺序保证不只是注释，是测试**。如果未来有人把 today 写在前面，测试 fail。
- **空 app trim 防御**：R71 测试加 "空白 app name 不应在 layer 里出现「  」"。**defense 在 pure helper 内部**——caller 可能传任何怪东西，pure 自己 trim+skip 比要求 caller 传干净数据更稳。
- **telegram parity 是 cluster 闭合的扫尾**：R70 把 reactive chat 加上，R71 既补 R70 (in-progress) 又把 telegram 拉齐。**单 iter 闭合两个口子是可以的**，前提是它们逻辑互相 reinforce。recent_speech 不 inject 到 telegram 是 modality-aware 决策——telegram user 没看 desktop bubbles，引用反而 confusing。
- **import path 复习**：crate::proactive::snapshot_active_app（glob re-export），不是 crate::proactive::active_app::snapshot_active_app（private mod）。R70 已踩过这坑，R71 直接用对的。
- **layer 链长到 4.5 层**：mood / persona / recent_speech (reactive only) / focus_context (with in-progress)。telegram 跳过 recent_speech 但拿到其他三层。**modality 决定哪些 layer 该 inject**，不是 one-size-fits-all。

## Iter R70 设计要点（已实现）
- **R69 IDEA 标的"换方向"立刻执行**：R69 闭合 deep-focus cluster 后写 "下 iter 该换方向了 —— butler / reactive chat / user_profile"。R70 选 reactive chat 方向，让 deep-focus 数据**穿过 modality 边界**到反应式聊天。**cluster 闭合后第一 iter 应该 explicit cross-domain**，verify 解耦了。
- **inject_focus_context_layer 紧跟 R9 inject_recent_speech_layer pattern**：注入位置（before first non-system msg）+ JSON 构造 + no-op when empty body 完全沿用 R9 的 idiom。**新 layer 加 reactive chat 已是 codified pattern**，第四个 layer 不发明新模式。
- **layer 顺序：mood → persona → recent_speech → focus_context**：mood = 当下情绪，persona = 长期画像，recent_speech = 主动开口轨迹，focus_context = 行为强度。**from immediate-state → long-term-identity → recent-output → behavior-data**。每层互不重叠，叠加给 LLM 多角度。
- **format_focus_context_layer 双 None / 双 zero-count → ""**：层级条件防御。今日 entry 存在但 count==0（理论不出现，今日 entry 仅在 finalize 后写）也 skip。**"什么都没说" 比 "今天完成 0 次专注" 更适合作为 system message**——empty 是合理的 unprime。
- **tail guidance 写"如果...自然提及；如果...回答简洁"**：只在 user 问起时才提及，不让 LLM 主动 brag stats。**system context = primer，不是 mandate**。这是从 R66 yesterday recap "自然带过即可，不必非提" 已 codified 的原则。
- **没有给 reactive chat 加 "current focus minutes" 实时信号**：考虑过加 active_app duration 实时数据，但 chat 是 user-initiated，不需要立即知道用户当前还在不在专注（user 都开 chat 了，明显从 task 切出来了）。**实时信号给 proactive；retrospective 给 reactive**——modality 决定数据时效性需求。
- **没用 LAST_HARD_BLOCK 或 take_recovery_hint**：R63 的 recovery hint 是单 shot for proactive。reactive chat 用 retrospective stat 而非 transient marker。**transient state 不跨 modality**——LAST_HARD_BLOCK 只 proactive 取，reactive 不重复消费。
- **跨模块 import path 的小坑**：`crate::proactive::active_app::Foo` failed 因为 module 是 private + glob re-export。`crate::proactive::Foo` 才对。**glob re-export 是 public API**, 内部模块路径不是。这种小细节比逻辑设计更易踩。

## Iter R69 设计要点（已实现）
- **R68 IDEA 候选三选一直接执行 trend 指示**：R68 IDEA 列了 (a) sparkline (b) butler deadline (c) trend。trend 是 cluster 内最自然延展（数据已有 + 视觉极小 + 用户单点 hover 能看完整 math），R69 选这条。**candidate list 选 highest cohesion-with-current-cluster 的执行**，避免每 iter 切方向。
- **cap 7 → 14 是必然要求**：trend 需要 prior week 数据 = 8-14 天前的 entry，cap=7 全 evict。**前 iter 的设计 affordance 不够 long-term 时**，下个 iter 必然要扩；这种 progressive widen 比一上来 cap=30 更安全（用过才知道要多大）。
- **direction = 三态而非二态**：up / flat / down。±15% 之内 flat 防止 user 看见 ↑↓ 频繁震荡（自然 day-to-day variation 就有 ±10%）。**threshold 是 reduce-noise 工具**，不是 strict equality。
- **down 用 muted gray 而非 red**：down 不是 negative judgment（"专注少了，不应该"）。是 informative ("你这周比上周少专注 X%")，**color 不传达 should/shouldn't**。green up 是肯定（成就感），gray down 是中性（信息）。
- **delta_percent 用 i128 中间值再 clamp i64**：防止 (a - b) * 100 在 u64::MAX 边缘下 overflow。**整数 percent math 必先 promote 再 clamp**。clamp ±999 是 display sanity，> 1000% 在 panel chip 显得 absurd，clamp 后 tooltip 仍清楚展示 raw。
- **prior week 用 today-13..=today-7 区间**：与 this_week today-6..=today 严格无重叠。**两窗口拼接覆盖 today-13..=today 共 14 天**，正好等 cap=14。窗口对齐 cap 是巧合 also nice — 任何时刻 history 都恰好覆盖 trend 计算。
- **None vs flat 的区分**：prior 全 0 → None（不能做除法 / 没基线），prior > 0 但 this == 0 → 也 None（这周完全无专注，trend 无意义）。flat 仅当两周都有 ±15% 内变化。**None / flat 语义不同**：None = "没法比较"，flat = "比较过且基本一致"。
- **panel 倾向 inline 而非新 chip**：可以单独加个 "📊 +20%" chip 但太抢焦点。inline ↑ next to weekly column 是低噪声 + 信息明确。**辅助信息靠近主信息更易 grok**。
- **R69 closes deep-focus cluster 第一阶段**：R62-R69 = 8 iter 把 deep-focus 从纯 backend (R62) 一路推到 weekly trend visual (R69)。下 iter 该换方向了 —— butler / reactive chat / user_profile 等待开发。**cluster 有自然 closure 信号 = "下一 iter 不在同一概念域"**。

## Iter R68 设计要点（已实现）
- **R66 的 cap=7 future-proof 立刻被 R68 用上**：R66 写"剩 5 个槽位预留'本周专注总分钟' 等扩展"，R68 就把这预留兑现。**前 iter 留的 affordance 是 promise**，下个 iter 用得上 promise 才合理；不用就是过度设计。
- **window filter 用 today - 6 days 而非 simple last-7-entries**：cap=7 的 vec 同样可能含 8 天前的 stale entry（如果某天 cap drain 之后又有新 entry 插入更早 date）。**date filter 是 semantic 真值**，cap 是 storage bound；二者不该混淆。helper 这样设计也让"future cap 升级到 14" 时 weekly window 不会自动变成 last-14。
- **boundary inclusive (today 以及 today-6)**：测试显式 cover 7 天前的 boundary 仍 included、8 天前的 excluded。**range 用 `>=` `<=` 双闭比 strict less 更符合自然语言"最近 7 天"语义**。文档措辞跟实现严格对齐。
- **none 而非 zero stat**：R65 codify "stat as confirmation, not zero-state"。R68 weekly summary 同理 — 全空 / total_count==0 都返回 None，PanelStatsCard 不渲染。**新 iter 复用 codified UX 原则**，不重复决策。
- **stat-card 横排放在 daily 之前**：UX 阅读顺序"本周 → 今日"自左向右递近，符合 expected mental flow（"我这周咋样, 今天到目前为止呢"）。tooltip 解释聚合源 + cap 边界给 power user 自检。
- **saturating math 一致性**：compute_weekly_block_summary 用 fold + saturating_add；跟 compute_history_after_finalize 同 saturating 风格。**stat 系统全程 defensive saturation**，永不 panic on overflow。
- **wrapper / pure 拆分继续**：current_weekly_block_summary 是非 pure（读 static + Local::now），compute_weekly_block_summary 是 pure。继续 R20 codify 的"pure helper 不知 settings/clock, wrapper 知"。
- **R68 cluster status**：R62 → R67 是"deep-focus pipeline"（gate / recovery / mode / today / yesterday / persist）。R68 是 cluster 第 7 iter，开始向"汇总 / trend" 方向延展。继续 cluster 的话候选：(a) PanelDebug 多日 sparkline；(b) butler_tasks deadline；(c) "本周比上周变化" trend 指示。

## Iter R67 设计要点（已实现）
- **R66 IDEA 标的"未来 R67+ 候选"立刻还**：R66 写"memory-only OK，先 ship 内存版；持久化留 R67+ 候选"。R67 立刻补上。**TODO 标"未来候选"是 promise，不是逃避；下一个 cluster slot 就还**。这种节奏类似 R29/R30 codified 之后立即在 R30 audit-and-backfill。
- **save 在 finalize 之外** 而非 inside DAILY_BLOCK_HISTORY 锁内：finalize_stretch 释放锁后才 save_block_history。**减少 mutex 持有时间**，IO 是慢操作，不该卡住其他读 / 写。代价是窗口内可能有 race —— 但 history 是 append-only 性质，race 顶多让"刚 finalize 没写完"，下次 finalize 会再写，无数据丢失。
- **load_block_history_into_memory idempotent guarantee**：startup 调一次 + memory non-empty 就 no-op。如果 process 重启早就发生过 finalize（罕见但理论上：startup 慢，proactive::spawn 启动 IT first tick 可能在 load_block_history_into_memory 之前），load 不会 clobber。**保证："load is safe to call regardless of state"** = startup ordering 不重要，简化 lib.rs。
- **error tolerance 三层**：(1) `dirs::config_dir()` None → 平台不支持 → fallback memory-only；(2) `read_to_string` 失败 → file missing 或 permission → 当作 empty Vec；(3) `from_str` 解析失败 → corrupt JSON → empty Vec + log。**坏的 JSON 不应永久 freeze stat 系统**，下次 finalize 重新写干净版本。
- **测试 race 暴露的问题**：cargo test 默认 parallel，多 test 同时 mutate DAILY_BLOCK_HISTORY 出错。R67 加 TEST_LOCK 串行化关键测试。**"Mutex<Option<>>" 单 slot 时代不暴露这个**（lock briefly + restore），但 R66 vec 让 finalize 写更复杂 + R67 disk IO 让窗口更长，race 显现。**测试套规模过 500 后必须考虑 cross-test isolation**。
- **TEST_LOCK 用 unwrap_or_else(|p| p.into_inner())**：tests 失败时 mutex 不 poison —— 让后续 test 能继续抢锁。常规 `lock().unwrap()` 在某 test panic 后会阻塞所有后续。**测试基础设施应该 graceful**，不让一个失败拖累一批。
- **不写测试一定不写到生产 file path**：load_block_history_into_memory 直接调 dirs::config_dir 真路径，测试不在 sandbox 下会污染 ~/.config/pet。R67 的"持久化测试"通过 std::env::temp_dir + 独立 path 验证 JSON 层，不调 wrapper 直接走 disk。**生产 IO wrapper 不便测，分离 pure JSON 层 + path 层就好测**。
- **未来 cluster R68+ 候选**：(a) panel 显示 last 7 days focus stretch 总分钟（cap=7 的 future-proof 用户）；(b) butler_tasks deadline 字段；(c) 持久化 LAST_HARD_BLOCK / LAST_ACTIVE_APP（让 recovery hint 跨重启）—— 但这些 transient state 重启后基本无意义，不必持久化。

## Iter R66 设计要点（已实现）
- **R65 single-Option 升级到 history Vec**：R65 用 `Mutex<Option<DailyBlockStats>>` 存今日 stat。今日第一个 finalize 会覆盖昨日的 record，**昨日 stat 立刻丢失**，跨日 recap 无来源。R66 改 `Mutex<Vec<DailyBlockStats>>` 滚动 7 天历史，今日 finalize 不再 evict 昨日。**single-slot → history vec 是不可避免的演进**，从一开始就该用 vec —— 但 R65 当时只满足"今日 stat" 需求，over-engineer 是浪费。**演进式重构在需求来时再做**。
- **cap = 7 = 一周的 future-proof**：今天 + 昨天只用 2 entries，剩 5 个槽位预留"本周专注总分钟" 等更广 stat。**cap 选择基于"未来一两个 iter 可能用到"的预期**，不为遥远功能预留太大缓存。7 是天然的人类周期边界。
- **compute_finalize_stats 删了，compute_history_after_finalize 是 superset**：R65 的 compute_finalize_stats 只处理 single-slot 转换，R66 的 compute_history_after_finalize 处理 vec increment / append / sort / cap 一站式。**两个 helper 重叠 ≥80% 就不要并存**，删旧 helper + 移植测试到新 helper。tests 数量净增 4（5 → 9），覆盖范围加大（cap eviction / sort 是新检查点）。
- **first-of-day 三 hint 互补**：cross_day_hint = 昨天最后 2 句话（continuity）；yesterday_recap_hint = 昨天 daily_review 的概要（high-level）；R66 yesterday_focus_hint = 昨天专注 stat（activity intensity）。**三层从 narrative → summary → behavioral**，给 LLM 多角度起手 context。**hint 的"互补维度"** 才值得多一条；如果 R66 hint 只是 R12 review 的子集就不值得加。
- **yesterday_focus_hint 措辞 "自然带过即可，不必非提"**：避免 LLM 把每次第一句都说成"昨天你做了 N 次专注..."。**hint 要给 context 但留 LLM judgment 的空间**。R12c 的 review 概要也是这种风格 —— inject info，不强制 verbalization。
- **memory-only 是 OK 的初始 trade-off**：R66 不写文件持久化。process restart 会丢历史。但 daemon-style 长跑 app 在 macOS 通常不重启 —— 早 commit 写持久化是过早优化。**先 ship 内存版**，观察 restart 频率 / 用户反馈再加 persistence（R67+ 候选）。
- **out-of-order finalize 的容忍**：sort_by_key(date) 兜底任何插入顺序。如果将来加"补录昨天的 finalize"（比如 process 启动时从 disk 读) 这个保证已就绪。**预留排序保证是低成本 future-proofing**，比预留 10 字段昂贵的 schema 廉价。
- **out_of_history 风险**：cap=7 + 8 天 finalize 会丢最早一天。如果用户连续高强度专注超 7 天，第 8 天 yesterday_block_stats 仍能拿到（只丢 7+ 天前的）。**cap 只压老数据，不压"昨天"** —— 永远不影响 R66 主功能。

## Iter R65 设计要点（已实现）
- **R62/R63/R64 cluster 至此扩展为 4 iter**：R62 阻塞 → R63 recovery → R64 mode dial → R65 stats accumulate。**这是 R-series 第二个超过 3 iter 的 cluster**（R52-R59 user-control 是第一个）。**cluster 长度不预定**，只要每个 iter 都打开新 surface 就该继续。
- **stretch 检测的 transition vs clean-end 二分**：每个 stretch 必有终点，但终点种类有二：(1) 用户切出来 → take_recovery_hint clean-end finalize；(2) gate skip 持续但没 run 路径触发 take（如 quiet hours 期间） → record_hard_block 看到 prev.marked_at > 120s 后 transition-finalize。**两个 finalize 路径互斥不重复**：take_recovery_hint 后立刻 *g = None，下次 record_hard_block 看 None → 不 finalize 任何东西，只 record 新 stretch。
- **120s 阈值的选取**：proactive interval 通常 60s，所以正常 stretch 内连续 record_hard_block 间隔约 60s。120s = 2× nominal，留一倍冗余应对调度抖动。**< 60s 太敏感** 会把单 stretch 切成多 stretch；> 300s 太松 会让真正中断的两段被合并。
- **finalize_stretch 锁顺序的小心**：take_recovery_hint 持有 LAST_HARD_BLOCK 锁时调 finalize_stretch（要求 DAILY_BLOCK_STATS 锁）。两 mutex 不同 instance 不死锁，但**为可读性 drop g 后再调 finalize**。这一点跟 R57/R58 的"ergonomic 重于优化"一致 —— 显式生命周期减少未来误读。
- **count 不含 in-progress**：用户体验"今日 N 次专注" = 完成的 N 次。当前还在 deep focus 中的 stretch 不算 count，不计 minutes。**stat 是 retrospective**，不是实时。如果 user 想看"现在专注多久" 看 active_app chip 即可。这种"完成 vs 正在"的二分让数字更稳定（不会跳）。
- **PanelStatsCard 的 "🛑 N 次/Xm" 显示决策**：count > 0 才渲染。空态不显"今日 0 次"，避免给"专注时间不长" 用户负面暗示。**stat 应该是 confirmation 而非 reminder**。看到 🛑 是"我今天确实专注了" 的肯定，看不到是"还没刷新到今日统计" 的中性。
- **date 滚动的处理 = 在 finalize 时检查**：compute_finalize_stats 看到 prev.date != today 就 reset。意思是"昨天最后那次 stretch 如果跨夜 finalize，会算到今日"。**简化模型：finalize 那一刻属于哪天就归哪天**，不做跨日 split。代价是某些极端 edge case（开发狗血特殊场景）count 偏移，正常使用场景无差。
- **u64 saturating_add 的防御意义**：total_minutes 用 saturating_add；现实中 u64 = 约 35 万年，但显式 saturating 标 intent —— stat 是"近似" 不是"账本"，溢出退化到上限优于 panic。

## Iter R64 设计要点（已实现）
- **R62 hard-block threshold 该不该全局固定？**：R62 用 const = 90，R29/R30 让用户能选 companion_mode (chatty / balanced / quiet) 但只调 cooldown / chatty_threshold。R62 的 90min 阈值跟用户偏好脱节 —— quiet 用户希望"我专注 60min 你就别打扰"，chatty 用户希望"我专注 2 小时你也试试"。**user dial 应该是 holistic 的**，每个 gate 的 magic number 都该跟 mode 一致。R64 把 R62 这个 magic number 也接进 mode 系统。
- **chatty=135 / balanced=90 / quiet=60 三档**：math 选 base × 3/2 / base × 2/3 — symmetric multipliers，integer math 在 90 → 135/60 上 round-trip 干净。**quiet=60 跟 R27 directive 边界重合 = 软硬同步**：quiet 用户 R27 directive 立刻升级硬阻塞，pet "不犹豫" 退后；balanced 留 30 min 缓冲让 R27 自我纠偏；chatty 直接跳过 R27 缓冲扩到 135。**multiplier 选 1.5x / 0.67x 而非 2x / 0.5x** 是因为 hard-block 不像 cooldown 那么 user-tolerant — 翻倍会让 chatty 用户在 3 小时同 app 后才阻塞，太晚。
- **API 边界 settings vs proactive**：apply_companion_mode_hard_block 放在 settings.rs（mode 调度是 settings 概念），但 ProactiveConfig::effective_hard_block_minutes(&self, base: u64) 让 caller 注入 base 而非硬编码 const。**避免 settings.rs 反向依赖 active_app::HARD_FOCUS_BLOCK_MINUTES const**。同 pattern 让 settings module 保持独立 / proactive module 保持自己的 magic numbers。
- **gate 用 compute_deep_focus_block 而非 wrapper**：R62 时 `deep_focus_block_minutes()` 是 wrapper hardcode HARD_FOCUS_BLOCK_MINUTES。R64 gate 改用 cfg.effective_hard_block_minutes(...) 算出 threshold + 直接 `compute_deep_focus_block(prev, threshold, now)`。**wrapper 唯一 caller 改用 pure helper 后 wrapper 死代码** —— 删除而非保留。**helper 设计原则：pure helper 必留，wrapper 仅当多处调用时存在，单 caller 直接 inline pure**。
- **panel chip threshold 从 snapshot 读**：之前 chip 写死 minutes >= 90，会跟 quiet/chatty 用户的 gate 行为脱节（chatty 用户在 90-134min 区间 chip 显 deep-red 但 gate 仍允许）。R64 加 ToneSnapshot.effective_hard_block_minutes 字段，chip 用此值做 threshold，hover tooltip 显当前 mode 的阈值。**chip 是 gate 的视觉镜像 — 阈值必须共享 source**。
- **tooltip 解释三档值**：当前 mode 阈值 + 其他两档对照（chatty=135 / balanced=90 / quiet=60）写进 hover。让用户切换 mode 时知道"调小或调大会发生什么"，**为后续可能的 mode 切换实验做 affordance**。
- **0 base 保留 opt-out 路径**：apply_companion_mode_hard_block(_, 0) 对所有 mode 都返回 0，跟 apply_companion_mode 一样。如果将来加 setting 让用户彻底关 hard-block (base = 0)，integer math 自动保留这个语义。**预留 future 控制点的方式 = math 不破坏 0 即 disabled**。

## Iter R63 设计要点（已实现）
- **R62 → R63 是 "gate skip + recovery context" 一对**：R62 让 gate 在 90min+ 直接 skip，但 skip 一直 skip 不留 trace —— 用户真切出来后 pet 像"什么都没发生"一样开口。R63 补上 recovery hint：第一个 non-blocked turn injection "[刚结束深度专注] 用户刚从「X」N 分钟专注里切出来"。**block + recovery 是配对的，缺一就少了"伙伴注意到了" 那层**。
- **take-on-use single-shot 模式**：take_recovery_hint 写完即清，所以同一个 block stretch 不会反复注入 hint。如果用户切到另一个 app 又快速回去深度专注，下一次 block 又被独立记录、下一次 recovery 又会触发。**state 设计跟着 user behavior 自动节奏**。
- **gate 写 / run-path 取 的耦合点**：gate.rs 在 R62 block 触发处写 LAST_HARD_BLOCK；run_proactive_turn 在 hint 注入处 take-and-clear。**两个不同 module 共享一个 static 是耦合**，但比 plumbing through 函数参数干净 —— Rust 模块系统下 `super::record_hard_block` 是 explicit 的，不像 React Context 那样 implicit。`pub use` re-export 让 super:: 路径短。
- **redaction 在 wrapper 而非 pure helper 内**：format_deep_focus_recovery_hint 是 pure formatter（不读 settings）。take_recovery_hint wrapper 在 format 前 redact。**遵循 R20 codified 的 "pure helper 不知道 settings, wrapper 知道" 边界**，避免 active_app_hint 那条已经 follow 的 pattern。
- **grace window = 10 min** 选 **取舍**：太长（>15min）会让 pet 在用户已经做了别的事 10 分钟后还说"刚切出来"，体验脱节。太短（<5min）会让其他 gate（cooldown/awaiting）的 skip 把 recovery 错过。10 是经验中点，cover 大部分 cooldown 残留秒数 + 给 user 切出来后的 reaction 时间。**没有不可调的常量**，未来观察到 miss/false-fire 再 retune。
- **prompt 中位置紧挨 active_app_hint**：assembler 把 deep_focus_recovery_hint push 在 active_app_hint 之后。两个 hint 可能同时 fire（block 刚解除 → user 仍在该 app 但分钟数 reset → active_app_hint 描述当前 < 15m 不 fire）。如果 user 切到不同 app，active_app 描述新 app，recovery 描述旧 app。**两 hint 语义互补不重复**：active_app = 当下，recovery = 刚才。
- **没改 PanelToneStrip**：R62 已加 deep-red 🛑 chip 显示 hard-block 状态。recovery 是 transient（只在一个 turn 中存在），surface 到 panel chip 会闪现一次没意义。如果 user 想知道 "刚才有没有 block"，看 PanelDebug timeline 的 Skip 记录就够了。**transient prompt-only 信号不必 panel surface**（self-correct R34 IDEA 的反例：streak 是 between-turns stable 才上 panel，recovery hint 是单 turn 的不上）。

## Iter R62 设计要点（已实现）
- **soft directive → hard block 的 escalation 第二台阶**：R15 (15m hint) → R27 (60m soft "极简或沉默") → R62 (90m gate skip) 三段。**每次升级都给上一台阶 30 分钟纠偏机会**——R27 在 60m 提示 LLM 自觉沉默，到 90m 还没切 app 说明用户**确实**在深度专注，硬阻塞是合理的。这种"先软后硬 + 阶梯化时长"避免了"60m 直接屏蔽"的过度反应。
- **gate-side osascript 的成本权衡**：R62 在 evaluate_loop_tick 加了一次 `current_active_window` 调用 —— osascript ~50-200ms / 60s tick = ≤0.3% overhead。**显式付出 IO cost 换取 snapshot 不 stale**。alternative 是让 snapshot 只在 run_proactive_turn 更新，但**那样硬阻塞一旦触发就永久卡住**（gate 跳过 → 不进 run → 不更新 snapshot → 持续跳过）。staleness 自我维持的 trap。多花 200ms / tick 换 gate 永远见 fresh state，well worth it。
- **idempotent refresh 是关键**：`refresh_active_app_snapshot` 写完后 `update_and_format_active_app_hint` 在 run_proactive_turn 里又会调 `compute_active_duration` 一次。两次背靠背调用必须 idempotent —— `compute_active_duration` 在 app 不变时返回 same since（"carries forward"）保证这一点。**多入口写同一 static 的设计前提是写函数 idempotent**。否则 gate 路径会 reset since 让所有 duration 重新计算。
- **R52 / R55 / R62 三 gate 的 short-circuit 顺序**：mute (R52) → deep_focus_block (R62) → mute 配置（pre_input_idle）。**最便宜的 gate 放最前** —— mute 是纯 mutex 检查 (~ns)，deep_focus_block 需要 osascript (~ms)，pre_input_idle 需要 read settings + clock + focus state。**多 gate 系统按 IO cost 升序排列短路顺序** = 平均每 tick 总成本最小化。
- **为什么不加 setting 让用户 disable hard-block**：R29 / R30 codified rule 是"yaml-only 字段必补 UI"，但 R62 只加 const 没加 setting，**目的是让 hard-block 是 invariant 而非可选**。如果用户能关，那"我用了 90 分钟它仍打扰我"就是用户 own choice，**避免 R62 沦为更激进的 R27 mood hint**。如果未来反馈说"我开会但需要被打扰"，再考虑加。**一次只加 invariant 一次只加 setting，不要 mixed**。
- **panel chip 4 段色映射 gate 状态**：< 15 灰 / 15-59 橙 / 60-89 红 / ≥90 deep-red + 🔒🛑。**色彩饱和度 / icon 数量随 intervention 强度递增**：从"无 prompt nudge" → "soft hint" → "directive" → "skip turn"。chip suffix `🔒` (R27 lock icon) 在 R62 升级为 `🔒🛑` 表示"还有 stop sign"，让"我看到了红 + 锁 + 屏蔽" 三层视觉递增。
- **跟 mute 概念区别**：mute 是用户主动按钮 → opt-in skip。R62 是系统观察 → opt-out skip（无配置可关）。**两个 skip 的"who decides" 不同**：mute = 用户当下意图，R62 = 系统从行为推断。前者写在状态 mutex，后者纯函数 derived。这种"显式 vs 推断" 的二分意识到了对 transparent UX 重要——chip 各显其色防止用户混淆"我关了静音怎么还不说话"。

## Iter R61 设计要点（已实现）
- **R-series proactive-audit cadence is forming**：R60 → R61 都是 audit-driven。R60 grep `redact_with_settings` 调用点查 prompt 注入完整。R61 grep `redact_text` 调用点查 tool output 完整。**每次 audit 锁定一个 "boundary kind"，systematic grep + 一次性修**。这种节奏比"feature ship + reactive bug fix" 更适合 mature project —— 主动巡查比被动响应高效。
- **redact_text vs redact_with_settings 命名隐患**：API 命名让 `redact_text` 看起来像 main entry point。但真正完整的是 `redact_with_settings`（含 regex）。**命名应该让"完整工具" 有更显眼的名字**，sub-helper 名字更长 / 标 internal。R61 IDEA 想到这点但不动 API 命名（影响测试 + 现有测试用 `redact_text` 直接调）。**API 命名重构的代价大于收益时，靠 audit + 文档** 替代。
- **substring 先 regex 后两-pass 顺序的隐性意义**：redact_with_settings 注释写明 "specific names get marker before wide email regex could swallow context"。这个顺序选择**保护"具体 > 通用" 的 redaction 优先级**。如果 regex `\\w+@\\w+` 先匹配，后面 substring "Alice" 可能匹配不到。先 substring 让 specific names 先打 marker，regex 处理剩余。**多 pass redaction 的顺序是有 semantic 意义的，不是任意拍**。
- **redact_text caller minor pattern**：grep 后发现 `redact_text` 只在 redact_with_settings 内部调一次 + 测试。**外部 caller 0 处** = R61 切换 system_tools / calendar_tool 后 `redact_text` 真的只剩 internal 调用。这是"audit 后 internal-only" 的一个简化机会 —— 未来可以把 `redact_text` 改 `pub(crate)` 限定在 redaction module 内部。
- **counter 副收益**：原 redact_text 不更新 REDACTION_CALLS / REDACTION_HITS atomic counters。切到 redact_with_settings 后 tool 调用也累加。**panel redact stats chip / API 现在反映真实总调用** —— 之前 active_app / calendar redact 是 invisible 的。
- **boundary-kind audit list**：R-series privacy audit 需要 systematic 走过所有 boundary kind。已 covered:
  - prompt injection (R60: feedback_hint)
  - tool output (R61: active_app / calendar)
  
  剩下候选:
  - log file (debug.log / butler_history / feedback_history) — 已 conscious 决定保 raw 给 dev
  - panel display — local-only，不需 redact
  - Tauri command return values — 多数已经从 ToneSnapshot 等去 — 已 redacted upstream
  
  R-series privacy audit 大致完整，剩下 case 都是 conscious decision (raw 保留)。**audit 不是无穷尽** —— 终点 = 所有 boundary 都明确 raw or redacted。

## Iter R60 设计要点（已实现）
- **proactive privacy audit 是 mature project 健康习惯**：R-series 之前的 redaction 加点都是被动响应（QG4 等）。R60 是第一次主动 grep 全 codebase 查 redact 完整性。**长 lived project 应该周期性主动审 privacy boundary** —— 不等到 bug 报出来。这是除"react to incidents" 之外的 proactive security stance。
- **storage 不 redact / prompt boundary 才 redact**：feedback_history.log 存原文，prompt 注入 redact。**redact 是 cross-process / cross-trust-domain boundary 的责任**，不是 inside-process 数据存储责任。Local 文件 / panel 显示都是 user-local，不需 redact 自己的数据；prompt 是发到 LLM (potentially external API)，必须 redact。这条边界划分清楚后所有 redact 调用点都该 audit "我跨 boundary 了吗？"。
- **closure parameter 是 R-series codified pure helper redact pattern**：format_reminders_hint / format_user_profile_hint / format_plan_hint / format_persona_hint / format_feedback_hint (R60) 都是 `&dyn Fn(&str) -> String` 签名。**统一 API** 让 caller 看到签名就知道这是 redact-aware 函数。如果未来加新 prompt-injecting helper 也该跟此签名 —— 跟 testability 双赢（测试传 identity，prod 传 redact_with_settings）。
- **excerpt 是 pet's own utterance 但仍可能含 private 内容**：第一直觉是"pet 自己说的不需要 redact"。但 LLM 写 reply 时可能 echo user_profile 里的私人词（用户告诉 pet 他在 company X，pet 在 reply 中提"X 的工作进展"）。**LLM 输出仍是 prompt 输入的"放大镜"** —— prompt 含的私人词 LLM 会传播到 reply，reply 进 history 又回到 prompt → 自循环。redaction at every prompt-boundary 让循环每一步都 hard-stop。
- **R-series 600 tests 走在路上**：519 已经是 ratio 好。R60 +1 因为已有 5 个测试只是 signature 更新（不算新增 logic 测试）。每次 codebase 加新数据流就该问"测覆盖了吗"，每次新 audit 触发就该问"是否需要 nail 一个 test 防退化"。R60 加 redact-applied test 是后者的好例子。
- **id_redact test helper 应该 codify**：测试 redact-aware 函数的 identity-closure pattern 重复出现 (format_reminders_hint tests / format_feedback_hint tests / format_*_hint tests)。**未来抽 shared test util module** if R-series 继续加这种 helper。同 R39 PanelFilterButtonRow 思路 —— use-3+ 才抽。当前 use 还在 2-3，先 copy。
- **R60 是 R-series 第一个 "audit-without-trigger" iter**：没有报告说 feedback excerpt 泄漏，没有 user complaint。R60 仅是因为我决定"audit 一下 prompt redact" 而触发。**这种 self-initiated audit iter 是 mature project 跟 reactive project 的区别** —— 前者在 issue 出现前补，后者在 issue 出现后救。R-series 已经累积足够 codebase 复杂度让 audit iter 有真实价值。

## Iter R59 设计要点（已实现）
- **read + write helper 对称是完整的 testability**：R53 + R56 测 read 路径 (compute_*_remaining)。R59 测 write 路径 (compute_new_*)。**每个 stateful 模块都应该有 read + write 两套 pure helpers**——单测覆盖才完整。如果只测 read 不测 write，setter boundary case (0/负数/empty/whitespace 等) 全是黑盒。R59 关闭这个 testability gap。
- **defense in depth: whitespace as empty**：compute_new_transient_note 把 " "/"\t\n" 当 empty 处理。诱惑是"frontend 应该 trim，backend 信任输入"。但**pure helper 不依赖 caller 行为** —— defense at boundary。Trade-off: backend 多 1 行 trim 检查 vs 一旦 frontend bug 就保存了空白 note。明显前者收益。这条 R-series 早期已用过（R12 daily_review skip empty body / R26 feedback_aggregate skip < 5 samples 等）。
- **`&str` for pure helpers**：compute_new_transient_note 接 `&str` 而不是 `String`。**pure helpers 默认 borrow 不 own** —— 让 caller 决定 ownership。Tauri command 接 `String` arg, pass `&text` 给 helper, helper 在 `text.trim().to_string()` 才转 own。这种"borrow when possible" 是 Rust 习惯。
- **trim ≠ collapse internal whitespace**：" in a meeting " trim 后是 "in a meeting"，不是 "in a meeting"（多空格压一个）。internal whitespace 是用户语义一部分（句子停顿 / 缩进），不该 collapse。**trim semantics 应该明确：仅去边缘 noise**。
- **R-series user-control cluster 8 iter 完整化**：R52 (mute backend) → R53 (mute read test) → R54 (mute preset menu) → R55 (note backend) → R56 (note read test) → R57 (note refresh-on-open) → R58 (mute refresh-on-click) → R59 (mute+note write test)。**8 iter 全在同 cluster，建立完整 stateful module pattern**：backend struct + 4 pure helpers (read active / read remaining / write new / clear) + 测试 + UI button + chip + popover/menu + auto-refresh。这套模板**适合所有 transient state 工具**，未来加 mood preset / focus level 等可以照搬。
- **518 tests 大多在 user-control cluster**：R-series 50+ iter 中 user-control cluster 单独贡献 ~30 tests（R53 5 + R56 4 + R59 9 = 18 + 间接覆盖 + boundary case）。**真正复杂的功能值得高密度测试**——单 iter 5-10 test 是 mature project 健康比例。
- **Tauri command 应该尽量是 thin wrapper**：set_mute_minutes / set_transient_note 现在都是 ~10 行 wrapper —— 调 pure helper + write mutex + format response。**Tauri 命令本身没有 logic 该 test**，所有 logic 在 pure helpers 里。这是 R-series 反复强调的纪律 (R52 mute / R55 note 等)，R59 把这个原则贯彻到底 —— 已经 ship 的 command 也回头还成 thin wrapper。

## Iter R58 设计要点（已实现）
- **codified rule audit 第一次践行 R57 原则**：R57 codified "transient state needs refresh on user-interaction entry"。R58 立刻 audit 同 codebase 还有谁违反 —— R52 mute 同样 latent bug。这种 codified rule → audit-and-backfill 是 R-series 反复践行的节奏（R20→R21+R22 / R29→R30 / R46→R47）。**rule 价值不在 codify 一次，而是反复 audit 让全 codebase 都跟上**。R58 是这条节奏的又一次实例化，证明 rule 真的 actively 应用。
- **Promise<boolean> return 解 closure-over-stale-state 坑**：React useState 的 setMuted 触发 next-render 才生效。如果 `setMuted(isMuted); if (muted) ...` 这种代码 sequence 里 `muted` 仍是 closure 里捕获的旧值。**直接 return fresh value** 让同 promise chain 内拿到 truth = 经典 React closure 坑解法。这条原则适用所有"先 fetch 再 act" 的 async handler。
- **graceful degradation in UI**：refreshMuteState catch error 时 return `muted` (current React state)。诱惑是 throw 让 caller 处理，但 click 路径要 best-effort —— 网络故障时 mute toggle 仍能 fallback 到 last-known state 工作。**UI 应该 fail soft 而不是 fail hard**。错误处理在 backend 严格在 UI 弹性。
- **copy + tweak 是 use-2 的合理选择**：R57 + R58 两次同样 pattern (refresh-on-interaction)。诱惑是抽 generic `useTransientStateRefresh<T>` hook。但 mute 只 boolean，note 是 (text, active) 双状态，generic 化得加 type parameters + signature 复杂度。**use-2 仍 copy 比抽 hook 经济**（R39 IDEA codified "use-3+"）。如果未来 mood preset / focus level 等加第三个 transient state 才抽 hook。
- **close 不 refresh 但 open refresh**：close 是 user 主动 + state 不会变，refresh 没价值。open 是 user 触碰 + state 可能 stale，refresh 必要。**fetch 时机 = user-interaction × state-might-be-stale 的交集**。R58 严格遵循 R57 IDEA 的"仅在可能 stale 的 user-interaction" 原则。
- **R52→R58 = 7 iter user-control cluster**：R52 mute backend, R53 mute test, R54 mute preset menu, R55 note backend, R56 note remaining, R57 note refresh, R58 mute refresh。**7 iter 全在同 cluster** —— 新长度纪录。说明 mature phase 中 cluster 长度由 user-domain depth 决定 —— 简单 domain 3 iter 闭环，复杂 domain (mute+note 两工具 × backend+test+UI 多维度) 自然延展到 7。**cluster size 跟 problem space depth 成正比**，不是 iter rule。
- **stale-state pattern 普适警告**：mount-fetch + 不 refresh 是 React frontend + transient backend state 的常见 footgun。任何"backend state 有 expiry / 时效" 的功能都该 audit 是否有这个 bug。R-series 类似工具（如未来的 mood override / focus level / etc）都要 same-iter 加 refresh-on-interaction 否则 ship 时就埋雷。

## Iter R57 设计要点（已实现）
- **transient state 跟 frontend cache 的同步陷阱**：R55 mount 时 fetch 一次，之后 frontend useState 当 truth。但 backend 在 expiry 时自动改状态，frontend 不知道。**任何 transient backend state 都需要在 user-interaction entry point refresh**——开 popover 是 user-interaction，应该 fetch。这条是普适原则：long-lived component 持有 transient state 时，每次 user 触碰该 state 入口都 refetch。
- **stale-state bug 在 mature feature 上的隐性出现**：R55 ship 时 mount-fetch 看似够。R57 才意识到"60 min 后 expire 期间 popover 不会主动 refresh"。**transient state 的 lifecycle 测试需要超过 mount window 的场景**——但单测不容易模拟"60 min 后再交互"。靠 IDEA + 经验觉察 vs 测试覆盖。
- **preserve draft 是防数据丢失的小细节**：if (text) load else don't clear。用户如果 type "我今天身体..." 但还没 save 就关闭 popover，重开仍显 draft 不丢。如果 R57 改成"始终 reset to backend"，draft 会被覆盖成空 = 数据丢失。**任何 user-typed text 都不该被 system 自动清** unless 用户明确表达"不要这条" (e.g. 解除按钮)。
- **open async vs sync trade-off**：handleNoteToggle async。设计选择：(a) sync set state + later async update (popover 先显 stale 然后跳); (b) await fetch then set state (popover 显示前 100ms 等)。选 (b)。**flash of stale state 比 100ms 等待更糟糕** —— 用户看到 popover 后 "已激活" 然后变 "未激活" 会觉得"出 bug 了？"，而 100ms 加载延迟在 popover 打开时几乎察觉不到。
- **fetch 仅在 state 可能 stale 时触发**：close 时不 fetch。理由：close 是 user 主动关，state 不会因为关闭而变 —— 没必要 fetch。**减少不必要 IPC** 是健康 frontend 设计纪律，每次 fetch 都该问"这一刻 state 真的可能改了吗"。
- **R55→R56→R57 三 iter ship feature 后立刻 polish 完整**：R55 backend+frontend feature；R56 加 remaining display；R57 修 stale-state bug。**feature 完整化需要 follow-up iter**。R52→R53→R54 mute 也是同样三 iter pattern (feature → test → polish)。**polish iter 是 feature ship 的隐性配套** —— ship 时不可能想全 edge case，靠后续 iter 补完。
- **codified pattern: refresh on user-interaction entry**：未来加 transient backend state (mood preset / focus level / etc) 时同 R57 模式 —— 任何 popup / modal / panel 打开时 refetch。这个 refresh 模式是 R55 引入 transient state 后的 codified hygiene。

## Iter R56 设计要点（已实现）
- **对称 surface 是 mature UX 标志**：mute chip 显"剩 30m"，note chip 不显—— 这种**不对称** 让用户疑惑"为什么 mute 有时长 note 没有？"。R56 加 transient_note_remaining_seconds 让 note chip 也显时长，**双工具对称完整**。同 user-domain 的相似工具应该尽量对称 surface，不一致 = mental friction。这条原则适用所有 R-series user-control 工具：未来加第三 transient feature 也该有 remaining。
- **镜像 helper 是 R-series codified 模式**：compute_mute_remaining (R52) + compute_transient_note_remaining (R56) 形态、boundary semantics、测试结构都对称。**同 pattern 重复让代码可预测** —— 看一个 helper 知道另一个怎么写。R23 / R34 / R35 / R51 都是这种"镜像 pair" 设计。
- **human-friendly time vs mathematically-precise**：59 秒 round-down 到 0 显示"剩 0m" 看着像已过期，但实际还有 1 分钟前的窗口。Math.max(1, round) 让 last 60 sec 显"剩 1m" — 不精确但符合"还有时间" 的 user perception。**显示数字优先 perceptual correctness 而非 mathematical precision** —— stats card R50 / R51 也是这种思路（< 10 1 位小数 vs ≥ 10 整数）。
- **chip 尺寸动态跟内容调整**：maxWidth 从 240 → 260px 因为加了 ~50px 尾巴。**panel chip max-width 应该跟最长可能内容匹配** —— 太紧 ellipsis 提前破坏，太宽吃别的 chip 空间。这种 micro-tuning 是 polish-phase iter 的常态工作。
- **backend 算 remaining vs frontend 自己 derive**：ToneSnapshot 给 remaining 数字而不只 ISO until。frontend 拿到 ISO 自己 (Date.parse - now) 也能算。但 (a) Tauri ISO 字符串 parsing 有时区/格式可能差异; (b) 跨 multiple frontends (panel + tray + future widget) 都要重复算; (c) backend 一次 compute share。**预 compute 在 backend 是 polyglot system 的稳妥选择**。
- **R56 是 R55 的"配套 polish"**：feature ship 后立刻收尾 surface 完整。R52 mute 跟 R56 note 都做完了 remaining display。**feature ship 不只是 logic 跑通，还要 surface 完整**。后续 IDEA 提醒：每加 transient state 都要 audit "remaining display 有了吗 / chip 跟 button 对称吗 / hover 解释了 lifecycle 吗"。
- **R52→R55→R56 = 5 iter 在 user-control cluster**：R52 mute backend+frontend, R53 mute test, R54 mute preset menu, R55 note backend+frontend, R56 note remaining。**5 iter 在同 cluster 是 R-series cluster 长度新纪录** (之前 max=3)。说明 user-control 是高 ROI 维度 —— 真实痛点解决 + 多个工具叠加生产力。

## Iter R55 设计要点（已实现）
- **mute vs note 是 user-control 双工具**：mute 是 binary block (R52)，note 是 contextual augment (R55)。两者**正交而非互斥**，可叠用。这种"不同工具承载不同 user intent"是 mature UX 的标志 —— 一个万能工具 (e.g. settings flag) 永远不如多个针对性工具好用。**R-series 进入 mature 期后 user-control 应该展开成多种细分**。下一候选：mood preset quick toggle ("让 pet 这小时低调一点 / 活泼一点"), tone preset 等。
- **directive 强度跟来源对应**：一般 prompt hint 是 system inference (e.g. "用户最近多次被忽略" — 系统观察)。R55 transient note 是 user explicit input。文案 "[临时指示] ... 不要怀疑或追问" 比一般 hints 强得多 —— 因为来源 trustworthy。**LLM 看到 prompt 时应根据 source 调 trust** —— 系统 inference 可争议，user explicit 该 obey。这条原则可推广：未来加 user-set 字段时都用 explicit "[xxx]" header 标注 trust level。
- **跨 idiom UI consistency**：R55 popover preset durations 用 30/60/120/240 跟 R54 mute presets 对齐（除 R55 多一档 240 给 longer meeting）。**多个 popover idiom 跨子系统 preset 数值统一** = user 学一套数字概念覆盖多场景。如果 R55 用 15/45/90/180，user 要建立第二个 mental model。
- **transient + persistent 用不同工具**：transient note 是临时 (auto-expire)，persistent state 用 memory_edit 写 ai_insights。**两种数据生命周期对应两种工具** —— 如果都塞 memory，user 要每天清理"假持久"的临时数据。生命周期 → 数据存储位置匹配。
- **outside-click close 跟 R54 idiom 复用**：R55 popover 同样 window addEventListener("click", close)。第二次同 idiom = pattern 进入 R-series stable vocabulary。但**第三个出现时该抽 PetPopover 共享组件**（R39 use-3+ 抽规则）—— Mute menu (R54) + Note popover (R55) 是 use-2，下次再加 popover 就是 use-3+ trigger。
- **R52→R53→R54→R55 是 user-control cluster 4 iter**：4 iter 都聚焦"用户 control pet 的 quick path"。R52 backend feature, R53 test debt, R54 fast+flexible 扩展，R55 第二个 control 工具。**cluster 内分阶段：feature ship → test → extend → 同主题不同工具**。这种节奏比 single-feature-per-iter 更累积，因为 cluster 内 iter 共享 mental model，新工具复用旧 idiom。
- **vertical feature stack 是 mature phase 的高 ROI iter**：R52 / R55 都端到端 backend + frontend + tests。比 polish iter (单纯样式) 投入大但 user-visible value 也大。**polish: feature ratio in mature phase 应该 ~3:1** —— polish iter 数量多但单 iter 影响小，feature iter 数量少但单 iter 影响大。R-series 50+ iter 中 feature iter (R12, R52, R55) ≈ 5-7 个，符合预期。

## Iter R54 设计要点（已实现）
- **fast + flexible 双轨设计 vs 单一 path**：R52 IDEA 写"fast path > flexible path 当 fast path 覆盖大多数用例"。R54 进化为**双轨**：fast path 不换（左键 30min），flexible path 加（右键 menu）。**保留 fast path 的关键** —— 不破坏既有用户的 muscle memory。如果 R54 改成"左键打开菜单"，R52 用户得多一次 click。**进化要 additive，不要 replacing**。Web app 标准：左键 = primary action，右键 = secondary/extended menu。
- **outside-click close 是 popover 必备**：window.addEventListener("click", close) + stopPropagation 内部 → outside click 关。这种 30 行 popover logic 不需要 floating-ui / portals lib。**小 popover 自己写比 lib 经济** —— 同 R39 "use-3+ 才抽" 思路：单 popover 不抽组件 / 不引入 lib。第 2-3 个 popover 出现时再抽 PetPopoverMenu 共享组件。
- **新代码也该 audit codified rules**：第一版我用 onMouseEnter/Leave 给 menu items 改 background。**违反 R41 codified "CSS pseudo-class > React state"**。立即重构为 `.pet-mute-menu-item:hover`。**codified rules 不是只 audit 老代码 —— 写新代码时也该自检**。每个新 React 组件 ship 前问自己：是否用 React state 做了 CSS pseudo-class 能做的事？
- **preset 选择跟使用场景对应**：15/30/60/120 不是随机数。15 = pomodoro 短 block，30 = R52 默认（中等专注），60 = 视频会议长度，120 = 深度工作 block。**preset 数值应该 anchor 在 user 心智模型的"自然时长"** —— 跟 R27 deep-focus 60min 选择同思路（双 pomodoro / 标准 deep work）。
- **同色语义跨 element family**：解除静音文案 #dc2626 红 = 跟 ToneStrip 红色 "🔇 静音中" chip 同色。视觉关联让 user 直觉知道"红色这件事跟 mute 状态相关"。**color = visual taxonomy**，跨 surface 一致让 user 不需要额外 attention 建立关联。
- **`position: relative` parent 是 popover 的隐性需求**：menu 用 `position: absolute` 需要找最近的 positioned ancestor 当 reference。如果父没 position，menu 可能 anchor 到 document。R54 把 🔇 button 包一层 div + relative，menu 就 anchor 到该 div。**popover 布局是 React 写惯的人偶尔忘的 CSS 知识**。
- **"未来抽组件" 候选预留**：R54 popover 是单 caller。如果未来 settings 或其他地方再加 popover menu (e.g. quick mood preset / quick instruction)，第 2 个出现时审视抽 `PetPopoverMenu` 组件可能性。R39 PanelFilterButtonRow 已经走过同样路径 —— **追踪 emerging patterns until use-3+ 出现，再 abstract**。

## Iter R53 设计要点（已实现）
- **test debt 应该 ship 后 1 iter 内还**：R52 IDEA 已经写"R-series 抽 helper + 测"模式（R33 / R23 etc），但 R52 自己偷懒没测。R53 还债。**这种"feature ship → test 跟上"是 R-series 健康节奏**：feature iter focus on shipping，test iter focus on solidifying。两者紧挨着，避免长期 untested code 累积。R29→R30 / R46→R47 都是同节奏。**承认 R52 偷懒并立刻还 比假装"feature 包含 tests" 诚实**。
- **pure helper + non-pure wrapper 是解 global state 耦合的标准**：mute_remaining_seconds() 读 MUTE_UNTIL 静态 + Local::now()，无法测。compute_mute_remaining(until, now) 接两个参数，pure。Wrapper 一行 plumbing，pure helper 承载 logic。**测 logic 不测 plumbing** —— wrapper trivial 不写测，pure helper 5 case 全测。这是 R-series 反复践行的设计纪律 (R33 count_trailing_silent / R23 classify_feedback_band / R20 classify_speech_register / R51 dynamic tooltip computation)。
- **boundary semantics: 到期即解除**：`> 0` 严格不等，`until == now` 时 returns None（不是 Some(0)）。**到期那一刻 mute 应该立刻失效**，不该多撑 1 秒。这条思路一致：R12 daily_review hour ≥ 22 (boundary fires at exact threshold), R7 ratio thresholds (>= 不 >). 每个 boundary case 都该明确写 test 钉死语义。
- **Option<X> 比 sentinel value 总是胜出**：mute "未设置" 用 Option<DateTime> 而非 `DateTime::min_value()` / Unix epoch。Option 是 Rust 表达 "可能没有" 的 idiomatic 方式。**任何 nullable / optional state 都该用 Option，不要发明 magic value**。理由：(a) 类型清晰，(b) 编译器强制 caller 处理 None case，(c) 不会出现 "epoch year 1970 是真的还是 placeholder?" 模糊。
- **500 tests 里程碑反映 R-series 健康度**：平均每 iter +9.4 测。这个比例不是 fluke —— **每个 logic-bearing iter 都该带 tests**，long-running project 的 test count 应该跟 iter count 大致同步。如果 50 iter 后只有 200 测 = test debt 严重；500 = 健康。
- **`use chrono::TimeZone` 在 fn 内 vs module 顶**：测试 helper now_at 用 TimeZone trait，import 限在 fn body 内 (`use chrono::TimeZone;`)。**避免污染整 module namespace** —— 测试用 trait 不该被 production code path 看到。这是 Rust namespace 的 micro-discipline，让 import 范围跟使用范围匹配。
- **Local timezone 测试构造**：`chrono::Local.from_local_datetime(&naive).unwrap()` 是 deterministic 构造法。不依赖系统时钟。**测试时间逻辑必须用 deterministic 构造**，不能 `Local::now()` —— 否则 CI 不同时区机器结果不稳定。这条原则 R12 / R14 / R17 等 daily 系列已经反复践行。

## Iter R52 设计要点（已实现）
- **真实功能 iter 比 polish 更影响产品**：R-series 30+ iter 大量是 polish (chip / animation / focus ring / threshold tune)。R52 是真实 user-need feature —— "focus session shut up"。**polish 让产品 feel right，但解决真实痛点的 feature 才让 user reach for it**。比例上 polish: feature 能保 2-3:1 是健康，1:0 就是停滞。R52 之后该看是否还有未解决的真实痛点（如"快速给 pet 一个临时 instruction" / "马上换 mood register" 等）。
- **user-driven 应该 gate priority 最高**：MUTE_UNTIL 检查在 evaluate_loop_tick 第一行。比 cooldown / quiet hours / awaiting 优先。原则：**用户显式 expression 永远 trump 系统 inference**。Cooldown 是系统判断"该歇"，user mute 是用户判断"我要你歇" —— 后者更强。这条原则适用 settings 类 toggle —— 用户操作产生的 state 永远 win。
- **transient state 不持久化是设计选择**：MUTE_UNTIL 是 in-memory static，重启清零。诱惑是写进 settings.yaml 让 mute "存档"。但 (a) "30 min mute" 重启后还有意义吗？时间已经流逝；(b) 永久 mute 应该用 `proactive.enabled = false` 设置。**transient 名字就承诺了 ephemeral**。这跟 R37 panel filter state 不持久化同思路。
- **fast path > flexible path**：button toggle 不下拉菜单，30 min 单 preset 不可选。理由：90% 用户需求是"快速 mute 30 min"，flexible path 让 90% 用户多 click 一次为 10% 自定义需求服务，net negative。**通过观察现实用例选择 default**，未来 user 反馈说"我想 mute 60 min" 再加 preset menu (R52b)。
- **single source helper 是 R-series 抽象成熟度信号**：mute_remaining_seconds() 同时给 gate (action) 和 ToneSnapshot (display data) 用。R23 (cooldown breakdown) / R33 (count_trailing_silent) / R34 / R35 都同 pattern。**抽 helper 不是为了 DRY，是为了 invariant 不会 drift** —— 两个 caller 用同一函数 = 两者输出永远一致。
- **R-series first integrated feature iter since R12**：从 R12 (daily review) 后大多 iter 是 small polish / signal addition / panel surface。R52 是端到端 feature stack（backend static + gate + 2 commands + ToneSnapshot field + frontend chip + button + lifecycle）。**这种 vertical-stack iter 难做但收益大** —— user-visible action loop 完整。R-series 应该周期性穿插这种 iter，避免长期 polish 让 codebase 不再 "ship features"。
- **chip + button 双 surface 互补**：button 让用户主动操作（control），chip 让用户被动看到状态（feedback）。两者必须同 iter 上 —— 单 button 没 chip = 用户点完不知是否生效；单 chip 没 button = 用户看到状态但改不了。**control + feedback 是 user action loop 的两端**，缺一就破。R52 同 iter 上 button + chip 是"封闭 loop" 的最小单元。

## Iter R51 设计要点（已实现）
- **long-term + short-term 双视角是 trend 揭示的最少工具**：单一 lifetime avg 看不出最近变化（一年数据稀释）。单一 week avg 看不出长期 character（一周可能是异常）。**两个一起 = 对比 = trend 直接可读**。这是金融 / 健身 / 学习 tracker 的共同 idiom：MA-7 vs MA-30 / 周均 vs 月均。R51 把它带进 pet panel。
- **smart denominator 而不是 fixed period**：weekSpeechCount / min(7, days + 1) 让首周 user 拿到正确 avg。如果固定 / 7，day 3 用户 average always 被低估 4×。**首日体验是 onboarding 关键** —— 数据正确性应该 day 1 就准，不能让 user 等 7 天才看到 stable signal。**boundary correctness > formula simplicity**。
- **dynamic tooltip 比 static label 信息密度高**：title 根据 weekAvg vs lifetimeAvg 比例动态生成"更健谈/更安静"文案。比"周 N 天均值"静态模板：(a) 直接告诉用户结论而不是数据；(b) 提供 actionable signal ("最近趋势"); (c) zero extra UI cost (already-present hover)。**dynamic content in static UI shells** 是 panel 信息密度优化的高阶手段。
- **±30% 是 noise vs signal sweet spot**：太严的阈值（±10%）让自然波动都触发警报，太松（±50%）极端 case 才提。30% 是凭直觉拍 + 与 R-series 其他三档 (>60% / <20% / mid feedback band, R7) 的"明显偏离" 阈值大致一致。**跨子系统阈值一致**有助于 user 心智模型构建 ("R-series 的'明显偏离' 大概是 30-40%")。
- **chip 主体简洁，hover 承担 trend signal**：诱惑是给 /周日均 chip 加 ↑↓ 箭头或换色 (绿 = 趋升 / 橙 = 趋降)。但 stats card 已 6 列，加 visual 复杂度让密度过载。**chip body simple, hover does the heavy lifting** —— visual 复杂度 vs 信息密度的 trade-off。R23/R28 cooldown chip + hover 同思路。
- **derived stats expansion 走完一轮**：R-series stats card 现 7 列。R50 + R51 是 derived expansion 一轮 (lifetime avg / week avg)。下一轮可能：speech length avg over time? Daily mood distribution? 但**stats card 信息密度边际收益递减** —— 6-7 列已经接近 visual budget 极限。后续 derived stats 可能放进 expandable section / chart 而不是 chip 行。
- **panel design = "多视角同数据"**：R50 / R51 / R23 cooldown breakdown / R26 feedback aggregate hint 都是同一原则的不同 surface —— 同 raw data 用 ratio / band / trend / aggregate 多视角呈现。**Mature panel 不是 "raw data dump" 而是 "perspective interface"** —— 让 user 选择"我现在想从哪个角度看"。这是 R-series mature 期 panel 设计的核心审美。

## Iter R50 设计要点（已实现）
- **派生统计是 panel 高阶维度**：raw counts (today/week/lifetime) 是基础数据。avg = lifetime/days 是**它们之间的 ratio**，揭示 base data 不能直接显示的特性 ("intensity of engagement")。**panel 设计成熟期应该多投资 derived stats**，不只 surface raw counters。R-series 之前的 panel chip 都是 raw signal (count / boolean / category)，R50 第一次显式加 derived ratio。后续候选：speech length avg / day 内 hour 分布 / topic frequency rank 等。
- **精度跟范围匹配**：小数值（< 10）保留 1 位小数，大数值（≥ 10）取整。原因：**0.5 vs 1.5 vs 2.0 对小数 avg 有 meaning**（区分"基本不说" vs "每日固定一两次" vs "频繁"），而 23.4 vs 23.0 几乎相同感受。这是统计 readout 设计的常见 idiom —— "absolute precision" 不如 "perceptually meaningful precision"。
- **色彩归属应表达"主导维度"**：avg 的色用 teal（陪伴日数同色）而非紫（累计同色）。原因：avg 的 character 由分母（陪伴天数）决定 —— 同样累计 100 次，30 天 vs 365 天的 avg 性质截然不同。**color reflects the "shaping factor"** ——不是简单"derived from 哪个 column"。这是 panel 视觉语义的细致设计。
- **zero-state hide 优于 fake 0**：companionshipDays = 0 时 "/日均" 列直接 hide。如果显 "0 /日均" 或 "Inf /日均" 都是 misleading 第 0 天数据。**面板列应当在数据无意义时整列消失**，而不是显错误数字。这条原则跟 R45 "👋N 仅 N>0 时显" / R26 "dismissed=0 时省略 segment" 一致 —— **zero-state visual 应当 invisible**。
- **panel 列顺序表达数据维度**：今日 → 本周 → 累计 → /日均 → 前开口 → 陪伴。这个序列是 short → medium → long → derived → instant → totale。**panel 列顺序是隐性 information architecture** —— 用户从左到右读，应该感受到数据维度的递进。如果 derived 放最前 / instant 放最后，序列变 chaotic mental scan。
- **R-series mature 期的细致信号**：R50 不引入新概念（speech_count / companionship_days 已存在），只组合现有数据加新视角。**mature phase 价值在重组、抽 ratio、加 derived view** —— 不一定要新增数据流。每个 backend 数据如果有多个可显视角（raw / ratio / band / trend），panel 应该都给。R23 cooldown breakdown 也是这种 reorganization。
- **Live2D cluster 暂搁置**：R49 改了 loading status 是 Live2D cluster 起点，但 R50 转 PanelStatsCard 而不是继续 Live2D 内部。**polish cluster 切换不必须 3 iter 完成 —— low-priority cluster 可以"开始即结束"**。Live2D 内部动画风险高、价值有限（Live2D 库自带 idle motion），R49 单 iter loading-status 改进已是该 cluster 实际能做的全部。**cluster size 应跟内容匹配，不是固定 3 iter**。

## Iter R49 设计要点（已实现）
- **dev message leaking to user 是隐性反模式**："importing pixi.js" 是 dev-mode 诊断输出，但被 setStatus 直接显给 user。**在 dev 期写的临时 status 文案 上线前应该 audit 替换** —— "对开发者 vs 对用户" 两个 audience 用同一 string 不合适。R49 第一次 codify 这个反模式。其他 components 该 audit：是否有 "loading from API…" / "checking cache..." 等技术语言直接 surface 给 user？
- **internal data / display value 二元 = audience separation**：R49 保留 `status` 用作 dev-visible state，`displayStatus` 是面向 user 的 transformed value。**两个 audience (dev / user) 看的应该是不同的 representation**。R23 cooldown breakdown 也用同样 idiom (internal `cooldown_breakdown` data 跟 hover-displayed math)。**应用一致**：anywhere 数据精确度 vs 用户友好度 trade-off 时，存 raw + derive friendly。
- **system event timing 偏慢 vs user-action timing 偏快**：R49 fadeIn 240ms vs R40 ChatBubble 220ms vs R41 :active 80ms。粗略规律：system spin-up 250-300ms（"加载需要点时间，理所当然"），user-triggered animation 200-250ms（"我做了什么应快速反馈"），press feedback 50-100ms（"我刚按下应即时"）。**timing 跟 action causality 对应** —— user 知道是自己触发的事件应快，system 自发事件可慢。
- **Error 文案不友好化**：保留 `Error: ${err.message}` 原始 detail。**friendly text 适合"成功流程"，error 适合"actionable detail"**。如果 error 也变成"出错了，请稍后再试"，user 失去诊断信息（"是网络？是文件？是权限？"）。错误是 user 唯一需要技术信息的时刻。
- **polish cluster 从低风险面开始**：Live2D 主体涉及 cubism4 库 + canvas 绘制，risky。R49 选 loading status div 这个**界面包装层** —— 零侵入 Live2D 内部。**Live2D 内部动画调整（model.motion / parameter 调用）该留给以后专门 iter** 或者根本不动（pet 已 functional）。Polish cluster 应当先动外围再动核心。
- **保留 6 个 setStatus 中间 stage**：诱惑是缩成 setStatus("正在唤醒…") 单 call。但 dev 需要 "stuck on which stage" 的诊断。**view 层简化 ≠ data 层简化** —— 同一原则在 R23 cooldown breakdown / R45 unread badge cap 都践行。两者各自服务不同 reader。
- **Live2D cluster 候选 R50/R51**：(a) idle breathing animation via Live2D model parameter (PARAM_BREATH)？(b) tap interaction visual feedback（pet 被点击后 scale 短暂）？(c) hover gaze（pet 看向鼠标位置）？这三都涉及 Live2D model API。**风险 / 价值匹配** — 价值最高是 hover gaze（让 pet 显著"活" 起来），但 risk 也最高（gaze 实现复杂）。如果 R50/51 做 Live2D 内部，需要先调研模型 API 再决定。

## Iter R48 设计要点（已实现）
- **stagger 比例决定 perception 不是任意拍**：3 dots 错峰 0.18s，跟 1.2s period 的 1/6.7。这让"波纹从左到右流动" 视觉清晰。如果用 0.4s 错峰，3 dots 接近同时（差距 < 1/3 周期），视觉退化成"3 dots 同时跳"。如果用 0.05s，差距太小，看不出错峰。**stagger 应该 ≈ 1/N period** (N = dot 数) 形成流动感，N 多了用 1/(2N) 让流动更密。这是 ambient cascade 设计的经验数字。
- **multi-dimensional motion > single-dimensional**：dots 用 opacity + translateY 双维。opacity 单维像 "fade in/out 闪光"；translateY 加上让它像"心跳起伏"。**两维让 motion 更有机** —— 单维容易显 "machine"，多维显 "alive"。但 dimension 加多了反而 chaos —— 3 维 (opacity + scale + rotate) 会显躁。Two is the sweet spot for "alive but not chaotic"。
- **ambient timing 跟 wait-state 紧迫度对应**：R44 tab idle ambient 用 1.6s（pet 等了很久，节奏慢悠悠"我在这，不催你"）。R48 chat input loading 用 1.2s（user 主动等 AI 回复，节奏稍紧"我在帮你想"）。R-series ambient 已经有两种节奏 —— **timing 是 mood signal**（R44 IDEA 已 codify），R48 在 active vs idle 维度践行。
- **brand color 反复用 = identity**：#38bdf8 又一次（focus ring R46/R47 / tab gradient R43 / 各处 accent / 现在 loading dots）。**单一色调跨多 surface = visual identity 累积** —— 每个新元素用同色 reinforce 整体感。如果给 dots 用 #ec4899 粉色看着醒目但破坏 R-series 蓝调一致。R47 IDEA codified, R48 应用。
- **R-series 第二个 cluster 完成**：R40-R42 bubble cluster (3 iter)，R43-R45 tab cluster (3 iter)，R46-R48 ChatPanel cluster (3 iter)。**polish phase 的 stable cadence**：每 cluster 3 iter，每 cluster 一个 component。下一 cluster 候选：Live2D character (动画 / hover gaze / 空闲呼吸)？Settings panels deeper polish？还是 ChatPanel 的 chat 历史 view？Live2D 难度高但价值最大。
- **isLoading visual 不挤占 textarea space**：dots 在 textarea 跟 ⚙ 之间，padding 0 6px。textarea flex:1 自动占余下空间。**isLoading 出现时 textarea 微缩 ~17px** (3 dots × 5px + gap + padding) —— 不破坏 user 当前在打字状态。alternative 是 dots 浮在 textarea 上面或 absolute positioning，但 absolute 会跟 cursor 重叠。inline flex 是稳妥选择。
- **conditional render = zero non-loading cost**：dots 只在 isLoading=true 时 mount。`!isLoading` 时 component 完全不存在 DOM 中，CSS animation 也不跑。**避免 always-render + display:none** 隐藏 — 后者 animation 仍会跑（即使不可见 GPU 也工作）。conditional render 是 React 的省电默认。

## Iter R47 设计要点（已实现）
- **descendant selector 是 CSS scope 的强项**：给整个 panel 加 className，CSS rule `.root input:focus { ... }` 自动 cover 所有 child inputs。**跟 className-per-input 比省 N 倍 edits** —— 10 个 inputs 变 1 个 className edit。也未来友好：新加 input 自动被 cover。这是 CSS scoping 比 utility-class (Tailwind) 强的场景之一 —— 后者要给每个新 element 重复加 class。
- **outline: none + :focus replacement 是正解，不是删 outline:none**：诱惑是"删 outline: none 让 browser default 回来"。但 default outline 跨浏览器不一致（Chrome 蓝色 / Safari 黑色 / Firefox dotted），跟 R-series 极简风格冲突。**保留 outline:none + 加 :focus replacement** = 控权 + visual identity 一致。这是 "explicit 优于 implicit" 在 CSS focus state 的应用。
- **R20 / R29 / R46 codified rule 都需要 audit pass**：每个新 codified rule 上线后通常需要 1-2 iter 的 audit-and-backfill。R20 → R21+R22；R29 → R30；R46 → R47。**这是 R-series 的稳定 cadence**：codify rule → 立刻应用一次 → 下个 iter audit 全 codebase 找其他违反 → backfill。**rule 不只是规范未来，也回头清理过去**。
- **focus ring color #38bdf8 + alpha 0.18 跨 component 一致**：visual identity 通过"反复使用同一颜色规格" 建立。R-series 蓝色调 (#7dd3fc / #38bdf8 / #0ea5e9) 在多处出现 — tab gradient / active app chip / negative band / focus ring 等。**重复 = identity** —— 用户在不同 surface 都看到同色调，潜意识感觉到"这是同一个 app"。
- **修 accessibility hole 跟 polish iter 一起做**：R46 + R47 都是 polish iter 但本质是修 accessibility issue (focus ring missing)。**polish 不该排除 accessibility audit** —— 反而是好 occasion 顺手修。Polish iter 的 expanded definition: "visual + functional + accessibility tweaks，所有 user-facing surface 改进"。
- **`input:focus` 的 box-shadow 比 outline 在 border-radius 上更可靠**：outline 在某些浏览器跟 border-radius 不 follow（变成方框）。box-shadow 0 0 0 2px 自动 follow border-radius —— 圆角 input 的 focus ring 也是圆角。**box-shadow focus ring** 是现代 web app 标准；outline 留给 system context menu / forced colors mode fallback。这条 R46 IDEA 已 codify，R47 跨 3 component 应用更稳固。
- **transition 跨 component 一致 (150ms ease-out)**：R46 + R47 三处 focus transition 都用 `border-color 150ms, box-shadow 150ms ease-out`。**transition timing 一致 = focus 进入感统一** —— 用户在 ChatPanel 切到 PanelSettings 切到 PanelChat focus 都是同一种"软进入" 节奏。如果各处 timing 不同（150 / 200 / 300）跨 panel 会感到 inconsistent。

## Iter R46 设计要点（已实现）
- **新 cluster 起点 = audit 旧债 + 加新 polish 一起**：R46 是 ChatPanel cluster 第一 iter。借机把"⚙ JS hover 没改 CSS" + "textarea outline:none 无 replacement" 两个旧债一起还，外加 R-series codified visual recipe 的应用。**cluster 起点 iter 比中间 iter 更适合 audit-and-fix combination** —— 因为开始触碰这 component 时 mental model 最 fresh，旧 issue 能被一并发现。中间 iter 倾向于 single-focus polish。
- **`outline: none` 是危险默认**：诱惑是设 `outline: none` 让 textarea "干净 default 视觉"。但 **stripping browser default 必须 replacement** —— 否则失去 keyboard accessibility (Tab 键无法看到 focus 在哪)。`outline: none` + 无 replacement 是常见 frontend 反模式。**累积下来的 R-series 类似的潜在 issues 该 audit 一遍**：还有哪些组件用了 `outline: none`？是否都有 replacement？
- **box-shadow focus ring > outline focus ring**：outline 在圆角 element 上某些浏览器还是矩形（特别老 Safari / Firefox）。box-shadow 0 0 0 Npx 自动跟随 border-radius，圆角 textarea / button 都 OK。alpha 软光晕比纯实色 outline 看着 polished。**现代 web app focus ring 标准是 box-shadow**，outline 留给 system context menu / 高对比模式 fallback。
- **inline `<style>` + className 模式跨三处践行**：ChatBubble (R40-R42), App.tsx tab (R43-R45), ChatPanel (R46) 都用这个 pattern。**stable vocabulary** = visual recipe 跨 codebase 一致。但 R-series 也要小心**"vocabulary 化"不等于"抽组件"** —— 共享 recipe ≠ 必抽 React 组件。recipe 是 mental pattern，组件是 reuse mechanism。R39 抽 PanelFilterButtonRow 是因为有 3 caller 共享 props 设计；R46 不抽是因为 styles 跟具体 element 紧耦合，"抽 ChatPanelStyles" 没 reuse 价值。
- **批量修 cluster 同 component 多 issue 比串行**：R39 (extract + 3 caller refactor 同 iter), R30 (4 settings field 同 iter), R32 (2 dead code 同 iter)，R46 (2 ChatPanel issue 同 iter)。**当 issues 在同一 surface 内、互相非独立时，1 iter 解决多个**。理由：(a) 只读这 component 一次；(b) commit history 不被同 file 多个 commit 撕碎；(c) 改一个 issue 可能 affect 另一个的 best fix。**single iter / single concern** 是 base rule，但 single iter / multi-issue-same-surface 是合理 exception。
- **CSS pseudo-class > React state 又一次践行**：⚙ hover 从 React state-mutation 改成 CSS pseudo-class。**这条 R41 codified 规则在 ChatPanel 也应用** —— 表明 codified rule 跨 component 一致是预期。R-series 后期 polish 期就是这种"应用 codified rule 还债" 多于"创造新 codified rule"。
- **focus ring color 跟 brand 协调**：#38bdf8 是 R-series 一直用的天蓝（tab gradient / 各处 accent）。focus ring 用同色调 = visual identity 统一。**polish iter 应该 reinforce visual identity 而非 introduce 新色**。如果用 #ec4899 粉色 focus ring 看着醒目，但破坏 R-series 蓝色调一致。**风格 inertia** 在新 polish 里持续胜出。

## Iter R45 设计要点（已实现）
- **polish iter 不限于 cosmetic**：R40-R44 都是纯 visual polish (animation, hover, ambient)。R45 在 polish cluster 里加了**新功能** (unread badge + 计数 + lifecycle 处理)。R-series codified "polish 期 iter 类型多样" 但都默认 visual。R45 突破这个误判 —— polish iter 也可以是 functional。原则是：**polish iter 应该承接 cluster 的 visual coherence**，但不强求 zero-feature。R45 在 tab cluster 里加 badge 是"扩展 tab 这个 component 的 affordance"，跟 cluster 主题一致。
- **useRef + useEffect 同步是 listener 长寿正解**：useEffect listener 注册一次（mount）但 closure 里 `hidden` 永远是 mount 时的值。如果让 useEffect deps 包含 hidden，每次 hidden 变化 listener 重 subscribe → 中间到达的 events 丢失。解法：`useRef` 持久化 + 单独 useEffect 同步 ref.current = hidden。**这是 React closure trap 的标准解法**，每个长寿 listener 处理 frequently-changing state 都该用。也可以用 zustand / valtio 等外部 state 库，但单 component 内 useRef 最轻。
- **state-driven > event-driven cleanup logic**：清零 unread 的两种实现：
  1. `mouse-enter` event handler 内 setUnreadWhileHidden(0)
  2. `useEffect(..., [hidden])` 内监听 !hidden 后 reset
  
  选 (2)。理由：mouse-enter 可能 fire 多次（user 频繁 hover 边缘），event 也可能跟 state 不同步（mouse-enter 触发 unhide 但 unhide 实际由 useAutoHide 内部 logic 决定）。**state-driven cleanup 更稳** —— state 是 single source of truth，cleanup 跟 state 同步天然正确。
- **industry convention 优于独创**：red unread badge 是 iOS/macOS/Windows 通用 visual。诱惑是 "R-series 极简风格，用蓝/灰更协调"。但极简风格不应该违反广泛认知 —— red badge = "新东西要看" 是 lifelong-learned visual language。**风格 inertia 让位于 universal mental model**。这跟 R42 hover 不加 box-shadow 的思考相反 —— 那是因为 R-series 内一致更重要；R45 是因为 universal pattern 优先。**何时听内部 inertia，何时听外部 convention 是品味判断**。
- **9+ truncation 表达"too many"**：badge 显 "11" vs "9+"，前者精确但占 2 字宽；后者损失精度但视觉紧凑 + 传 "嗯多到该回去看看"。**panel chip "信息密度" 跟"精度"的取舍**：dashboard 数字普遍 chip 化 (👋3 / 📏 长 / 🤐 沉默 ×N) 都是精确数字。Badge 是 attention-grabber，不是分析数据 —— 9+ 模糊化 acceptable。
- **listener mount-once 模式**：`useEffect(async () => { unlisten = await listen(...); }, [])` 是 React + Tauri 的标准 listener 注册模式。R-series 多处用（useChat, App.tsx 各处）。**这是 stable mental model**——长寿 listener 配 useEffect empty-deps + cleanup return，frequently-changing state 配 useRef sync。两个 piece 一起就解决"listener 看到最新 state 但不重 subscribe" 的痛点。
- **R-series 第一次在 polish 期加 user-facing 行为**：R45 不只是动画 / refactor / threshold tune。它实质让 pet 在 hidden 期间的"behind the scenes" speech 变 surfaceable。**polish phase 不该一刀切排除 functional**。判断：feature 是否扩展 cluster 主题？是否 small & contained？R45 答都是 yes —— badge 是 tab 的 affordance 扩展，30 行代码内含。Polish iter 加 functional 的 budget = 跨 component 改动小、scope 不蔓延。

## Iter R44 设计要点（已实现）
- **ambient animation = 新动画维度**：R-series 之前的动画都是 *event-driven* — mount fadeIn / hover lift / press scale 都响应特定 event。R44 是 *持续运行* —— infinite loop，独立于 user action。**ambient 是 long-running UI 必备**，让"安静等待中" 的 visual 不至于死板。比如 OS 系统的 Spinner / Pulse light / Cursor blink 都属此类。
- **subtle magnitude > dramatic magnitude**：translateX(-2px) 是 1/3 箭头宽度。诱惑是 -4 / -6 / -8 让动作明显。但 ambient 的本质是"持续低强度提醒"——动作太大变成"持续高强度干扰"。**用户应该几乎不察觉 ambient 但视觉皮层处理到了**。这种"sub-conscious attention" 是 ambient 设计的核心。
- **timing = mood**：0.5-1s 节奏 → 焦虑 ("hurry up")；1.5-2s → 等待 ("ready when you are")；3-5s → 沉睡 ("I'm here but no rush")。R44 选 1.6s 表达"在等你但不催你"。**ambient 节奏选择直接传 mood signal** —— 比文字解释快 10 倍。
- **ease-in-out for organic vs linear for mechanical**：bob 用 ease-in-out 加速度变化让运动像 "subtle swing" / "呼吸节奏" 而非 metronome ticking。**机器人运动用 linear，活物用 ease-in-out** 是动画 design 的 binary 直觉。Pet 是活物，所有动画都该 ease。
- **state 优先级：hover > ambient**：hover 期间 animation-play-state: paused 暂停 bob。如果不暂停，bob translateX 跟 hover 加宽叠加，箭头 drift 出 tab 边界 visual 错乱。**explicit state 比 ambient state 优先级高**——user 行动时 ambient 让位。同样适用未来设计：focused element 暂停其 ambient pulse / wallpaper 动态在窗口前不显眼等。
- **`animation-play-state` is the right primitive**：不需要 JS 控制 animation 的 play/pause。CSS pseudo-class + animation-play-state combo 是 native 一行解决。**了解 CSS 高级 primitive 让 React state 减负**——这是 R41 IDEA "CSS > React state" 原则的延续。
- **修饰元素 bob 不抢戏**：tab 主体（背景 gradient 渐变 box）静止，仅内部箭头 bob。**ambient 应该作用在"次要 visual"** —— 让 user 感知 ambient 但不让他视觉中心转移。如果 tab 主体 pulse，会让"用户找回 pet 入口"变成"pet 在自己跳舞" — 角色错位。
- **R44 是 R-series first infinite animation**：之前所有 keyframes 都是 fadeIn 一次性。infinite ambient 是新工具加进 vocabulary。**未来候选**：bubble 在等用户回复 N 秒后微微 pulse？panel 等数据加载时 spinner？需要 "持续等待" 状态的地方都可考虑。但 R-series 极简风格 = 用 ambient 要慎，太多反而 noise。

## Iter R43 设计要点（已实现）
- **inline `<style>` + className pattern 跨 component 复用**：R40-R42 在 ChatBubble.tsx 用了"inline `<style>` 嵌 component-scoped CSS" 模式。R43 把它复用到 App.tsx 的 inline tab JSX。**pattern 复用 ≠ 一定要抽组件** —— R39 抽 PanelFilterButtonRow 是因为 3 caller 同样形态。R43 的 Tab inline JSX 只有 1 caller，pattern 复制即可。**当 visual recipe 跨组件相似但实现细节不同时，复制 recipe 比抽组件经济**。
- **transform 已用就别再加 transform**：tab 用 transform: translateY(-50%) 居中。诱惑是用 transform: translateX(-100%) 做滑入。但叠加 transform 容易冲突 — keyframe 写 `transform: translateX(-100%)` 会覆盖 translateY(-50%) 让 tab 失去居中。**用其他属性 (left)** 做滑入避免冲突。这是 CSS animation 的 footgun pattern — animation 的 from/to 属性会替换整个 transform，不是 merge。
- **timing 微差表达 component 角色**：bubble 220ms = "活物开口"，tab 280ms = "system 元素摆放"。差 60ms 不是任意拍 — 是想让 user 感觉到 bubble 比 tab "活" 那么一点。**timing 是 visual 的语气** —— 同样是 fadeIn，宠物的略快略轻盈，UI chrome 的略缓略机械。
- **hover affordance：形状变化 > 颜色变化**：诱惑是 hover 时换 tab 颜色加深。但 tab 已经有 gradient (#7dd3fc → #0ea5e9)，加深会让 visual 变重复杂。形状变化（width 16→22）是更直接的 "召唤"，跟 native scrollbar / file dropdown handle 的 hover 一致。**形状是 affordance 的第一语言**，颜色是次级强化。
- **+37.5% width 是 hover 加重的甜蜜点**：太多增（30→50）让 tab 喧宾夺主。太少（16→18）user 察觉不到。20-25% 觉察 + 不抢戏 — R43 选 22 (37.5%) 偏多一点考虑这是"找回 pet 的关键入口"，应该比普通 hover 更勾人一点。**hover 强度跟 affordance 重要性匹配** —— 关键入口可比普通 hover 更显著。
- **不抽组件的纪律**：Tab 30 行 JSX 在 App.tsx 内，0 state，0 复用。抽 `<TabIndicator />` 会引入 prop drilling (hidden 状态从 App 传入) 但 0 收益。**< 50 行 + 单 caller + 0 state 不抽** 是 React-component 经济。R39 抽 PanelFilterButtonRow 是 3 caller + 共享 props 设计，那种 case 才值得抽。
- **polish cluster 流转**：R40-R42 bubble cluster (3 iter) → R43 tab cluster start。**polish 期连续 cluster 切换是健康节奏** —— 每 cluster 收 endings 完整再换下一个。后续可能：R44 tab 再加 1 个 micro-state（如 active press）→ tab cluster 完成 → R45 转 Live2D 或 ChatPanel。**clusters as iter unit** 是 mature phase 的高阶抽象 —— 不只 single iter 闭环，还有 cluster 闭环。

## Iter R42 设计要点（已实现）
- **interaction state machine 完整 = polish 完成**：4 micro-states (idle / fadeIn-mount / hover / press) 每个有自己的 visual transition。这是 desktop UI 的 mature interaction model —— 缺一种状态都会让 affordance 模糊。R40-R42 三连 iter 把 bubble 从"显示文字的盒子" 升级到"可交互的活物"。**polish phase 的"完成" 标志 = state machine 的 N 状态都有 visual signature**。
- **transition timing 跟 metaphor 匹配**：transform 80ms 快（press 是物理瞬时反应），border-color 120ms 慢（hover 是视觉渐进强调）。两者不同 timing 让 affordance 自然区分 "我感觉到了" (fast) vs "我变得显眼了" (slow)。**timing function 不是装饰参数，是 metaphor 的物理建模**。新加 transition 时该问"这个变化是物理动作还是视觉强化？"。
- **CSS source order 是 specificity 决战的第二维度**：:hover 和 :active 都 1 specificity unit。同 source 文件中后写赢。让 :active 排 :hover 之后 = press 期间 transform 取代 hover translateY 的自然 cascading。**了解 CSS specificity tie-break by source order 让多 pseudo-class 互动可控**。如果 :active 写在 :hover 前，press 时 hover translateY 会赢，press scale 失效。
- **风格 inertia 是 visual coherence 护城河**：诱惑是 hover 加 box-shadow 让 bubble "浮起"。但 R-series 早期决策"无 boxShadow"，加 shadow 破坏 visual identity。**新 effect 之前先 audit "R-series 一贯做法是什么"** —— 风格统一比每处局部最优更重要。translateY(-1px) 是符合 R-series 极简风格的 lift 实现。
- **R40+R41+R42 cluster = depth > breadth 验证**：R41 IDEA 提了"polish phase 选 component 投 2-3 iter 直到完整"。R-series 30+ iter 中第一次践行这条 — 三连 iter 都聚焦 ChatBubble.tsx 一个文件。**结果：bubble UX 在 3 iter 内从 functional → polished → interactive**。如果分散投资（R40 在 bubble，R41 在 panel chip，R42 在 settings），每处都半成。**集中投资在 polish 阶段比 innovation 阶段更重要** —— innovation 时分散加新轴，polish 时集中收 endings。
- **interaction state machine 的 4 micro-affordances**：fadeIn (宠物开口) + hover (你看到了) + press (你点了) + dismiss (你说不要)。**每个状态都传达不同的 social signal** —— bubble 不只是显示文字，是"宠物 - 用户"两端互动的 visual mediator。这种 "every state has meaning" 设计比"加几个动画装饰" 深一层 —— 让 user 感觉到 reciprocity。
- **下一 polish cluster 候选**：bubble 完整后，下一个 3-iter cluster 该选哪个 component？候选：(a) ChatPanel (聊天主面板) — 输入聚焦 / 发送动画 / message bubble。(b) Live2DCharacter — tap motion / hover gaze / 空闲呼吸。(c) Tab indicator (auto-hide tab) — hover / pulse / drag。Live2D 最高价值（pet 主体），ChatPanel 次之，Tab 最低。**polish cluster 的优先级 = 用户停留时间 × 视觉重要性**。

## Iter R41 设计要点（已实现）
- **CSS pseudo-class 是 native UI feedback 的极简正解**：press feedback 一种实现是 React state (`isPressing` + onMouseDown/Up listeners)，另一种是 CSS `:active` pseudo-class。React state 涉及 re-render + 多 event handler 写法。CSS `:active` 是浏览器 native — 0 JS 开销，0 state machine。**当 native CSS 解能实现需求时，don't reach for React state**。这条原则适用所有 hover / focus / active 等纯 visual 状态。
- **subtle 动画的双重 budget**：duration + magnitude 都要小。R41 是 80ms × scale(0.97) — 时间短 + 幅度小。如果 200ms × scale(0.92) 就会变成"按钮被按瘪"。**这两个维度 multiplicative**：duration 长 + magnitude 小可接受（缓慢柔和），duration 短 + magnitude 大也行（快速回弹）；duration 长 + magnitude 大 = 卡顿臃肿。R-series polish 从来都选 small × fast。
- **CSS animation + transition 共存的 transform 协调**：fadeIn (R40) 用 `transform: translateY(...)` via animation，press (R41) 用 `transform: scale(...)` via :active rule。两者都改 transform 属性。但 animation 只在 mount 后 220ms 内 active，之后 transform 回归 inline style 的 base value (translateY(0))。`:active` 期间 pseudo-class CSS 覆盖 inline style — scale(0.97) 取代 translateY(0)。**CSS specificity ordering 自然解决冲突** — 不需要手动协调。
- **user-visible polish 应该连续 cluster**：R40 fadeIn → R41 press feedback 是连续 2 iter 在同一 component 上叠 polish。**polish 投资分布应该 cluster 而不是 scatter** —— 一段时间深耕一处比每周散投更有累积感。R-series 后续 polish 也该按这个节奏：选定一个 component / view，连续 2-3 iter 集中投入直到完整再换下一处。**深度 > 广度** 在 polish 期。
- **R-series 之前的"假交互"债**：R1b dismiss + R24 ✕ 角标 + R40 fadeIn — 这些都说"bubble 是 interactive"，但 R41 之前**点击时没有 visual press**。功能链完整但触觉环节缺。**discoverability triple (function / feedback / discoverability)** 之前以为 R24 完成了，R41 又补一层 ——"discoverability" 不只视觉提示也包括"按下时的反馈"。R-series 的 codified principles 在 polish 期反复 audit 出新债。
- **className "pet-bubble" 不会碰名**：诱惑是 `bubble:active`、`pet:active` 等更短 selector。但 codebase grow 后命名碰撞概率增加。`pet-bubble` 是 namespace + 用途 双关 — pet 是 product 名，bubble 是组件名。**naming with namespace prefix** 是大 codebase 的小代价。
- **R40 + R41 验证 inline `<style>` 模式**：R40 单一 keyframes，R41 加 :active rule。两者复用同一 `<style>` tag （rename const + 扩展内容）。**inline style scope 越用越值** — 比拆 CSS file 局部 + 比 CSS-in-JS lib 轻量。但只适合 short / 共关联的 styles； 长 CSS 仍该拆 .css。

## Iter R40 设计要点（已实现）
- **invisible signals 期投资到 visible UX 期的转换**：R20-R39 主要在打磨 *invisible* 系统 — prompt hints, panel chips, codified rules, signal mirroring。这些都是 dev-facing observability 或 LLM-facing context。**真正的 end-user 看到的 UX 几乎没动**。R40 是 conscious 转向 — 220ms fadeIn 是用户能直接感觉到的差别，不是 panel chip / prompt hint。**长 iter 系列应该周期性回头投 user-visible polish**，否则 codebase 越来越聪明但用户看不出。
- **物理直觉驱动动画 timing**：220ms 不是 magic number — 100-150ms = perceptible but feel "snappy"，200-300ms = perceptible "settle" 节奏，>400ms = 拖沓。ease-out (开始快，结束缓) 模拟"物体被放下"的减速。**timing function 选择应该匹配 metaphor**：bubble 是被"放下"，所以 ease-out。如果是被"扔上去"用 ease-in。
- **subtle > dramatic in pet UX**：translateY(4px) 是 minimal 偏移。诱惑是 8-16px 让动画明显。但宠物的视觉调性是"轻盈陪伴" — dramatic motion 像 system notification toast，破坏角色感。**polish iter 的克制是品味标志** —— 大幅动画看着用心但实际让 UI 显 cheesy。
- **mount animation cheap，unmount animation expensive**：CSS animation 在 React mount 时自然跑一次。Unmount 需要 framer-motion / react-spring 等库介入（必须延迟 unmount 等动画完）。R40 选 mount-only 是性价比 sweet spot。**dismiss 是 user-initiated abrupt action — 立刻消失反而 feels responsive**。这条原则适用所有"出现/消失" 动画：appearance 配 fadeIn，disappearance 看场景（user-driven 该立即，system-driven 可 fade）。
- **inline `<style>` 是 component-scoped CSS sweet spot**：不需要 CSS-in-JS 库（emotion / styled-components），不需要全局 .css 文件。React 18+ 多 instance 的 `<style>` 自动 dedupe（同样 children 不重复插）。**适合简单 keyframes / minor styles**。复杂 case 才上 CSS-in-JS。
- **R-series 进入 mature 期的 hallmark = 投资分布多元化**：early R-series 几乎全 prompt + observability。R32 (cleanup) / R36 (threshold retune) / R37-R39 (interactive panel) / R40 (UX) 是不同 investment direction。**长 iter 系列应该 visible 看到投资方向 diversify** —— polish 期 iter 类型多样比 innovation 期单调更健康，因为不同 dimension 都该被覆盖到。
- **animation as system signal**：bubble fadeIn 不只是装饰 —— 它告诉用户"pet 这一刻活了" 的 visceral signal。R-series 一直在 signal 上做文章但都是 *information signal*。R40 的 fadeIn 是 *embodied signal* — 通过视觉运动传达 "存在感"，比文字描述"宠物活着" 直接 10 倍。后续 polish iter 可以考虑：bubble dismiss 时 Live2D motion ?  user 输入时 pet 转头 ? 等 embodied signal upgrades。

## Iter R39 设计要点（已实现）
- **lazy abstraction (use-3+) 第一次真正落地**：R32 IDEA 写"等到第 3 次重复再抽组件"，R38 IDEA 写"3rd timeline filter triggers extraction"，R39 当真做了。**纸上规则到实战践行的滞后**：从 R32 (cleanup iter 写 nudge) 到 R39 (实际 follow rule) 隔了 7 iter。R-series codified rule 的有效性 = "我以后真按这做吗"。R39 通过这关。
- **generic on V<extends string>**：TypeScript generic 让 component 通用但 caller 保 narrow union ("all" | "Spoke" | ...)。如果用 plain string，caller 会失去 exhaustive matching 的安全。**generic 是 reuse + type-safety 双赢**——不要因为复用就退到 lowest common denominator (string)。
- **caller 注入 accent vs component 内置色板**：generic component 不绑定具体语义。三个 caller 各自有自己的 kind→color mapping（feedback、decision、tool_risk），accent 由 caller 传入让 component 在不同 timeline 都正确。**如果 component 内置 "spoke = green / silent = purple" 等 mapping，第 4 个 caller 出现时要改组件**。注入方式让 component frozen-in-place。
- **职责分离：row 给组件，body 留 caller**：empty filter 的"暂无匹配"文案 caller 自己渲染。如果组件统一渲染，三 caller 文案差异就要 prop 传文案 → 接近 "整个 list 渲染都进组件"。**抽得太多反而绑死**。R39 选择"组件管 row 一致性，caller 管 list 多样性" 是 sweet spot。
- **R-series first shared component**：NumberField 是 R-series 之前就有的，PanelFilterButtonRow 是 R-series 第一个抽出的。**长 iter 系列的"首次抽组件"是里程碑** —— 之前所有抽象都是后端 logic helper（read_ai_insights_item, classify_speech_register, count_trailing_silent 等）。前端组件抽象更费心思因为涉及 prop 设计 + style flexibility。
- **3 caller 同步重构是 high-leverage iter**：R39 一个 iter 同时做 (a) extract component, (b) refactor 2 existing callers, (c) add 3rd caller。三件事一起上比分 3 iter 各做 1 件干净 —— 因为 component design 必须满足 3 caller 的 union of 需求；单独抽出来再加 caller 容易让 component miss 某 caller 需求 → 二次返工。**多 caller 同步重构验证 component design**。
- **不写组件单测的纪律延续**：纯 presentation, no state, no logic branches。tsc 验类型，三 caller 渲染验集成。R21 / R25 / R28 / R31 / R34 都同此原则。**前端组件单测应该聚焦 logic-bearing components**（state machines, hooks, pure transforms），不该测纯 presentational widgets — testing-library 风格的"render and check DOM" 边际收益低。
- **Polish 期累积进入 codebase taxonomy**：R37 = first interaction，R38 = pattern 复用，R39 = component 抽取。**polish 期 iter 单看小，连起来看是 codebase 抽象层级在抬升**。30 iter innovation + 9 iter polish 后，前端 components 多了 1 个共享 widget，后端 helper 多了若干 pure fn — 这些都是 long-running project 的隐性资产。

## Iter R38 设计要点（已实现）
- **codified pattern 第一次实战复用**：R37 IDEA 写"pattern reusable for other timelines"。R38 当下一个 iter 立刻验证。这是 codified-rule 落地的标准节奏：rule → first application → re-test on second → 沉淀为公认 pattern。**rule 的有效性必须靠多次复用验证** —— 一次写规矩不算 codified，第二次复用不修改才算稳定。R38 通过这关，`PanelFilterButtonRow` pattern 进入 R-series stable vocabulary。
- **N=4 是 button row sweet spot 的反证**：原本 decision log 9 种 kind 都可以放 button。但 9-button row 横向会爆。**filter 的目的是 "isolate signal"**——只 surface 高频 + 有 retrospect 价值的 kinds。Run / Silent / LlmError / ToolReview* 都很罕见或语义独立，归"全部"反而更清。**不是所有 kinds 都该有 filter button** —— 只有 user 真想 "isolate this" 的 kinds 才需要。
- **复制粘贴 vs 抽 component 的纪律**：R37 + R38 都各自定义 btnStyle 私函数。诱惑是抽 `PanelFilterButtonRow` shared component。但 (a) PanelDebug.tsx 是大 monolith，没 utils 子目录；(b) 2 个 caller 抽 component 是 R18 之前的 case；(c) 第 3 个 filter button row 出现时再抽。**lazy abstraction 在 polish 期同样适用**，don't refactor at use-2 — wait until use-3+。
- **fontFamily inherit 是细节品质**：decision_log 区段用 monospace。普通 button 默认 sans-serif 会让 button 在 mono 段里"跳出" 感觉错位。`fontFamily: "inherit"` 让 button 跟周围环境一致。**继承 styling 跟着 context** 是 panel 设计成熟度信号 —— user 不会注意到 "对" 的细节，但会注意到"错" 的不一致。
- **Pattern reusability 验证 = surface duplication**：R37 + R38 共有同样代码结构（4 buttons + filtered list + 空文案兜底）。**完成同 pattern 第二次实例化后，duplication 已经显式可见**。如果继续走相似路径，第 3 个 timeline filter 上线时正式 refactor 抽 PanelFilterButtonRow。**duplication 不是 evil，但有 critical mass** —— 2 次属于"留着观察"，3+ 次进 backlog refactor。
- **decision filter "Run" 不入按钮但 └ 仍画**：filter 到 Spoke 时 outcome 行画 └ 连接器，看着同 kind 重复。Acceptable trade-off ——保持 visual consistency 比 special-case "filter 时去掉 connector" 简单。**"非完美但内部一致" 优于 "局部完美但需特例**" 是 UI 实现的常见 trade-off。
- **R-series polish 期价值在 audit + 复用**：R36 retune（数字调）+ R37 新交互（功能加）+ R38 pattern 复用（rule 验证）— 三种 polish iter 各 1 次。**polish 期的 iter 多样性** = 数字 / 功能 / 复用 / cleanup / refactor。比 innovation 期的 "加新轴" 节奏更细碎，但持续推进 codebase 健康。

## Iter R37 设计要点（已实现）
- **panel 从只读 dashboard → 渐进交互**：R-series 之前 panel 几乎全是只读：chips, hover tooltips, modal viewers, timeline lists。R37 是首次加交互按钮（filter row）。**dashboard 的渐进演化** —— 早期阶段重信息密度，成熟期加 retrospection 工具。filter 是最低成本的交互（1 click toggle, 0 typing）。后续可以考虑给其他 timeline（decision_log / butler_history）同 pattern 加 filter，形成 dashboard 操作 vocabulary。
- **active button color = matching pill color**：filter 按钮 active 时颜色跟它 filter 出的 pill 颜色一致。**color reuse 让 cross-component mental model 稳定** —— user 看到红色 active 按钮 "点掉" + 红色 pill "点掉" 知道这俩讲的同一件事。这种 visual coupling 比"按钮和内容用不同色 + 文字解释" 高效。
- **count + label 合并按钮文案**：每按钮显"回复 5 / 忽略 12 / 点掉 3"，count 嵌入 label。合并比 button + 旁边 count chip 紧凑 1.5 倍。**information density 在 dashboard 是首要美德**。但合并的 cost 是文字重排（用户切换 button group 时数字位置变）—— 这次接受，因为 dashboard 用户理解 "数字 = 该 kind 计数"。
- **empty filter 显 "暂无" 而非 hide section**：UX 关键是**preserve UI scaffolding** when filter is active but no matches。如果直接 hide list area，用户会困惑 "我点了过滤怎么 panel 区域消失了"。"暂无匹配条目" 灰 italic 文案让 user 知道 (a) 过滤是 active 的，(b) 这个 kind 现在 0 个。**show empty state，don't hide section**。
- **transient UI state 不该 persist**：filter selection 重新打开 panel 时 reset 到 "all"。诱惑是 localStorage 持久化。但 (a) retrospection 工具每次从 fullview 开始更友好；(b) 持久化引入"filter persist 但 entries 不再含 dismissed"等 edge case。**dashboard interaction state 应该 ephemeral** —— UI 状态跟 panel session lifecycle 同步。
- **dropdown vs button row 选择由"选项数 + 切换频率"决定**：4 选项 + 频繁切换 → row of buttons 比 dropdown 快 1 click。如果 8+ 选项或不常用 → dropdown。**UI 控件选择算法**：N=2 用 toggle，3-5 用 segmented buttons，6+ 用 dropdown，10+ 用 search。R37 N=4 stride 中段 → 按钮 row。
- **R-series 后期 polish iter 的形态**：R36 是 threshold retune（数字调），R37 是 panel 首次交互（功能加），R32 是 cleanup（删）。**polish 期 iter 类型多样**——不只是 cosmetics，还包括"老 surface 加新维度操作"（如 R37）和"经验数字调整"（R36）。innovation iter 数量减但单 iter 性质丰富。

## Iter R36 设计要点（已实现）
- **absolute thresholds drift, percentile thresholds don't**：R31 阈值 (1500/3000) 是 absolute 数字。R-series 加 hint 后 baseline 上移，阈值过时。如果当时用 percentile（"prompt 比过去 80% 短" / "比过去 20% 长"），threshold 会跟着 baseline 一起移动 —— 不需手动 retune。**absolute thresholds = brittle，percentile/relative thresholds = self-adapting**。但 percentile 实现复杂（要历史数据、算分布）；R-series 项目里 absolute + 偶尔 retune 是更经济。每 5-10 iter audit 一次阈值是健康节奏。
- **panel UI self-documents own evolution**：R36 hover 文案明示"R36 retuned: ..." 解释为什么这数。**threshold 不是 magic number，是设计决策**。把决策原因 inline 写进 panel hover 让未来的 maintainer / 自己 不需查 git log 就懂。这种 "self-documenting threshold" 是 long-running project 设计纪律 —— 数字旁边永远配 *为什么是这数字* 的解释。
- **threshold retune iter 是 polish 周期的常态**：R-series 30+ iter 后核心结构 mature，不再加新轴线；后续多是这种"阈值微调 / 文案打磨 / chip 顺序优化" polish iter。**长系列 mature 期的 iter 尺寸应该缩小但保持节奏** — 大改动罕见，小调整保 codebase 健康。R32 deletion + R36 threshold retune 都是这种节奏。
- **+1000 整数增量比精确算更干脆**：理论上能精确算"R32-R35 4 hints × 平均 X chars = Y bump"。但精算 noise 大（每 hint 字数取决于 input data）。**round number 易记忆且不假装精确** —— 选 +1000 是承认估算不精确，便于沟通。如果一个月后发现 +1000 不够 / 过头，再调一次也无负担。
- **R31 IDEA "3 是甜蜜点" 持续践行**：R36 没拉到 4 段。诱惑是"加 ≥6000 红色 critical 段表达更细致"，但 chip 视觉空间有限，3 段 (lean / normal / heavy) 已经分类明确。**克制是 panel 设计的成熟度信号** —— 每多一段 = 多一种 cognitive 区分负担。R20 / R27 / R36 都用 3 段，应该 codify 作 default chip-segmentation count。
- **iter 范围控制不 creep**：R36 完全没碰 chip 颜色 / icon / position / hover layout，只改阈值 const + 同步文案。诱惑是"既然在改 chip，顺便重写 hover 风格 / 加 emoji 装饰"。但 scope creep 让 iter 难以推理。**单 iter 单 atomic concern** 比 mixed iter 干净 —— 阈值调就只调阈值，UI 重写另起 iter。R32 IDEA 写过同样原则。
- **R-series threshold 一致性是隐式 mental model**：R7 ratio 阈值 0.6/0.2，R-series streak 阈值都 3，feedback aggregate min sample 5，speech length thresh 25/8 —— 这些数字跨 iter 稳定。**变化的应该明显变化（如 R36 retune），稳定的应该明显稳定**。如果 R7 阈值随便调 0.6 → 0.55，认知锚就摇摆。**只有"baseline drift" 这种系统性原因才该 retune**，otherwise 数字不动。

## Iter R35 设计要点（已实现）
- **mirror pair 完整 = closure**：R33/R34 给 pet 自我意识 ("我最近一直沉默")，R35 给 user-feedback 意识 ("用户最近一直拒绝")。两者完全对偶 —— pure fn 形态、threshold、UI chip 都 mirror。**Mirror feedback loops** 是 cognitive architecture 的 closure pattern：信号闭环既要 outbound（看用户）又要 inbound（看自己）。R26 + R33 第一次完成 mirror（aggregate vs streak 都两端均有），R35 把"trailing 维度" 也补全 mirror —— **mirror 的 mirror，对称完美**。
- **同 sample threshold = mental model 锚**：3 出现在 R7 / R11 / R19 / R26 / R33 / R35。**项目内 "minimum confidence sample" 概念稳定 = 3**。当用户 / future maintainer 看到任意 R-series 阈值 3，知道意思是"够了不噪音"。如果每个子系统独立 tune，认知开销大。**跨子系统统一 magic numbers** 是反复践行的 IDEA。
- **色彩升级表达严重度**：R34 🤐 silence 橙色（"卡住" — 中性，pet 自身问题）；R35 🙉 拒绝 红色（"明确反对" — 用户问题，更紧迫）。**color escalation = severity gradient**。这扩展 R-series 视觉 taxonomy：橙色 = "stuck pattern worth noting"，红色 = "active negative signal needs response"。R20-R34 之前红色只用于 R27 deep-focus 锁标，现在 R35 加入"用户拒绝" 同色 —— **同色等价是"高 stakes 等待响应" 状态**。
- **CJK 引号在 Rust format!: "" → 「」**：第一版我写"我说的不对" 用 ASCII 双引号，Rust 编译报错 "expected `,` found...". 因为 ASCII `"` 在 format! 字符串字面量里就是字符串结束符。中文 quote 用「」既符合中文排版又**避免 escape 陷阱**。这是 prompt 文案写作的 i18n hygiene —— **多语言 fallback：在 i18n 字符串里偏好 native quote marks 而非 ASCII**。
- **软 nudge phrasing 已成 R-series grammar**：R27 "极简或选择沉默"，R33 "否则继续沉默也无妨"，R35 "或者干脆这次沉默也行"。三个 directive 都给 LLM escape hatch，没一个是硬命令。**这种 grammar 已稳定** —— 未来加新 directive 时该 audit "有 escape hatch 吗"，否则 LLM 在合理判断时会被强迫 override。
- **R26 vs R35 不重复 是 different lenses**：R26 是 20-window ratio（如 "20 次里 12 次被忽略 60%"），是平均水平。R35 是 trailing streak（如 "最近 4 次连续都被拒"），是急迫程度。**两者用同 underlying 数据但不同 lens**：smoothed 反映长期 trend，streak 反映 acute pattern。LLM 同时看到不冗余因为各自承载不同语义。**多 lens 看同数据是 prompt design 的 sophistication**。
- **R-series 30+ iter 后的 closure 节点**：R33-R35 把"meta-cognitive mirror" 完整闭合后，R-series 信号设计可能进入 polishing 期 — 后续不太可能再加新轴线，多是这些 axis 的 fine-tuning（threshold 调 / 颜色调 / hover 文案 polish）。**长 iter 系列的 closure** 不是 abrupt 停下，而是核心结构 mature 后渐入 polish 节奏。

## Iter R34 设计要点（已实现）
- **IDEA 决策不是不可纠错**：R33 IDEA 写"streak 是 transient → no panel chip"，R34 重新审视后发现该判断**错的**——polling rate (每秒) vs turn rate (≥5min) 让 streak 在两 turn 间完全 stable，没有 flicker。所谓"transient" 是基于错误的 polling-vs-turn rate 直觉。**R-series IDEA 是 reasoning artifact 不是 immutable rule** ——发现旧决策错了就改，而不是为了保面子继续延续。这是 long iter 系列健康的标志：能看见自己的错。
- **flicker vs stable 看 update frequency**：决定一个状态是否适合 panel chip 的关键不是"它最终会变化吗"，而是"在 panel polling 周期内它会频繁变化吗"。Streak 一次更新需要新 turn → 5 分钟级。Panel 每秒 poll → 数百次看到同一个值。**"看似 transient" 但 update frequency 远低于 polling 时，实质是 stable**。这条原则补充 R20-codified "stable 上 panel / transient 留 prompt"——具体看 update granularity，不要靠直觉拍。
- **single source of truth helper 是反 drift 的护身符**：build_tone_snapshot + run_proactive_turn 都调 `count_trailing_silent(&snap)`。如果 R-future 改 trailing 定义（比如允许 1-spoke 间隔），改一处所有 caller 同步。如果两处分别 inline 同样 logic，drift 就成了潜在 bug —— "为什么 panel chip 显 streak=3 但 prompt 没 nudge?"。**核心 logic 永远抽 pure fn，让 caller 共用**。R23 `classify_feedback_band` / R20 `classify_speech_register` 同思路。
- **chip 显隐 trigger 跟 prompt actionability 对齐**：panel chip 出现的瞬间 = prompt 已经 nudge 的瞬间。如果 chip 在 streak=2 就显但 prompt 在 streak=3 才 nudge，user 看到 chip 时会困惑"那为啥 prompt 没动"。**chip threshold === prompt threshold** 让两个 surface 时间同步，user mental model 简单。
- **"卡住" 信号视觉 family**：📏 monotone register / 🔁 repeated topic / 🪟 deep focus / 🤐 silent streak —— 都用橙色，都表达"系统当前停滞 / stuck"。**chip 颜色作分类语义** —— 同语义同色，user 一眼分群。这是 panel design 的 implicit visual taxonomy，不显式说出来但累积起来非常稳固。
- **R20 codified rule 第三次 audit**：R20 codified "prompt 信号 = panel surface"，R21+R22 是第一轮 audit，R29+R30 是 settings 字段扩展，R34 是回头审视 R33 时漏的 surface。**rule audit 不是一次性事件**——每个新 prompt 信号 same-iter surface 之外，还需要 *re-audit 旧 trade-off 决策是否仍然成立*。R-series 后期会反复 cycle through 这种 audit。
- **explicit 自我纠正 IDEA 比假装从未误判更有价值**：R34 不只是改 code，IDEA 明确写"R33 当时判断错了"。**版本历史可见 reasoning 演化** 让未来的 maintainer 看到不只决定，也看到何时为何 decision 改变。这是 long-running project 的隐性 documentation 价值。

## Iter R33 设计要点（已实现）
- **meta-cognitive signals 是 prompt design 的高阶层**：R-series 之前给 LLM 的信号大多是关于 *外部世界* —— 用户反馈、active app、time of day、recent speeches。R33 是第一个 *关于 LLM 自己行为模式* 的信号 —— "你自己最近一直沉默"。这种 self-aware feedback loop 是真实智能体的特征，把 pet 从"接收 + 响应外部" 升级到"observe self + act"。**外部信号 → 内部信号** 是 R-series 后期的方向。
- **trailing-only > majority-in-window**：诱惑是用 "last 5 中 4 次 silent" 触发。但 trailing-only 严格得多 —— 它意味着"现在正在 silent loop"，而 majority 可能只是"过去碰巧多次 silent 但已经 broken 了"。**streak 才有 actionable urgency**，scattered count 没有。这条原则适用所有"streak detection"场景：邮件提醒 / 健身打卡 / 学习连续天数 —— uninterrupted tail 才是真信号。
- **transient state 留 prompt 不上 panel**：panel chip 适合 stable status（feedback summary、active app duration、register classification）。R33 streak 是 transient — pet 一开口就清零，每次 silent 又涨 1。Panel chip 显"streak=3" 然后下一秒变 "streak=0" 是 flicker 噪音。**stable signal → panel chip; transient signal → prompt only**。这条原则补充 R20 codified rule —— 不是所有 prompt 信号都该 panel surface，需先看 lifecycle。
- **pure fn 接 slice 不接 Mutex**：测试关键决策。production 调用方 lock + clone + 传 slice 给 pure fn。**业务 logic 函数应该接 plain Rust types**，IO/锁操作留在 caller。这条纪律让单测 trivial（直接 hand-craft Vec），让 logic 跟 infrastructure 解耦。R20 / R23 / R26 都践行同样设计 — pure on slice, async wrapper outside.
- **soft nudge phrasing 是 prompt design grammar**：R27 deep-focus 写"极简或选择沉默"（保留判断空间），R33 写"否则继续沉默也无妨"（escape hatch）。这种 "preserve LLM judgment" 是 prompt design 的稳定 grammar —— 硬指令"必须开口" 会破坏 LLM 在合理 silence 场景的判断。**every directive prompt should have escape hatch**。
- **threshold 跨子系统统一**：3 是 R-series 反复出现的 minimum sample 数 — speech_history.detect_repeated_topic min_distinct_lines, 现在 R33 silent streak threshold。**阈值跨子系统的一致性是隐性 mental model 锚** —— "为什么这里是 3？因为 3 是这项目的 minimum-confidence 数"。一致性比每处独立 tune 更易理解。
- **R26 + R33 mirror 是 closure 完成**：R26 inject "user 怎么反应你"（trend），R33 inject "你自己怎么表现"（streak）。两者完整覆盖 pet 的 *bidirectional self-awareness* —— 既看 outbound 反馈又看 inbound 行为。**mirror feedback loops** 是 cognitive architecture 的标准 pattern。后续如果加更多 meta-signal（如"你最近回复 latency 在变长"）也走同样形态。

## Iter R32 设计要点（已实现）
- **dead code rot 是真问题不是 paranoia**：dead 文件从未被 import，但每次 TypeScript / Tauri / React 升级它都是潜在 break 点。Codebase grow 时新 contributor 看到 dead 文件会困惑"这是干啥的"，浪费认知带宽。**未维护代码 = 慢慢变 broken 但表面看不出**，紧急时反而帮倒忙。删 > 留。
- **git is the backup, codebase is not**：诱惑是"也许以后用得上 留着吧"。但 git log 完整保留 deletion，需要时 `git log --diff-filter=D` 直接捞回。**留 dead code 等于把 git 当 archive 用**——双倍空间管理同一信息源，违反 single source of truth。删除 = 让 git 真正承担 history 角色。
- **deletion iter 应当单主题**：R30 IDEA 写"audit 跟 cleanup 不混"。R32 实践这条：纯 deletion，没顺手"重构这两文件残留的某个工具函数到 utils/" 之类。**单 iter 单意图**让 commit history 可读 —— 看到 R32 commit 知道是删除，看到 R29 知道是 UI surface。混合操作让 future review / git bisect 难。
- **negative test from compiler**：dead code deletion 的"测试" 不是写新单测，是看 build/tsc/cargo test 是否还 pass。如果有 stale import / stale call，编译器会立刻喊。**leveraging the type system as test infrastructure** 是 strongly-typed language 项目的福利 —— 不必写"deletion didn't break X" 测试，编译器自带。
- **R-series 三种 iter 类型轮替**：
  1. **创新 / 新功能**（R12 / R20 / R23 / R26 等）—— 加新 surface 或 logic
  2. **还债 / audit-and-backfill**（R18 / R21 / R22 / R29 / R30）—— 把 codified rule 应用到老违反
  3. **cleanup / refactor**（R32）—— 删 dead code、抽 helper、解耦
  健康长 iter sequence 三类应该轮替，不该全做创新（积累债务）也不该全做 cleanup（停滞）。**iter type diversity 是项目可持续的指标**。
- **dead code audit 的两步**：(1) `grep -rn "ComponentName"` 找所有引用；(2) 排除自身文件，剩下应该是 0。如果 > 0 就不能删。R32 用一段 shell `find ... | while ... grep ... wc` 一次扫整个 components/ 目录 — **scriptable audit** 比手动逐文件检查可靠。下次 cleanup 直接复用这种 pattern。
- **comment 自述 "remove when done" 是反模式**：DebugBar.tsx 顶部 comment 自承"用完即删"，但实际从未删除。**code-comment 自我承诺是不可靠的承诺** —— 没人定期 grep "remove when done" 来收口。如果 author 真打算用完即删，应该 (a) 在临时 branch 写不 merge 到 main，(b) merge 时立刻 schedule TODO ticket。**靠 comment 提醒未来自己 = 注定遗忘**。

## Iter R31 设计要点（已实现）
- **dev-facing observability 也是 codebase health surface**：R31 chip 给"我"（持续维护者）看的多过给最终用户。但持续维护是 long-running system 的 lifeblood —— **panel 应该有 dev-mode chips 不只 user-mode chips**。如果系统设计只考虑 end-user surface，maintainer 调优时就要盲操作。R31 把"R-series 累积 prompt 是否过胖" 的反馈循环 panel 化，类似一种"实时 lint warning"。
- **CJK chars().count() 是 i18n 代码的 baseline 纪律**：R19 第一次踩坑，R23 / R31 反复践行。任何"长度感知" 业务（threshold band / display digit / pagination）必须用 chars 不 byte。**Rust String::len() 是 byte 长度** 这个事实在多语言项目里反复给开发者上课 — codify 这条 rule 让未来不再踩。
- **meta signal 应该 visually 区别于 data signal**：📝 prompt 大小 是关于 *prompt 自身* 的，不是 user / pet 的状态。chip 放置选择反映这层 — 不混进"宠物开口形态"或"用户上下文"集群，作 visual transition。**meta = how the system measures itself**, 区别 data = what the system measures of the world。两层信息不应该 visually 混淆。
- **band 阈值经验拍 vs schema lock**：1500 / 3000 是经验数字，不是从某规范来的。**经验阈值应该 inline comment 解释来源** 让未来 maintainer 知道这是 cardinal 决策点。R31 IDEA 写"R-series 现状大概 2500-3500" 是 data anchor。如果未来 prompt 重新 lean 到 1000-1500，阈值可以下调；阈值不该 unchanged 当 baseline 移动。
- **null vs zero 在 fresh process 区分**：第一次 process 启动，没 prompt 历史。null 表达 "无数据" — chip 不渲染。如果用 0 表示，chip 显 "📝 0字" 是 misleading "prompt 是空的"。**null = absent，0 = empty value** —— 在 UI 数字字段尤其重要。
- **codified rule 自带 testing surface**：R31 chip 让"prompt 太胖" 状态 visible，等于给 R-series 加一道 visual lint。下次写新 hint 时如果让 chip 变 orange，**视觉会喊我"考虑裁旧 hint 才加新"**。这是 codified rule 之外的 *visual reminder rule* — 阈值 + 颜色 = 自动 review。
- **dev-tool chip 的克制**：诱惑是再加 "📝 last_reply_chars" / "📝 N tool_uses_count" 等 dev meta chips。但 R31 一个 chip 已经表达了"prompt 体量" 维度；more 都是 marginal。**克制 dev surface** 跟 user surface 一样重要 — chip strip 不是 dashboard，是 status bar。

## Iter R30 设计要点（已实现）
- **codified rule audit 是常态而不是一次性事件**：R20 codified "prompt 信号 = panel surface"，触发 R21 + R22 两次 audit。R29 codified "settings field = same-iter UI"，触发 R30 audit。**每个 codified rule 通常需要 1-2 次 audit-and-backfill 才完全落地** —— rule 之前的所有 violations 都欠债。系列开发的健康姿态：codify rule 后立刻 audit，不要等下次违反时再回头查。
- **两同性质 debt 一次还**：发现 stale_once_butler_hours + stale_daily_review_days 都欠 UI。诱惑是分两 iter 各自做。但 (a) 两者结构完全相同（都是 number field），(b) 同 PanelNumberField 行 layout 就能放，(c) 两者都属于"memory_consolidate 字段补全" 一类操作。**同性质 debt 一次清干净** 比拖两次更经济 —— 减少 commit 数 + 减少 PR review 负担。
- **min=0 vs min=1 反映业务语义**：stale_reminder/plan/butler 都是 hours，0 几乎没意义（"立刻清"，user 几乎不会要），所以 UI min=1。stale_daily_review_days 后端 0 是 explicit "关闭剪枝"——保留所有日记永不删。UI 必须允许 0。**前端 UI 约束应该 = 后端业务约束**，否则会出现"后端支持的语义前端禁用"的异常状态。这条原则适用所有数字字段：先看后端 0/负数有没有特殊语义，再决定 UI min。
- **hint 文案密度跟字段密度匹配**：4 个 stale 字段配一段 4-segment hint，2 个字段是 2-segment。**单段文案承载 N 个字段时**，分别用句号 / 顿号 separator 让眼睛读得舒服，而不拆 N 段独立 hint div。后者会让 settings 页面 fragmentation 严重。Hint 是 inline education 不是文档章节。
- **dead code 缓刑而不是即删**：R29 IDEA 标了 SettingsPanel.tsx 是 dead code 但 R30 不删。**audit iter 应该聚焦 audit 主题**，混合多种 cleanup 类型让 commit 模糊。下个 cleanup iter 该专门做"删 dead code + 清 unused imports + 整理 warning silenced 的代码"集中操作。**单 iter 单主题** 是 commit history 可读性的护城河。
- **codified rule 自带 audit 工具流**：R29 rule 让我下一次写新 settings 字段时**自动检查 UI 是否同步**。R20 / R29 这些 rule 是 cognitive checklist —— 写代码时 mental ping 提醒"这条 rule 是否被违反了"。**好的 rule 是 self-enforcing 的**，写出来后未来 PR 自带 reviewer 视角。
- **多 iter 系列的演化模式**：R-series 现在 30 iter 不是 30 个独立 feature —— 是 5-6 个核心 codified rule 的反复 audit + backfill + new application。R20 / R23 / R27 / R29 都是 rule 创立 iter，其他多数是 application iter。**codified rule 是系列的"骨架"**，application 是"血肉"。这种结构让长系列保持可理解。

## Iter R29 设计要点（已实现）
- **新加 settings 字段必须 same-iter 上 UI**：R13 (2026-05-03) 加 companion_mode 后端时 IDEA 显式写"前端 UI 暂缺"。这种"先后端，等以后做前端" 的 split 看着合理但实际让"功能上线" 跟"功能可用" 错位 — 7 iter 过去用户没法用。**应该 codify**：每个新 settings 字段同 iter 必须有 UI 入口，否则 = hidden feature。R20 已 codify "新 prompt 信号 = 同 iter panel surface"，R29 把这条扩展到 "新 settings 字段 = 同 iter UI 入口"。
- **option label 显数学 multipliers 是 surface-the-math**：诱惑是 dropdown 写简短 "balanced / chatty / quiet" 让用户自己 read docs 理解差异。但 dropdown 是用户做选择的瞬间 —— surface the consequence at decision time 比让 user 走流程查文档好得多。R23 cooldown hover "configured × mode × feedback = effective" 同思路 — **panel UI 应该把数学 surface 出来**让用户可见可算。
- **多层系统的 settings UI 应解释 layering**：companion_mode 是 base layer，R7 adapter 在它之上叠加。如果 UI 不说明 "你选了 chatty 但实际可能更安静（R7 在 fine-tune）"，user 会困惑 "为啥我选 chatty 还是安静"。**explicit limitations** 比 implicit power 让 user 心智模型对齐。这条原则适用所有有 layered behavior 的系统。
- **string vs union-type 是前后端 schema 经济**：TypeScript 喜欢 union "balanced" | "chatty" | "quiet"——type-safe 优雅。但**后端 R13 选了 String 不 enum**（serde tolerance / 未来扩展），前端硬约束 union 等于让前端先进入"加新模式必须先改 union"局面。**string + 文档约定** = 前后端 schema 同步无摩擦。union 在这里是 "looking smart but creating friction"。
- **dead code 不投资**：SettingsPanel.tsx 是 legacy file，无 import 调用。诱惑是"也加上 dropdown 保持一致"。但加上等于持续维护永远不被加载的代码。**deletion is the cleanest fix** — 但今天不删（不在本 iter 范围），至少**不投资 dead code**。下个 cleanup iter 该考虑 grep + delete。
- **R-series 还债节奏**：R29 是回头补 R13 留的债，跟 R18（R16 IDEA 标的 helper 抽取）/ R21+R22（R20 codified rule audit）/ R28（hover details → at-a-glance）一样属于"有意识的回头收口"。**长 iter 系列的健康标志是周期性还债**——纯做新功能的系列会越积越多 hidden cost。
- **base=0 invariant 一直延续**：R13 IDEA 已强调 cooldown_seconds=0 时三档都返 0（用户 explicit opt-out 不被任何 multiplier 重新打开）。R29 hint 文案明确这一点 — UI 帮助 user 看到 invariant 不会被 mode 选择破坏。**对 user 透明 invariant 是 trust-building**。

## Iter R28 设计要点（已实现）
- **hover details → at-a-glance surface 是 incremental upgrade pattern**：第一次 surface 一个新信号通常用 hover/tooltip 包住所有细节（容易写、不抢视觉）。第二次升级把"哪部分信息有价值前推到 chip 颜色 / 字重 / 文案"。R23 是第一次（hover breakdown），R28 是第二次（chip color band）。**这种 incremental discovery 让"什么细节值得前推" 来自实际用户经验，而不是预先想象**。新 chip 上线先 hover-only 是合理的。
- **变化的色彩才有信号意义**：cyan 保 default 是有意识的。如果三 band 都变色（cyan / green / amber → red / blue / yellow），baseline 状态消失，每个状态都被 visually elevated 等于没人被 elevate。**保留一种"安静默认"色让 user 知道"现在没事"**。这是 neutral-as-baseline 设计。
- **multi-cue 强化重要状态**：non-neutral band 不只换色还加 fontWeight。颜色 + 字重双重 anchor "adapter 在干预" 这个状态。**多重感官信号叠加** 比单一信号更难被忽视，但要克制 — 太多 cue 让 chip 像 emergency。R28 选 2 种（color + weight）刚好。
- **R27 codified rule（band derive at view edge）继续 paying off**：R28 完全没碰 backend，前端读 cooldown_breakdown.feedback_band 直接 mapping 颜色。如果 R23 时把 band 计算放在 backend 还塞 chip_color 字段，R28 就成了 backend 改色 + frontend 读色 —— 不必要的耦合。**view-layer derive primitive value into visual encoding** 让前后端各司其职。
- **不让 user 配置 chip 颜色是 settings 经济**：理论上 user 可以喜欢"我希望 high_negative 是红色"。但 (a) 大多数用户不在乎 (b) 默认对绝大多数用户合适 (c) 多一个 setting 是认知负担。**让 settings page 简洁** > **让每处颜色可调**。
- **R-series 的 surface 升级是 long compounding**：R10 first chip → R23 hover breakdown → R28 chip color。同一 signal 三轮 surface upgrade，每轮信息密度递增、user 视觉成本递减。**长系列的真正价值在累积**，不在单次创新。
- **小 iter 也要有教益**：纯样式改动易被视为"装饰"，但 R28 IDEA 提到的"variable color = signal" / "neutral baseline" / "multi-cue concert" 都是可复用 panel design 原则。**抽 IDEA 即使从小 iter 也能提炼通用洞察** —— 让小事情变 codified rule 是 R-series 一直延续的 mode。

## Iter R27 设计要点（已实现）
- **descriptive vs directive 的 prompt 升级路径**：R15 写 "用户在 X 已经 N 分钟"，是事实陈述。LLM 自己得 infer "所以我该闭嘴吗？"。R27 升级为 "...这次开口应当极简或选择沉默" 直接告诉 LLM 怎么做。**信号强度 dial**：低强度信号给事实，让 LLM 自己 judge；高强度信号给 directive，减 LLM 误判风险。Pet design 学问 — 哪些 case 值得 directive 升级，哪些保 descriptive 让 LLM 自由发挥。R27 的判断是 ≥60min = 高 stakes 不容犹豫，需要 directive。15-59min 还可酌情 → 留 descriptive。
- **threshold 用人类时间单位锚**：60min = 2 pomodoro / 一个 deep-work block。这是用户**已经熟悉的时间单位**。任意拍数字（45 / 90）听上去差不多但 lacks anchor。**用 widely-shared mental unit 比经验拍数字更稳** — 用户 / 同伴模型 / 设计者三方对 60min 同样理解 "深度专注"。
- **band 计算下放 view 层**：backend 没新字段（band/is_deep_focus）。Frontend if/else 判 minute → color；prompt formatter if/else 判 minute → directive。乍看违反 single source of truth。但**band 是 derivable 不是 stored**：raw value 是 minutes，band 由 same threshold 在两端独立 derive。两端用同 const 算 → 必然一致。如果 backend 加 band 字段反而引入"backend 算的 vs frontend 显示的"分歧风险（万一 frontend 写错 const）。**when value derivable from primitive, prefer derive at edge**。
- **directive 文案保留 judgment 空间**："极简或选择沉默" 不是"必须沉默"。LLM 有时遇 reminder due / butler task scheduled / late-night-wellness 触发等强 prompt 拉它说话 — 这种 case 极简（一句话）也算 valid。硬指令"不许说"会让这些合法触发被吞。**prompt 设计哲学：保留 escape hatch** —— LLM 是判断者不是奴隶。
- **softer cool 色阶 vs harder warning 色阶**：R-series 之前 chip 都是绿 / 橙 / 红 + 灰背景，现在 R27 引入红 + 🔒 锁标，是第一个"严肃" 信号视觉。**信号梯度跟视觉强度梯度对齐** — 红色 + emoji 锁是 most prominent visual statement，对应 most prominent prompt directive。当系统稳定后，新加 chip 可以参考这个映射 —— 多严肃信号 = 多视觉冲击。
- **derived band ≠ data band，纪律**：R20 / R21 / R22 都用 derived view-layer banding（minutes → color，ratio → color）。R27 延续。这是个隐含 codified rule —— 一旦 raw 数据足够 (minutes / ratio / count)，**前端 derive 比后端塞 band 字段干净**。后端只算 primitive，前端做 visual encoding。**MVC 分层在 panel chip 里成熟体现**。
- **3-band 比 4-band 是甜蜜点**：考虑过 ≥120m 加"极深" 第 4 段。但 chip 视觉负载 / 阈值新增 ROI 都低。**视觉 quantization 应当少而粗**：3 段（OK / mild / severe）已能抓住"哪个区域"，4+ 段只是装饰。Panel design 中"克制是成熟"。
- **R27 不加 panel UI test**：chip color / 文案是 inline 渲染分支，没有跨函数 logic。**渲染分支** vs **业务 logic 分支** —— 后者必须测，前者 tsc + cargo build 已验类型 + 边界。这条纪律延续 R21 / R25 IDEA 写过的"测 logic 不测 wiring"。

## Iter R26 设计要点（已实现）
- **latest event vs trend = 树 vs 林**：单点 latest 信号容易被异常值误导（最后一条恰巧是 dismissed → LLM 认为"用户讨厌我" 但其实是个 outlier）。aggregate trend 平滑掉 outlier 反映真实 base rate。但只有 trend 没 latest 也有问题 —— LLM 看不到刚才发生了什么具体事件，会缺乏 contextual reaction。**两层都给** 让 LLM 既看 *what just happened* 又看 *what's been happening*。这是 prompt design 的经典对偶。
- **min_samples = 5 是 R-series 跨函数稳定阈值**：FEEDBACK_ADAPT_MIN_SAMPLES (R7), SPEECH_LENGTH_MIN_SAMPLES (R19), FEEDBACK_AGGREGATE_MIN_SAMPLES (R26) — 三个都是 5。这是有意识的一致：跨子系统用同一个 "low-confidence cutoff" 让 mental model 简单。**common knobs should have common values** —— 设计 trade-off 一致比每处独立 tune 更可读。如果未来发现某场景需要不同阈值，再 case-by-case 拉开。
- **零状态省略文案不只是 visual noise 减少**：dismissed=0 时省"0 主动点掉" segment。表面是减少 visual noise (panel chip 同 pattern)，深层是 **避免无意义信号注入 prompt**。"0 主动点掉" 这句话技术上是真信息但 actionable density 是 0 —— LLM 看到不会改行为。**省略零信号 = 提高 prompt token 单位 actionability**。
- **同 fetch 多 consumer 是经济也是 consistency**：原本 prompt 路径 recent_feedback(1)，gate 路径 recent_feedback(20)。两窗口不同 → 万一中间用户 reply 了一条，prompt 看到的"上次"和 gate 看到的"trend"基于的样本会错位（R26 同步前的潜在 race condition）。改成共用 (20) 不只是省 IO —— **同一时刻 prompt 和 gate 看到完全一样的 feedback 历史**。一致性是隐藏的正确性属性。
- **chip vs prompt 不必 1:1 mapping**：R20-R23 codified "prompt 信号同 iter 加 panel surface"。R26 反过来 —— **trend 信号已经在 panel surface（R10/R1c chip 显 N/M + 👋K），prompt 这层是补**。原则是双向：prompt 信号缺 panel 时补 panel，panel 信号缺 prompt 时补 prompt。但 R-series 的"应该 panel 可见" 不是说"也只有 panel 能看到"。**prompt 和 panel 是两个 audience**，各自需要的信息密度可以独立 tune。
- **共享 fetch 抽变量比 helper 抽函数轻量**：R20/R21 把 5-line speech fetch hoisted 到 build_tone_snapshot 顶部 `let recent_for_signals`. R26 把 feedback fetch hoisted 到 run_proactive_turn 中部 `let recent_feedback`。两者都是变量层面 sharing，没抽函数。**抽函数适合"有自己 IO 边界的逻辑"**，抽变量适合"同 scope 内多个 read 复用"。区分这两个层级让 refactor 决策不过度。
- **统一中文 label 是跨 surface mental model 的纽带**：R1c panel chip 用 "回复 / 忽略 / 点掉"，R26 prompt aggregate hint 也用 "回复 / 静默忽略 / 主动点掉"（aggregate 加修饰词更具体）。**LLM 读到的描述 ≈ 用户在 panel 读到的描述** = 两者讨论同一件事时对得上。这种"统一术语"是 multi-surface system 的隐性 hygiene —— 比 "ignored vs ignore vs 忽略" 三种说法更专业。

## Iter R25 设计要点（已实现）
- **隐式状态 vs 显式标签**：reply 字段是空字符串 → 用户得记住"哦这意味 silent"。这是 **隐式状态**，依赖每个 reader 自己 decode。显式 outcome 字段是 explicit label —— 字段名+值告诉读者结论。**任何需要 reader decode 才能知道含义的字段都是 cognitive debt**，给它一个 explicit label 释放 reader 大脑。这条原则放到所有数据结构上：能加 label 不要让 reader infer。
- **同源逻辑写两遍是隐患不是冗余**：原本 SILENT_MARKER 检测在 push 点之外（4 行后的兜底返回）。R25 在 push 点又写一次同条件。看着像重复 —— 但**两处其实判断时刻不同**，push 是为了打 outcome label，下面是控制返回 reply 字段。两个 caller 各自需要这个判断。如果未来 prompt format 改了 silent marker 含义，两处一起改是 *features* 不是 bug —— 同源逻辑抽 helper 反而模糊"两处都依赖这条规则"。**复制粘贴是有时是更诚实的代码**。
- **优先级判断：locality > DRY**：ProactiveTurnOutcome 在函数末尾才构造，但 push 点在中间。要在 push 点判 outcome，要么 (a) inline 重做条件，要么 (b) 把 outcome 计算 hoist 到函数早期、最后构造时 reuse。(a) 是 5 行重复，(b) 是 30 行的 control flow refactor。**读者看 push 点想知道 outcome label 怎么算时，inline 判断最易读** —— DRY 这里牺牲一点，locality 保住。
- **frontend 字段 optional 是 forward compat 工具**：`outcome?: string` 让 frontend 能拿到 R25 之前 build 的 backend 数据（即使在 dev 中也可能有 stale process）。其实 ring buffer 是 in-memory 进程重启就清，理论上 R25 backend deploy 后立即一致。但 `?` 是廉价 forward compat 投资 —— 加一个问号就给"接收 stale data" 留余地。**类型系统的 cheap insurance** 应该常用。
- **R25 是 R20 codified rule 的隐式延伸**：R20 说"prompt 信号要 panel 可见"。R25 不是 prompt 信号，但是 backend 状态 → 用户看的 panel surface。同一个原则的更宽泛版本：**任何 user 看的 surface 应该有 self-explanatory label，而不是让 user decode 隐式状态**。R20 是 prompt → panel surface 的强制律，R25 是 backend state → panel display 的字段命名律。两条放一起作"observability hygiene"。
- **count chip 不必 every signal 都做**：R25 没加"过去 5 turn 沉默 N 次" 的聚合 chip。诱惑是：modal 一个一个看费时间，给个汇总 chip 更快。但 panel 已经有 outcome counter (decision_log + LLM outcome buckets)、ring buffer 每条独立 surface 都够。**信息密度是有收益曲线**：第一种 surface 收益高、第二种边际、第三种就开始噪声。**克制是 panel 设计的成熟度信号**。

## Iter R24 设计要点（已实现）
- **三段闭合 affordance：function / feedback / discoverability**：R1b 让 dismiss 工作（function）。R1c 让 panel 看到信号（feedback）。R24 让 bubble 看起来可点（discoverability）。**任何 user-facing 行为都该过这三关**，否则等于 hidden feature。R1b 上线时只过了 function，R1c 上线时过了 feedback，但 discoverability 留了 7 iter 才补 — 期间用户根本没途径知道这功能存在。教训：**新 user 行为应该 same-iter 完成三段，否则有效用户量为 0**。
- **affordance 的视觉重量匹配交互重要性**：标准 close button (红色 / 显眼) 在 system dialog 是合适的，因为关闭对话框是高频显式操作。chat bubble 的 dismiss 是"我看到了，不太想理"的软交互，所以 ✕ 应该是软提示而不是硬按钮。半透明灰色 ✕ 0.55 透明度让它**存在但不抢戏**。**UI 元素的视觉强度应该是 user 注意力分配的引导**，不是装饰美学。
- **event bubbling > dual handler**：✕ 不需要自己的 onClick — click 自然 bubble up 到父 div 已有的 handler。诱惑是给 ✕ 单独 onClick 让"语义清晰"，但那是双 handler 的双 source of truth — 改 dismiss 逻辑要改两处。**bubbling 是 React/DOM 的 native 工具**，default 让它工作 = single source。stopPropagation 应该是显式 opt-in，不是默认。
- **tooltip 是 inline education**：R23 cooldown hover 解释 derivation，R22 active app hover 区分"prompt fired vs panel only"，R24 ✕ tooltip 解释"5s 内 = 强信号"。这种"chips 不只显数字、hovers 解释为什么这样"是 R-series UI 一以贯之的纪律。**panel 是用户慢慢学的工具**，不是一次性 dashboard — tooltip 替代 docs 让学习曲线 inline 内置。
- **pointerEvents:none 的 footgun**：第一版我用 pointerEvents:none 让 ✕ 纯装饰。但浏览器对 pointer-events:none 元素的 title attribute 显示行为**不一致** —— Chrome / Safari / Firefox 各有差异。改成 default pointer events + bubbling 一起处理 — title 稳定显示，click 通过 bubbling 走 parent handler。**CSS 工具不是免费的**，每次禁用 native 行为都要思考 side effect。
- **frontend 常量到 tooltip 的耦合是 acceptable trade-off**：R1b 5000ms threshold 在 App.tsx，R24 tooltip 文案"5 秒内"。改 threshold 要改 tooltip。理论上可 export 常量然后 tooltip 用 template literal 引入。但 5000ms 是 stable UX 决定（不会随便改），bonus 复杂度不值得。**轻度耦合在 stable invariant 上是 acceptable**，过度抽象反而 obscure 简单逻辑。
- **discoverability 是隐藏的 N+1 工作**：function + feedback 是 N，discoverability 是隐藏的 +1。每次以为某 iter 已经"完成" 都该再问一次"用户怎么发现这个"。R-series 后期会反复 audit 这种"+1" — 老的 user-facing 行为很多还停在功能完整但 invisible 状态。

## Iter R23 设计要点（已实现）
- **Surface bug discovery 比 surface design 更值钱**：R23 起步是"加 hover breakdown" 这个 UX 改善。中途读代码发现 chip 用 base cooldown 而 gate 用 effective，**修了 6+ iter 没人注意的 bug**。这是 surface 工作的隐藏价值 — 强迫你重读已有代码、对照不同 caller 的逻辑，bug 自然浮出来。**每个"surface old signal" iter 都该顺手 audit signal 的所有 consumer 是否一致**。
- **derivation 比 value 信息密度高**：chip 显 "30m" 是 value，hover 显 "1800s × 0.5 × 2.0 = 1800s" 是 derivation。后者让用户从"看到结果"升级到"理解机制"。当用户问"为什么 30m" 时，derivation 直接答了；当 user 问"为什么是 30m" 时，value 啥也没说。**panel 应该越来越 explanation-rich**，每个数字都该在 hover 解释来源。
- **classify_feedback_band 抽出来 = 跨 caller 单一真相**：chip + gate 两个 caller，原本各自 inline R7 三档判断 logic。两份 copy 的风险是"修 R7 阈值忘改其中一处" → chip 显 "high_negative" 但 gate 实际还按 "mid" 走。抽 classify 出来 + 5 单测，**任何 R7 阈值改动只改这个 fn 的 ADAPT_* 常量，所有 caller 自动同步**。这是 R18 read_ai_insights_item 同思路 — single source of truth 在 logic 层面而不只数据层面。
- **mode_factor 用 division 而非 hardcode 表**：`after_mode / configured` 取代 `match mode { "chatty" => 0.5, ... }`。这种"用现有 helper 的输出反推 factor" 是优雅的，因为 future mode addition 不需要碰 chip 代码。**绕过 hardcode 表是 future-proofing 的小技巧** — 当一个数字可以 derived 时优先 derive 而不是重新声明。
- **修 bug 不分 commit**：诱惑是"R23 应该只做 surface"，bug fix 单开一个 commit。但 bug fix 没单独的 test fixture（只能靠观察 chip 数字验证），surface upgrade 的 hover breakdown 顺便就修了 bug — 同 source diff 的两件事。**逻辑 / 数据流 一致的多件事可以一起上**，分散到多 commit 反而失去 "为什么这两件事关联" 的 context。
- **structured payload vs strings**：feedback_band 是 `String` 不是 `enum`。理论上 enum + serde derive 更类型安全。但前端也只是 display label，String 让 backend / frontend / panel hover 三方共用同一文案。**rust enum 是 backend 内部表达**，IPC boundary 用 String 减 serde 复杂度。Rust 习惯是 enum-on-by-default，但跨 boundary 的 enum 反而是负担 —— 理解清楚什么时候 enum、什么时候 String 是在 Rust 写 fullstack 的成熟度信号。
- **`Vec<_> = (0..n).map(|_| entry()).collect()` 替代 `[entry; n]`**：FeedbackEntry 是 Clone 不是 Copy（含 String 字段）。`[item; n]` array literal 要 Copy；`vec![item; n]` 要 Clone（这才是 R23 测试可以用的）。第一版我用了 array literal 编译失败 — 学到 Rust array vs Vec 的细微区别。`(0..n).map(|_| ...).collect()` 是显式 fallback，绕开 Copy 约束。**测试代码也要 idiomatic**，反面是用 work-around 而忘记 Vec 的标准 init。

## Iter R22 设计要点（已实现）
- **read-only helper 与 mutating helper 分离的反例诱惑**：第一直觉是"复用 update_and_format_active_app_hint"。读它的实现 — 它每次都 `compute_active_duration` 然后写回 `*g = Some(new_snapshot)`。如果 prev_app != current_app，新 snapshot 的 since 是 now()，**就把"用户已经在 X 待 N 分钟" 重置成 0**。Panel poll 每几秒一次，会让 panel 永远看到"刚才进 X" — 错的。**任何"读"路径如果走过 mutating helper 都会污染 state**。教训：每次面对"是不是复用旧 helper" 的问题，先看那个 helper 是否 mutating；如果是，read 路径必须自己抽。
- **observability-wider-than-prompt 是 panel 设计的核心张力**：R15 prompt hint 设 15min 阈值因为"低于这个不值得 nudge LLM"。但 panel 不该照搬这个阈值 — user 想知道"我现在在哪" 即使停留 5 min。R20 mixed register / R22 < 15m active app 都是这个 pattern：**prompt = 异常时干预，panel = 全部 state**。所以 panel 应该 surface 更多 state，但用色彩区分"对 LLM fired vs 仅 observability"。橙色 = "正在影响 prompt"，灰色 = "panel only"。这种"色彩编码 prompt 是否 fired" 是这次 audit 中浮现的新模式 — R20 / R22 都用，可以推广到未来 chip 设计。
- **chip cluster 概念化的累积**：💬 feedback / 📏 register / 🔁 topic / 🪟 active_app — 这 4 个 chip 现在形成一个隐含 cluster（"宠物-用户互动状态"）。⏱ period / 📆 day / 👤 idle / ⏳ cooldown 是另一 cluster（"时间状态"）。**chip strip 现在是 self-organizing visual semantic**，新 chip 加入时该思考"我属于哪 cluster"，不是简单 append。R22 决策让 🪟 在两 cluster 之间的过渡位 — pet-user-interaction 后 / time-context 前 — 因为 active_app 既是"用户上下文" 也驱动"宠物开口判断"。
- **codified 原则的 audit 是有限工作**：R20 codify 后 R21 audit R11，R22 audit R15。第三个候选是 cross_day_hint (R14) / yesterday_recap_hint (R16) — 两者都是 first-of-day transient，panel 显示也只在 morning 第一次有效，剩下时间 chip 永远 null。**有些信号本质就 transient，强行 panel surface 价值低**。R20 原则的 audit 应该结束在 R22 — 剩下信号要么已 surface 要么 surface 没意义。**原则不是教条**，audit 到边界就停。
- **测试 logic vs 测试 wiring 的纪律**：snapshot_active_app 是 4 行：`lock → option → compute → redact → return`。每行都是 1-3 标准操作，没分支没异常。给它写"测试 mutex lock 成功 / 测试 redact 调用了" 价值近零，反而增加 test 维护成本。**单测应该追逐 logic（compute_active_duration 三分支 / format 阈值边界），不追 wiring（read 静态 + arithmetic）**。CI 的 cargo build + clippy + tsc 已经覆盖类型 wiring 正确性，单测覆盖 *behavior* 正确性。
- **panel surface 完成 R-series 的对称性**：R 系列从 R1 开始都是"加一个信号 / 调一个 gate / 写一个 reading hint"。R20+R21+R22 把累积的"prompt 写但 panel 不读" 一次性 audit 完。从此 R-series 加新信号会更短 — 因为新增的 prompt hint 会带 panel chip 一起设计，不会留新债。这种"建立原则 + 一次 audit + future 自动遵守" 是技术债管理的健康节奏。

## Iter R21 设计要点（已实现）
- **codified 原则的第一次还债**：R20 commit message 写下"所有 prompt 信号都该 panel 可见"作 codebase rule。R21 立即回头看老信号 R11，发现没 panel surface — 立刻还。**新原则不只指导 future iter，也应该 audit past iter** 找 violations。如果只对 future iter 生效，旧债永远等着；audit + back-fill 是原则真正落地的方式。
- **fetch 共享提取的时机**：build_tone_snapshot 原本 speech_register 字段 inline 自己 `recent_speeches(5).await`。R21 加 repeated_topic 时面对选择：(a) inline 第二次 await 同样数据；(b) 提到外部变量两个字段共享。选 (b) 因为这是单一函数体内的二次同源 fetch，**locality is preserved**（变量在最近的祖先 scope）但 IO 节省。如果是跨 module / 跨函数，外部状态/缓存就过 designed。**fetch 共享的设计成本应该匹配 fetch 共享的范围**。
- **不要打包独立概念到 aggregate fn**：诱惑是 `analyze_speech_signals(lines) -> { register, topic, ... }` 一个超级 fn 抓所有 5-line analyses。但 register 关心长度分布，topic 关心字符 ngram 重叠 — **数据流共享但分析维度独立**。打包 fn 必须 maintain 所有 sub-analysis 的相关性，未来加第三种 (e.g. emotion)、第四种 (e.g. question-vs-statement ratio) 都得改 aggregate signature。**保持 analysis fn 单一职责，只在 caller 层共享 fetch**。
- **chip 色彩语义稳定** = panel 视觉协议：R10 feedback 用红/绿/灰 三 band。R20 register 用橙（卡）/绿（健康）二色。R21 topic 用橙（卡）— 没绿色对应（因为 None 时根本不渲染）。三 chip 共享"橙 = anomaly worth noting" 一致，user 看到橙就知道"这个维度有 issue"。**panel 是 visual programming language**，色彩是其中的 keyword，应该有稳定语义。
- **chip 顺序 conceptual cluster**：feedback 💬 / register 📏 / topic 🔁 三个都关于"宠物开口"，所以视觉相邻；time period ⏱ / day_of_week 📆 / idle_register 👤 / cadence 💬 / cooldown ⏳ 是"上下文" cluster。**panel chip 不只 list，也要 group** — 相关 chip 放一起降低 user 的 mental scan cost。
- **redact 在 backend 而非 frontend**：诱惑是"backend 给 raw topic，frontend 自己 redact"。但 redact 的 settings.privacy.redaction_patterns 在 backend，frontend 没法直接读。**敏感数据净化 always at backend boundary** — 不让原始数据离开 backend 信任域。Defense in depth + clean separation。
- **R21 不加单测因为没新逻辑**：R11 detect_repeated_topic 已有 7 测。R20 classify_speech_register 已有 4 测。R21 只是 wire signal 到 ToneSnapshot 字段 + chip — Tauri Snapshot building 没单测（需要 fixture），但 cargo build / clippy / tsc clean 已经验类型对齐 wiring 正确。**单测应该覆盖 logic，wiring 测试由 type system + integration test 覆盖**。

## Iter R20 设计要点（已实现）
- **新原则：所有 prompt 信号都该 panel 可见**：R1c 把 dismiss 信号 surface 到 panel；R20 把 register 信号 surface 到 panel。这不是巧合 —— 两次都是回头补"信号写但没看见"的债。**新加 prompt hint 应该 same iter 加 panel chip** 而不是事后 follow-up。从此往后这条作 codebase rule，免得每过 2-3 iter 就要回来还一笔。
- **classifier 同源 + 双消费者形态各异**：R19 prompt 只要"long/short" 两态（mixed 不 nudge），R20 panel 要"long/short/mixed" 三态。诱惑是写两个 fn — `format_speech_length_hint` 和 `format_speech_register_chip`。但底层"看 5 条 speech 怎么分类" 就是一个判断，**应该一处实现**。抽 `classify_speech_register` → 两边 pull format。这是"data + view 分离" 在 Rust 里的实现 — pure data fn 出在 lib，view-specific transformer 各 module 自己写。
- **mixed 在 panel 是 first-class，在 prompt 是 silent**：同一种 state 不同 surface 的 visibility 应该独立判断。Panel 是 dashboard — 让 user "看到我的 pet 在 mixed register" 是 useful。Prompt 是 nudge channel — 没异常的时候不打扰 LLM 是 useful。**不要把 surface 决策和 classifier 决策耦合**。这次重构前 R19 collapse 了 mixed → ""，等于 classifier 给 view 让步；R20 把它矫正回独立。
- **颜色编码 = 信号 binary 化**：橙（卡）/ 绿（健康）的 chip 颜色是给 user 一眼判断的视觉密码。如果三态都用 gray，user 必须读文字才知道"这是好还是坏" — 多 100ms 认知。两色简化够用：长 / 短 都"卡"，所以同色（橙）；mixed 是"健康"另色（绿）。**用色彩做 binary classification + 文字做 detail** 是 dashboard 设计的成熟模式。
- **抽 classifier 时 Summary 字段要超 caller 当前需求**：mean_chars + samples + kind 三字段。R19 prompt 要 mean 和 samples（写在文案里）。R20 panel 当下只用 mean 和 kind。R-future（panel hover / prompt hint refinement / panel chart）可能要 samples。**helper return 应该匹配 callers 的 superset**，跟 R18 read_ai_insights_item 同思路 — 多 1-2 字段 cheap，砍了再回来加贵。
- **panel chip placement 跟 conceptual 邻居放一起**：📏 register 接 💬 feedback chip 后，因为两者都在 talk about "宠物开口的形态"（feedback = 用户怎么接受，register = 宠物怎么说）。⏱ period 是时间维度，单独一组。**chip ordering 是隐性 information architecture** — 相关 chip 视觉相邻让 user 把它们建立 mental cluster。
- **括号里 "(数字)" 而不是 "(avg N 字)"**：单 chip visual budget 极小（5-8 字内）。括号文字越短越好，hover 才放完整 "avg X 字 / 共 Y 句"。"📏 长（27）" 比 "📏 长 avg 27 字" 干净 4 倍。**panel 文字优化是反复练习的 craft**，每个 chip 都该问"hover 能塞的为什么放 chip 上"。

## Iter R19 设计要点（已实现）
- **register variance 是"像真人" 的关键 micro-cue**：内容多样性 (R11) + 时间分布 (R7 cooldown) + 长度多样性 (R19) 是三条独立的"机器化 vs 人化" 维度。三条任意一条单 register（同话题 / 同时间 / 同长度）都会让 pet 显得 robotic。R11 检测话题重复后，R19 是同一思路在 length 维度的延伸。**让 LLM 自己看到自己的统计** 是 prompt design 的强招 — 它不会自审 character distribution，但你给它"你最近 5 句平均 30 字" 的硬数字，它会调整。
- **"全或无" 比"variance metric" 更稳**：诱惑是 std deviation < 5 chars → 单 register。但 5 个样本计 std dev 噪声极大，几个 outlier 就让阈值进进出出 thrash。"全部 ≥ 25 → 警告偏长，全部 ≤ 8 → 警告偏短，混合 → 不警告" 是简单 gate，不会因边界 case 在两次 tick 间反复 fire。**判定函数应该有 hysteresis**，全或无天然有 — 一旦混合就立刻安静，不会摇摆。
- **`chars().count()` vs `len()` 是低级 bug 高发区**：Rust `String::len()` 返字节数，UTF-8 中文 1 字 = 3 字节。`"今"`.len() = 3，`"今"`.chars().count() = 1。用 len() 写 char threshold 会让中文都被误判超长。R19 显式 chars().count() + 专门 test 钉住中文不 misjudge。**所有"字符数"逻辑应当 chars().count()**，这是 Rust 写 i18n 代码的硬性纪律。
- **Empty-line filter 在 percentile 之前**：log 文件 corruption 偶有 "<ts> <empty content>" 行（写入失败时 partial）。如果直接算 mean，empty (count=0) 会拉低均值导致"偏短" 误判。**清理脏数据是任何统计的预步**，R19 这点呼应 R11 的 "skip whitespace-bearing window" — 都是用 filter 在 stat 之前去噪。
- **复用 fetch binding 是 R11 IDEA 写过的经济**：speech_hint + repeated_topic_hint + length_register_hint 三层都来自同一份 recent_speeches(5)。一次 fetch 三层洞察 — 避免 disk read 重复，避免 stale read（如果三层各自 fetch，理论上中间可能写入新一句导致两层看到不同窗口）。**单一 fetch + 多层 transform** 是 prompt building 的 cleanest pattern。
- **静默兜底 = 不 over-corrective**：mixed register 不报警，让 pet 安静地做对的事。诱惑是"无论什么状态都给 LLM 一句反馈" — 但那会让 prompt 变成"教师反复点评学生"。**只在异常 deviation 时给信号**，正常状态留 LLM 自由发挥。这是 prompt budget 经济。
- **25/8 char 阈值经验拍**：没有 systematic 数据 — 直觉判断"日常对话 5-30 字"。25 是"已经偏 essay" 的边界，8 是"已经偏 emoji" 的边界。可改 settings 但暂时不做 —— 用户反馈没有"length 不对" 之前先观察 default 行为。**避免 settings 膨胀** 也是产品成熟度的体现：每多一个旋钮就多一份"用户该 / 不该调"的认知负担。

## Iter R18 设计要点（已实现）
- **"等到第 N 次再抽象" 比"看到 2 次就抽象" 健康得多**：R16 IDEA 写下"当 helper 数到 6 时强制 refactor"。R17 把数字推到 7。R18 抽。**lazy abstraction** 的好处：(1) 让具体调用点先涌现各种变化（有的要 description、有的要 updated_at、有的要 trim、有的要 default），帮助你设计正确签名；(2) 避免给只用过 1-2 次的"抽象" 浪费命名/位置思考。premature abstraction = 选错位置 / 选错签名 / 选错命名概率高。看到 7 次同样模式时，签名设计已被实战验证过 — 抽出来一气呵成。
- **返 cloned struct 比返单字段精简**：诱惑是 `fn read_ai_insights_description(title) -> Option<String>` — caller 主路径就是要 description。但 helper 要服 6 种 caller，其中 2 种要 updated_at。如果做"description 专用 helper"，updated_at caller 还得 inline 写老 boilerplate，refactor 不彻底。**helper 的 surface area 应该匹配 callers 的 superset**，不是匹配 majority caller。clone() 在这里是 cheap insurance — 字符串短 / 调用频率低 / 收益大。
- **Pattern A 抽，Pattern B 不抽**：6 个 caller 都是"找特定 title 的单条" (Pattern A)；1 个 caller 是"遍历整个 category 过滤" (Pattern B)。把 A 抽成 helper，B 留 inline。**抽象单一 modality**，不强求 generic — 通用 `query_ai_insights<F: Fn(&MemoryItem) -> bool>(predicate)` 看着优雅但只有 1 个 Pattern B caller，其本身复杂度大于目前的 inline boilerplate。R12 IDEA 已写过"single caller 不抽抽象" 的纪律，R18 再次实践。
- **重命名 vs 用旧名**：原本 4 个 helper 都叫 `read_*` (read_daily_plan_description, read_daily_review_description)。新 helper 起名 read_ai_insights_item 跟它们 family 一致。命名一致性让 grep / 阅读 codebase 时 mental model 稳定 — 看到 `read_*_*` 知道是"按某种条件读 memory"。**命名是文档的隐藏部分** — 不一致的命名让读者每次需要重新建立"这是什么类型函数" 的判断。
- **抽 helper ≠ 必然 LOC 减少**：6 个原 boilerplate 段落每段 8-10 行 = 50-60 行总。new helper 5 行 + 6 个 caller refactor 后每个 1-3 行 = 13-23 行调用 + 5 helper = 18-28 行。**净减少 ~30 行**。但 LOC 不是抽 helper 的真正目标 —— 真正目标是**单点真相**：未来要改"读 ai_insights"语义（比如加缓存、加 panic-safe 包装、加 metrics）只改一处而不是 6 处。"refactor 主要为 LOC 减" 是错误价值观；"refactor 主要为 single-point change" 才对。
- **fmt 的换行重写 = 隐藏的代码风格审查**：build_plan_hint 改完后 fmt 把 `format_plan_hint(&description, &|s| crate::redaction::redact_with_settings(s))` 强制换成 multi-line，保持 80-col。这种 fmt 自动调整反而暴露代码 readability —— 一行写不下意味着 inlined arg 太多，应该考虑提个变量。R18 这个 case 是 closure literal 太长，不是真问题。**rustfmt 是隐性 reviewer**。

## Iter R17 设计要点（已实现）
- **每个写入信号都欠一笔 retention 债**：R12 写 daily_review 当时只考虑了 happy path（"每天写一条很好"），没考虑 365 天后会怎样。**任何长期 append 数据流都隐藏一个未付的 retention 设计** — reminder 有 stale_reminder_hours，plan 有 stale_plan_hours，butler-once 有 stale_once_butler_hours，daily_review 终于有 stale_daily_review_days。R17 是补 R12 时欠下的债。规则：**新建 append-style 数据流时立即想好 retention 策略**，否则下一次 R-iter 就要回来还。
- **schema-based protection > hardcoded allowlist**：sweep 用 `parse_daily_review_date` 的 None/Some 来决定要不要删，不是维护一个"protected_titles = ['current_mood', 'daily_plan', 'persona_summary']"。后者每次加新 protected 项都得改 sweep 函数 + 测试。前者 schema 自然演化 — 加新 item title 不需要碰 sweep 代码，只要 title 不匹配 daily_review_YYYY-MM-DD pattern 就自动安全。**让 schema 自己说话** > **维护 list 同步**。
- **retention=0 = disabled，不是 aggressive**：fail-safe 默认是数据保留方向。如果代码哲学是"0 = 立刻全删"，用户误配置 0 → 历史 review 全没 → 不可逆。代码哲学是"0 = 保留"，用户误配置 0 → 没人删 → 慢慢手动 spotted → 可恢复。**删除是不可逆的，保留是可逆的** — 在不可逆操作上设 conservative default。
- **30 天默认 sweet spot**：考虑过 7 / 14 / 30 / 90 / 365。7-14 太激进（"我上周写的回顾呢？"）；365 不够侵略性（panel 列表过载）；90 也合理但 30 对应"过去一个月" 是用户日常 retrospective 的自然单位。**用户能直觉描述的时间窗 ≈ 月** 是 default 的 anchor 点。
- **`>` 而非 `>=` 给边界 1 天 buffer**：delta == 30 不删，delta == 31 才删 — 等于"在第 31 天才丢掉"。tests 里 stale_review_returns_false_within_retention_window 钉住这个边界（4 月 4 号的 review 在 5 月 4 号是 delta=30，不删）。这种"strict gt 给边界缓冲"是 UI / time-bound 数据的友好惯例，避免"恰好满 30 天" 用户误以为还在但已经丢了。
- **signed_duration_since 处理时间倒流**：用户改系统时间 / yaml 手写 future date / clock skew 都可能让 review.date > today。chrono 的 signed_duration_since 返负数，num_days() < retention_days → not stale。**比 `today - date` 然后 unwrap 安全得多**。R12b [1/0] 同思路 — graceful degradation 优先 panic。Rust 的 chrono::Duration 也有 saturating semantics 是对的设计。
- **同步 sweep 优于 async sweep（除非有 async 依赖）**：butler-once sweep 是 async 因为要写 butler_history。reminder / plan / daily_review sweep 都不需要 history — 同步函数 + 同步 memory_edit 完全够。**不要为了"看起来现代化"而无脑 async**。async overhead = 调用方必须 .await + 必须在 runtime 里 + 错误处理变复杂。能同步就同步是 Rust 风格。

## Iter R1c 设计要点（已实现）
- **"写到" ≠ "看到"**：R1b 把 Dismissed 信号写进 feedback log + 接到 R7 ratio 计算，技术上完整。但 panel UI 依然只显示二元"回复/忽略"。这是个 hidden invariant 违例：**新增数据维度后，相关 surface 必须同步**，否则用户看到 panel 数字以为是旧二元，对系统行为产生误解（"我点了 dismiss 为什么 cooldown 没变?" → 实际变了，但没显示给用户看）。R1c 把这条 invariant 主动还掉。
- **"replied/total" 仍然是正确的中心数字**：诱惑是改成 "replied / (ignored + dismissed)" 或三元数字。但 panel 是稀缺 visual budget，单 chip 只能讲 1-2 个数字。R7 adapter 阈值（>0.6 negative）跟"replied/total"是简单互补关系（ratio + negative = 1），所以 "replied/total" 已经完整表达了 cooldown 决策输入。dismissed 是细节维度，做尾巴小字而不是抢主标题。**主信号占 C 位、辅信号做修饰** 是 dashboard 设计经典原则。
- **颜色对应信号强度梯度**：绿（replied 正）→ 灰（ignored 弱负）→ 红（dismissed 强负）。从左到右视觉重量递增，匹配 sentiment 强度。这种"色彩-语义一致" 是 panel 设计的 nice-to-have 但很值钱 — 用户扫一眼就 build 起 mental model，不需要 hover。
- **`👋` icon 选型背后**：考虑过 ❌（拒绝感太强，user 不一定真的"恨"这条）/ 🚫（formal）/ 🗙（uncommon）/ 🚪（"出门"含义不直接）/ 👋（挥手 = 软告别）。最后选 👋 是因为它最匹配"我看到了，但 not now" 的实际心情 — confrontational 程度刚好。Iconography 选择不是装饰，是**语义打包**。
- **timeline pill hover 文案做"自我解释"**：写"5s 内主动点掉 — 比被动忽略信号更强" 看似冗余（pill 颜色已经表达了），但**panel 不只是给我自己看，也是给未来回看的我或别人看**。一个月后用户看 panel 不记得 R1b 是怎么实现的、为什么红色比灰色"严重"，hover 文案就是 inline 文档。每个新 chip / pill 都应该带"为什么"。
- **dismissed 字段加在已有 struct 而非新建**：诱惑是新建一个 `DismissSummary { count }`。但 dismissed 只是 feedback 的细分；强行划分子结构反而割裂 mental model。"加字段不加结构"是 schema 经济原则 — 只在确实有 *独立生命周期 / 独立 caller* 的概念才拆新结构。dismissed 跟 replied 是同一个 feedback 数据流的不同切片，留在 FeedbackSummary 内最自然。
- **dismissed 后缀条件渲染**：`dismissed > 0 && <span>...</span>` 而不是永显 "👋0"。"显示 0" 是错的 — 用户大部分时间 dismissed 就是 0，永远显示一个 0 是 visual 噪音，让其他更有意义的 chip 视觉权重被稀释。Zero state 应该 *invisible* 而不是 *empty*。

## Iter R1b 设计要点（已实现）
- **frontend gate vs backend gate 是 UX 边界判断**：threshold "5 秒内 = quick" 这个数字属于"用户视角的快慢" 不是业务规则。frontend 决定。后端 record_bubble_dismissed 只接受请求 — 信任 caller 已经做了过滤。这种 separation 让"调阈值" 是改 1 行 TS const 而不是后端逻辑变更。一般原则：**UX 决策不该跨语言**。
- **rename `ignore_ratio` → `negative_signal_ratio` 是必要的**：诱惑是"加 Dismissed 但保持 ignore_ratio 名字" — 短期省 LOC 但长期误导（"为什么 ignore_ratio 包含 dismissed 项？"）。语义扩展时**rename 比加注释更便宜**。caller 只在 gate.rs 单点（grep 只 2 处），rename 成本极低。这是"refactor 时择捷径还是择正确" 的小型例题：选正确。
- **uniform weight 比 weighted 计入 ratio**：Dismissed 应该算 1.5 ignored 吗？理论上是。但加权后 R7 step function 不再"心算友好" — panel 看到 4 个 dismissed + 6 个 replied 就知道 ratio 0.4，加权变 4×1.5/10 = 0.6 完全不同直觉。"auditable simplicity" 优先。Dismissed 信号自然比 Ignored 强这件事**会通过 frequency 自己说话** — 用户真不喜欢就会经常 click，ratio 自然 push 到 1.0。
- **双信号容忍是 feature 不是 bug**：dismiss + 下一 tick ignored 双计 = 同一个负事件在 ratio 里被算 2 次。第一感觉是"重复计算" 想 dedup。但实际上"用户 click 立刻拒绝 + 整段窗口完全没回" 比"只是被动没看到" 是不同强度的负反馈。算 2 次正好让 ratio 反映"这个 turn 是真负面" — emergent weighting 通过 event count 而不是 multiplier。设计哲学：**让 multi-event 的频率代替显式 weight**。
- **bubbleShownAt 用 useRef 不用 useState**：阈值判断只读，不触发 re-render。用 useRef 避免 setShownAt(timestamp) → component re-render → useEffect 又 reset 一遍 → 死循环。这是 React 中"参数化的 mutable state" vs "影响 render 的 state" 的经典区分 — 前者用 useRef，后者用 useState。
- **`onClick` prop optional + cursor 配套切换**：ChatBubble 单独 import 时（如 storybook / 其他 view）不一定有 dismiss 回调，cursor 该是 default 而不是 pointer 误导用户"这能点"。`cursor: onClick ? "pointer" : "default"` 把 affordance 跟 capability 绑定，干净。
- **向 R7 添加输入信号比改 R7 输出公式更便宜**：本来想做"R7b: Dismissed 触发更激进 cooldown 翻倍"，但发现只要把 Dismissed 计入 ratio，原 R7 step function（>0.6 翻倍）已经响应正确 — 多用户 dismiss → ratio↑ → 自动翻倍。**新信号 + 现有 adapter** 比 **现有信号 + 新 adapter** 通常更便宜，因为 adapter 是 gate-critical path 一动得验全套。

## Iter R16 设计要点（已实现）
- **写→读对称是 memory subsystem 的隐藏 invariant**：R12 / R12b 写 review 是上半场，但**写完不读 = 写了等于没写**。系统设计里"someone writes X, eventually someone reads X" 是 implicit invariant — 一旦发现"写完没人读"，要么是死代码，要么是漏了 read 路径。R16 是对这条 invariant 的还债。每加一种 memory write，都要 ask "what reads this?" — 没答案就先别加 write。
- **两层 hint 互补 vs 单层合并**：第一直觉是"R14 已经有 cross_day_hint，把 yesterday recap 拼进去就行"。但两者**信息颗粒度不同**：recap 是"全貌摘要"（昨天主动开口 7 次，计划 3/5），尾声是"具体片段"（最后两句的内容）。**合并会让两个不同分辨率的信号挤在一行模糊化**。分开 push 让 LLM 可以独立选择 — 它可以"今天先用 recap 总结打开"或"直接续昨晚最后那句话题"。"高密度 + 低密度" 信号应该独立可见。
- **first-of-day 三层 callback 收齐**：截至 R16，first-of-day 的 prompt 现在含 (1) 时间问候段（已存在）+ (2) cross_day_hint 尾声 (R14) + (3) yesterday_recap_hint 总览 (R16) — 三层各司其职。这个层叠是有边际效用递减的，再加第四层（如"过去 7 天 trend"）不太可能再 step-up；R16 应该是 first-of-day prompt 的收官。后续早起感知如果还要做，应该是 prompt-side 的对齐 / formatter 优化，而不是再 inject 第四层。
- **`replacen(.., 1)` 是 future-proofing 模式**：当前 deterministic description 永远只有一个"今天"。但 R12c LLM-summary 可能产生"今天我们一起聊了 X，今天计划完成了 Y" 这种重复"今天"的句子。`replacen(.., 1)` 在两个场景都正确：deterministic 时只有一个所以替不替都行，LLM 时只换开头保留语义。提前选 `replacen(1)` 不是无意义谨慎 — 是为已知未来变更留 schema 容忍度。
- **走 description 不走 detail 是 prompt budget 经济**：detail .md 是完整全天 speeches bullet list（可能 30+ 行）。description 是单行高密度摘要。Prompt 是有 token 预算的稀缺资源，能用一行说清楚就别用 30 行。这反过来也说明了 R12b 把 plan progress 编码进 description 是对的 — 越浓缩的 description 喂 prompt 越合算。
- **proactive.rs 里"读 memory 类别"的 helper 已经堆了 4 个**：read_current_mood / read_persona_summary / read_daily_plan_description / read_daily_review_description。每个都是 8 行 boilerplate（`memory_list → categories.get → items.iter().find`）。如果再多 1-2 个就值得抽成 `read_ai_insights_item(title) -> Option<&MemoryItem>` 共享 helper。先记下来 — 当 helper 数到 6 时强制 refactor，避免 pattern 复制蔓延。"先重复，等到第 6 次再抽象" 是 lazy abstraction 的纪律。

## Iter R12b 设计要点（已实现）
- **deterministic refinement 比 LLM upgrade 优先**：R12 留下的 R12b 原本计划是"LLM 一句话总结"。但深入看了下，需要的依赖：AiConfig / McpManagerStore / LogStore / ShellStore / ProcessCountersStore / ChatMessage / CollectingSink / run_chat_pipeline。把现有 clock-pure module 改成 app-aware 是非平凡 refactor。而 description 缺信号这个具体痛点（"有计划"太空洞）有更便宜的解 — 复用 daily_plan 已有的 `[N/M]` 标记。**先做便宜的高 ROI 升级，把 LLM 版本拆成独立 R12c**。这是"把一个大 iter 拆成多个小 iter" 的实践 — R12 + R12b 一起上线，LLM 升级独立排队。
- **`[N/M]` parser 设计要 robust against schema collision**：codebase 里方括号有多种用途：`[N/M]` 进度、`[remind: HH:MM]` reminder、`[every: HH:MM]` butler schedule、`[once: YYYY-MM-DD HH:MM]` once-fire、`[review]` 前缀、`[motion: Tap]` mood、`[error: ...]` failure。如果 parser 直接 split_once('/') 不验证 — `[remind: 09:00]` 会把 "remind: 09" / "00" 当成数字（实际不会因为 "remind:" 含字母）。但 `[19/05/03]` 这样的日期会真的被误算（虽然没人这么写）。strict digit-only check 是 minimal 但有效的 defense。**多 schema 共用同一种 syntax 的代价 = parser 要做 disambiguation**。
- **`[1/0]` skip 是 graceful degradation**：完美的 case 是 plan 永远不出现 zero-total marker。但代码要 robust to user 误输入。用户写"· task [1/0]" 大概是 typo（想写 [1/10]），在这种 case 下：(a) 不要 panic / divide-by-zero (b) 不要让整个 review 失败 (c) skip 这条但保留有效的邻居。"软失败 + 继续"是 user-input parser 的金标准。
- **u32 saturating_add**：理论上 plan 不会超过 100 条 / 单条不会超过 999/999。saturating_add 与其说是必要不如说是 *cheap insurance* — 多打几个字节但永远不会因为 overflow panic。在 Rust 里整数运算的 default 是 panic on overflow（debug）+ wrap（release），saturating 比其他 mode 都更"贴近用户意图"（"满了就停在最大"）。统一用它是好品味。
- **三分支 description 排版选择**：1) `Some((c,t))` → "计划 c/t"（具体）；2) `None, has_plan` → "有计划"（兜底）；3) `None, !has_plan` → 无后缀。诱惑是把 (2) 删了 —— "如果没有 progress markers 就也不显示 plan suffix"。但有用户写 "· 自由文本计划" — 没 marker 但确实有计划。如果完全略过会让 description 误导（"今天主动开口 5 次" 听起来像没定计划）。"有计划" 兜底比 "" 兜底更准确。**不要为了简化代码删信号**。
- **解析器位置 = 跟它服务的 formatter 同 module**：`parse_plan_progress` 放 daily_review.rs 而不是 plan_assembler.rs / time_helpers.rs。原则：纯计算放在 *最接近其唯一 caller* 的 module — 谁用就放谁旁边。如果未来另一个 caller 也需要 `[N/M]` 解析（如 panel UI），再 hoist 到共享位置。**premature abstraction = bad**；late abstraction = better。

## Iter R12 设计要点（已实现）
- **deterministic vs LLM 总结分两步**：第一直觉是"R12 必须有 LLM 一句话总结，否则不是 review"。但实测 deterministic bullet list 已经回答了"今天发生了什么"的核心问题。LLM 总结是 polish，不是 fundament。先把 deterministic 路径打通 + 完成 idempotency / 触发逻辑 / memory schema —— 这些 LLM 升级路径后也用得上。R12b 仅替换 description 文案 + 可选追加 detail 顶部的总结段，对底层 schema 零冲击。"先 backend 后 polish" 在多步 feature 里是通用模式。
- **22:00 trigger gate 的"first tick after"语义**：天真做法是开个 cron-like 后台 task 在 22:00:00 准点 fire。但 (a) 多一个 background loop 是多一个失败点；(b) 用户不在桌前的 22:00 fire 没意义；(c) Tauri 的进程模型不保证后台 task 在 22:00 还活着。复用 proactive loop tick + "first eligible tick wins" — 用户人在的时候才 review，逻辑简单 + 不需新基础设施。这是 R15 active_app "复用 proactive cadence" 模式的延续。
- **双重 idempotency 是必然的**：单纯 LAST_DAILY_REVIEW_DATE（进程内 mutex）会在 app restart 后 None — 如果用户 22:30 review 完后 23:00 重启 app，下次 tick 再 fire → 二次写入。单纯 index existence 检查 O(n) 每 tick 都查，量大就慢。两者叠加：fast path（mutex 命中）跳过 disk read，cold start 才查 disk。这是 cache + persistence 的经典层叠 pattern。
- **title 用 date 后缀而非 daily_plan 单条覆盖式**：daily_plan 是"今天的目标"——只有一份才有意义，明天会被新 plan 覆盖。daily_review 是"每天的日记"——每天独立才能"翻看"。两者用不同 schema 反映了"覆盖型 vs append 型" memory 的本质区别。如果做"宠物的回忆录"功能，date-suffixed schema 让 panel 可以直接列出最近 7 天 review，daily_plan-style 单条覆盖就做不到。
- **`[review]` description 前缀是为未来 R12b 留接口**：第一次写 description 是 deterministic 的"今天主动开口 N 次"。R12b 升级 LLM 总结后会变成"今天我们一起..."。两种格式都需要被识别（panel UI / future prompt），加 [review] 前缀是 namespace 划分 — 类似 [error:] / [every:] / [once:] 这些 schema 标记。在 codebase 里已经形成统一惯例，新场景沿用。
- **silent write 不进 mood / speech_history**：诱惑是"review 写完后 push 一条 'review 完成' 到 chat 里"。但那会 (a) 占今天的 chatty quota 影响后续判断 (b) 让 review 看起来是"宠物开口" 但其实是后台沉淀。silent 是正确的 trade-off — review artifact 沉默存在 memory 里，等明天 prompt / panel UI 主动读它，是"宠物大脑长期记忆" 而不是"宠物当下话语"。
- **R12 把 R14 的"昨日尾声"升级到"昨日全貌"的可能性**：R14 提取昨日最后 2 条 speeches 作 cross_day_hint。有了 R12 之后，理论上下一步可以让 cross_day_hint 改读昨日 review.md 的"今日开口记录" 段——拿到昨日全 speeches + 计划完成度，比尾声 2 条更丰富。但 (a) prompt 长度膨胀、(b) 当下"昨晚最后说过的话"反而比"昨日全部"更适合作开场白引子。这是 信息密度 vs 信息量 trade-off — R14 留 2 条本身是判断过的，R12 sediment 不强求 R14 改 schema。

## Iter R15 设计要点（已实现）
- **后台 baseline vs LLM-call tool**：get_active_window 是 LLM 自助 tool — 它要主动调才有数据。Iter R4 已经看到 env tool spoke_with_any 比例不高，意味着 LLM 经常"开口前没看一眼"。R15 不依赖 LLM 主动 — 后台每 tick 拉，注入 prompt 当 baseline。"LLM 自由 tool" + "loop 强制 baseline" 双轨提供同源数据：LLM 想精确就调 tool，想顺手就读 hint。
- **15 分钟阈值 = 信号-噪声 trade-off**：1 分钟太敏感（"用户在 Slack 里 1 分钟" 没意义）；30 分钟太钝（错过"专注 20 分钟该歇一下"窗口）。15 分钟 ≈ 一次 deep-work 段 / 一次会议 / 一次专注阅读。低于这个值 hint 完全不出现，避免噪声污染 prompt。是"sparseness as a feature" 的应用。
- **redact 在 format 时不在 snapshot 时**：snapshot 留原文是为了**transition 比较稳定**。如果 user 中途增加 redaction pattern "Cursor"，已经在用 Cursor 的他下次 tick 拿到 redacted "[redacted]"，跟 prev snapshot "Cursor" 比较会假 trigger 一个 app change → since 重置 → 就此永远算不出"已经待了多久"。raw 留 snapshot + 仅 format 时 redact 解决该 race。
- **Instant 而非 SystemTime**：monotonic clock 不受用户调时区 / 系统休眠 / NTP 校正污染。"用户离开 8 小时回来，前台还是 Cursor" 用 SystemTime 算可能给出 480 分钟（实际只在使用中），用 Instant 应该也给 480 分钟（这里 Instant 不暂停） — 边缘 case 跨长 sleep 暴露 noise。但 saturating_duration_since 让 monotonic 的"时间倒流"不会 panic（时区调整下 SystemTime 反而会）。这次选 Instant 是 "less worst" 决策。
- **osascript 复用 = 单一事实源**：把 system_tools 的 osascript 抽成 `current_active_window()` 纯 fn，让 tool path 和 loop path 同源。如果之后改 osascript（比如加 PID），改一处所有 caller 同步。"DRY 但不过度" 的应用 — 不抽象 logging / redaction（两 path 需求不同），但抽象核心数据 fetch（两 path 完全一致）。
- **粒度=interval_seconds（不另起 background loop）**：诱惑是"开个 1 分钟 tick 的轻量 loop 专门追踪 active app，更高分辨率"。但 (a) 高分辨率被 15min 阈值过滤掉了 (b) 短期跳变本来就不该 surface (c) 多一个 loop 是多一个失败点 + 多一个 osascript 调用源。复用 proactive loop 的 5min cadence = 0 额外开销 + 行为正确。"做最少的事" 在系统设计中常胜。
- **R15 把"在做什么"加进 R14 的"做了什么"上**：R14 是"昨晚说过 X" — 历史轴。R15 是"现在在 Y" — 实时轴。两轴正交补全：pet 现在既看得到时间深度（昨晚→今天）也看得到当下宽度（在 Cursor 写代码 / 在 Slack 沟通 / 在 Safari 浏览）。是 companion grade 体感的两个支柱。

## Iter R14 设计要点（已实现）
- **跨日叙事是 companion 体感的关键 step-up**：之前 pet 每天都是"重启"，最多 R9 让 reactive 看到 bubble 当天历史。但"昨天我们一起经历了 X"是真实朋友的心智 — pet 必须在"叙事时间轴" 上活下去。R14 把 first-of-day 当成"今天的开场白" moment 注入昨晚尾声，是叙事连续性的最低成本实现。
- **first-of-day 复用是经济**：today_speech_count == 0 信号已经存在（drives first-of-day rule label）。R14 piggy-back 这个信号触发额外 hint —— 不需要新计数器、新 state。"在已有信号的边缘加新行为" 比"加新信号" 好得多。
- **2 条窗口 vs 5 条**：诱惑是"既然在做这个，给 LLM 多看点 history 让它选"。但 2 条已经对应"昨晚的尾声"——更多让 LLM 在多个昨日话题里挑反而引入噪音。R14 是"启发性 hint" 不是"全 context dump"。
- **"自然能续上就续，不必生硬呼应" 是 prompt design 关键**：直接说 "请承接昨天" 会让 LLM 强行复读 → 显眼尴尬。给 LLM 自由度让它自己判断是否合适承接。这种"建议 + escape hatch" 在 prompt design 中比"硬指令" 表现好得多。
- **NaiveDate parameter 让测试免依赖系统时钟**：speeches_for_date 接 target_date 而不是从 chrono::Local::now() 算。`now - 1 day` 是 caller 的责任。测试可以输入"2026-05-03" + 自由生成 sample，不依赖测试运行的实际日期。这是 D series time_helpers 一直延续的设计原则。
- **时间戳过滤的时区行为是 surprise minimization**：用 `with_timezone(&chrono::Local).date_naive()` 把 RFC3339 timestamp 转本地时区再算 date。用户旅行跨时区时"昨天"按当前本地时区判，符合"我现在的昨天" 心智模型。如果 future 想 enforce 一种 timezone 可以再扩展。
- **R14 是 R9 的跨日扩展**：R9 让 reactive 看到 bubble 当天历史；R14 让 proactive 在新一天注入昨天历史。两者一个 reactive、一个 proactive，一个当天、一个跨天 — 拼成完整 "pet remembers" 的体感拼图。

## Iter R13 设计要点（已实现）
- **高层级 dial vs 低层级 knob**：cooldown_seconds + chatty_threshold 是工程师 mental model（"我想 30 分钟一次 + 5 句封顶"）。普通用户的 mental model 是"今天我希望宠物多说还是少说"。companion_mode 把后者直接 surface，前者作为底层用户可微调。两者并存的好处：高级用户精调，普通用户预设。
- **String + fallback 比 enum 更宽容**：`enum CompanionMode { Balanced, Chatty, Quiet }` 看着 type-safe，但 (a) serde 序列化 enum 的 case-sensitivity 容易翻车（"Chatty" vs "chatty"）；(b) 用户手改 yaml 拼错 → reject 整个 settings 加载；(c) 未来加 mode 还要改 enum + serde。String + match + `_ =>` fallback 的代码 LOC 更少 + behavior 更宽容。这次选 String 是经过权衡的。
- **`effective_*()` method on ProactiveConfig**：原本想各自 caller 调 `apply_companion_mode(&cfg.companion_mode, cfg.cooldown_seconds, cfg.chatty_day_threshold).1`。冗长 + 容易写错（拿 .0 vs .1）。method 把它封装成 cfg.effective_chatty_threshold() 一行。这是"对外暴露简单接口、隐藏内部协调" 的经典 OOP 优势。Rust 用 impl block 拿到同样收益。
- **layered 设计：mode → R7 → 实际 cooldown**：用户 mode 选"chatty" → base 减半。然后实际跑了一段时间，R7 看到"用户其实经常忽略" → 在 mode 减半的 base 上再 ×2 还原回原值。两层叠加产生"用户想 chatty 但实际效果是 balanced" 的自适应行为。这种"高层意图 × 实测自适应" 是良好控制系统的健康架构。
- **base=0 invariant 重申**：用户故意把 cooldown_seconds 设为 0（关闭 cooldown gate）后，无论 mode 怎么选都应该保持 0。`apply_companion_mode("chatty", 0, 0) -> (0, 0)` 通过整数除法自然成立，加 explicit test 钉死。同 R7 的 zero-base 处理。
- **frontend UI 暂缺**：用户得手改 yaml 才能换 mode。这是 backend-first 路线的常见 trade-off — 后端契约稳定后，前端 dropdown 是 1-day iter（label / option / save → 调 save_config_raw）。R13b 留位。

## Iter R11 设计要点（已实现）
- **machine 检测 vs LLM 自觉**：speech_hint 已经把过去 5 条 bullet list 给 LLM 看了，原则上 LLM 应该自觉避免重复。但实践中 LLM 在 prompt 整体很长时容易忽略 bullet list 的内容（"我看到了但没真的对照"）。R11 用机器代替 LLM 做对照，给出**结构化警报** "你说了 N 次 X" — 比让 LLM 自审更强信号。这是"explicit 比 implicit 强" 在 prompt 设计中的应用。
- **char ngram 是 Chinese-friendly 的 lazy tokenization**：jieba / pkuseg 之类真分词依赖外部库 + Chinese training data + 大量启动成本。4-char sliding window 是 0-deps 的"足够好" 近似 — 4-gram 在 Chinese 里对应"双词组"的 95% 情况（"工作进展" / "项目早会" / "周末计划" 等）。stop-word 过滤通过 whitespace/uniform-char skip 简单规避。这种"用编程语言原生工具做 80% 的工作" 在多语言场景下经常优于"上重型 NLP 库"。
- **空格 + uniform-char 这两个 skip 规则**：从 false positive case 反推。空格 → 跨词边界（"了 哎"）；uniform-char → filler（"嗯嗯嗯嗯"）。这两个简单 rule 把误判率显著降下来，不需要复杂 stopword list。production 中如果再发现新 false positive 模式，加第三第四 rule。
- **windowing parameter 对齐 R7 / R10 / R6**：3-of-5 ratio 与 R7 cooldown 同源思路（>60% trigger）。"60% repetition = 显著重复"是数字直觉。其他位置如果要"显著" threshold 也用 60%。
- **redact 过 ngram 输出**：detector 不读 settings，只看 raw text。如果用户名是"张三"且最近 3 条都提到 "和张三", 4-gram "和张三同" 可能命中。过 redact 防止 ngram 文字本身泄漏。是 QG4 redact-on-reinjection pattern 的延续。
- **复用 recent_speeches 单次 fetch**：原 speech_hint 调一次 recent_speeches(5)。改成 binding 后 speech_hint + repeated_topic_hint 两层共用。零额外 IO，相同窗口意味着两层 hint 一致 mental model。微小但合理的优化。

## Iter R10 设计要点（已实现）
- **R series 的"chip 化"是 R6/R7 之后的自然下一步**：R6 在 PanelDebug 加了反馈 timeline，R7 用 ratio 改 cooldown。但 timeline collapsible 默认收起，用户日常 panel 看不到。chip 化让"宠物现在被听见多少" 进入 always-visible 一行。这是 D series chip strip 的延续 design pattern：每个有意义的 binary/ratio signal 应该有一个 chip。
- **chip 颜色与 R7 adapter 临界点对齐**：>0.6 红 / <0.2 绿 / else 灰，正好对应 cooldown ×2 / ×0.7 / unchanged。这种"chip 颜色 = 系统行为预测器" 的契约让用户的 mental model 很清晰：看到红色 → 知道宠物会自动安静下来 → 不需要 manual settings 调整。
- **共用 20 entries 窗口的健康选择**：R6 panel 显示 20 条 + R7 gate 用 20 条 + R10 chip 用 20 条。三处共用同一个 magic number 让"看到的就是发生的"。如果将来某层想换窗口（比如 R7 想用 50 条更稳）该单独提取一个 const，但 yet ROI 不到。
- **chip 是 ratio 的 ambient surface vs timeline 的 detail surface**：UX hierarchy 清晰：chip 看一眼"是不是有问题"；timeline 展开"问题在哪些 utterance"；R7 adapted cooldown 自动调整"我做了什么"。三层 surface 各自负责一种用户问询深度。
- **不加 prompt 提示**：考虑过 "如果忽略率 > 0.6 就在 proactive prompt 里加一行 hint"。但 R7 已经通过 cooldown 调整间接传达了信号——pet 自然变安静。再在 prompt 里加 "你最近被忽略多了" 容易让 LLM 产生"自我责备" 语气，反 effect。让数据通过 mechanism 影响行为，但 prompt 里不直接 nag the LLM。
- **路线 R 系列还有 5 个候选**：R11-R15 写到 TODO。每个都是独立小 iter，重点在"丰富宠物对环境/状态的感知" + "让积累的数据反哺行为"。整体目标：宠物从"开口判断" 进化到"全方位 contextual presence"。

## Iter R9 设计要点（已实现）
- **bubble 历史 ≠ chat session 历史是产品理念分裂**：bubble 是即时通知（F1 自动消失 60s），chat session 是持久对话。但用户心智里两者是"同一只宠物在跟我说话" — pet 自己看不到自己的 bubble 是个 broken mental model。R9 用一层 system 消息把两者粘合，用户问"刚才说啥" 终于能得到答案。
- **inject_*_layer 已经成 idiomatic pattern**：mood_note + persona_layer + soul_refresh + 现在 recent_speech。每个都是 "在 first non-system 位置插入一条系统消息" 的同模式。chat() 顶部的"信息分层" 架构清晰可扩展——下一个想塞的 context（最近 mood 趋势？管家任务摘要？）都按这模板加。
- **空列表 silent skip 是新装机用户保护**：format_recent_speech_layer 返回空字符串时 caller 不插入 system 消息。新装机用户第一次 chat 时 LLM 系统消息只有 SOUL + mood + persona，没有"最近主动开口" 的诡异空 bullet 段。这种"零数据时干净"的细节是好 UX 的累积体现。
- **redaction 一致**：proactive 的 speech_hint 已经 redact（QG4），R9 layer 也 redact —— 两者读同一份 speech_history.log，应用同一份 privacy filter。任何走 LLM 的内容都过 redact 是 R series 之后的稳定 invariant。
- **窗口 5 与 proactive 对齐**：故意。proactive prompt 看 5 条避免重复，reactive 也看同样 5 条 — pet "记得" 的范围两者一致，符合"同一个宠物" 的体感。如果某天发现 reactive 需要更长 history 再单独调。
- **测试钉死 header 字符串契约**："旧→新" + "接住话题" 是给 LLM 的 instruction signal。如果未来误删 / 改字，alignment 测试不会捕（这是 chat 模块不是 panel）；这一组 4 个 unit test 是唯一防回归。

## Iter QG5e 设计要点 + QG5 全程总结（已实现）
- **stashes + recorder 一个 mod**：两个子模块也合理，但合在一起的优势：(a) cohesion — 都 serve 同一目的（panel observability + 决策日志）；(b) future maintainer 一眼看到"telemetry 这片是什么" 不用跨 file；(c) test 命名也容易（`mod tests` 一个 mod）。模块化的目标是 readable 而不是"切到极致"。
- **`ProactiveTurnOutcome` 留 parent 是 orchestrator 数据 vs telemetry 数据的边界**：record_proactive_outcome 拿这个数据来记录，但 outcome **本身** 是 orchestrator (run_proactive_turn) 的产物。把 ProactiveTurnOutcome 移到 telemetry 反而暗示"telemetry 决定 outcome 形态"——倒了。`use super::ProactiveTurnOutcome;` 显式 import 表达"我消费这个 type，但不拥有它"。

### QG5 全程回顾（5500 → 3028，~45% 缩小，6 个 sub-modules）

- **incremental beat big-bang**：开始时 QG5 是 single TODO，多次 deferred ("too big")。改成"一次切一片" 后用 6 iter 完成（QG5a/b/c-prep/c1/c2/d/e）。每片：
  - reminders (110)
  - butler_schedule (642)
  - time_helpers (308)
  - prompt_rules (229)
  - prompt_assembler (342)
  - gate (640)
  - telemetry (204)
  - 累计 2475 行减小（vs 实测 2470 — 数字对得上）
- **glob `pub use` 是 backward-compat 的银弹**：每片新模块抽离都通过 `pub use self::sub::*;` 让 spawn loop body / run_proactive_turn / 外部 caller (consolidate.rs / chat.rs) 0 修改。这种"切代码不切 API" 是模块化重构的关键 invariant。
- **测试与代码同居 vs 留 parent 的判断**：butler_schedule / gate / time_helpers / reminders 测试随源走（mod tests in sub）；prompt_rules / prompt_assembler / telemetry 测试留 prompt_tests in parent。判断标准：tests 是 self-contained mod 还是依赖 base_inputs() / 跨多模块 fns？前者随源，后者留 parent。
- **proactive.rs 最终 3028 行的健康终态**：剩 spawn loop / run_proactive_turn / InteractionClock / ToneSnapshot data type / Tauri commands / spawn function。这些是 orchestration "胶水代码"——绑定子系统、暴露 IPC 接口。再切下去会破碎主流程。
- **6 个 sub-modules 平均 380 行**：从 reminders (283) 到 gate (654)，每个都是可独立阅读的 cohesive unit。打开 `proactive/butler_schedule.rs` 你看到一整片管家逻辑；打开 `gate.rs` 你看到一整片决策门——这是模块化重构想要的体感。
- **Iter 间 mechanically 重复 = 有信心继续**：6 iter 的 cargo test count 从 383 一动不动。不变行为是 hard contract，glob re-export 让外部不破坏。每 iter ~500 行 diff，review-friendly。这种"重复且单调" 的执行节奏是大型重构的健康信号。
- **下一步如果还要切**：可以再考虑提 InteractionClock 到 sub-module（"clock.rs"），把 spawn loop body 单独提（"loop.rs"）。但 ROI 越来越低——剩下的就是真正的胶水。我会停在 3028 行 acceptable。

## Iter QG5d 设计要点（已实现）
- **gate 子系统的 cohesion**：7 个 gate（disabled/awaiting/cooldown/quiet/focus/idle/input-idle）+ 一个调度器 evaluate_loop_tick + 一个 LoopAction enum + 一组 wake softening helpers — 这一片自然成 unit。Tests 多达 25 个 + 470 行也合理：每个 gate 都有 active/inactive/boundary 三种至少一种 case。
- **测试整体迁移 vs 留 prompt_tests 的对照**：gate_tests 是 self-contained mod（只用 super::* + ProactiveConfig + ClockSnapshot），所以可以整个搬走。prompt_tests 因为 base_inputs 跨多模块依赖留在 proactive.rs。判断标准简单：tests 用 super:: 解析的 items 是不是大部分跟 source 一起搬？是 → 跟着搬。否 → 留 parent。
- **LoopAction private → pub 升级**：spawn loop body 在 proactive.rs，需要 match `LoopAction::Silent / Skip / Run` 三种 variant。pub 化是必要的。同 SILENT_MARKER 在 QG5c2 时的处理。
- **`super::ClockSnapshot` 跨子模块依赖的标准模式**：gate 里需要 ClockSnapshot 但它定义在 proactive.rs (parent)。`use super::{ClockSnapshot, InteractionClockStore};` 干净显式。如果将来想再细分 InteractionClock 自己到 sub-module，gate 就 import 更深 — `super::clock::ClockSnapshot` 之类。子模块 import parent 项是 backward-compatible 的稳定设计。
- **41% 累计 = QG5 已经 mostly done**：起 5500 → 3232。还剩 ~3200 行的 telemetry (record_proactive_outcome / append_outcome_tag) + run_proactive_turn + Tauri commands + InteractionClock + spawn loop。最后 QG5e 后 proactive.rs 大概会稳定在 2500-3000 行的 orchestration-only 体量——基本是 spawn loop + 一个 run_proactive_turn 巨函数 + Tauri command surface + InteractionClock。这是 acceptable 的 mid-term 终态。
- **没必要再切到极致**：proactive.rs 最终 2500-3000 行 acceptable，因为 spawn loop body + run_proactive_turn 是"上层 orchestration"，再切只是把"上下游连接代码" 也分文件，反而难追踪。优秀的 module split 应该让"独立 cohesive unit" 有自己文件，但"主流程胶水代码" 留 parent。

## Iter QG5c2 设计要点（已实现）
- **决策 / 渲染分两层是干净的 prompt 体系**：QG5c1 抽走了 rule-label 决策器（哪个 rule 该 fire），QG5c2 抽走渲染器（rule-label → 文本，PromptInputs 数据 → prompt 字符串）。两层分立后，未来想换 prompt 模板（紧凑版 vs 详尽版 vs 多语言）只动 assembler；想加新 rule 类别只动 rules。
- **超大 prompt_tests 测试 mod 不挪是被 super::* 习惯绑住**：1620 行的测试 mod 用 super::* 解析十几个跨多个新 sub-module 的 fn。如果挪到任何一个 sub-module，super 变窄丢失可见性。若挪到 proactive 自己的 ./tests/ 目录又意味着把它从 cfg(test) inline 模块变成 integration test (different test binary, no access to private items)。最低 friction = 留在 proactive.rs。这是"测试 vs 代码 colocation 完美" 的小妥协；可接受。
- **复制中文 prompt 时字符一致性是 quality bar**：原 prompt 使用全角括号「（不必每次推进，看时机自然）」。复制粘贴 IDE 自动转换 / 我手抖换成 ASCII `(...)`。grep 立即捕到差异。这种"字符级别的精度" 是 prompt-as-code 的额外维护负担——通过细致 diff 对比避开。
- **多 use super::{...} explicit 比 use super::*; 好**：assembler 用 explicit `use super::{active_composite_rule_labels, ..., LONG_IDLE_MINUTES, ...};` 而不是 super::*。原因：(a) 让人一眼看到这模块依赖了哪些 sibling/parent 项；(b) 加新 sibling 不影响这文件 import；(c) 对 IDE / 编辑器 navigation 更友好。glob 仅用于 parent 的 re-export 端，不在子模块的引入端。
- **SILENT_MARKER pub 升级是符合"封装到模块" 的逻辑**：当函数 in same mod，private const OK。一旦把 ower 函数移走，run_proactive_turn 还需要这个常量来识别 LLM 沉默标记 → pub 即可。Rust visibility 规则强制这种"contract 浮出 mod 边界" 思考：你想让外部用什么，pub 它；想藏起来，留 private。
- **30% 累计缩小 = 一半的 QG5 工作完成**：起 5500 → 3872。剩约 2200 行的 gate / telemetry / run_proactive_turn / Tauri commands。预计 QG5d + QG5e 后稳定 1500-2000 行 orchestration-only。
- **5 sub-modules 总 1869 行 + 主 file 3872 行 = 5741 行**：相比起点 5500 行多 ~240 行（test docstrings + 5 个 module headers）。代码本身没变多，但视觉上拆成 6 个文件后阅读性显著好了：你想看 reminders 就读 reminders.rs (283 行)，不用滚 5500 行长文件。

## Iter QG5c1 设计要点（已实现）
- **拆 source ≠ 拆 tests**：之前几片 QG5 都是源 + 测一起搬。这次故意留 tests 在 prompt_tests，因为 active_*_rule_labels tests 和 proactive_rules / build_proactive_prompt tests 在同一 mod 深度交错，先拆一半会让 prompt_tests 残骸难看。等 QG5c2 把整个 prompt 系统一起搬，整 mod 一起迁移。"`use super::*` + glob re-export" 的组合让这种"分阶段 source/test 迁移"零代价。
- **rate-limit machinery 跟 rule 走**：LAST_LATE_NIGHT_WELLNESS_AT static 是 R8 给 late-night-wellness rule 加的 rate limit。它是 rule 实现细节，不该外露给其他子系统。跟 rule 一起迁移让"如果未来加新的 rate-limited rule，模式继续在这一个文件" 成立。
- **拆细路径上的"什么是 cohesive unit" 反复审视**：迁移到第四个 sub-module 后开始能看到 cohesive unit 的轮廓更清晰：reminders 是用户提醒，butler 是宠物管家，time_helpers 是纯时间标签，prompt_rules 是规则决策器。剩下 prompt_assembler（QG5c2）+ gate (QG5d) + telemetry (QG5e)，应该都能保持 cohesion。
- **prompt rules vs prompt assembler 分割是有的**：rule-label 决定 *哪些* hint 进 prompt（决策层）；proactive_rules + build_proactive_prompt 把 hints 加 PromptInputs 数据 *组装* 成 prompt 文字（渲染层）。分两层独立 testable 并且未来如果想换 prompt 模板（比如 markdown 风格 vs 紧凑风格），只动 assembler 不动 rules。
- **23% 累计缩小，剩 ~2200 lines 估**：当前 4214 行，剩下约 2200 行的"prompt assembler + gate + telemetry + run_proactive_turn + tone snapshot + Tauri commands" 集合。预计 QG5c2 + QG5d + QG5e 三 iter 后稳定在 1500-2000 行的 orchestration-only 体量。

## Iter QG5c-prep 设计要点（已实现）
- **prep iter 的价值**：直接做 QG5c (prompt rules) 会需要同时搬 prompt rules 本身 + 它依赖的 8 个纯 helper + 三个独立 test mod + 嵌入 prompt_tests 的 4-5 个 helper test。一次性 diff 容易出错难 review。先抽 pure deps（依赖 graph 上的叶子）让 QG5c 的 diff 严格只 about prompt rules——staged refactor。
- **依赖 graph 的叶子先抽**：这个原则在 QG5 全程都适用：reminders / butler / time_helpers 都是叶子（不依赖其他 proactive 子系统）。下一片 prompt rules 是中间层（依赖 time_helpers），再下一片 gate (evaluate_pre_input_idle 用 in_quiet_hours) 也变得简单。
- **跨 mod test 整理是 free 的清理**：原 pre_quiet_tests / cadence_tests / period_tests 三个 mod 现在都合到 time_helpers::tests 一个 mod 里。prompt_tests 也少了 7 个嵌入 helper test。模块边界更干净了。
- **glob `pub use` 累加而不是重新组织**：proactive.rs 头部现在 `pub use self::butler_schedule::*; pub use self::reminders::*; pub use self::time_helpers::*;` 三行。每次新加片只是 append 一行——不需要每次 reorganize。
- **`#[allow(dead_code)]` 不出现在抽离的代码**：所有抽过去的代码都被外部用，glob re-export 不触发 unused_import lint。这是 QG5a 学到的 — 用 explicit `pub use {a, b, c}` 会触发 "unused import"，glob 不会。
- **proactive.rs 4443 行 = 19% 累计减小**：起点 5500 行，三 iter 累减约 1060 行。剩余 prompt rules（最大）+ gate + telemetry 估计还能减 1500-2000 行。最终 proactive.rs 应稳定在 1500-2500 行的 orchestration-only 体量。

## Iter QG5b 设计要点（已实现）
- **同模式重复使用降低 risk**：QG5a 跑通的"创建 sub.rs + glob pub use + 删 src + 删 tests + run cargo test"流水线在 QG5b 直接复用。第一次抽离 ~30 分钟试错（debug glob re-export, dead_code warning 等）；第二次 ~15 分钟。模板化后效率翻倍。
- **跨子系统私有 helper 处理**：`parse_updated_at_local` 同时被 `is_butler_due` 和 `is_completed_once` 用，是子系统私有 implementation detail。决策跟着移走（不外露）。如果将来 reminders 要解析 `updated_at`，要么把这个 helper 提到 proactive.rs（而非两个子模块各自复制一份），要么提取一个 `proactive/time_helpers.rs`。但现在过早抽象。
- **测试与代码同居 (mod tests in sub) 的可读性优势**：QG5a 时 17 测试随 reminders 走；QG5b 24 测试随 butler 走。proactive.rs 里 mod prompt_tests 块越来越短 = 它"什么是 prompt_tests" 的语义越来越纯——只剩跟 prompt assembly 真正相关的测试。
- **每片 ~600-700 行的合理 chunk size**：reminders.rs (283) 是小片；butler_schedule.rs (628) 是中片；接下来 prompt rules 估计 1500-2000 行（最大块）。这种渐进切让 git review 一直 manageable。
- **当前 proactive.rs 4751 行 = 已经可一屏滚到的体量**：起点 5393 行；QG5a + QG5b 共减 642 行。剩下 prompt rules / gate / telemetry 三片预计能把 proactive.rs 压到 1500-2000 行——纯顶层 orchestration（spawn loop + run_proactive_turn + 几个 IO-heavy builder）。
- **避免改动行为是 refactor 第一原则**：每个 QG5 子 iter 我都问自己"这次有没有动到 actual behavior？" 答案永远是 no——只是搬代码 + 重新 wiring 命名空间。如果哪天忍不住"既然在动这块，顺便修个 bug" 就破坏了"行为不变" 契约。bug 修复留单独 iter。

## Iter QG5a 设计要点（已实现）
- **"切大象的时候用刀，不要用挖土机"**：proactive.rs 5500 行，QG5 卡在"太大" 状态多 iter 没动。换成一次切一片（reminders / butler / prompt rules / gate / telemetry 五片独立提炼），每片可独立 ship + revert。这是"big refactor 卡住时的标准对策" — turn it into a series.
- **`pub use self::sub::*;` 是 backward-compat 的银弹**：consolidate.rs / panel commands 都用 `crate::proactive::ReminderTarget`。glob re-export 让外部代码完全不知道发生了什么，0-line external diff。如果某天想把它们全部 promotion 到 `proactive::reminders::ReminderTarget` 让 namespace 更明确，可以做单独的 cleanup iter。
- **Rust 2018 module nesting (`proactive.rs` + `proactive/sub.rs`) vs mod.rs**：选前者保留 git blame。如果重命名 proactive.rs → proactive/mod.rs，git 看到的是"删除 proactive.rs + 新建 mod.rs"——blame 会断。Rust 2018 的双格式让我们能避开这个 trap。
- **测试同居 (mod tests inside sub.rs) 比 mod sub_tests 在 parent.rs 更健康**：测试和代码同文件，scrolling 之间不远，未来对 proactive_rules 改一行能立刻看到对应测试。Rust 这个 convention 比 Python 的 tests/ 子目录更适合"behavior shaping" 类代码。
- **挑 reminders 当首片不是随便选**：完全 self-contained（不持有 InteractionClock 等共享状态），有清晰公共界面，外部已有调用者（consolidate.rs）—— 是验证"glob re-export 真的 work" 的 minimum-risk 切口。如果第一片选 prompt rules（最大），失败的话 revert 成本巨大。"先用最简单的形状证明流程对，再上复杂形状" 是 refactor 顺序的常识。
- **行有余力 vs 强迫症**：每个子模块抽出来后，理论上可以再做一些"内部清理"——比如让 reminder 解析有 explicit `Result<>` 错误而不是 `Option<>`。决策：第一波只做"行为不变的纯移动"；任何 nontrivial 改动留给后续 iter，让 diff 干净易 review。
- **预期 5 iter 完成 QG5**：reminders / butler / prompt rules / gate / telemetry。当前每 iter 25 分钟节奏下，约一个工作日全完。比"一次大爆炸式重构 + 数小时 review" 健康得多。

## Iter R7 设计要点（已实现）
- **capture → surface → drive 三段范式 ship 完整**：R1 采集，R6 显示，R7 让数据真正影响行为。这种顺序很关键——如果先 R7 后 R6，行为变了但用户不知道为什么；先 R6 后 R7，用户先看到了"原来宠物在记账"，再放心让账本驱动行为。这是产品安全感的递进。
- **step function vs smooth curve**：smooth 看起来"科学"（adapted = base × (1 + α·(ratio−0.5))）但实际不可审计——panel 用户看 ratio chip 没法预测 cooldown。step 是 "ratio 跳到 0.6 以上 cooldown 直接翻倍"，肉眼可证。这是对"behavior-shaping logic 必须 auditable" 的让步。
- **base=0 special case 是 settings 契约的边界**：用户设 cooldown=0 是 explicit opt-out（"我希望宠物频繁说话"）。adapter 不该违背 user intent 自行强制 cooldown。`base=0 → multiplier=任何 → 结果=0` 自然成立但加测试钉死。这是"adapter 是 nudge 不是 override" 的设计原则。
- **min_samples=5 是新手保护**：第一天装机用户的 1-2 个 ignore 不该立即 2x cooldown。等 5 条数据才动手，匹配"两个月才会有偏好" 的 R 系列 baseline。如果未来想做"周内 vs 周末" 不同 baseline，min_samples 仍然是健康前提。
- **evaluate_pre_input_idle 签名改动是不可避的**：原本想在 gate 外面加一层 wrapper "if adapted < base, additional skip"，但那只能更严格（收紧 cooldown），不能放松（low-ignore 0.7×）。一旦想要双向调节就必须把"effective cooldown" 推进 gate 自己。19 个 test call sites 一次性 update 是合理代价。
- **panel ratio chip 与 gate 对齐**：R6 显示 6/20 ignored，R7 用同样 20-条窗口算 ratio。如果 R6 显示 30%（6/20）panel reader 能预测"还在 mid band, cooldown 不变"。如果未来 R6 拓宽到 50 条但 R7 仍 20 条，会出现"panel 数和 gate 数不一致" 的混乱——所以两个使用同 const 是契约。

## Iter R6 设计要点（已实现）
- **R 系列 capture → surface → drive 三段式**：R1 采集（capture），R6 暴露（surface），R7 才会回填到 cooldown 决策（drive）。先 surface 再 drive 是 product 级安全：让用户先在 panel 看到"反馈数据是真实的、合理的"，再放心让它影响行为。如果直接 R1→R7，用户没办法 audit 这层数据，黑盒了一段非平凡逻辑。
- **title 里嵌核心 metric**：3/8 回复 这种数字嵌在 collapsible header 里，等于"标题就是一个迷你 dashboard"。这比另起一行显示节省空间，且让 collapsed 状态也有信息密度。这是从 D series chip strip 学来的"多信息一行" 思维延伸到 collapsible UI。
- **`#[serde(rename_all = "lowercase")]` + 显式 as_str() 二选一其实可以共存**：序列化由 serde rename_all 处理，as_str() 服务测试 + 内部 format_line 调用。两者结果相同 = 不会漂移。这是 Iter R4 也用过的模式：测试不依赖 serde 内部，但实际序列化也用同一格式。
- **测试钉 serde 字符串契约**：跨语言 IPC 的 enum→string 映射如果改了 enum 名字 + 漏了 rename_all，编译过 + cargo test 过 + 前端 panel 渲染挂掉。这种"编译期通过但运行时坏" 的回归只能靠前置 contract test 拦。这是面向 IPC 系统的标准防御。
- **预测 R7 入口**：R6 完成后，"feedback ignored ratio drives cooldown" 的实现会很自然——已经有 `recent_feedback(window)` 异步函数，加一个 ratio 计算 helper 就能在 gate 路径用。R6 让 R7 实现成本降到几十行。这是"surface 是 drive 的 substrate" 的杠杆。

## Iter R8 设计要点（已实现）
- **R3 上线后的真实 bug 反思**：R3 完成时还自我感觉良好（"硬规则 wellness override 是设计闪光"），但漏掉了"硬规则需要节流" 这个常识。任何"无论如何都要触发" 的事件都需要 rate limit；wellness 没节流 = harassment 而非 care。这是"feature 与 rate-limit 应该一起设计" 的教训。
- **三层 helper 模式 (pure / wrapper / writer)**：late_night_wellness_recently_fired_at 是纯函数；late_night_wellness_in_cooldown 是封装 production-side 副作用的 thin wrapper；mark_late_night_wellness_fired 是写副作用。三层让测试只动 pure 部分，production 路径只调 wrapper，符合 D series 以来的"view-time mirror" 思路。
- **dispatch-time stamp vs reply-time stamp**：选 dispatch-time 是因为：(a) 简单，rule 出现就 stamp 一次，不需要 thread "是否 Spoke" 状态；(b) LLM 拒绝 wellness 也是用户已经看到了一次"该睡了" 心智的机会；(c) 边缘情况不该污染主流程。这是"简单完整流 vs 完美精确语义" 的实用主义选择。
- **新 9 个参数有 too_many_arguments 警告但 already allow**：QG1 时已经在 active_composite_rule_labels 上 `#[allow(clippy::too_many_arguments)]`。本 iter 再加一个参数没违反 lint，因为 allow attribute 已在。这种"接受 too_many_arguments 的稳定决定"在 QG1 时就做好了——本 iter 只是 collect dividends。
- **测试 boundary 选 `15min/30min/60min` 三档**：在边界两侧各取一个 + boundary 自己。`exactly_at_gap` 的语义是"刚到 30 min 了，可以再触发"——`<` not `<=` 在 helper 的语义上是 "still cooling"。测试钉住这个 strict-less-than 语义防止未来误改成 `<=`。
- **mark_*_fired 用 dispatch-time 而非 LLM 后**：如果在 LLM Spoke 后才 mark，那 LLM 选 silent 时第二轮也会 mark（因为 rule 被再次激活），可能导致 stamp 滞后。dispatch-time stamp 一次定锚 30 min 干净。

## Iter R4 设计要点（已实现）
- **结构化捕获 vs 日志解析**：log line parsing 看似简单，实际维护噩梦——每改一次 log format 都要更新 regex 不然 panel "突然空了"。结构化 ring buffer 在 call site 原子写入，shape 由代码而非字符串契约定义。这是 Iter E4 的同模式（LAST_PROACTIVE_TURNS），证明可复用。
- **5 个 review_status 分支映射 5 个 pipeline branch**：MissingPurpose（TR1）/ NotRequired（low/medium 直执行）/ Approved + Denied + Timeout（TR3 三种 outcome）。`Ok(Err(_))` channel-lost 收编进 Denied 因为效果一致。把这五个明确枚举出来给前端 badge 渲染零猜测。
- **truncate_excerpt 用 chars().count() 而不是 bytes**：UTF-8 中文字符 3 字节但应算 1 字符。bytes 切割会断在 codepoint 中间 panic。这是 Rust 处理国际化文本的标准 gotcha，所有 truncate 应统一用 chars。
- **测试隔离 mutex**：cargo 默认并行测试，static Mutex 状态共享 = 测试互相污染。`HISTORY_TEST_LOCK` 序列化访问。这是处理"全局可变状态测试" 的最小成本方案——不需要 serial-test crate dep。
- **`#[allow(dead_code)]` on as_str()**：serde rename 已经能产生这些字符串，但 explicit fn 让测试不依赖 serde 实现细节。生产 path 真没调用，留作前端契约文档。这是"代码规范的成本是 < 1 行 attribute" vs "保留 design 意图清晰" 的权衡——选后者。
- **collapsible default-collapsed**：长 session 可能有 30 条 tool call，always-on 会把 panel 撑死。用户主动展开看 = "我现在在调 prompt"，关闭 = "我在看其他 chip"。这是"高信号高密度数据 → opt-in 渲染"的 UX 模式。
- **purpose / risk / status 三层 surface 同一张卡**：TR1/TR2/TR3 是 backend 抽象的递进，但前端用户只关心"那次调用怎么了"。把三层数据合一展示，对应"产品视角"而不是"实现视角"。这种"分层实现 + 合一展示" 是好的产品演进。

## Iter R5 设计要点（已实现）
- **审计推翻 TODO 假设是健康事件**：原 TODO 写"SOUL.md 得重启 app 才生效" 是错的——proactive / telegram 都已 hot-reload。我自己作为前面 iter 写 TODO 的"人"，那时没 audit 当前路径就写下了假设。这次审计后发现真正 gap 在 reactive 会话烘焙。教训：写 TODO 时先看一眼当前实现再描述差距，避免盲写假需求。
- **session 持久层不动是设计选择**：直觉上"既然 SOUL 变了，session 存的也要更"。但 session 是历史记录——它应该忠实保留对话当时的 system context，不该被未来 SOUL 编辑回写。"LLM 看到的"和"session 持久的" 分两套语义就清晰了。
- **purity for testability is a recurring win**：refresh_leading_soul 是 pure (messages, soul) → messages，5 测试全 in-memory ChatMessage。get_soul 是 disk IO，分开后测试零 setup。这是 D series 以来反复用的模式。
- **不引入 file watcher**：watcher 是"主动通知"，每 turn 重读是"被动拉取"。watcher 增加 (a) 跨平台兼容（macOS / Linux / Windows）；(b) 重新加载触发时机（read 期间编辑怎么办）；(c) panel 同步（watcher 改 cache 时，panel 要么轮询要么 emit event）。每 turn 重读避开所有这些复杂度，cost 可忽略。
- **皮肤场景：用户改 SOUL 想看效果**：user workflow 是 (1) 改 SOUL.md 在 panel 设置里点保存 (2) 立即 send 一条 chat 消息看回应。没改之前 (1)→(2) 之间需要 cycle session 才生效，是开发摩擦点。本 iter 让 (1)→(2) 之间不需要操作，user → AI 反馈 loop 缩短。
- **panel 按钮拒掉是 UX 减法**：spec 写的"立即重新加载 SOUL" 按钮在自动 hot-reload 后反而困惑用户："我什么时候需要点这个？" 自动机制下按钮变成幽灵控件——存在但永远不该被点。删掉它就是"少一个认知项"。

## Iter R3 设计要点（已实现）
- **硬规则 vs 软规则的边界是 wellness 标志位**：proactive_rules 之前都是"在合适条件下推荐 LLM 怎么开口" 的软引导。wellness 这个第一次出现"无视常规 cadence/chatty/pre_quiet 的硬 override"。这个区分以后会有更多：例如 "用户当前 mood 是焦虑 + idle 长 → 强制柔和 register"，也是硬 override。把 wellness 做出来给后面这种规则建立模式：override 时不应 gate on 那些通常的克制信号。
- **hour 之前没有进 PromptInputs 是历史遗留**：period 是早就有的（"上午/下午/晚上"），但 raw hour 直到现在都通过 inputs.time 字符串隐式传递。本 iter 把 raw hour 暴露出来后，未来其它规则（深夜 / 清晨开机 / 中午午休等）都能用具体小时数判断而不需要 parse 字符串。这是"原始数据进结构、派生字段进 prompt"的清洁分层。
- **LATE_NIGHT_END_HOUR=4 vs 3**：spec 写的是 0-3 点。我用 4 是为了包含整 03:xx 段——03:30 仍在工作的人和 02:30 不应该有差别。4 点这个边界 → 凌晨 04:00 准时 silent → 这时候大概是早起人群，他们值得不被打扰。
- **chatty/pre_quiet override 是设计的关键**：wellness 是关于"健康"，不是关于 "cadence"。如果今天 pet 已经聊了 10 句而用户半夜还在工作，那是"今天聊得多" + "今天该睡了"两件独立的事——wellness 不该被 chatty 抑制。这是 rule layer 设计上的语义清晰：每条 rule 应有自己独立的"是否触发"逻辑，而不是层层 gate。
- **测试三 scenario 的 fingerprint coverage 是非平凡的**：late-night 触发条件（hour<4 + idle<5）和 long-absence（idle≥240）+ wake-back（wake_hint 非空）+ pre-quiet 等条件互斥，不能同时单 call 验证。已有 fingerprint test 加 s3 scenario + universe enumeration 加 chained second composite call，是这种"高维状态 space" 测试的常用模式。
- **`labels.contains(&"x")` 比 `iter().any(|l| *l == "x")` 更地道**：clippy 的 `or_fun_call`/`needless_collect` 一类的 idiom lint 在这里命中。Vec<&str> 的 contains 接 `&&str` 引用，这是 Rust slice contains 标准 API。
- **不暴露 settings**：wellness 是宠物的"opinion"——"我觉得你应该睡了"。如果让用户调阈值，就把判断责任推回了用户，违背"宠物有自己性格" 的产品设定。两个 const 写死即使有边界 case（用户是夜班 / 习惯凌晨工作）也接受，那种用户自己会忽略宠物。

## Iter R1 设计要点（已实现）
- **state machine 的隐藏复利**：本来打算搞一套 ChatBubble 点击事件 + 倒计时 + Tauri 命令记 dismiss/timeout/reply 三档。后来注意到 InteractionClock.awaiting_user_reply 已经是被动观察"用户回没回"的真值——读它就能分类，**前端零改动**。早期投资在"对的状态机"会在后续 N 个 iter 里反复变现。
- **raw vs effective 分两套语义很关键**：D11 给 awaiting 加了 4h 自动过期，是为了 GATE 不让宠物永久哑巴。但 FEEDBACK 分类需要的是"用户事实上有没有回"——和时间无关。所以加了 `raw_awaiting` 单独暴露不带 expire 的真值。同一字段两种读法，对应两种业务语义。
- **dedup key = prev timestamp**：每次 proactive turn 可能因为 panel 手动 fire / 后台 loop 等多入口触发，但 LAST_PROACTIVE_TIMESTAMP 在 `mark_proactive_spoken` 后是单调推进的。用它做 LAST_FEEDBACK_RECORDED_FOR 的 key 既稳定又不需要额外计数器。
- **40 字符 excerpt 是 prompt 经济性平衡**：太短（< 20）认不出是哪句话；太长（> 80）prompt 体积重复内容多浪费 token。40 是常见短句长度，长一点的也能保留信息密度。
- **不做用户主动 dismiss 信号**：dismiss<5s 在 spec 里看着是有用的"立即拒绝"信号，但 ChatBubble 当前没点击事件，加了之后还要做 (a) 防止 user 误碰；(b) 60s timer race；(c) 与回复路径区分。一个 iter 做不完干净，留 R1b。
- **prompt hint 写"放短/沉默" 是 nudge 而非命令**：proactive_rules 里有刚性约束（chatty / wake-back），但 feedback hint 是 soft 引导。LLM 看到"用户没回应——这次放短或沉默" 会自己判断这次 context 适合哪个，不会一刀切。这种"软引导 + 硬规则" 组合是 prompt design 健康姿态。
- **后续：feedback ratio 驱动 chatty_threshold 自适应**：现在 chatty_threshold 是 settings 里手动设的；feedback_history 攒多了就能算"过去 24h replied/ignored 比"，自动收紧阈值。R3 wellness nudge 也可以从这数据出条件。

## Iter R2 设计要点（已实现）+ 后续路线规划
- **timeline 统一比 tab 分立更有信息量**：原本可以加 "Tool Review" 专门 panel tab，但 review 是低频事件（高 risk 工具调用一般 < 几次/天）。混在 decision timeline 里反而能让用户瞬间看到"今天 12 决策 + 2 review"，对 review 异常突增时更敏感。density first, separation second.
- **Optional 字段叠加是 ToolContext 演化的稳定模式**：tools_used (Iter E4) → tool_review (TR3) → decision_log (R2)。每个都是 `Option` + `with_X` builder。autonomous 路径（telegram / consolidate）始终用 None，desktop / proactive 路径 attach。Rust 这种"零成本 opt-in" pattern 是 backward compat 的优秀解法。
- **kind 字符串 const 化是面向未来 parser 的契约**：`KIND_REVIEW_APPROVE` 等 pub const 让 panel 测试 / 未来 log scraper / metrics aggregator 都不用 hardcode 字符串。同时 Rust 编译期就能 catch typo。
- **gap analysis 在 backlog 干涸时强制做**：到这步所有 explicit TODO 都做完了，剩下的或 gated（8b / 12b）或太大（QG5）。这时不应该编小修小补让 TODO 看起来满，应该真正 stand back 评估"距 companion 目标差多远"。R1-R5 是这次 stand-back 的产物：每条都是"现有数据再向前一步利用"，不是新加抽象。这是 backlog management 的 healthy moment。
- **R 系列优先级原则**：(R1 反馈采集) 是 input 层最大杠杆——pet 当前对 user 反应近乎盲；做了之后 prompt 才能真的"learn from sessions"。R3-R5 都是 R1 的下游或者独立 polishing。所以 R1 是下一个 iter 自然的接力。
- **不做 toolreview-specific dashboard**：tool-review 本质上是不该频繁发生的事件。如果发生频繁（panel timeline 都被它淹没），那就是 prompt + 工具集需要重新设计的信号——不是"加个独立面板"能解决。timeline 一线诊断够用。

## Iter TR3 设计要点（已实现）
- **TR1 → TR2 → TR3 递进式安全设计**：先有 purpose（每次调用要写明意图），再有 classifier（按工具名 + args 分级），最后有 enforcement（高风险阻塞）。每步都独立可工作 + 数据上下游兼容。这是"安全机制循序渐进"的范式：先 audit，再 classify，再 enforce。如果一开始就直接做 review gate，没有 purpose 字段 panel 就显示不出"为什么 LLM 要调它"。
- **polling 设计的杠杆**：QG6 把 panel 收敛成 1 Hz 单 IPC，TR3 直接复用——`pending_tool_reviews` 加进 snapshot 字段就完了，前端 polling 自然检测到。架构投资在 N 个 iter 后产生复利：QG6 是抽象基础，TR3 是受益方。
- **`oneshot::Sender` + `tokio::time::timeout` 是 Rust async coordination 教科书 pattern**：register 时建 channel，等待方 await receiver，submit 方 send。timeout 包装让超时变成 Err 分支，可统一处理。Future 被 drop 时 receiver 失效，PendingEntry.sender 留在 map 里——所以需要 cancel() 手动清。
- **default-deny 不是 default-permit**：高风险按定义就是"不该自动跑"。如果 user 60 秒内不响应，更可能是 ta 离开了/没看到——那么自动允许 = 把脚塞进门里跑路；自动拒绝 = LLM 收到结构化错误 + safe_alternative，下一轮自己绕开。这是 fail-safe 思维的一阶应用。
- **`ToolContext.tool_review: Option` 是 backward compat 利器**：telegram / consolidate 早就跑通了，加 review gate 会破坏 autonomous 流程。Optional 让 desktop 路径有 review、自动化路径无 review，不需要 if/else 重构。这是 Rust 语境下"渐进式扩展接口"的常用做法。
- **结构化 JSON tool result 是 LLM-loop 的设计哲学**：denied_result_json / timeout_result_json 都给 LLM 看 `{error, reason, safe_alternative}`。LLM 在自己的下一轮里看到这个结果就懂——不需要让 LLM 学我们的内部协议，它能从语义读懂。这是"用工具结果做沟通通道"的优雅一面（TR1 的 missing_purpose_error_result 已经验证过）。
- **未做的扩展**：(a) review 决定写入 decision_log（TR2 的 reasons 已经在 PendingToolReview 里，但 panel 决策日志没记录"用户对 tr-N 的判定"）；(b) 重复同名工具调用的"记住选择" 选项（"允许 + 30 分钟内同 args 的相同工具自动允许"）；(c) telegram 端 reply-button approve / deny。这些都是后续打磨方向，TR3 的核心 ship 已经完成。

## Iter TR2 设计要点（已实现）
- **observe-only 是 security 工作里的杀手锏**：直接上"高风险即阻塞" 等于把 bash / write_file / memory delete 全停掉，宠物可能整周无法做任何事。observe-only = 分类逻辑就位 + 数据流通 + 真实场景跑过几天，看到具体 high-risk pattern 后再设计 gate UX。这是"先收集 ground truth 再设计 enforcement" 的 shadow-deploy pattern。
- **`_purpose` 参数留位但本 iter 不用**：保留签名让 TR3 + 未来语义策略（"purpose 含 'cleanup'+'rm -rf' 直接拒"）不需要再改 callers。这是 TR1 的"对未来留口子" 设计的延续。
- **`safe_alternative` 是机制级帮 LLM 学好**：单纯 reject 无意义；告诉模型"想做 X 应该走 Y" 让它下次自己绕开。bash → 专用工具，write_file → edit_file，memory delete → update。这种 alt 提示也是给 TR3 review UI 准备 — 拒绝按钮配 alt 文案，用户能瞬间懂为什么拒。
- **三档分级而非二元**：原本想做 binary（safe / dangerous）。考虑后发现：edit_file 是"局部改本地文件" — 比 write_file（任意覆盖）安全但比 read_file 危险，硬塞二元会假装它跟 bash 同等级，迫使所有人工审核流程过它。三档让 medium 自然跨过，UI 上也有差异化处理空间。
- **tool 名是分类主轴，不是 args 内容**：除了 memory_edit 看 action，其他工具都按 name 整体分类。理由：(a) args 是 LLM 写的，攻击者可能伪装；(b) 工具名是注册时定义的常量，是更可靠的边界。TR3 / 未来需要细化某个 high tool（如 bash 内有"git status"等只读命令）再做 inner classifier — 那是分立 work。
- **新模块 vs 函数加 chat.rs**：放进 chat.rs 早晚污染那个 1000+ 行文件；放 src/tool_risk.rs 是 standalone module，TR3 / 任何风险 policy / 未来"风险随时间 decay" 等都可在这模块内长大，chat.rs 只引用 2 个 pub fn。原则：当一个新概念产生 2+ pub fn 就值得单文件。

## Iter TR1 设计要点（已实现）
- **TR 系列前置 = 给 risk decisioning 提供数据源**：TR2（risk assessment）和 TR3（人工审核 gate）的输入都包含 purpose。如果不先做 TR1，后续 risk_level 决策只能基于工具名 + args，缺少最关键的"模型为什么调它"。先建立 protocol 让数据就位，再决定怎么用。
- **purpose 进 args 而不是 outer field**：OpenAI 和 Anthropic 的 tool calling 协议，`function.arguments` 是 LLM 唯一可控的 JSON。把 purpose 放进 args 让 LLM 通过现有协议传递，零网络/序列化改动；放到 outer 字段需要 wrapper 扩展协议层，跨 MCP 还要协调。
- **recoverable error 设计很关键**：第一次写时考虑 hard error（return Err 给 caller），但那意味着第一轮 tool call 的 LLM 错误会让整个 chat 失败。改成 synthetic tool result 后，LLM 在自己的对话里看到 error JSON + hint，自然就在下一轮纠正。这是 LLM-loop pattern 里"用工具结果做协议"的优雅一面。
- **`deny_unknown_fields` 不存在 = purpose 字段对所有工具透明**：搜了下 src/tools 没人用 `#[serde(deny_unknown_fields)]`，所有工具的 args 解析都是 `args["x"].as_str()` 风格，extra `purpose` 自动忽略。如果未来有人加 deny_unknown_fields，pipeline gate 会先剥 purpose 再传给 tool 是可能的下一步——但目前不必要。
- **prompt-level "强制 / 必须" 措辞**：之前的 TOOL_USAGE_PROMPT 教 butler / user_profile 用"应该 / 可以" 偏建议性语气；purpose 是协议级要求，必须用"强制 / 必须" 让 LLM 第一轮就执行。和 QG2 的"硬上限" 同理——基础设施级 invariant 用强语气。
- **未做：前端 purpose 展示**：TODO 原本要求"前端 ToolCallBlock / debug panel 展示 purpose"。决策延后到 TR2/TR3 一起做。理由：(a) purpose 现在已经在 sink.send_tool_start 的 args 字符串里了，前端 parse 一下就能拿到；(b) 但要做得好需要 UI 设计（purpose tooltip / inline / collapsed），不是几行 patch；(c) 协议先就位，UI 可以独立迭代不阻塞 TR2。

## Iter QG6 设计要点（已实现）
- **跳过 QG5 (拆 proactive.rs)**：proactive.rs 现在 4500+ 行，按 gate / prompt rules / reminders / butler / telemetry 切是合理重构，但对单 iter 太重——纯移动代码 + 保持 API 稳定 + 每步跑测试是 multi-iter 工作。先做 QG6（contained scope，纯改前后端 IPC 协议）保留 QG5 等专门 session。
- **聚合命令而非"少 polling"**：另一个解法是把 1 Hz polling 降到 5 Hz 或 push-based。但 (a) 1 Hz refresh 是 panel UX 重要（数字跳动让用户感觉 alive）；(b) push-based 需要 Tauri Channel/Event 改造，是大动作。聚合命令是"改 protocol 不改频率"——保留 UX，砍掉只有 IPC 序列化的损耗。
- **`from_counters` 是抽象的最低要求**：Tauri 命令本质上是"取 State → 拼 struct"。把"拼 struct"提成 from_counters，"取 State" 留在命令里，是经典 thin-controller pattern。同样适用于 build_tone_snapshot——它 body 就是 "拿 deps → 计算"，State 取 deps 是命令责任，计算是纯函数。
- **`State<'_, Arc<X>>::inner() -> &Arc<X>` 自动 Deref 到 &X**：这是 Rust 隐藏的便利。我本来要写 `clock.inner().as_ref()` 或 `&**clock.inner()`，结果直接传 `clock.inner()` 给 `&InteractionClock` 参数，编译器通过 `Arc::deref` 自动转。签名干净，调用点也不需要解释 Arc 包装。
- **Inline anonymous type in invoke generic**：前端写 `invoke<{ logs: string[]; ... }>("get_debug_snapshot")` 而不是导出 DebugSnapshotType。理由：(a) PanelDebug 是唯一调用方；(b) panelTypes.ts 已经够大，不需要为聚合类型再加；(c) 如果以后要复用，提取成本一行的事。"内联到能被自动复用为止"是 TS 项目里减少 type-noise 的 pragmatic 做法。
- **保留旧 Tauri 命令**：删除等于 lib.rs handler list 翻动 + 大概率漏掉某个调用方（PanelPersona 已经撞上 `get_companionship_days`）。保留的代价：每个旧命令 ~3 行 + handler entry，binary 大小 negligible。删除的收益：清单短一点点。trade-off 不成立。
- **不引入 watcher pattern / event push**：考虑过让后端在 stat 改变时 emit Tauri event，前端订阅。但 (a) stat 不是事件驱动——atomics 持续被 increment，没有"事件"；(b) 1 Hz polling + 1 IPC 是合理 baseline；(c) Tauri event 有自己的开销（subscription 管理、序列化）。Pull-based aggregator 是这种场景的对症解。

## Iter QG4 设计要点（已实现）
- **三个漏点正好揭示了"redact 不是一次性补丁，是 design pattern"**：早期 inject_mood_note 加 redact 时是按"补漏"思路写的——遇到一个补一个。结果 build_persona / butler / user_profile 后续做了，但 mood_hint 的另一个 entry point（proactive）漏了；reminders 和 plan 是后期 Iter Cβ/Cγ 加的，作者没意识到要 redact。QG4 应该订下一条：**任何把 memory description / title 拼回 prompt 的新 builder，必须有对应的 format_X_hint pure helper + 闭包 redact 参数 + 至少一个 redact 测试**。
- **closure (impl Fn / dyn Fn) 比 patterns: &[String] 灵活**：闭包 = wrapper 决定怎么 redact（substr / regex / 两段都做），formatter 不知不论；patterns 强迫 wrapper 二选一。代价是签名带个泛型/dyn —— 调用点不知道实际是哪种 impl，但调用点也不需要知道。这是把"实现细节" inflate 进 type system 而不让它泄到 caller 体感。
- **test 用 `redact_text` 直接构造 closure，不动 settings**：上 iter QG3 学到 ProcessCounters::default() 可以 in-process 构造；这 iter 同样地，redact 闭包是 `move |s| redact_text(s, &owned_patterns)`。tests 完全独立、并发安全。redaction 模块的 `redact_text` 暴露 patterns 是 pub fn，正好为这种用例设计。
- **mood_hint 与 inject_mood_note 没融合**：两边 prompt 内容差异大（chat.rs 还教 LLM 怎么 update mood），融合等于把 chat-only 引导泄进 proactive。两边都过 redact 即可，formatter 不必同源。
- **拒绝面向 panel 命令做 redact**：`get_pending_reminders` 是 user-facing 的 Tauri 命令，redact 它会让用户看到自己的提醒变成 (私人)——这违背了 redact 的定义"防止 LLM 二次泄漏"。redact 范围严格限定在 LLM-bound 路径。decision_log 反 reason 里也不 redact —— 那是 panel 给开发者看的，又不是 LLM 输入。
- **没动 consolidate.rs**：审计中看 consolidate 也读 memory，但它读完只用于"删除/清理"动作（找 stale reminder, 找已完成 butler）——不是把内容拼回 prompt。这是不需要 redact 的边界。

## Iter QG3 设计要点（已实现）
- **prompt_tilt 故意不为 manual 计入** —— 这是设计上的硬决策。tilt 桶（restraint/engagement/balanced/neutral）是 *prompt 内容*的统计，依赖 `active_labels`；manual 旁路了 gates，也没"针对性应用规则"。如果硬塞空 labels 进 record_dispatch，会全部归入 neutral，把 panel 的 tilt 饼图扭歪——尤其在用户高频手动 fire 验证 prompt 时。明确不计 = tilt 永远反映"loop 行为"这一种数据源。如果未来真要 surface "manual 触发了 N 次"，加一个独立 manual_trigger_total counter 比污染 tilt 更干净。
- **source 用 reason 字符串里的 `source=X` tag 而不是 ProactiveDecision 加新字段**：panel 已经渲染 reason 字符串，加新字段需要前后端 schema 联动 + 类型生成。tag-in-reason 是零 schema 改动，前端如果要硬解析也只是 `reason.includes('source=manual')`。decision_log 本来就是 free-form reason，再加结构化字段才是技术债。
- **rules_tag 是 `Option<&str>` 不是 `&[&str]`**：helper 接 caller 传来的现成字符串，不在内部从 labels join。loop 已经在 dispatch 时 join 过一次给 Run 决策用，传引用最便宜；manual 直接传 None。如果让 helper 自己 join，签名就要带 `&[&'static str]` + 拷贝逻辑，反而把决策耦合下来。
- **`append_outcome_tag` 提升为 pub module-level**：原本是 loop 内嵌的 closure 风格 inner fn（line 1522）。提到 module 顶层后立刻被三个 path 复用：loop Run 决策 reason、helper 自己、未来的 manual rules_tag 拼接。pure 函数 + 测试覆盖三种边界（空 / dash / chained），就是 D series "view-time mirror" 思路的延续。
- **不做整套 active_labels 抽取到 async helper**：本来想顺手把 1535-1601 那段 70 行的 labels 计算也抽成 `async fn compute_active_prompt_labels(app)` 给 manual 用。但那意味着 manual 也要付那笔 IO（mood / wake / build_*_hint / snap）的开销，每次手动 fire 多 ~50ms 不必要的等待——因为 manual 完全不需要这些 labels（gate 都没走）。决策：manual 让 prompt_tilt 留空 + rules_tag=None；如果未来真有"我想看 manual 触发时也带规则上下文"的需求，再做提取。
- **测试用 ProcessCounters::default() 而不是 mock**：counter 类型自己是 Default + 原子，可以直接构造一份"干净 process"出来，不需要 Tauri State / mock 框架。这种"可单独构造的 state object" pattern 给单测帮了大忙——所有 D 系列、E 系列的 panel 数据测试也是一个套路。

## Iter QG2 设计要点（已实现）
- **8 是个工程值**：1-3 轮覆盖大多数；5 轮以内是 tool-heavy（butler 委托后还要查 memory 再 search）；6-7 轮已经在循环边缘。8 留 1 轮 buffer，再上去基本是失控。如果实际生产观测到合法 turn 卡 5+ 轮，再调高，但不会突破 32。
- **const 而不 settings**：刚做 QG1 时把同样的 trade-off 做了一次（fmt allow 或重构）。这次再次选"硬编码 + 充分 comment"。理由：(a) 新 settings 字段意味着前端 UI、reload 逻辑、迁移、文档全要跟；(b) 普通用户调这个 const 等于"让宠物允许更长循环"——不是健康的可配置项；(c) 真要做成动态配置，是 QG3-style 大题（会带 settings 模式扩展、加测试），不是 QG2 范围。
- **`const _: () = assert!(...)` 是 Rust 隐藏宝石**：编译时 const 断言不需要 macro。用它把 magic number 的合理范围锁在编译器层。clippy 不会嫌（因为这是 const context，永远评估到固定 bool），同时它比 runtime test 强：没人能 ship 一个值 100 的 const，因为根本编译不过。test 里的 assert! on const 反而被 clippy 嫌（`assertions_on_constants`），那是 lint 提示用 const _ 这条路。
- **三路 fail-loud**：ctx.log（持久 + LogStore in-mem）+ sink.send_error（前端 stream）+ Err(_) (caller-level)。一开始考虑只 sink.send_error，但那只到 frontend；server-side 调用方（Telegram bot 用 CollectingSink）不 send_error，所以必须 Err(_) 让外层处理。三路并行的代价是几行；遗漏任一路就会出现"某入口 silent fail"。
- **pure helper enforce_tool_round_limit**：把 gate 做成 `usize → Option<String>` 而不是直接 inline 写 `if round >= MAX { ... }`。这样测试不需要任何 HTTP 模拟，纯函数 in/out 就 cover 边界。是 D series "view-time mirror" 思路的延伸——所有重要决策都有 pure helper。
- **不写 HTTP mock 集成测试**：mock 一个返 tool_calls 的 LLM endpoint 来 drive 真实 loop，能验证"确实没多发一次 LLM 请求"。但 (a) 工程量是本 iter 的 3-5 倍；(b) 只 cover 一个边界；(c) 现有 inspect 已确认 loop 头部 gate 在 LLM call 之前。投资回报率不够，拒掉，留笔记给未来 QG。

## Iter QG1 设计要点（已实现）
- **从堆功能切到质量收口**：D / E / F 系列连续加 surfacing / devtools / UX 后，TODO.md 顶部新加了 "下一阶段：质量收口" 段——这是一个明确信号（来自代码质量评估），下一批工作应该是 "把 alpha 推向可维护"。QG1 是入门门槛最低、最少改业务行为的一项，先做掉建立 baseline。
- **"先 fmt 再 clippy" 顺序**：fmt 是机械重排，对 clippy 的代码定位行号会有影响——先 fmt 后 clippy 让所有 clippy 报错的行号都基于已格式化代码，未来对照 git blame 时不会被搞混。
- **`#[allow(clippy::too_many_arguments)]` vs 重构成 struct**：两处选 allow。原因：(a) Tauri command 的 `State<'_, T>` DI 必须每个 state 一个独立 param，无法合并；(b) write_llm_log 是 thin wrapper，9 个字段都来自不同上游来源（StreamEvent 监听 / Instant timer / serde_json::Value），打包成 struct 实际上把 plumbing 推到调用点，那里是 hot path。allow + 一行 comment 解释 reasoning，比"为了 lint 干净而强行重构"诚实。
- **`is_some_and` vs `map_or(false, ...)`**：clippy 1.94 主推的，比 map_or(false, ...) 语义更直接。这是最近几个 Rust 版本里最常见的"小升级"，每次 toolchain 升级都会冒出几个。批量修是合理时机。
- **doc_lazy_continuation**：proactive.rs 那两处 doc 注释把"列表 + 总结句"挤一起，clippy 现在要求列表项后空一行才知道下面不是列表 continuation。这是 markdown 渲染的合理诉求，加空行不影响人类阅读。
- **不做 CI hook 这一步**：QG1 只是清掉债务，建立"下次再红就立刻能查"的 baseline。把 fmt/clippy 接进 pre-commit/CI 是后续 QG（更大动作，需要协调）的事。

## Iter F1 设计要点（已实现）
- **F series 拐回用户体验**：D 是 surfacing，E 是 dev tool，F 是 user UX 改进。这次发现"bubble 永久挂屏幕"是个真实长期 gap—只是前面 30 多个 iter 集中在功能加成，没人审视过 bubble lifecycle。
- **60s 选择**：30s 太短（用户走开倒杯水回来错过）；120s 太长（视觉负担）。60s 是 "能读到 + 不烦人" 的舒适带。后续若用户反馈再调；不暴露到 settings 避免配置项又一个不必要选项。
- **不区分 reactive / proactive**：本来想"reactive 用户主动发起的，可能想细看，不该自动消失"。但 reactive 的完整对话已经在 ChatPanel 里随时可读；bubble 是临时表层通知，60s 消失对所有路径都合适。
- **state 上提到 App.tsx**：bubble 自身没法干净地 owns auto-dismiss——它接 message prop 但不控制 visibility 来源。把状态在 App 层管，ChatBubble 仍是 dumb display 组件。
- **isLoading 期间不计时**：reactive 流式回复 isLoading=true，需要 bubble 持续显示直到生成完。useEffect 依赖 [displayMessage, showBubble, isLoading]，isLoading 切换 false 时 useEffect re-run，这次 timer 真的开始。
- **不写新 cargo / tsc 测试**：纯前端 useEffect timer，无 Rust 接口变更。前端无 React 测试 harness。

## Iter E4 设计要点（已实现）
- **VecDeque 在 const Mutex**：`Mutex::new(VecDeque::new())` 在 Rust const context 也成立——VecDeque::new 是 const fn。这种细节让 static 初始化无需 lazy_static / once_cell，干净。
- **保留 E1/E2/E3 mutex**：本可以全砍掉、所有 Tauri 命令从 ring buffer 读 last。但 E1/E2/E3 命令对外 API 是 `String` 单值；如果切到从 buffer 读 last 还要 unwrap empty case。多保留两个独立 mutex 的代价是几行复制——换得 backward compatibility 简洁。如果未来确认 panel 是唯一调用方，可以再清理。
- **5 容量 = 5 个 minute-scale turn**：默认 proactive interval 300s，5 个 turn 大约 25 分钟连续观测。这是研发短 session 改 prompt 的典型 window；超出走 logs。
- **navigator 索引方向**：« 是"更早" / » 是"更新"。这和"reverse buffer to newest-first" 配合：index 0 = 最新，index N-1 = 最旧。« 增加 index = 往后走时间 = 更早；» 减小 index = 往前走时间 = 更新。和 chat history navigation 习惯一致。
- **状态收敛**：原来 3 个 useState（lastPrompt/lastReply/lastTurnMeta）→ 1 个 (recentTurns) + index。current 派生为 currentTurn，再派生为各字段。UI 改动最小，但 source of truth 单一化避免"三个 state 不同步"风险。
- **disabled 按钮视觉**：cursor: default + 浅灰背景，非 disabled 是白底 cursor pointer。让 user 立刻看到边界（已经在第一条 / 最后一条）。

## Iter E3 设计要点（已实现）
- **timestamp 在 prompt build 时 set 而不是 reply 后**：prompt 里有 `time` 字段（now_local.format），那是 LLM 看到的时刻——和 panel 显示的时刻保持一致更直观，让"这个时间正是 LLM 看到的时间"。如果在 reply 后 set，会有几秒到几十秒漂移（LLM 调用耗时）。
- **tools_used dedup with BTreeSet**：原 tools 是 per-call list（一次 turn 可能 call 多个工具，每个 call 一行）。UI 想看的是"调过哪些"，去重 + 排序后展示更紧凑。BTreeSet 同时给排序，`get_active_window · memory_edit` 而不是按调用顺序的 `memory_edit · get_active_window · memory_edit · get_active_window`。
- **combined `get_last_proactive_meta` 而不是两个独立**：timestamp + tools_used 几乎总同时 want，一次 IPC 比两次便宜——E series 实践积累的小优化。E2 的 Promise.all 同时拉 prompt + reply 是同样思路。
- **不持久化**：和 E1/E2 一样，process 重启清空。如果 user 需要长期回看，应该走 logs（已有）或更系统化的 trace 存储——E series 是 transient inspect 工具。
- **modal 头部紧凑布局**：标题 + char count + ⏱ + 🔧 + copy msg + ✕ 全在一行。中文工具名（如 active_window）较短，` · ` 连接符即使 5-6 个工具也不会太挤。如果未来更多工具或更长名字再考虑换行。
- **E series 三连后形态稳定**：E1 (prompt) + E2 (reply) + E3 (meta) = 一个 modal 看完整 chat round。下一步 E4 候选可能是 "同 modal 切换查看上 N 次的 prompt 历史"——但那需要 ring buffer，复杂度上一台阶。先 ship 三连看实际使用是否有 demand。

## Iter E2 设计要点（已实现）
- **同 E1 完全镜像 pattern**：static Mutex stash + Tauri command + 在 run_proactive_turn 关键点 clone。E series 的 dev tool 设计模式正在自然涌现。
- **modal 双段而不是 tab**：tab 切换增加交互成本；通常 user 想同时看 in/out 找因果。两段用浅色背景（slate / green-50）区分 + 段头 emoji 箭头（⇢ ⇠）使方向感强烈。
- **复制按钮在每段头**：而不是 modal 顶部统一一个。LLM 用法上常常"复制 prompt 出去试别的 model" + "复制 reply 进 chat 工具分析"，两个独立操作，分开按钮符合实际 workflow。
- **navigator.clipboard.writeText**：Tauri 的 Webview 默认支持。如果某天 webview 模式变化无法用，fallback 是 textarea + execCommand("copy") 老方法——目前不需要。
- **toast 自动消失 2.5s**：消息出现在 modal 顶部信息栏（character count 旁），不阻塞操作；2.5s 是 "看到 + 但不长留" 的平衡。
- **stash reply 在哪**：选 `let reply = run_chat_pipeline(...)` 之后立即 clone，比"在 emit 之前最后一步"更早——即使后续处理出错（比如 persist_assistant_message panic），reply 也已 stash 让 panel 能 inspect。

## Iter E1 设计要点（已实现）
- **process 内 static Mutex 而不是文件**：last prompt 是 transient 信息，写盘没意义。每次 stash 是 lock + clone + write — 微秒级，不会让 run_proactive_turn 慢可观察。
- **clone 到 Option<String>**：每次都 clone 整个 prompt 字符串看似浪费，但 prompt ~1-2KB / proactive 触发频率分钟级，实际开销忽略。alternative 是 `Arc<String>` 但增加复杂度无收益。
- **modal 而不是 inline expand**：上次 prompt 通常 1-2KB 中文文字，inline 展开会推开下面所有 logs 让排版乱。modal 更专注、可滚动、点 backdrop 关闭符合习惯。
- **modal pre + whiteSpace pre-wrap**：保留 prompt 段落分行（`\n`）但允许长行 wrap，比 plain `<div>` 更接近"读文档"体验。研发常用复制粘贴整段去外部 LLM 工具试，pre 方便选取。
- **没有自动刷新模态内的 prompt**：modal 打开瞬间抓取一次，之后不再 poll。理由：用户开 modal 就是要看那一次的 prompt；连续轮询反而让"当时的 prompt vs 现在的"混淆。如果用户想看新的，关掉 + 立即开口 + 重开。
- **D series → E series 转向**：D 是"信号 surface"，E 是"工具向"。E1 是看 raw prompt，未来 E2/E3 可能是"对比两次 prompt 差异"、"模拟改 settings 后 prompt 长什么样"等。E series 服务研发 / 高级用户。

## Iter D12 设计要点（已实现）
- **disabled chip 设计 vs 隐藏**：本来 chip 在 strip 里通常表"激活信号"（chip 出现 = 状态成立）。disabled 是"禁用状态" — chip 出现就表示问题。这种"反向 chip"在视觉上稍特别，但语义上正确：用户看到 chip = 有事。
- **置于 strip 首位**：最显眼。其它 chip 虽然按"时间维度→用户→宠物→gate"分组，但 disabled 状态一旦出现要压过所有其它 — 因为后续的所有信号都"不会被引擎使用"。把 chip 放最前是 visual hierarchy 的应用。
- **深灰底白字**：和其它 inline 文字 chip 不同，用胶囊形 background 表"系统级状态告警"。和 ✨ companionship 的彩色渐变（庆祝感）形成对比 — 这是"该处理"chip。
- **fallback enabled=true**：settings 读失败时 chip 不显示。错误情况不显示等于"假告警"和"假静默"之间选后者——前者会引导用户去关一个本来已开的开关。
- **D series 总体回顾**：12 iter 把"为什么宠物现在如此"从黑盒打开成 11 个 chip 维度（period / day_of_week / idle_register / cadence / wake / pre_quiet / in_quiet / focus / cooldown / awaiting / disabled）+ 时间行隐含的 idle/input_idle 数字。如果未来加新 gate 或 prompt signal，pattern 是 ToneSnapshot 字段 + chip。
- **没有把所有 chip 套个 "gate 类" / "context 类" 分隔**：本可以加 vertical separators 把 "context"（period 等）和 "gate"（cooldown 等）视觉分开。但 strip 现在已经 11 个 chip 满满当当，加 separators 反而拥挤。让 user 自己用 emoji 类型快速识别——⏱📆👤💬 是时间维度，☀🌙😴⏳💭🎯🔕 是 gate 维度。

## Iter D11 设计要点（已实现）
- **看到 D10 之后立刻意识到这是 bug**：D10 加 chip 时检查 awaiting 的 lifecycle，发现"只有 mark_user_message 清"。Cooldown 有 wake_soft 软化，awaiting 没任何 time-based 释放。这是项目里被搁置了很久的潜伏问题。如果不是 D10 强迫我盯着这个 gate，可能再过几个月才发现。
- **state vs effective 双轨**：raw 状态留在 ClockInner，effective 在 snapshot 返回。这种"权威态/视图态"分离让"用户回了一句"仍然是清除 awaiting 的唯一权威路径——可以追溯、可以 invariant-check；effective 是 snapshot 时的视图，可以根据时间衰减。这个 pattern 借自 D5 的"updated_at 是 schema 真值，前端把它转成相对时间"。
- **4 小时阈值理由**：lunch + 会议典型 < 2h、单次睡眠 ≥ 7h。4h 在两者之间——足够 honor "polite wait"，又不至于变成"用户回家几小时还得听宠物等回应"。如果未来用户反馈"宠物太快忘了 polite wait" 或 "宠物太久不动" 再调。
- **None case defensive**：如果 awaiting=true 但 since_last_proactive=None（不该发生但 belt-and-suspenders），返 false。让 mark_proactive_spoken 设原子地维护 invariant，但 snapshot 不依赖那个 invariant。
- **同 wake_soft 平行**：cooldown 在 wake-recent 时 soft；awaiting 在 4h 后 soft。两个机制各自处理对应 gate 的"长别豁免"。可以想象未来加一个统一的"长别时所有 gate 都软化"——目前两个 gate 各管各的更可读。
- **测试只测 pure 函数**：snapshot 调用本身依赖 Instant，无法 deterministic 单测；提取 `effective_awaiting` 后所有边界用 (bool, Option<u64>) 输入测，简单且完整。

## Iter D10 设计要点（已实现）
- **D10 是 D series 必然延伸**：D9 surfaced cooldown 那一刻我已经知道 awaiting 是同样问题——不同 gate 同样 invisible。本来想在 D9 一起做，但分两 iter 让 commit 历史更清晰、scope 更小、也方便单独 revert。
- **状态 vs 时间双 gate**：awaiting 是 state（boolean，事件驱动 reset），cooldown 是 time（duration，时钟驱动 reset）。两个 chip 并列出现时给用户的认知不冗余——明白宠物因为两个独立原因都在等。
- **紫色 #a855f7**：和 ★ motion 一致——proactive engine 的"内部状态"色系。和 ⏳ cyan（功能性 / 已知 schedule）形成对照。
- **不显示 awaiting 持续时长**：snapshot 里没有"awaiting 自何时开始"的 timestamp。只显 boolean 即可——用户给宠物回一句话就清掉 (mark_user_message)，时长不重要。如果未来想加，需要在 ClockInner 加个字段。
- **gate 全集小结**：7 个 gate 现在 5 个有 chip / 2 个隐含。没强迫每 gate 都做 chip——disabled 是配置态、idle / input-idle 已经在 ⏱ 行隐含数值。整体观察：D series 是把"为什么宠物现在没说话"从黑盒打开成 5 个 chip 维度。

## Iter D9 设计要点（已实现）
- **mirror gate 而不是 reimplement**：cooldown gate 的 `since < cooldown_seconds` 检查在 spawn loop 已经写过；ToneSnapshot 这边写一次同样的逻辑。两处可能漂移——但都是 4 行算术，比抽 helper 还简单。如果某天调整 gate 逻辑（比如 cooldown 在 wake 后 soften），两处都要改，但 grep `cooldown_remaining` 就找到了。
- **Option<u64> vs (bool, u64)**：传 Option 表达"gate 关时无 N"——比布尔 + 数字双字段更紧。Some/None 的语义在 TS 里 nullable 也对应 1:1。
- **chip 在 cadence 之后**：cadence 是 "上次开口多久前"（已发生），cooldown 是 "下次开口最快还要多久"（未发生）—— 时间轴上 cadence → cooldown 是连续推进，并列陈列读起来自然。
- **青色 #0891b2**：和 ☀ wake 一致（也是 informational gate 信号）。区别 ⏰ 红 (urgent due) / 🌙 红 (warning approach) / 😴 灰 (in dormant) / 🎯 紫 (in focus)。整个 strip 现在有一个相对一致的语义/颜色映射。
- **wake_soft 不影响 cooldown_remaining**：proactive 路径在 wake_soft=true 时跳过 cooldown 检查。但 cooldown_remaining 始终按字面计算 — panel 显示"还有 12m"是事实，gate 是否会 honor 由其它逻辑决定。把这两个解耦避免 panel 显示逻辑被多变量复杂化。如果未来想显示"cooldown 但 wake softens 了" 类二级状态再加。

## Iter D8 设计要点（已实现）
- **lightweight `get_user_name()` vs `get_settings()` 全量**：PanelPersona 5 秒轮询。每秒拉全量 settings（API key 等大字段）浪费 IPC 带宽。一个 dedicated 命令 wraps `get_settings().map(|s| s.user_name).unwrap_or_default()`——零成本封装，意图明确。
- **位置在 "陪伴时长" Section**：name 是关系绑定 ("我和谁陪伴")，比起放 "自我画像" 段（宠物自己写的）更切题。companionship 段就是"我们俩"主题，name 是其中一支柱。
- **空态斜体提示路径**：`🐾 还没设名字（Settings → 你的名字）` 让用户立刻知道去哪里设。比单纯空白或 "未设" 更 actionable。
- **emoji 🐾 而不是别的**：🐾 宠物 / 关系语义；不和其他 emoji（⏱ 时间 / 📆 日历 / 👤 用户 / 🎯 focus 等）冲突。
- **不动 SettingsPanel**：那里已经有 user_name 输入字段（Cτ 添加）；本 iter 是单向反向显示，不重复 affordance。

## Iter D7 设计要点（已实现）
- **propagate up vs persist**：曾考虑加 `last_consolidate_summary` 全局 atomic，让 panel 任意时间能拉。但 banner 是"点完按钮立刻看"的临时 feedback——没必要持久化，让 trigger_consolidate 直接返回更直接。如果以后做 "consolidate 历史" 时再加。
- **160 vs 200 chars**：logged 截到 200，banner 截到 160。banner 在 panel 横幅里高度敏感，160 加 prefix 字符串大约填一行半；200 容易换行多次。
- **spawn loop 调用兼容**：`if let Err(e) = run_consolidation(...).await` 在新签名下 Ok arm 含的 String 自动 drop，行为不变。这是 Rust `if let Err()` 的隐式好处——添加 Ok payload 不破坏只关心 Err 的调用方。
- **prefix · summary 分隔符**：`·` 比 `\n` 在 banner 单行更合适；比 `:` 视觉更轻；和项目其他地方用 `·` 作 chip 分隔器一致。
- **不写新单测**：纯返回值传播，没新逻辑分支。LLM summary 的具体内容是 model-driven 不可单测；时长格式化 / chars().take(160) 是 trivial。

## Iter D6 设计要点（已实现）
- **prompt-only fix vs UI 改动**：曾考虑做"butler 执行检测后强制 emit 一条 toast"。但那意味着 panel 持续监听 butler_history poll、状态机复杂；prompt 内一句话教 LLM 把执行反馈带进 bubble 是更简洁的路径。LLM agent 模式的好处之一就是：行为塑造不必硬编码到代码层。
- **位置 schedule 之后、错误之前**：footer 里现在有三个段落：操作指南（update/delete）→ schedule 含义 → "记得提一下" → 错误处理。"提一下"放第三段是因为它依附于"成功执行"语义，紧跟操作指南之后；错误处理段需要 LLM 已经知道前面的执行流程才有意义，所以放最后。
- **给两个示例 vs 让 LLM 自由发挥**：「我帮你写好 today.md 了」/「Downloads 整理完了」是固定句式示例。LLM 看到具体例子比"请简短描述"指令更可靠地输出短句——降低过度解释的风险。
- **强调 "不必描述细节"**：LLM 默认倾向把"你做了什么"展开成详细汇报；明确"一句话即可"避免 bubble 文案膨胀。
- **contract test 钉关键 phrase**：和 Iter Cι（teach delegation）/ Cσ（teach user_profile）一样的契约测试模式。如果未来重构 footer 文字，test 立刻 fail——避免"我以为某段 prompt 还在但其实被改没了"这种隐藏退化。
- **不动 butler_history schema**：如果想要"硬保证宠物提到了执行"，可以在 chat pipeline 里检测 LLM 输出并强制 inject 一句话——但那是 prescriptive style，违背 LLM agent 自治。Prompt 教学 + 实际行为观察是更轻巧的路径。

## Iter D5 设计要点（已实现）
- **改返回类型 vs 加新命令**：曾考虑加 `get_persona_summary_meta`，把现 String 命令保留以便其它调用方不改。但这是单一调用方（PanelPersona）的命令，没有外部 API 兼容性顾虑，直接升级返回类型是干净路径。
- **7 天阈值**：consolidate 默认 6h interval × 7 天 = 28 次机会。若超过 7 天还没更新，要么 consolidate 关了、要么 LLM 27 次评估都觉得"信号不足跳过"——两种都该提醒用户。
- **stale 用红 + ⚠**：和 ❌ 错误 / 🌙 距安静时段 等"该处理"信号同色系；非 stale 用浅灰斜体 = "信息性、不打扰"。
- **本地时间在 tooltip**：精确时间（locale string）放 tooltip，主显示用相对时间—— "3 天前" 比 "2026-04-30T08:15:00+08:00" 直观，但调试场景需要精确。两层信息密度。
- **不动 ai_insights/persona_summary 的 updated_at 写入路径**：memory_edit 已经在每次 update 时刷 updated_at，零额外工作。这是项目里 update_at 字段的"sleeper feature"被本 iter 唤醒——已经在 schema 里好几个 iter 没被 leverage。
- **不写新单测**：纯 wire-up + 渲染，依赖既有 memory schema 测试。如果未来 PersonaSummary 字段增多到承载 logic（不只是 dumb pass-through），再加单测。

## Iter D4 设计要点（已实现）
- **D3 之后再审视发现盲区**：上一个 iter 我说"D 三连 closes parity"——但仔细看 PanelToneStrip 的实际行为，pre_quiet 是 transient 的（仅 15min 窗口），跨过窗口后那段几小时的 quiet 期间 panel 信号全空白。这是"D series 自检"的价值——封口后再走一遍能发现自己当时遗漏的细节。
- **fn 改 pub 即可**：in_quiet_hours 已经 private，已被 4 个单测覆盖完整 boundary cases。改 pub 是最小改动；test 都不用动。
- **chip 颜色 vs pre_quiet 红**：pre_quiet 用红色 (#dc2626) 表"快进入了，要警觉"；in_quiet 用深灰 (#475569) 表"已经在睡，平静"。视觉强度梯度对应"approaching → in"的紧迫感降级。
- **emoji 选 😴 而不是 🌙**：🌙 已被 pre_quiet 用了（月亮表示"快天黑"）。😴 表"在睡了"，语义不重叠。如果未来想加"刚醒来" chip 可以用 🌅。
- **panel 11 chip 这事**：太多 chip 也是问题。但目前每个都对应一个 prompt 决策维度，算高密度但有意义。如果将来加更多，考虑分组 / 折叠 — 比如 "时间维度" / "用户维度" / "宠物维度" 三组。

## Iter D3 设计要点（已实现）
- **复用 focus_status() 的代价是再做一次 IO**：每次 get_tone_snapshot 调用都会读 `~/Library/DoNotDisturb/DB/Assertions.json`。panel 1s 轮询 → 每秒一次 disk read。考虑成本：单文件几 KB，OS 读缓存友好，几乎无开销。但如果 panel 多窗口同开（每窗口 1s 轮询），IO 会乘数。先用简单方案，必要时 cache 100ms 也容易做。
- **fallback to "active"**：当 macOS 不能解析出 focus name（旧版本格式不同）但 active=true 时返回 "active" 字符串。chip 显 "🎯 focus: active" 比直接 None 更有信息——告诉用户"我们看到 focus 在跑，只是不知道是哪个"。
- **chip 紫色加粗 vs 红/橙**：Focus 不是"警报"，是"用户在专注"——紫色（identity / persona 用色）传达"这是用户当下身份状态的事实"。加粗略提示重要性但不抢眼。
- **不在 stats card 加 focus**：stats card 是聚合数字 + 长期 identity；focus 是 live transient signal——属于 strip。和 D2 把 milestone 放 stats card 反过来（milestone 罕见但持久整天，focus 频繁但短暂）。
- **D series 收官 mostly**：经过 D1/D2/D3 三连，proactive prompt 的所有 ambient 信号（除了 user_name 这种纯静态 settings 字段）panel 都能可视化。如果未来 prompt 加新 signal，D 系列 pattern 就是直接答案：ToneSnapshot 暴露 + panel 加一个 chip。

## Iter D2 设计要点（已实现）
- **复用 Cρ helper**：companionship_milestone 已经是 pure 函数，Tauri 端直接调用 + map to String。和 D1 一样都体现"single source of truth": prompt rule 和 panel chip 都从同一个函数读，不会因为某天扩展阈值（比如加 14 天里程碑）而漂移。
- **chip 在 stats card 而不是 tone strip**：D1 把 day_of_week / idle_register 加进 strip 是因为它们是高频变化的信号（time line 每分钟变）；milestone 一年才几次，放高频区域突兀。stats card 的"陪伴 N 天" column 是 milestone 信号自然的承载位置。
- **渐变色 chip 而不是单色**：⏰ 红 / ❌ 红 / ⚠️ 橙等已经被其它"该立刻处理"的事件占用。milestone 是"庆祝一下"语义，需要积极但不刺眼的表现。橙→粉 linear-gradient 给"温暖、特殊、轻微闪耀"感受，不抢眼。
- **附带 companionship_days 字段**：strip 不一定立刻用，但放在 ToneSnapshot 让未来"宠物心情 + 陪伴时长"等组合视图无需额外 IPC 即可读。这是给将来留的小后路。
- **不写新 cargo 测试**：companionship_milestone 在 Cρ 已锁 4 个 test。本 iter 是 wire-up 不是新逻辑。如果有动到判断逻辑（比如改阈值）才需要新测。

## Iter D1 设计要点（已实现）
- **route 命名转 D 系列**：Greek 字母用到 ω 后再延续会进入 unicode 怪区（𝝰 等），且没人想打那种符号。从 D1 开始作为 "diagnostics / dashboard surface" 的新轨——和 Iter Dx（Memory tab）共享 D 前缀但用数字区分。
- **复用 helpers 而不是从零计算**：format_day_of_week_hint 和 user_absence_tier 是 Cβ/Cμ 的 pure 函数；get_tone_snapshot 直接复用——保证 prompt 和 panel 永远不会漂移。这是"single source of truth"原则的一次硬规整。
- **idle_minutes 透传 vs idle_register only**：曾犹豫只暴露 register 字符串。但精确数字给 tooltip / 调试有用——chip 显示文字、tooltip 显示数字，两层信息密度。
- **chip 顺序：⏱ period → 📆 day_of_week → 👤 idle_register → 💬 cadence**：从"现在几点"→"今天什么日子"→"用户在哪"→"宠物自己有多久没说话"，认知顺序自然。
- **不在 strip 里加新 chip 表示 user_name**：name 是 settings 静态值不变化，每秒打到 strip 上是噪声。Persona tab 已经 implicit 承载这个（用户在 Settings 里改、看到自己输入即知）。
- **没有新 cargo 测试**：这次只是数据透传 + 渲染层；guards 来自既有 day_of_week / user_absence_tier 单测（覆盖了 source）。前端 strip 没有测试 harness，这一点是项目的长期 gap，不在本 iter scope。

## Iter Cω 设计要点（已实现）
- **发现 bug 的过程**：本来打算"加一个 API health 指示器"。审查现有 chip 时直接看代码`silent + error > spoke + silent + error`——脑子里立刻消去左右公共项：`0 > spoke`。永远 false。这种"代数化简找 bug"的微习惯救过我多次；以后还得保持。
- **整数算术 vs float**：本来写 `(silent + error) / total > 0.5`。但 `silent * 2 > total` 等价、避免 float、和后端 redaction.rs / data_driven helper 用过的整数比较模式一致。
- **error 单独子标签**：曾考虑融进主标签（"LLM沉默+失败 X/Y"）。但失败和沉默是不同语义——沉默是 LLM 自主选择，失败是基础设施问题。合并会让"沉默率高" tone 调优反馈和"API 出错" 配置反馈混淆。子标签红色独立呈现，user 视觉直接分辨。
- **IIFE 重构**：原来全部 inline JSX，多次重复 `llmOutcomeStats.spoke + llmOutcomeStats.silent + llmOutcomeStats.error`。提到 `total`/`silentPct`/`restrictive`/`hasErrors` 局部变量后，行短了、可读性提了。这种"render block 局部命名"在 React 里可能被认为重，但当 expression 重复 4-5 次时 worth 抽。
- **不写测试**：前端无 React 测试体系，本次修复完全是 chip 渲染的局部化。但 bug 本身就是教训：Rust 端的 cargo 测试应当能覆盖这种纯 view-time bug，但前端这层一直裸奔。如果未来加 vitest + RTL，这种 chip 渲染应该是早期目标。

## Iter Cψ 设计要点（已实现）
- **复用 tone 而不是新 invoke**：since_last_proactive_minutes 已在 ToneSnapshot 里、PanelStatsCard 已接 tone prop（chatty 判断要用）。这次只是从已传进来的 tone 上多读一个字段——零 IPC 增加、零 state 增加。Iter 99 把 PanelStatsCard 从 PanelDebug 抽出来时把 tone 当 props 传，这次正好享受当时的设计 dividend。
- **"前开口" 反语序而不是 "上次开口前"**：测试小标签拼"前开口"读起来像"距上次开口的时长"——和 "X 天陪伴" 一样的尾置 label 模式。如果写"上次 X 开过口"则 tooltip 的语义重复在主标签上，反而臃肿。
- **null 显 "—" 而不是 "从未"**：fresh install 第一天，宠物还没开过口很常见。"从未" 隐含批评意味（你没让宠物说过话），"—" 中性。tooltip 解释 None 的含义。
- **颜色按 60 分钟切**：和 idle_tier 的 60 分钟边界一致——刚说过话 (< 60) 算"还热"，超过 60 算"凉了"。视觉权重对应这个语义切分。
- **不显示秒级**：since_last_proactive_minutes 已经按分钟取整。秒级精度对"是否说过"没意义、徒增噪声。
- **不动既有列**：今日/本周/累计 columns 字号和颜色都保持 Cρ 之前样式，新列插入间无破坏。这种"加一列不动其他"的演化模式让 PanelStatsCard 演进顺滑。

## Iter Cχ 设计要点（已实现）
- **TS regex 而不是 Rust helper**：Cπ 的 has_butler_error 是双轨 — 后端 substring + 前端 indexOf。这次 strip 也走 TS 端 regex，与 Cπ 的前端 parse 路径一致。后端要不要加 strip helper？暂不——目前唯一需要 strip 的是 panel 这一处用户操作；如果以后 LLM 也想自动 strip，再加 Rust 工具不迟。
- **不上 butler_history**：这是 Cλ sweep 同样的判断——consolidate / 用户清除都是配置变更而非宠物执行。butler_history 是"宠物对你做了什么"，timeline 上看到 "user cleared" 反而冲淡信号。
- **chip 内嵌 ✕ 而不是行尾按钮**：把清除入口贴在被清除的 chip 上，UI 心理学上更直观——"我要消除这个东西，按它身边的 X"。如果放行尾要用户从 chip 视觉点跳到行尾决策点，多了一段眼动距离。
- **不加确认对话框**：清除失败标记是低风险动作（保留任务 + schedule，只去 marker），加确认会让用户怀疑这是高危动作。和 ⏰ 标记自动消失（任务被 update）的机制对应——失败标记同样应该轻量可消。
- **regex `\[error[^\]]*\]\s*` 与 Rust has_butler_error 的 `[error` substring 对称**：has_butler_error 检测，TS strip 移除。两个方向一对——和 Cπ Cθ 同样的"view-time mirror"模式。
- **保留 schedule prefix**：strip 只针对 [error: ...]，不动 [every:] / [once:]。schedule 是用户的"什么时候做"配置，不该被"清错"动作误删。

## Iter Cφ 设计要点（已实现）
- **复用 trigger_consolidate 而不是新 command**：consolidate 已经是 robust 命令——调用 LLM 反思 + 更新 persona_summary。不需要专门写"only-persona"variant，反正同一次 LLM 调用顺便也整理其它记忆，浪费几乎为零。
- **空态内嵌按钮 vs 顶层按钮**：曾考虑在 Persona tab 顶部加一个全局 "立即整理" 按钮（像 Memory tab 那个）。但 Persona tab 大多数时候 personaSummary 已经存在，按钮会一直挂在那当摆设。空态条件渲染既能解决"找不到入口"问题、又不在常态污染 UI。
- **12 秒后清除消息**：consolidate 的状态消息（"Consolidation finished in N ms"）成功后没必要常驻——5 秒轮询会刷出新的 personaSummary，那才是真正想看到的反馈。msg 持续到 12 秒兜底告知"刚刚跑过"，之后让位给画像本身。
- **不引入 Tauri event listener**：trigger_consolidate 已 await 直接返结果。没必要监听 emit 事件。简单 await + setTimeout = 已经覆盖全部 UI 状态机。
- **分两态写消息颜色**：失败红 / 成功青。和 PanelMemory 的失败/成功 toast 同色系一致。
- **不动 Memory tab 的"立即整理"**：那个是宠物记忆维护工具，整体语义；Persona 这个是"我想立刻看到画像"——两者目的不一样，UI 也理应不一样的位置。

## Iter Cυ 设计要点（已实现）
- **复用 Cτ 措辞 1:1**：reactive chat 的 persona_layer 和 proactive 的 prompt 现在用完全相同的"你的主人是「X」——开口时..."一句话。让用户在两个路径里听到的 LLM 表现一致。如果两处用不同 wording，LLM 见到不同 framing 会做出微妙差异的称呼策略，对用户来说显得分裂。
- **行内 if 而不是抽 helper**：本来想抽 `format_user_name_line(name)` helper 让 persona_layer 和 proactive 都调。但每处都是 1 行 trim 检查 + 1 行 format!() = 共 4 行代码——抽 helper 反而让人要跳到另一个文件读细节、阅读断裂。这种"小尺度 duplication"在项目里多次接受过（Iter Cθ 的 TS/Rust 镜像也是双份）。
- **位置：companionship 之后、persona_hint 之前**：companionship 是"我们认识多久"的事实，user_name 是"我跟谁说话"的事实，persona_hint 是"我对自己怎么看"的反思。从客观事实 → 关系定位 → 主观自描述，叙事链条自然。
- **`base_inputs` 默认 user_name=""**：和 Cτ 同样思路。新加 PromptInputs 字段时如果默 fallback 触发新行为，所有现有测试都要重做断言；改默值保持中性、专门 case 单独 set。
- **不接 ToneSnapshot**：用户在 panel 已经能在 SettingsPanel 看到自己输的 user_name，不需要 panel 再回显一遍。ToneSnapshot 是 prompt 决策状态，user_name 是 prompt 输入数据——架构区分应当保持。
- **proactive 的 build_persona_hint 不动**：那是 ai_insights/persona_summary 的 reader，是宠物自我反思内容，和 user_name（owner identity）不该混。两个独立 system context channel。

## Iter Cτ 设计要点（已实现）
- **settings 字段而不是 user_profile 条目**：曾考虑让 LLM 通过 user_profile.title="姓名" 间接管。但 (a) 用户期望"输入名字 → 立刻被称呼"是直接路径，绕一层 LLM 太曲折；(b) settings 字段在 UI 上立刻可见可改，user_profile 是 LLM 的世界；(c) 名字是 first-class 关系绑定，配 settings 字段更符合"宠物 vs 主人"语义。
- **prepend 而不是 append**：persona_layer 原本是"陪伴时长 → 画像 → 情绪谱 → tail 指引"。把 user_name 放最顶让 LLM "先知道是谁，再读身份背景"——叙事更顺。LLM 实际生成时会把 prompt 当对话准备阶段，最重要的"指代对象"放最早最不容易被忽略。
- **persona_layer only，不动 proactive**：proactive 有自己独立的 build_persona_hint（Iter Cw redaction 那条线）。本 iter scope 控制为 persona_layer 路径（reactive chat + Telegram）。proactive 想要也能加，但要改 PromptInputs 字段 + 三个 callsite，单 iter 外延出去过大。留作 Cυ 候选。
- **placeholder 写「留空则用「你」」**：直接告知用户"不填会怎样"——降低迟疑成本。新装用户最常的疑问"我必须填吗？" 立刻有答案。
- **trim 而不是 raw**：用户常会复制带空格的字符串（"  moon  "），trim 一次让显示干净；whitespace-only 视为空避免出现「你的主人是「  」」这种空名字 prompt。这种"对人类输入容错"的小细节累积起来体感差很多。
- **trims 测试用户名**：`format_persona_layer_trims_user_name_whitespace` 钉住「  moon  」→「moon」，避免未来某次重构去掉 trim 静默回归。
- **不和 SOUL.md 合并**：SOUL.md 是用户可自由编辑的 prompt 段落——可能用 markdown、长篇 instruction。user_name 是结构化字段，要参与 prompt 构造逻辑（trim、empty check）；混进 SOUL 就需要解析 SOUL 找名字，复杂多了。两个 namespace 各管各的。

## Iter Cσ 设计要点（已实现）
- **对称的捕捉/注入架构**：Iter Cα 做了"读 user_profile → 注入 prompt"，Cσ 做了"听到 stable fact → 写 user_profile"。两个方向一对就形成完整记忆闭环。这种"对称设计"原则在 butler 路径也有：Cγ-Cπ 的 delegate（Cι 教捕捉）↔ inject（Cγ ambient block）↔ execute（proactive）↔ 留痕（Cε），每个方向都有对应教学。
- **三正三反例**：单方向例子可能让 LLM 误推（"我累了"是不是该写？）。三正 + 三反明确边界——尤其 "我老是忘喝水" 反例引到 todo+remind，对应 Cι 教过的 reminder vs butler 区分；这里多对一个轴（user_profile vs todo）让 LLM 不会把 ephemeral fact 错判成 stable preference。
- **dedup via update 而不是堆叠**：user_profile 容易膨胀——用户每提一次都 create 会 1 条变 5 条相同条目。明确教 LLM 如果相近条目存在就 update。代价是 LLM 要先 memory_list 检查（多一次工具调用）——但这成本可接受，因为 user_profile 通常 < 10 条，list 一次便宜。
- **不强制每次 acknowledge**：曾想要求 LLM "捕捉后必须用一句确认"。但用户大多数时候不希望这种 process noise——「我用 mac」 「好的我记下了」 显得官僚。改成 "简短确认或自然 acknowledge"——LLM 自己判断，可以一句"嗯，mac 党呀"自然带过，也可以完全不提（事实进了 user_profile 就行）。
- **捕捉 → 注入 因果讲明**：末段一句"这些条目会自动出现在你后续 proactive 的提示里，让 ta 越用越懂用户"——告诉 LLM 它这次捕捉的工作以后自己会受益。这种"教 LLM 它的行为如何回流到自己"是 prompt 工程的隐藏增益层：让 LLM 把这件事看作 self-interest 而不仅是被 instructed。
- **不改 SOUL.md**：SOUL 是 identity（"我是一只什么样的宠物"），TOOL_USAGE_PROMPT 是 operational（"你应该怎么做事"）。识别 / 处理 user fact 是 operational，不该上升到 identity——否则 SOUL 越来越多 instruction，identity 失焦。

## Iter Cρ 设计要点（已实现）
- **data-driven 还是 environmental**：milestone 用 `companionship_days` 计算（counter，data 性质），但语义更像"今天是某种特殊环境状态"。最终选 data-driven 因为：(a) data-driven 已有 `chatty` 这种"基于 counter 的状态"先例；(b) environmental 已经 6 个字段了再加变沉重；(c) data-driven 加一个 u64 参数边际成本低。
- **rule body 强调"作为底色"**：曾写过更激进的版本「告诉 ta 这是百日，要怎么纪念」——但和 Iter 5 cooldown / Iter 75 chatty 的"克制原则"冲突。LLM 容易把"今天是 100 天"当大事件、强行要求用户回应或开长篇感慨。明确写"轻轻提一句、不要要求 ta 回应、纪念日只做底色"是把这条规则定位成"语气微调"而不是"话题选择"——和 prior persona/mood register 系列同思路。
- **365 之后每年一次 vs 复杂规则**：本来想加"满 1.5 年"、"500 天"等。但里程碑越多越频繁，每次都触发会让用户疲劳，反而失去仪式感。固定 6 档 + 周年是经过权衡的——保留稀缺度。
- **base_inputs 默认 5 而不是 0 或 30**：天 0 会触发"今天初识" framing；天 30 触发新 milestone 让所有 base_inputs 测试间接踩到新规则。天 5 安全：既过了"第一天"特殊状态、又不是里程碑。这个 fixture 调整在 Iter Cξ first-of-day 时也做过同类操作（today_speech_count 0→1）。
- **production 加 await**：spawn loop 和 get_tone_snapshot 现在多 1 次 `companionship_days().await`。companionship 的实现读 install_date.txt 一次 IO，几微秒可忽略。但是要注意 await 必须在 async 上下文——fortunately 这两处都已 async，没问题。
- **ToneSnapshot 不暴露 milestone 字段**：panel "prompt: N hint" badge 已经覆盖（label 进 active_prompt_rules），不需要专门的字段告诉 panel "今天是百日"。如果未来要做"today is your X day" 视觉徽章，再加。

## Iter Cπ 设计要点（已实现）
- **`[error]` description 约定，不引入新文件**：曾考虑加 `butler_errors.log`（类似 butler_history 但只记错误）。但那要：(a) 决定哪一层捕获错误（chat pipeline 太晚 / tool 层太杂 / proactive turn 后扫描 history 不准）、(b) 持久化模型一致问题、(c) 又一种文件用户搞不清。description 字段约定路径最低成本——LLM 自己负责写、prompt 教会、parse 一处、UI 一处，all done in 一 iter。
- **不动 butler_history.log**：error 不是事件流，是状态。如果一个任务失败 5 次成功 1 次，butler_history 看到的还是最近一次 update（成功），description 里 error 被 LLM 移除。这种"状态而非事件"的建模更接近 user 心智："这个任务现在卡着吗？"答 yes/no，不需要看历史滚动。
- **substring `[error` 而不是 `[error:`**：LLM 写法不稳——可能写 `[error: x]` / `[error :x]` / `[error]`。锁死 `[error:` 会漏匹配。子串 `[error` 不会和正常文本误碰（"error" 单词出现是常态，但前面跟 `[` 几乎只可能是 marker）。
- **marker 顺序：错误前 / 到期后**：`❌ 错误 · ⏰ 到期 · title` vs 反向。两者都急但错误更要求人为判断（要不要重试 / 要不要改任务），到期更程序化（自动会下一次 proactive 选中）。读 chip 顺序时人脑先关注最需要决策的——错误。
- **header 走 4 路 match**：原来 due_count > 0 / == 0 二路，现在 (0,0) / (d,0) / (0,e) / (d,e) 四路。如果以后再加状态（比如"这次跳过"或"暂停"），可能要重构成 builder——先保持 explicit match，模式不复杂。
- **后端 footer + 前端 chip 双轨**：footer 教 LLM 怎么打标，chip 给用户视觉反馈。两者协议一致（都看 `[error`），但靠不同代码实现——后端 has_butler_error / 前端 parseButlerError——是 Cθ 同样的 view-time mirror 模式。
- **chip 颜色比 ⏰ 软**：`#fef2f2` 背景 + `#991b1b` 文字。⏰ 到期是 `#fee2e2` + `#b91c1c`。两个红区分明显但不刺眼——错误是"该看一眼"不是"立刻动作"，到期是"程序自动会动手"。视觉权重对称。
- **不向 TOOL_USAGE_PROMPT 增 error 段**：reactive chat 路径里 LLM 几乎不执行 butler 任务（它们大多在 proactive 时被触发）；让 reactive 也学这个 marker 反而会污染普通对话。proactive prompt 里 footer 已经教得很清楚，单点教学。

## Iter Cο 设计要点（已实现）
- **新 Tauri 命令而不是复用 tone_snapshot**：tone_snapshot 已经返回 `mood_text` 和 `mood_motion`。Panel Persona 是不是直接 invoke 这个就行？反对：(a) tone_snapshot 是 debug 用大杂烩（10+ 字段），加载慢；(b) Persona 只需要 mood，多拉数据浪费；(c) tone_snapshot 是面向 prompt builder 的视角（per-tick fresh），Persona 可以 5 秒拉，是不同节奏。新命令小、目标明确、独立缓存友好。
- **`raw` 字段也返回**：本来 text + motion 就够。但加上 raw 可以让前端在"motion 解析失败但 raw 有内容"时仍能展示原始字符串——LLM 偶尔写不规范，raw 是 fallback。这种"暴露原始 + 派生"的 API 模式让前端能 graceful degrade。
- **空 vs 未写入区分**：以前的 `read_current_mood` 返 `Option<String>`，None 表示未写入。前端拿不到 None 直接当空处理一下就完了——但加这一区分能让"还没记录"和"写过但被 LLM 用空字符串覆盖"两种情况显示不同提示。我用 `raw == ""` + `motion is None` 的组合来推断，不另开 enum/option，简化前端模型。
- **MOTION_META 在前端写一份**：之前 mood→motion 映射在 LLM prompt 里（"这四组分别对应啥情绪"）+ 前端 keyword fallback 里。这次再加一份用于 Persona 显示。三处一致是约定不是代码——人来维护。如果 motion 集合扩到 6 组，三处都要改，工程上不优雅但实际成本极低（4 元素扩成 6）。
- **Section 顺序：自我画像 → 当下心情 → 心情谱**：persona_summary 是中长期身份（consolidate 时更新，~天级粒度），current_mood 是当下（更新可能是分钟级），mood_trend 是更长期（50 条 history，覆盖 ~周级）。三块按时间从中段 → 现在 → 长期排，不一定严格对应"过去现在未来"，但读起来「写了我的画像 → 此刻是这种状态 → 最近一直怎样」的叙事流畅。
- **motion 视觉用 emoji 而不是 icon font**：emoji 在所有平台开箱即用，不引入额外资源；32px 大小够看清；颜色 + 文字标签提供冗余 cue。如果后续做 Live2D expression 切换（路线 B），UI 这边可以单独再加一层。

## Iter Cξ 设计要点（已实现）
- **environmental 而不是 data-driven 或 composite**：first-of-day 看的是 today_speech_count（确实属 data-driven 范畴），但语义是 "环境状态：今天是不是新一天的开端"——和 wake-back / first-mood / pre-quiet 这种"现在是什么状态"同质。data-driven 是统计纠偏（chatty / icebreaker / env-awareness），composite 是组合触发（engagement-window / long-idle-no-restraint / long-absence-reunion）。把 first-of-day 放 environmental 比放 data-driven 更贴切——它是"日界 = 一种环境"，用 today_count 只是实现手段。
- **基于 today_speech_count == 0 而不是某个开关**：曾考虑加 `had_first_today: bool` 持久化字段，跨 session 标记"今天第一次"。但 today_speech_count 已经是 source of truth，再加一层 derived state 是冗余。每次 base_inputs 都要 set 是小代价，换得"日切换永远准确"。
- **base_inputs 默认 today=1 而不是 today=0**：要避免现有所有测试不假思索地触发 first-of-day 重新断言。1 是最小非零值，仍然 < chatty 阈值，对 chatty 测试无影响。注释里写清楚原因，未来加规则时同样套路。
- **不限制时段**：曾犹豫"first-of-day 在 深夜（22-04）也触发是不是怪"。但 (a) 深夜时段往往 pre_quiet 也活跃，rule body 已经给了"深夜→简短关心或不打扰"指引；(b) 限制时段会让"凌晨第一次开宠物"的 edge case 漏掉问候。让 rule body 自己处理时段差异，比让 fire 条件查时段更简洁。
- **firing order 排在 first-mood 之后 pre-quiet 之前**：宠物 internal state（mood bootstrap）优先于人类节奏（日界打招呼）优先于结束节奏（pre-quiet 收尾）。如果以后排不下，可以排 dependency 树，但目前线性顺序读起来就符合直觉。
- **整合 first-of-day rule body 中提到 wake-back / long-absence-reunion**：让 LLM 看到所有三个"回来"类规则同时存在时知道自己什么差异。互相 cross-reference 是"prompt 里把规则之间的关系也写明"的实践，比单独写每条让 LLM 自己拼好得多。
- **不和 icebreaker 显式互斥**：icebreaker 看 lifetime（< 3）、first-of-day 看 today（== 0）。新装+一天没说过话的场景两条都 fire——这样宠物开口同时是"破冰"和"早安"，很自然，没必要强制选一个。

## Iter Cν 设计要点（已实现）
- **rule 而不是延 idle_register**：本来犹豫——既然 Cμ 已经 ambient 加了 `user_absence_tier`，是不是没必要再加 rule？但 rule 的价值是 **structured guidance** + 进 active_prompt_rules 标签系统：(a) 进 panel "prompt: N hints" badge，让用户看得到这次开口被这个 register 塑造；(b) 触发 PROMPT_TILT 累计统计——长久看 engagement 倾向。ambient 字段是 hint，rule 是政策；两者都需要。
- **240 分钟（4 小时）**：阈值取偏保守值。曾考虑 120（午休回来），但很多人午休 90 分钟内，不算"长别"——不如保持午餐回来时不触发，留给"上下班"或"出门半天"那种真离开。如果实际使用觉得太迟再调。和 LONG_IDLE_MINUTES (60) 拉开倍数关系（4×），不会和它互相干扰。
- **wake_back vs long-absence**：本来想合并——"反正都是回来"。但触发源不同：wake_back 来自 wake_detector 的系统级 event（OS 休眠唤醒），long-absence 来自 InteractionClock 的 idle_seconds（用户没操作宠物）。两者都可能单独发生：lid-closed laptop 唤醒 + 用户 5 秒前才动过键鼠 → 只 wake_back（用户根本没"离开"）；laptop 一直亮 + 用户去开会 4 小时 → 只 long-absence（系统没休眠）。两个都需要。语气也不同：wake_back 是"你电脑刚开机，先克制"，reunion 是"你不在了好久，欢迎回来"。
- **不和 chatty / pre_quiet 共触发**：under_chatty 和 !pre_quiet 是常见的"engagement 类规则的两个守门员"——chatty 说今天聊够了，pre_quiet 说要进入安静时段；任一为 true 都不该再开新 register。和 long-idle-no-restraint 共享这两个守门员是有意为之，让"engagement 类规则集"行为一致。
- **三条 composite 共存**：engagement-window + long-idle-no-restraint + long-absence-reunion 同时 fire 时，label 顺序按 push 顺序确定，rule 文本也按这个顺序。新增的共存测试钉住排序——测试名 `_three_can_coexist` 配套 `_both_can_fire_together`，按数量顺位扩展。如果以后还加复合规则，再写 `_four_...`。
- **base_inputs 默认 idle_minutes=20 不变**：不让现有测试默触发新 rule。idle_minutes=20 远低于 240，所以现有测试如 `prompt_includes_required_sections` 不会突然发现多了一条新规则。新 rule 由专门的 boundary test 单独覆盖。
- **production callsite 把 snapshot 合并**：spawn loop 原来分两次 await `clock.snapshot()`（一次 since_last，一次别的）。Cν 第三个 callsite 加 idle_minutes 时把它合到一个 snapshot 变量——race 风险消除（两次 snapshot 之间 clock 状态可能变），代码也更直白。这是顺手 refactor，本来不在 iter scope 但成本低且消一种潜在 bug。

## Iter 74 设计要点（已实现）
- **挑这个迭代是因为路线 F 已收官、路线 G 还很轻**：companion register 大改动需要更深的设计迭代（mood/persona/语气都已经很多互动），而这个 cosmetic 增量是低风险高频可见——天天打开 panel 就能看到。
- **"本周" = 今天 + 过去 6 天 = 滚动 7 天**：而不是"自然周一到当前"。理由：(a) 滚动避免周一早上"本周"显示 0 的尴尬；(b) 用户感知"上周这时候"更接近"7 天前"而不是"上一个 ISO 周"。代价是周界限不再对齐 ISO 周，但这层显示是体感量化、不是统计学。
- **`sum_recent_days(map, today, n)` 而不是直接硬编码 7**：让"周/旬/月"未来都能复用。如果以后要加"本月"列，调用 sum_recent_days(30) 就行。这是把 magic number 7 提出来的小坚持，符合项目里的 pure-helper 习惯。
- **column 顺序：今日→本周→累计**：从近到远，符合阅读 flow（左到右越来越长时间维度）。本来想"本周→今日"——但今日是最常被读的数字，应该在最左视觉权重最强位置。
- **fontSize 16px 给本周**：今日 20px、累计 28px 是已经定了的。本周 16px = "辅助但仍清晰可读"。companionship 也是 16px——所以视觉上本周和陪伴在同一辅助层级。
- **不引入 chatty_week_threshold**：曾考虑加一个 weekly cap rule 类似 chatty_day。但周维度更模糊（中间 1-2 天爆发但其他天为 0 算 chatty 吗？）。daily threshold 已足够，weekly 是 informational only。
- **不动 ToneSnapshot**：本周值不参与 prompt 决策，仅 panel 显示。stays out of `get_tone_snapshot` 这个 prompt-feeder。

## Iter Cμ 设计要点（已实现）
- **新函数 vs 复用 `idle_tier`**：`idle_tier` 已有，但其 framing 是 pet 自身视角（"刚说过话还热着" / "几小时没说话"）。如果给 user-absence 用同一个函数，文字会错位——pet 不是"自己几小时没说话"了，是"用户几小时没出现"了。两个 axis 各自的措辞独立，pure-function 多一个比挤进同一个更清楚。Iter Cβ 加 weekday 也是同思路：每个语义维度独立 helper。
- **register 注入位置**：放在已经存在的"约 N 分钟"后面括号里，最小破坏现有 prompt 结构。曾考虑做成独立一行 hint section，像 wake_hint / speech_hint 那样，但那会把"距用户互动 X 分钟"的语义切成两片：数字一行、register 另一行。括号同行更紧凑、阅读 flow 不断。
- **6 档 vs 5 档**：用户视角的时间分辨率比宠物自身高——"一整天没出现"和"至少一天没互动"在 register 上有差别（前者今天还在、只是没找你；后者已经过夜）。pet 视角"上次聊已经是昨天或更早"压在一起没关系，因为反正已经"很久没聊"。这不是技术 trade-off，是语言密度选择。
- **不和 `cadence_hint` 合并**：cadence_hint 表示 pet "上次自己开口" 的时间感，是 since-last-proactive 数字 + idle_tier 文字。user_absence 与之并列但 source-of-truth 不同（since-last-interaction）。两个一起存在让 LLM 看到完整时间矩阵：宠物自己上次说话什么时候、用户上次互动什么时候。这种信息密度提升的边际成本（几个 token）远低于 register 选错的概率收益。
- **不写 contextual rule**：曾想加一条 "long-user-absence" rule（"用户已经走超过 N 小时，开口偏向问候而非续话题"）。但 register 本身已经是行为引导——LLM 看到 "用户至少一天没和你互动" 会自然按那个 register 开口，再加一条规则是过度规定。规则系统适合"硬约束"（如 chatty_day 上限），不适合"语气微调"。
- **base_inputs 设 `idle_register="用户离开了一小会儿"` 而不是 `""`**：因为 idle_register 是 ambient 总在的字段，没有"空"语义。把 fixture 设成与 idle_minutes=20 自洽的值，避免出现 "20 分钟（）" 那种错乱 prompt。

## Iter Cλ 设计要点（已实现）
- **复用 sweep 模式比创新好**：reminder / plan 都是 deterministic 时间窗口 + memory_edit delete + 写日志。butler once 完全是同型问题——同样的 cutoff 字段、同样的 sweep 函数形态、同样的 settings 字段。沿用模式 → 三周后看代码也能立刻识别"这是 sweep family"。如果发明了新接口（比如 lifecycle policy），多一种结构对维护无益。
- **手动 butler_history.record_event**：Cε 设的钩子在 tools/memory_tools::memory_edit_impl 那一层。consolidate 直接调 commands::memory::memory_edit（绕过 tools 层），就不会触发 hook。这其实是个一直存在但未被注意的不对称——本 iter 把它显式 patch 上：sweep 函数自己 record。如果以后给 memory_edit 增加更多副作用（比如 dispatch 事件），同样的 pattern 还是适用：低层 API 调用方负责镜像副作用，或者升迁到一个共享的 wrapper。我倾向于后者，但暂且只做眼前最小修补。
- **grace = 48h**：两个考量。一是给用户一天时间在 panel 上看到完成的任务（"昨天我帮你做了 X"），二是给 daily_summary 一个 cycle 把它写进 recap（consolidate 默认 6h 间隔，48h 内会跑 8 次，几乎确保至少一次抓到这个任务）。比 reminder 的 24h 长，因为完成的 butler 任务的"记忆价值"比单纯过期 reminder 大。
- **不做 UI 动作**：用户在 SettingsPanel 改的 grace 数字会立即生效（后续 consolidate 会用），但当下 panel 不会显示"X 个任务即将被清理"——那种 affordance 多半是杞人忧天，因为 grace 默认 48h，被删时用户已经记不清了。如果后续观察到用户抱怨"任务不见了"，再加可视化。
- **保守的 unparseable updated_at**：和 Cζ 的 is_butler_due 相反——这里 unparseable → 不删（保留），那里 unparseable → 视为未执行（标记 due）。两边的方向都符合"不确定时偏向显示给用户"原则。delete 是 destructive，更应该保守。

## Iter Cκ 设计要点（已实现）
- **客户端时钟而不是 Rust 命令**：和 Cθ 一致——overdue 计算也是 view-time。每秒重渲染未必需要，但 panel 既然 15s 已经在 poll butler_history，那个 setInterval 触发的 setState 会让 React 重渲染，overdue 计算会随之刷新。
- **复用 trigger_proactive_turn**：曾考虑写一个 butler-scoped 的 manual trigger（只把 due 任务带进 prompt、跳过 mood/persona/etc）。优点是 LLM 不会被无关 context 分心。但坏处明显：(a) 又一条独立路径，要维护；(b) 失去"管家任务和聊天伙伴是同一个宠物"的一致性，user 会感觉 ⏰ 触发的回复语气和正常 proactive 不一样；(c) 现有 prompt 已经把 butler 任务标 ⏰，LLM 看到自然会选。共享 pipeline 的简洁性赢。
- **mostRecentFire 提到独立 helper**：原 isButlerDue 内部就有这段逻辑，但 overdueMinutes 也要算同一个时间点，提出来后两个函数都用一行调用——既消除重复，也让"什么是最近一次 fire"在一个地方有定义。这种 small refactor 的价值容易被忽略，但对长期维护是关键。
- **OVERDUE_THRESHOLD_MIN = 60 而不是更激进**：曾想用 30 分钟——更早提示。但 panel 上 ⏰ 到期 chip 已经先红，再加一个琥珀 chip 太挤；而且 30 分钟挂着可能是用户出门吃午饭，不是 bug。60 分钟是"明显异常但不会误报"的中间值。
- **"立即处理"在 section 级而不是 per-task**：每个 due task 都加一个 button → 视觉重复、且按钮多容易误点。一个全局按钮在 section 头部、count 显式，点了 LLM 自己挑——契合「LLM 是 agent」的范式而非「button = trigger task X」的命令式 UI。
- **复用 message state**：触发后用 `setMessage` 写状态——和 handleConsolidate / handleSaveEdit / handleDelete 一致。不再起一个 `proactiveStatus`-like 独立 state，state 越多越乱。

## Iter Cι 设计要点（已实现）
- **改 TOOL_USAGE_PROMPT 而不是另起一段 system message**：Cγ 当时把 butler tooling 提示放在 proactive_rules 的 conditional rule 里——只在 butler_tasks_hint 非空时 fire。reactive 路径完全没看到 butler。本可以再起一段独立的 "butler delegation" system message 在 reactive 的 inject 链上加一道，但那意味着两个 prompt 来源（一个在 proactive、一个在 reactive）讲同一件事，将来语义漂移。改 TOOL_USAGE_PROMPT 是单一来源——它已经被 chat pipeline 的所有路径（reactive、proactive、telegram、consolidate）一致注入。一处改，处处生效。
- **加一段而不是大改**：曾想把 TOOL_USAGE_PROMPT 整体重构成更结构化的格式（按工具分组、加更多对比例子）。但那会触发"重写一个一直管用的 prompt"的尴尬——既改动大、又难量化是否真的更好。增量加一节 "## 任务委托判断" 是最小风险路径，且新内容自然在末尾，不打扰前面的"工具选择" / "文件操作" 等已经稳定的指令。
- **三个具体例子优于抽象规则**：之前 Cγ 在 memory_edit 工具描述里讲过 butler_tasks 的用法。但工具描述是工具调用前 LLM 看的，对话理解期不一定 active。在系统提示层面再 reinforce 一次、用对话例子，是"在不同语境下重复关键约定"的常见 prompt engineering 套路。
- **对比例子选「提醒我喝水」vs「整理文件夹」**：这两类是用户最容易让 LLM 混淆的——表面都是"你帮我..."，但前者的"做"是在某个时刻提醒，本质是给用户的 nudge（todo），后者的"做"是 LLM 自己执行（butler_tasks）。明确给反例比单方向的"butler_tasks 该用于什么"更清晰。
- **测试钉住关键字串**：内容测试 (a) "butler_tasks"、(b) "[every:" + "[once:"、(c) "todo" + "提醒我" 三组——任何一组缺失都说明那一节被改坏了。这种"prompt 字符串契约"测试在 TS 系统不常见，但 Rust 里很自然——constants are strings, tests can read them.
- **不动 inject_persona_layer**：persona_layer 是"长期人格画像"——属于 identity context，不该混进操作指南。butler 委托是 how-to，归 TOOL_USAGE_PROMPT。两个 system message 各有各的 namespace，加错地方会让 LLM 把"我是会执行任务的小管家"当人格而非操作能力，可能漂移成炫耀型语气。

## Iter Cθ 设计要点（已实现）
- **TS 重写而不是 Tauri command**：曾考虑加一个 `compute_due_butler_tasks` Rust 命令一次性返结构化数据。但 (a) 每个 task 只是几行算术，IPC overhead 占比反而高；(b) panel 已经在轮询 butler_history，重渲染时序天然，不需要刻意触发；(c) Rust 端要给 panel 暴露 schedule 详情就得新建一个 wire format（字段 ButlerScheduleDto / ButlerTaskWithDueDto），又是几十行 boilerplate。TS 重写两个 pure 函数 = 50 行，结束。Rust 端是 source of truth（决策路径），TS 端是 view-time mirror（显示路径），两端独立但语义对齐。
- **风险：Rust/TS 漂移**：如果未来 Rust 改了 due 语义（比如加宽限期），TS 端不会自动跟。缓解：(a) 单元测 in Rust 锁定语义、(b) TS 函数注释明确指向 Rust 函数名。要 hardcore 防漂移就得做 wasm 模块共享，超出迭代范围。
- **strip prefix 在 display**：把 `[every: 09:00] 写日报` 显示成 `写日报` + 蓝色 chip——视觉密度更低、信息更突出。但编辑模态拿原始 description（没 strip），所以编辑/保存往返不丢前缀。这种"显示态简洁化但数据态保真"的取舍在很多 form UI 里见过，是个好默认。
- **chip 颜色按 every / once 区分**：every = 蓝（recurring，常态色），once = 琥珀（一次性，提醒色）。和"⏰ 到期"红 chip 形成三色梯度：常态 / 临时 / 紧急。如果以后加更多 schedule 类型（per-week / per-month），按饱和度延伸。
- **不在 chip 上加进度信息**：本来想在 chip 里加"上次执行: HH:MM" 之类。但 chip 已经从 title 旁占走横向空间；信息密度再加就太挤。"上次执行"可以去看 "最近执行" 时间线（Cε 加的），那边有完整 5 行 history。
- **不引入 dayjs / date-fns**：浏览器内置 Date 够用——`new Date(iso)`、`getFullYear/Month/Date`、加减毫秒。一个外部库要加 ~30KB 给一个 50 行的函数，不值。chrono 在 Rust 端是 source of truth；TS 这边数学最简单的算就行。
- **Tooltip 里写人话不写格式**：`title="计划时间已到、自上次到期后还没被宠物 update——下一次 proactive 会优先处理"` 比 `title="due since most_recent_fire and updated_at < fire"` 友好。这是给最终用户看的，不是给 LLM 看的。

## Iter Cη 设计要点（已实现）
- **不塞 speech_history**：原 TODO 是"塞进 speech_history 让用户回看"。但 speech_history 同时驱动 chatty_day_threshold / today_speech_count / lifetime 三个计数，加 daily_summary 进去会让"今天宠物开口了几次"虚增——一个 consolidate 自动写的句子被算成"主动开口"，会让 chatty rule 误触发，prompt 输出"今天聊了不少了"——但其实 N 句里大半是 summary。隔离到独立 `butler_daily.log` 是一行代码改动，零认知耦合。
- **每日只一行 vs 每次 consolidate 一行**：consolidate 默认 24h 触发，但用户可以"立即整理"——这意味着同一天可能跑两次。每次都 append 会让今日有多个"今天我帮你..."摘要，多余且自相矛盾。用 `<date>` 作为 key upsert，最新一次 wins，自然解决。代价是不知道"上午跑过一次的中间态"——但那是开发期 debug 才需要的细节，正式用户不会关心。
- **deterministic 在 LLM 之前**：摘要不需要 LLM——它是机械聚合。放在 LLM 之前还有一个好处：即使 LLM 阶段崩了（OpenAI 限流、tool 失败），今日的摘要也已经持久化。consolidate 的"反思 + 整理"是 LLM 工作的核心，但 daily_summary 是数据的展示层，自然属于 deterministic 部分。
- **过滤用 `starts_with(date_prefix)`**：曾考虑用 `contains` 简化代码，但 description 里可能写日期字符串（"提醒我 2026-05-03 看医生"被记进 butler_history 的 desc-snippet）→ 误匹配。`starts_with` 正确利用了"butler_history 行强制以 ISO 时间戳开头"的格式 invariant。这种"利用格式 invariant 减少边界情况"的小决定，比"防御性多过滤一遍"成本低也更准。
- **dedup 按出现顺序**：用 HashSet 做去重但保 Vec 的顺序——同一任务一天 3 次 update 折叠成一次。这在 prompt 角度是冗余收敛，在 UI 角度避免"今天我帮你 推进了「早报」「早报」「早报」"的尴尬。代价是 hash 操作小成本，但每天 ~10 任务级别完全可忽略。
- **Updates 在前 Deletes 在后**：句法上"推进了 X，撤销了 Y"读起来比"撤销了 Y，推进了 X"自然——人类倾向先讲做了什么再讲拒了什么。这是个微小但能感受到的措辞选择。
- **panel 颜色与 timeline 区分**：每日小结用浅黄 `#fefce8` + 琥珀色 header（高信息密度的 daily summary），最近执行用浅蓝 `#f0f9ff` + 蓝色 header（事件流水）。同一区域两块视觉就明确区分"概览 vs 流水"。
- **不做"昨天的对比"**：曾想加"昨天我帮你做了 X，今天 Y"对比格式。但这增加 prompt token 与 UI 复杂度，且大多数日子是相似的事——对比意义低。先做最小可用，"对比/趋势"留给后续 weekly retro 能力。

## Iter Cζ 设计要点（已实现）
- **不开新调度线程**：原来想加一个独立的 tokio task 按分钟级 tick 检查每个 butler_tasks 的 schedule，到期就 emit。但那意味着：(a) 又一条独立循环，要小心和 proactive、consolidate 协调（比如 quiet hours 是不是也应用到 schedule 触发？）；(b) 通讯路径：调度线程触发 → 怎么走 LLM？如果直接 invoke proactive turn，等于强插一次跳过 gate 的开口。整套链路复杂度超出单 iter。
  - 选 pure-function 路线：proactive 已经按 N 秒 tick 跑了，每 tick 构造 prompt 时顺手用 `is_butler_due` 检查每个任务，到期的标 ⏰、推到顶。唯一代价是检测精度 = proactive interval；对"每天 9 点"足够，对"上午 9:00 整"也足够。"我每分钟都要做点什么"那种 cron 用例不打算支持。
- **`Every` 不需要内部状态**：任何"昨天/今天最近一次 fire 是什么时候"都从 (now, HH:MM) 推出，对照 `updated_at` 就能知道是不是已经做过——零状态。这是为什么 LLM 执行后必须 update 任务条目（footer 已经写明）：update 是给"是否已做"的唯一信号。如果 LLM 忘记 update，下一轮 proactive 还会显示到期——是个自我修复的循环。
- **`Once` 没设过期**：曾考虑加个"过期太久就不再显示"——比如 once 任务过了 24 小时还没执行，是不是该自动 retire？但那会让"我半夜睡了一觉醒来发现昨晚有个 once 任务"的场景失败：用户期望宠物醒来后还提醒。所以 once 任务一旦到点就一直 due 直到被 update/delete。consolidate 后续可以扫"过 N 天还没动的 once 任务"建议清理，但不在本 iter。
- **fail-open on parse failure**：`parse_updated_at_local("garbage")` 返 None → 视为"从未执行" → due。这是有意为之：宁愿多提醒一次，也不要因为时间戳格式漂移导致任务永远不显示到期。代价是如果 memory 索引坏了（updated_at 全部坏掉），所有 every 任务都会显示到期；但那是个独立的"索引坏了"问题，应该在那里修，不是在调度逻辑里 hack。
- **footer 把 `⏰ 到期` 的语义写出来**：之前的 footer 只讲"完成后 update / 不要了 delete"。现在加上"看到 ⏰ 该这一轮优先处理"——LLM 看到只是表情符号不一定知道含义；写出来从"表情"变"指令"。这种"prompt 里把每个特殊符号都解释一遍"原则在 [motion: X] / `<silent>` 等也用过。
- **不动 prompt rule 系统**：本可以加一条 contextual rule "有任务到期 → 强烈建议这一轮就做"，但：(a) hint block 已经包含到期信息 + footer 指令，rule 是冗余；(b) 加 rule 要改 active_environmental_rule_labels 签名 + 三处 alignment test + frontend dict——一个 iter 的 budget 撑不过去。先看 LLM 看到 ⏰ 后的实际响应率，如果观察到大量"到期但没执行"再加 rule 升压。
- **`ButlerSchedule` 命名**：第一版手抖写成 `BulterSchedule`，后来用 `replace_all` 一次性改正——18 处替换，一致性更好；如果留着 typo 后面每次看到都会本能地纠正一下，徒增心智成本。

## Iter Cε 设计要点（已实现）
- **新文件而不是塞进 speech_history**：曾考虑把 butler 事件也写到 `speech_history.log`，加一个 type 字段区分。这能省一个文件、让"今天的事件流"在一处看。但耦合后："今天宠物开口几次" / "今天 butler 完成几项" 两个统计要扫同一文件再过滤——读放大；而且 butler 事件没有 mood、不该计入 chatty_day_threshold。两个独立的 log 各自只关心一件事，反而干净。命名也清晰。
- **只记 update/delete，不记 create**：create = 委托发生（用户或 LLM 写下了任务），update/delete = 执行 / 撤回。前者对"宠物为我做了什么"无信号，后者才是。如果哪天加了"用户 vs LLM author"区分，create 会更有意义；目前 author 不区分，记 create 只会噪声。
- **轮询而不是事件 emit**：Tauri 的 emit 需要 AppHandle，但 ToolContext / memory_tools 这一层没有 AppHandle 引用——传过来要改 trait 签名 + 多处构造。15 秒轮询便宜（一个 string[] 拉过来），butler 事件是分钟级，15 秒粒度感官上等同实时。等以后做"宠物视觉反馈"（执行完跳一下）再加 emit。
- **handleSaveEdit / handleDelete 立刻刷新**：用户自己点了"保存"或"删除"，期望立刻看到时间线更新——别让他等下一个 15 秒 tick。这是 UX 的细节，但写出来感觉差很多。
- **不 redact butler_history**：Cw/Cy/Cz 一系列把所有 outbound prompt 输入都 redact 了。但 butler_history 是 inbound（给用户看的执行记录），需要看到原文。redaction 是"出门前别带这个出去"，不是"我自己也不能看自己"。
- **格式 `<ts> <action> <title> :: <desc>`**：分隔符 ` :: ` 而不是 ` | ` 是因为 description 里 `|` 可能出现（用户随手写"读 a | b 的差异"）。`::` 在中文场景几乎不出现，且视觉上像"键值之间的连接符"，意图清晰。
- **panel 上 update teal / delete red 的色彩**：和 chip strip 的"hits>0 teal"、"engagement-window green"等保持同色系。teal = 正向行动、red = 撤销/删除（语义中立但视觉上需要区分）。
- **不做 panel 顶层 tab**：曾想在 panel 上加一个独立的 "执行流" tab，跟 Debug / Memory / Persona / Chat 同级。但事件类型只有 butler 一种，做整个 tab 杀鸡用牛刀；嵌进 Memory/butler_tasks 区域里是事件 + 上下文（待办列表）的天然组合。如果后续接入更多 source（speech、focus 切换、task schedule 触发），再考虑独立 tab。

## Iter Cδ 设计要点（已实现）
- **placeholder 而不是 helper text**：曾考虑在 textarea 上方加一段灰字说明"butler_task 格式建议..."。但 helper text 永远占布局空间；placeholder 只在空状态出现，输入后消失，对已经会用的用户零干扰。代价是 placeholder 不能太长（每行截断），所以我把示例切成几个短行用 `\n` 隔开——这在 textarea 里 native 支持换行 placeholder。
- **快捷入口仅给 butler_tasks**：曾考虑给所有类别都加 "+ 新建 X" 顶级按钮（todo / butler_tasks 都是用户写的），但这会让顶部按钮区从 3 个按钮（搜索/清除/整理）变 5 个，视觉密度太高。butler_tasks 是"宠物管家"方向的主轴，单独给它快捷入口符合"actionable 类别上调"的方向；加 reminder 仍走分区下的 "+ 新建"——reminder 流程已经熟，且 todo 现在大多是 LLM 写的。
- **不做 panel 段落级 subtitle**：原本想在 "管家任务" sectionTitle 下加一行小字"（你委托给我做的事）"。但分类的 label 已经是中文了，再加副标题是冗余；加上每条任务的 description 本身就是说明，区域级解释属于 over-explanation。模态的 placeholder 已经承担了"教用户怎么写"的职责。
- **minHeight 按分类切换**：butler_tasks 用 100px，其他用 60px——任务描述要包含"做什么 + 多久 + 写到哪"三要素，60px 显得逼仄；ai_insights/user_profile 多是单句 fact，60px 够。这是个非常小的细节但能让"用户开始写任务"的体验顺滑很多。
- **不写前端单测**：项目当前没有 React 组件测试体系（vitest / RTL 都没装），强行起一套会破坏 Iter 大小约束。改动是纯视觉 + 交互、TS 类型保 contract，可接受。

## Iter Cγ 设计要点（已实现）
- **方向变了，前移管家方向**：用户明确把目标从"实时陪伴 = 主动观察 + 攀谈"扩展为"实时陪伴 + 实用管家"。这意味着以后选迭代时优先级要重排：能让宠物**真正帮用户做事**的能力 > 单纯让宠物**说更贴的话**的微调。Iter Cγ 是这个方向的第一刀。
- **新建类别而不是复用 todo**：开头犹豫过——是不是把"用户委托给宠物的事"都塞进 `todo` 用前缀区分（比如 `[butler] xxx`）？ 反例：`todo` 里的 reminder 已经是用前缀 `[remind: HH:MM]` 标记的，再加一种前缀就会让 prefix 解析复杂；而且 `todo` 在面板上是"用户的待办"语义，前端 / consolidate / reminder sweep 都基于这层语义。**类别才是 namespace**，前缀是 namespace 内的 sub-format。把 butler_tasks 单独成类后，所有"我做的事"和"我提醒用户的事"自然分离，将来给 butler 做触发器 / 报告 / panel UI 时不会撞上现有 reminder 流程。
- **按 updated_at 升序而不是降序**：这是和 user_profile_hint（Iter Cα，降序）相反的选择，理由：user_profile 是"我对用户的认知"——最近更新的版本最准确；butler_tasks 是"我的待办 backlog"——最早委托的最不能让我忘了。两种 ordering 服务两种语义。
- **block 内自带 footer 而不是另起一条 rule**：footer 写"完成后用 memory_edit update / 不需要的 delete"——本来可以做成另一条 contextual rule，但 footer 离任务列表近，LLM 看的时候上下文耦合度更高，比拆到 rules 段里更不容易漏。
- **rule 是 conditional 的**：只有 hint 非空才把"你也是小管家"这条规则推进 rules——避免在没有任务时浪费 token，也避免给"prompt: N 条 hint" 面板 chip 加噪声（rule 用条件化方式不进 active_prompt_rules 系统）。
- **不进 active_prompt_rules 标签系统的取舍**：rules 系统现在分四种 nature（restraint / engagement / corrective / instructional），butler-task 哪种都不像——它不是"对开口语气的塑造"而是"对工具调用范围的扩展"。加进去会让"倾向 X%" chip 失真。后面如果 butler 任务多到值得专门统计 LLM 接管率，再单建一个统计维度（类似 env_spoke_with_any 那样的 atomic 计数器）。
- **不立即做触发器**：单 iter 不做"按时间自动执行 butler_task"——那需要从 cron / chrono 任务调度切入，工程量过大。当前迭代只让 LLM 在每次 proactive turn 看到任务列表，由 LLM 自己判断要不要这一轮执行某项。先看 LLM 自治能不能动起来，再决定要不要加机械触发器。
- **写 panel 顺序成 actionable-first**：`PanelMemory.tsx` CATEGORY_ORDER 改成 `[butler_tasks, todo, ai_insights, user_profile, general]`。butler_tasks 是新加的"用户最常 add"类别——置顶让用户加任务就能看到。todo 紧跟（也是用户写作）。下面三类是宠物自己写的，下沉。这种"按谁是 author / actionability 排序"也许该写成更显式的 metadata，但现在两类太少先用顺序表达。
- **consolidate 加一行而不是重写整段**：consolidate prompt 已经够长，只在第 2 条"过期/失效"的现有列表里加一句 butler_tasks 也归这一类，零结构改动。等 LLM 实际开始用 butler_tasks 后看 consolidate 的整理质量是否需要更精细的指引。

## Iter Cβ 设计要点（已实现）
- **拼字符串还是加结构化字段**：可以选 (a) PromptInputs 加两个字段 `weekday: &str` + `weekday_kind: &str`，prompt builder 自己拼；(b) 加一个合并字段 `day_of_week: &str`，调用方拼好。选 (b)：builder 是格式化模板，多接收一个独立字段会让 time 行变成 `format!("现在是 {}（{}，{} · {}）...", time, period, weekday, kind)` 同时引入 `·` 这个表示分隔的字符到模板里——一旦未来想给周末特殊渲染（比如 emoji），就要改模板。把拼接逻辑放在 `format_day_of_week_hint` helper 里，模板只负责"插一个 string"，分离得更干净。
- **三个函数还是一个**：`weekday_zh` / `weekday_kind_zh` / `format_day_of_week_hint` 拆开。如果合一个 `format(...)`，未来 ToneStrip 想单独显示"周日"就得复制一份逻辑。三个 pure 函数 = 三段独立可测可复用的小积木。这种"小函数比合并大函数更值"的取舍在项目里反复出现（period_of_day 也是同样的形态）。
- **不重新加 active_prompt_rules 标签**：weekday/weekend 是 ambient context（始终告知），不是条件触发的"规则"。规则系统（panel "倾向 X%" chip）是用于"这个开口是被某些克制/引导规则塑造的"——day_of_week 不影响倾向分布，所以不进规则枚举。和 Iter Cα 的 user_profile_hint 一样的判断。
- **不做"周五晚上"特殊处理**：曾考虑加一个 "weekend_eve"（周五傍晚 + 周六凌晨）特殊语气标签——但这种细分把简单逻辑复杂化，而 LLM 看到 `周五 · 工作日` + `傍晚` 完全够用。加更多枚举会让 prompt builder 越变越像查找表。先跑简单版本，看 panel 上 LLM 沉默率 / spoke 比例是否在周五傍晚 vs 周一上午之间真的有可观察的差再说。
- **2026-05-03 = 周日**：base_inputs 把默认改成 "周日 · 周末" 而不是占位字符串——保持测试 fixture 内部一致，避免出现 "time = 2026-05-03 但 day_of_week = 周二" 这种自相矛盾的测试 setup。
- **Datelike 和 Timelike 两个 trait 都 use**：chrono 把 hour() 和 weekday() 分别放在 Timelike / Datelike 里，必须 `use chrono::{Datelike, Timelike}` 才能调到。一开始想偷懒只加 Datelike 让 hour() fallback，但 cargo check 就提醒了。

## Iter Cα 设计要点（已实现）
- **加 ambient block 而不是加 rule**：曾考虑做"如果 user_profile 非空 → 加一条规则提示 LLM 在开口前 search"——但那只是把"调工具的责任"换种方式重申，根本问题（每轮都要 tool round-trip）还在。直接把摘要塞进 prompt 是同一个 token 预算下的更好交换：6 条 × 80 字 ≈ 500 tokens，远低于一次 memory_search 调用 + 结果回灌的开销，并且节省一次 round-trip 的延迟。
- **跟 persona_hint 同级而不是嵌进 rules**：`persona_hint`（自我画像）/ `mood_trend_hint`（情绪走向）/ 现在的 `user_profile_hint`（对用户的认知）三者都是"长期记忆 → 当前 prompt"的注射点，结构上对称。放一起方便将来一起调（比如做"长期画像被静音"开关）。
- **按 updated_at 降序而非按 created_at**：用户习惯会变（用户从 dark theme 换到 light theme），最近更新的版本更可能反映现状。如果按 created 排，老旧描述会挤掉新的。代价是 `MemoryItem.updated_at` 写不规范（缺值时为空串）会被排到末尾，但目前 memory_edit 强制写 updated_at，没这个风险。
- **6 条上限够吗**：当前 user_profile 通常只有 0-5 条（启动初期），6 条上限给了 buffer。如果未来 LLM consolidate 后膨胀到 20+，再考虑把上限和 desc cap 暴露到 settings。先按 6 / 80 跑一段，看 panel chip"环境感知 spoke_with_any" 比例是不是上来了——上来了说明 LLM 现在不需要靠 memory_search 也能开聊。
- **pure helper 的小坚持**：`build_user_profile_hint` 走 disk，没法干净测。拆 `format_user_profile_block(items: &[(String, String, String)])` 出来后，所有排序/截断/cap 逻辑都成了纯函数测试。下次如果 LLM 写了某种格式问题（比如 description 里有换行），测试可以直接喂进 tuples，不必 mock 文件系统。这种拆分套路在 Iter 6/19/93 等也用过，是项目里反复证明值的一招。
- **redact 还是不 redact**：内部记忆经过用户视角是"我自己写的"，但下一次 LLM 还是 outbound——同样的 LLM provider 看到。所以也走 `redact_with_settings`，跟 `build_persona_hint` 在 Iter Cw 的处理一致。`memory_list` 直接面板视图（`PanelMemory.tsx`）不走这个路径，看到的还是原文，不影响用户调试体验。
- **不做 vs 做新的 prompt rule**：`active_*_rule_labels` 系统是为"上下文条件 → 提示文本"设计的，user_profile 是"有没有数据 → 要不要加这一段"——更像 persona_hint 的开关式插入，不需要进规则面板（panel 上的"prompt: N 条 hint"badge）。所以不动 panelTypes 也不动 PROMPT_RULE_DESCRIPTIONS。

## 目标
让 AI 宠物像真实伙伴一样陪伴用户：除了被动回复，还能后台运行，主动观察用户在干嘛，在合适的时机主动开口。

## 当前实现回顾（截至 e7657a6）
- 被动聊天：用户输入 → 流式 LLM 回复 → Live2D 角色 + 气泡显示。
- 工具调用：read_file / write_file / edit_file / bash / memory / MCP。
- 记忆系统：YAML 索引 + 分类 md 文件（ai_insights / user_profile / todo / general）。
- Session 持久化、Telegram bot、面板窗口。
- 自动隐藏到屏幕边缘。

## 与目标的差距
1. **完全被动**：宠物只在用户输入时才说话，没有任何后台自主行为。
2. **环境无感**：不知道用户在用什么 app、键鼠是否活跃、几点了。
3. **无情绪/状态演化**：每次回复都是无状态的，没有"心情"、"近期兴趣"。
4. **无节奏控制**：缺少"什么时候该说话、说几句、什么时候闭嘴"的逻辑。
5. **记忆只在工具调用时被动写入**：没有定期反思、整理记忆的机制。

## 总体策略
分多次迭代，从最简单的"主动 ping"开始，逐步加入真实的环境感知和情绪状态。每次迭代都要可见、可测试。

## 迭代规划（粗）
- **Iter 1（本次）**：后台 tick 引擎 + 最简主动问候。每 N 分钟检查一次，若用户空闲超过阈值则让 LLM 决定要不要开口。打通端到端链路。
- **Iter 2**：macOS 当前前台 app/窗口标题检测，作为新的 LLM 工具，让宠物"看到"用户在干嘛。
- **Iter 3**：用户输入空闲时间检测（几分钟没有键盘鼠标活动），作为更强的"该不该说话"信号。
- **Iter 4**：宠物的"当下心情/状态"持久化到 memory，每次主动开口前读，开口后更新。
- **Iter 5**：节奏控制——基于最近发言历史避免连环主动发言（cooldown、用户回应才继续）。
- **Iter 6**：定期记忆整理任务——每天/每若干轮自动 consolidate（合并、去重、过期）。
- **Iter 7**：日历/天气/系统通知集成（通过 MCP 或新工具），让主动话题更丰富。
- **Iter 8**：让宠物的 Live2D 表情/动作根据情绪变化（替代单一动作）。

## Iter Cv 设计要点（已实现）— redaction 命中可视化
- **静态 atomic 而非 ProcessCounters**：`redact_with_settings` 调用方多数没有 ToolContext 或 AppHandle（`inject_mood_note` 是 sync 函数，`build_persona_hint` 同理）。要么改所有调用方 plumbing（上游 5 处全改 + 测试 churn），要么用全局 static atomic。后者写一次零 plumbing。Atomic 是 Send + Sync 默认，多线程访问安全。
- **calls vs hits 双计数而非单计数**：单 hits 计数不够——用户看 "命中 5 次" 不知道 calls 是 5（100% 命中→ patterns 过松）还是 1000（0.5% 命中→正常环境干扰）。两个数字才能让用户判断过滤行为是否合理。
- **hits = "input != output"** 而非 "n_replacements"：定义为粗粒度二元（这次有没有任何 pattern 命中），不算具体替换次数。简单 + 用户语义清晰（"这次 redact 起作用了"）。如果将来想细粒度（每个 pattern 命中多少次），再加分组 atomic。
- **chip 颜色三态**：hits > 0 青色（filter 在干活）/ hits = 0 灰色（calls > 0 但没东西匹配，可能 patterns 太严或环境干净）/ calls = 0 不渲染。三态让用户一眼知道"过滤器在不在跑 vs 跑没跑出东西"。
- **跨重启不持久**：与其他 ProcessCounters 一致——重启清零让用户能针对一段使用窗口测量 redaction 频率。如果想看"过去 30 天 redact 多少"需要文件持久化（类似 speech_daily），不在本 iter 范围。
- **不写计数行为测试**：static atomic 在同进程内被多 test 共享，cargo test 默认并发会让"调用 N 次 → 期望计数 N"的断言不可靠。RedactionStats 的 serde 测试 + 既有 redact_text/redact_regex 14 个测试保障核心。如果将来 atomic 行为出现 bug，加 thread-local 或测试 fixture 隔离。
- **路线 C 闭环 v3**：v1 子串 + v2 正则 + v3 命中可视化 = 用户能"配置→验证→调整"完整循环。下一阶段路线 C 候选：每 pattern 单独命中计数（panel 显示哪条 pattern 真正生效），但需要先看用户反馈再决定是否值得。

## Iter Cz 设计要点（已实现）— 路线 C 加正则维度
- **regex crate 而非自写正则**：Rust `regex` crate 是 RE2-style——线性时间复杂度、不支持 backreference、不支持 lookaround。这些限制是 ReDoS 防御的核心：传统 PCRE 的灾难性回溯需要 backreference / 嵌套捕获组才能成立。用 `regex` crate 不需要单独做 ReDoS 防御，写多复杂的正则都不会让宠物卡死。
- **每次重新编译而非 cache**：理论上可以缓存 `Regex` 对象。但 redaction 频率低（每次工具调用 / 每次 prompt 构建），编译成本是微秒级。缓存意味着维护 invalidation——用户改 settings 后旧 regex 不能继续用。简单重编译保正确性。
- **两阶段顺序：子串先 / 正则后**：思路是"具体优先泛化"。如果用户配子串 "Bob" + 正则 `[A-Z][a-z]+`：子串先把 "Bob" 替换为 "(私人)"——marker 不会再被正则匹配（"私人" 不符 `[A-Z][a-z]+`）。如果反过来，"Bob" 先被正则匹配为 "(私人)"，然后子串再扫——已经没目标。两个顺序结果相同的场景下选先具体（用户期望），让顺序在边界场景也合理。
- **invalid pattern silently skip**：用户 textarea 多行编辑容易写错一条。如果一条错误 pattern 让整个过滤抛错，所有真私人内容都漏到 LLM——隐私故障应当 fail-safe 而非 fail-loud。silently skip 错误 pattern + 仍然应用其他 pattern 是正确的"部分失败优于全部失败"语义。后续可加面板侧 lint UI 提示哪条 invalid。
- **统一 settings update 模式**：textarea onchange 必须同时设两个数组（`redaction_patterns` 不变 + `regex_patterns` 新值）。React 浅 merge 的坑——直接 `privacy: { regex_patterns: ... }` 会丢掉子串字段。两个 textarea 各自显式保留另一个的旧值，模式重复但安全。
- **placeholder 给经典样例**：`\b\d{4}-\d{4}-\d{4}-\d{4}\b` 信用卡 + `[\w.+-]+@[\w-]+\.[\w.-]+` 邮箱——非高级用户也知道这两类该被 redact。让首次接触 regex pattern 的用户能"复制 + 修改"出符合自己需求的 pattern。
- **路线 C v2 闭环**：v1（Iter Cx-Cw）= 子串覆盖 5 通道；v2（Iter Cz）= 加正则覆盖 5 通道。下一步可能是 redaction logging / 面板上看"过去 N 次工具调用 redact 了 M 个 match"——但只在用户实际使用后反馈再做。

## Iter Cw 设计要点（已实现）— persona_summary 自循环入口也 redact
- **build_persona_hint redact，get_persona_summary 不 redact**：刻意的不对称。前者是 prompt 注入通道（送给 LLM），后者是 panel 显示通道（给本地用户）。同一份原始数据，两种 surface，不同处理——redaction 是"对外可见性"过滤而非"数据修改"。注释明确写出这个区分，避免后续维护者困惑。
- **5 个通道闭环**：active_window 工具 → calendar 工具 → mood note → speech_history hint → persona_summary hint。这 5 个是 LLM 能从 prompt 读到的所有"用户/历史相关"自由文本字段，全部经过 redact。剩余字段（companionship_days / cadence / motion 标签 / 时间戳 / 设置参数）都是结构化数字或固定文案，无 leak 通道。
- **没有抽 trait/abstraction**：5 处都是 `redact_with_settings(text)` 一行调用。如果将来超过 10 处再考虑用宏 / 中间层。当前的 spread-out call sites 让"哪些通道经过 redact"在 grep 时一目了然——比抽象隐藏的"自动 redact" 更清晰。

## Iter Cy 设计要点（已实现）— redaction 扩到 self-loop 通道
- **read-time 而非 write-time redaction**：speech_history.log 文件保持原文。如果在写入时 redact，用户改 patterns 后过往 leak 永远留在文件里。read-time redact 让"我刚加了新 pattern" → 下一次 prompt 注入时新+老内容都被覆盖。可逆 + 即时生效。
- **redact_with_settings 抽成 helper**：Iter Cx 的两个工具入口手写 `get_settings().map(...).unwrap_or_default()` + `redact_text(...)` 4 行模板。Iter Cy 把它抽成一行 helper——3 处调用（active_window / calendar / mood note / speech_history）现在写法统一。如果将来加 ToneSnapshot.mood_text 也需要 redact，再加一处调用同样简洁。
- **mood 是 self-loop 风险点**：LLM 写 mood 时不会自我 redact——它看到原文 active_window 就可能在 mood 里写"为 Dr. Smith 担心"。这是用户 pattern 还没生效或宠物历史早期没被覆盖的窗口期 leak。每次 inject_mood_note 重新 redact 是兜底。
- **speech_history.log 文件不动**：宠物"实际说过什么"是宠物的人格记录，不应该被 redaction 改。如果将来想让用户审计 / 导出宠物语料，原文保留。redact 只在"对外发送给 LLM"那一瞬间应用。
- **路线 C 现在覆盖 4 个 prompt 注入通道**：active_window 工具 / calendar 工具 / mood note system message / speech_history 反哺 prompt 段。剩余可能 leak 通道：persona_summary（LLM 自写人格画像可能带私人词）和 mood_trend_hint（仅含 motion 标签 + 数字，无文本，零 leak 风险）。persona_summary 加 redact 是 future iter 候选。

## Iter Cx 设计要点（已实现）— 路线 C 起步：本地 redaction
- **substring 而非 regex**：用户场景是"我不想让宠物把 Slack DM 对方名字传给 LLM"——典型输入是公司名 / 客户名 / 项目代号这类硬字符串。regex 会带来 ReDoS 风险 + 用户得学语法 + 编辑器里写正则容易出错。明确选 substring：用户列出固定词，命中即替换。
- **case-insensitive 默认**：用户输入"slack"应该也匹配"Slack"——大部分情况这是预期。如果有人想严格 case，未来可以加一个"严格大小写" toggle。当前 case-folding 通过 `to_lowercase` 镜像扫描实现，O(n*m) 但 m 很小（10-20 个 patterns × 短字符串），可忽略。
- **`(私人)` 中文标记而非 `[REDACTED]`**：界面默认中文，用户读 panel 日志看到 `(私人)` 比 `REDACTED` 友好。LLM 也能理解"this is a redacted personal item, the user chose not to share details"。
- **空 / 空格 patterns 跳过**：用户在 textarea 里多按一下回车留空行不该让所有内容被替换为空（"".replace 会无限循环或匹配全字符）。一行 trim 后空就 skip——textarea 输入容错关键。
- **UTF-8 boundary 推进**：传统 `text.replace` 也工作但没法做 case-insensitive UTF-8。手写扫描时小心 `is_char_boundary` 才不切坏中文字符。emoji safety 测试是关键防线。
- **每次调用读 settings**：`crate::commands::settings::get_settings()` 在每次 tool 调用时读一次。简单且即时生效——用户改 patterns 立刻下次工具调用就用新设置。但 IO 开销小（settings.toml 几 KB），不优化。
- **path 选 active_window + calendar 不选 weather**：weather city 名是用户故意公开的（在 settings 里配置），不是泄漏；title / window_title / event_title 才是隐私敏感的环境读数。范围明确而非"全部 redact"。
- **替换发生在工具结果，不是 prompt 末尾**：在工具产出 JSON 前过滤——LLM 既看不到原文也无法推断，且工具 cache（Iter 28）也存的是 redacted 版本，避免一次缓存命中 leak 全部历史。语义干净。
- **路线 C 起步而非全做**：仅 substring 模式，未来可加 regex / glob / 正则模式 / "all caps automatic redaction" 等。本 iter 选最简单的 substring，覆盖 80% 场景。

## Iter 105 设计要点（已实现）— Persona panel tab
- **三个 section 一对一映射 prompt 三层**：陪伴时长（companionship_days）/ 自我画像（persona_summary）/ 心情谱（mood_trend）—— UI 结构镜像 prompt 注入的三层结构。用户看到的"宠物当前画像"和 LLM 看到的"长期身份背景"是同一份数据的两种 surface，只是给人 vs 给模型。
- **复用 prompt 用的 mood_trend_hint 格式 vs 单独画图**：本可以让 panel 把 mood 计数变成柱状图。但选 plain text trend hint 让 panel 显示的就是"LLM 实际读到的那段话"——零 surprise，用户看 panel = 看 prompt 真相。chart 反而是另一种 view，需要解释。
- **persona_summary 显示原始 description，不加 header**：proactive prompt 里给 LLM 看的是带 header `你最近一次自我反思的画像（来自 consolidate）：` 的版本。panel 用户看不需要这个 meta-框——他们已经在"自我画像" section 里了。所以新加 `get_persona_summary` 命令而非 reuse `build_persona_hint`。
- **轻量 5 秒 polling vs 1 秒**：PanelDebug 1 秒 poll 是因为决策日志 / 计数器要做 live dashboard。Persona tab 是 review 性质，5 秒足够——consolidate 周期 6 小时，mood 几分钟级转变，没必要更密。同时减少 Tauri invoke 频率。
- **"今天初识" 在 PanelStatsCard 已有，PanelPersona 也保留**：两处都用相同 day 0 文案（Iter 106 + Iter 105），让 panel 不同入口显示一致——StatsCard 是首屏 chip，Persona 是详细页，但两处看到的都是 "0 天（今天初识）"。
- **footer 解释数据流向**：用户看到一堆 panel 数据可能不知道这有什么用——加一条 dashed 分隔线下方的 footer 说明"这些会被注入到 prompt 中影响宠物行为"。建立"看到的数字 ↔ 宠物的行为"心理映射，让 panel 不只是装饰。
- **不加编辑能力**：persona_summary 当前是 LLM 自己写的，user 不能编辑。理论上加个"重写"按钮也行，但那破坏了"自我反思"的语义——summary 是宠物自己得出的，不是 user 设定的。如果用户真要改可以直接编辑 `~/.config/pet/memory/ai_insights/persona_summary.md` 文件——明确的 escape hatch。
- **路线 A 全链路可见**：现在 Iter 101-107 累积起来形成完整可观察的人格层：backend 生成 → prompt 注入 → 宠物行为 → 用户在 panel 看到 → 心智模型形成。

## Iter 107 设计要点（已实现）— Telegram 也接入人格层 + opt-out
- **Telegram 加 opt-out 而非 desktop 也加**：desktop 用户和宠物的关系紧密，几乎全程使用——人格层默认开启且没必要给 toggle。Telegram 是"远程 / 偶尔 / 工具向"使用场景：用户可能就想问"今天天气"或"提醒我吃药"，不需要宠物每条消息前都灌"我们认识 N 天 + 我观察自己倾向短句"——长 system note 也烧 token。所以 Telegram 唯一带 opt-out。
- **存在 HandlerState 而非每条消息读 settings**：每条 Telegram 消息都重新 `get_settings()` 浪费 IO，且语义上"切换 persona_layer_enabled" 应该需要 restart bot 才"立刻生效"——用户改这个 setting 不太可能希望立刻看到效果，多半是配置完一并启动。如果将来需要热切换再加 reload 逻辑。
- **Default 手写而非 derive**：`#[derive(Default)]` 在 bool 字段会得到 `false`，但我希望 `persona_layer_enabled` 默认 `true`（与 desktop chat 默认行为一致）。手写 impl 让 default 和 serde-default 都一致，避免 toml 缺字段时反而禁用人格层。
- **复用 inject_persona_layer 而非重写 Telegram 专用版本**：人格层文案、注入位置、ordering 完全跨路径一致——Telegram 走特殊版本会让"宠物在不同路径上像不同的宠物"。这个一致性是路线 A 的设计核心。
- **PanelSettings checkbox 文案明确列出三个组成部分**：「注入长期人格层（陪伴天数 + 自我画像 + 心情谱）」让用户看到 toggle 时直接理解关掉会失去什么——比 "Enable persona layer" 的英文/笼统措辞决策成本低。
- **路线 A 三路覆盖完成**：proactive 路径 + desktop chat 路径 + Telegram 路径都已注入。如果将来加新对话路径（比如 watch app / 桌面快捷键 ad-hoc query），照同样模式 inject 即可——build_persona_layer_async 是 single source of truth。

## Iter 106 设计要点（已实现）— 陪伴天数面板可见
- **数字层级 28 → 20 → 16**：lifetime 是品牌数字（"我们一共聊了多少次"），today 是状态数字（"现在是不是克制日"），companionship 是身份数字（"我们认识多久了"）。三者重要性递减，字号也递减让视觉自动按价值密度排序。
- **左侧 1px 分隔线**：把 companionship 与前面的"今日 / 累计"块视觉上分开——它们计的是同一件事（开口次数），companionship 是不同维度的概念（时间）。一道分隔线让"两类信息"在 1 行内并存而不混淆。
- **青色 #0d9488 而非紫绿橙红**：现有 panel 颜色系统中 #7c3aed 紫（lifetime）、#0ea5e9 蓝（today）、#dc2626 红（克制/沉默警告）、#16a34a 绿（spoke / 引导）、#ea580c 橙（环境感知低 / 克制模式）、#a855f7 紫（mood/motion）已经被占用。青色 #0d9488 是 tailwind teal-600，在 panel 既有颜色系统里相对中性，不与任何"警告 / 鼓励"类语义冲突——适合标识"长期陪伴"这种中性身份信息。
- **day 0 文案分支 "今天初识"**：与其显示"0 天陪伴"（数字读着像负面 / 冷冰冰），不如改写成"今天初识"——表达友好的"刚认识今天"。N ≥ 1 用"N 天陪伴"，"陪伴"二字是这个 chip 的情感锚点。
- **不再加重置按钮**：companionship 是单调递增的 lifetime stat，"重置"会破坏整个路线 A 的语义（让数据看起来像是"陪伴 0 天"）。如果将来用户真的想重置（搬到新设备 / 装在新机器），删除 install_date.txt 即可——明确的低频高代价操作不应该一键化。
- **复用 ensure_install_date 的 zero-config 写入**：get_companionship_days 命令调用 → 内部走 ensure_install_date → 文件不存在则写今天。意味着新装用户打开 panel 第一秒，install_date.txt 就被自动写好。无需任何 setup 流程。
- **接入 PanelStatsCard 而非新 chip**：companionship 是身份层信息，应该和 lifetime 这种"长期账目"放一起，而不是和 cache hit / LLM 沉默率这种 process-level chip 混。stats card 有更高 typographic weight 也合理——这个数字值得被看到。

## Iter 104 设计要点（已实现）— 路线 A 延展到反应式路径
- **proactive 注入 vs chat 注入**：proactive 的"长期人格" hints 是嵌在大块 `[系统提示·主动开口检查]` 里的 sections。reactive chat 走另一种语境（用户来聊），人格背景应该是**独立 system note**，让 LLM 看到"这是宠物长期身份描述，与具体对话无关"。所以做了独立的 `[宠物的长期人格画像]` 包装。
- **format_persona_layer 是 pure 而非 async**：把 IO 抽到外层 wrapper（build_persona_layer_async）。pure 函数能被单测精确锁顺序、空处理、whitespace 等行为，IO 部分只剩薄薄一层组装。这是 Iter 89/90/91 alignment 测试同样的设计哲学：业务逻辑可测，IO 边界扁平。
- **whitespace-only 当空**：persona / mood_trend 内部已经处理 None / 空时返空字符串，但保险起见这里再 trim 一次。多重防御让"空内容混了空格"不会偷偷在 system note 里加一个空 block。
- **顺序保持**：companionship → persona → mood_trend，和 proactive 的顺序一致。LLM 在两条路径上看到的人格层结构相同——降低"宠物在 chat 里和 proactive 里像两只不同的宠物"概率。
- **复用 inject_mood_note 的 insertion 规则**：找到第一个非 system 的位置插入。如果未来用户聊天历史里有多条 system message（比如未来加了某些 ad-hoc 系统指令），新 note 会被插在所有 system 后但用户消息前——和 mood_note 同位次。
- **不让 chat 也写 mood_history**：record_mood 只在 proactive turn 后调，因为那里有 mood update 的真实信号。chat 后用户聊一句也"读"了一次 mood，但那不是宠物自发的情绪转变；如果计入会让 trend 偏向"用户来聊我就 Idle"——破坏 trend 信号纯度。
- **Telegram 自动跟随**：Telegram bot 也通过 run_chat_pipeline 调用，但 inject_persona_layer 是在 chat handler 而非 pipeline 内。如果要让 Telegram 也注入，需要在 telegram/bot.rs 那边显式调一下。当前先保持只 desktop chat，未来再扩展（看 Telegram 用户对长期人格感的体感再决定）。

## Iter 103 设计要点（已实现）— 路线 A 第三步（路线 A 收官）
- **去重而非全部记录**：mood 在 proactive 周期被频繁 re-read，但实际转变不那么频繁。如果每次 record 都写，"我最近 30 次心情" 容易变成 "30 次都是同一个 Idle"——失去趋势意义。dedup 让 history 抓"心情演化的关键点"。
- **`<ts> <motion> | <text>` 格式而非空格分隔**：mood text 含中文标点可能含空格、特殊字符。pipe + space 三字符 ` | ` 作 separator 几乎不会和真实文本碰撞，但又比 JSON Lines 轻量——`parse_motion_text` 一行 split_once 搞定。
- **`-` 表示 no-motion 而非 None**：解析时区分 "Tap" 和 "Idle" 容易，要表达"这条 mood 没 motion 前缀"需要一个占位。`-` 简单可识别，trend hint 在格式化时 filter 掉它（无信息量），但仍占 total 计数（便于阈值判断）。
- **format_trend_hint 双 fallback**：(a) total < min_entries → None（早期不输出虚假 insight）；(b) 全是 `-` → None（filter 后 body 空也不输出）。两条 fallback 让 prompt 注入要么有意义要么不出现，不会出现 "你最近 N 次心情：（无）" 这种空尴尬。
- **window 50 / min 5**：window 大于 30 因为 dedup 后实际记录的转变次数 ≪ proactive 调用次数；50 大约覆盖几周的转变窗口。min 5 让"前几天试用"的宠物不会因 1-2 个 Idle 就显示"你最近偏 Idle"——还得攒一阵子才有 trend。
- **建议在 prompt 措辞 "可以让 ta 渗进当下语气，但不必生硬带出"**：和 persona_hint 类似，不让 LLM 把 "我最近 Tap × 12" 直白复述给用户（"你知道吗我最近 Tap 12 次"），而是让 trend 影响选词风格。这是 prompt design 教 LLM **subtle 应用而非 verbose 报告** 的细节。
- **路线 A 收官**：companionship_days（瞬时身份 = 我们认识 N 天）+ persona_summary（中期身份 = 我观察自己的语气）+ mood_trend（长期身份 = 我的情绪谱）。三层覆盖时间尺度，每层 prompt 注入位置由 backend 控制，每层都可独立 evolve。前置 SOUL.md 静态人格 + 这三层动态人格 = "陪伴一年的宠物"和"刚装上的宠物"在 prompt 上有可观测的差别。

## Iter 102 设计要点（已实现）— 路线 A 第二步
- **复用 consolidate 的 LLM 调用而非新加一次**：consolidate 已经是周期性 LLM 调用 + 已经允许 LLM 改 memory。把"reflect 自己写 persona_summary"作为该调用的第 5 项任务，零新 LLM 成本。如果将来想让 reflection 频率独立于 consolidate，再拆出独立调用。
- **第一人称写法**：prompt 明确要求"写第一人称（如 我倾向...、我注意到...）"。LLM 写关于自己的话用第三人称（"宠物的语气倾向..."）会让 proactive 读到时显得疏离；第一人称让 description 直接像"我自己的笔记"，proactive prompt 拼回去时角色一致。
- **~100 字限制**：persona_summary 注入 proactive prompt 后是固定增量。100 字（约 200-300 tokens）是经验值——足以表达"语气 + 互动模式 + 偏好"三个维度，又不至于 prompt 膨胀。如果将来发现 LLM 写超长，加 prompt 强约束或后置 truncate。
- **< 5 句跳过的"信号下限"**：consolidate 可能在宠物刚装上、还没说几句的时候就跑（如手动触发）。强行让 LLM 总结 0-2 句话会得到不准确的人格描述（"我话很少" — 其实只是没启动）。明确门槛 5 句，让首次反思有意义。
- **strip_timestamp 后再投喂**：speech_history 文件每行带 ISO timestamp，对 LLM 总结语气/模式没用，反而占 prompt 长度。strip 后 LLM 看到的就是干净的"宠物说过的话清单"。
- **persona_summary 复用 daily_plan 模式**：build_persona_hint 完全镜像 build_plan_hint 的结构（read memory_list ai_insights → find by title → format with header）。两个 hint 同型让代码风格一致，未来加第三个 self-state hint（如 mood_trend / Iter 103）一行复制即可。
- **位置：mood → companionship → persona → context**：从瞬时（mood）→ 关系时长（companionship）→ 长期自我认知（persona）→ 当下环境（context）。"我现在感觉怎么样" → "我和你认识多久了" → "我看到自己是个怎样的我" → "现在用户在做什么" 的递进，每层时间尺度不同，LLM 容易合成。
- **"特殊保护"扩成两条**：current_mood + persona_summary 都不能 delete。consolidate 的 LLM 有时会过于积极清理 ai_insights 类条目；明确写在 prompt 里防止意外删除"这只宠物的灵魂"。

## Iter 101 设计要点（已实现）— 路线 A 入口
- **首次启动 zero-config 写入**：用户不需要在任何 settings 配置 install_date——首次 proactive turn 跑 `ensure_install_date` 自动写入今天。这是 setup-friction = 0 的关键，符合"宠物自己开始累积时间"的隐喻。
- **数字而非文字传给 prompt**：本可以让 backend 直接拼好 "已经 N 天" 字符串塞进 cadence_hint。但传纯数字让 prompt 构造层（format_companionship_line）拿到完整决策权——day 0 用初识措辞、day N 用相处时长措辞，未来想加 "100 天纪念"、"半年" 之类阶段化文案也不需要改 backend。
- **day 0 显式分支**：本可以让 day 0 = "已经 0 天" 文字，让 LLM 自己理解。但 0 天对 LLM 来说语义模糊（是"刚认识"还是"已经过了不到一天"？），明确措辞"第一天 / 初识的客气感"更可控。N >= 1 信任 LLM 用"N 天"做语调判断。
- **clamp 负数到 0 防御**：用户改系统时钟、跨时区飞行、手动编辑 install_date.txt 写未来日期——三种 corner case 都可能让 today - install < 0。clamp 到 0 让宠物不会出现"我和你认识了 -3 天"的 nonsense。clock skew 是真实存在的现实，工程上必须处理。
- **install_date.txt 而非 ProcessCounters atomic**：天数必须跨重启活下来——这是 lifetime stat。文件方案和 speech_count.txt / speech_daily.json 一致，9 类持久化文件中又添一类，模式统一。serialize 简单到不需要 JSON：单行 YYYY-MM-DD，肉眼可读，用户想"作弊"调整自己的陪伴日数也能一键编辑。
- **base_inputs 默认 30 而非 0**：默认必须让既有 18+ 个 prompt 测试零修改通过。0 会让 day 0 的"第一天"措辞出现在所有既有测试的 prompt 里——破坏不少 contains 断言。30 是中间值，特殊情况测试再 override 到 0 或 365。
- **位置插入在 mood_hint 之后**：build_proactive_prompt 的叙事顺序是"现在时间 idle → cadence → 心情状态 → **我和用户的相处时长** → 上下文 hints → 用户问题 → 约束"。从 self-state 到 relational-state 到 environment-state 是合理的递进。
- **route A 之路**：Iter 101 是"宠物知道时长"的最小步骤；Iter 102 计划让宠物"反思形成自我画像"；Iter 103 让"心情趋势"也进 prompt。三步走形成"动态人格"——SOUL.md 静态文本之上叠加可演化层。

## Iter 100 设计要点（已实现）— 里程碑盘点
- **第 100 次迭代不写代码而是写盘点**：90 多次微观迭代之后容易陷入"机械再拆一个组件 / 再加一个 chip"的本地优化。100 是个值得停下来对照原始目标看走到哪的标记点。盘点结果应该让后续迭代重新对齐高价值方向，而不是继续琐碎累积。
- **STATUS.md 而非把内容塞进 IDEA.md**：IDEA.md 已经是"每个 iter 的设计思考"日记，按 iter 倒序累积——结构上不适合放"项目当下整体状态"。新文件 STATUS.md 单独承载 high-level 盘点，未来用户/协作者打开仓库一眼能看到当前进展。
- **按差距逐项核对而非按 iter 历程讲述**：DONE.md 已经是按时间线的列表。STATUS.md 用"目标 → 差距 → 闭合度"映射给读者拓扑视图：差距 ① 主动 → 哪些 iter 在做、做到什么程度。这比"按时间线读 99 个 iter 的 changelog"对理解项目状态高效得多。
- **诚实评估"是真实伙伴吗"**：技术 vs 体感分开评估。技术 ✓（5 条差距全闭合），体感还缺人格深度 + 表情丰富度。这个 honest 是为了避免里程碑文档变成自我吹捧——下一阶段路线指向真正的间隙（A 路线：长期人格演化）而不是已经够好的领域。
- **路线 A 选 "长期人格演化"作下一焦点**：现有 infra（mood / speech_history / memory / prompt 规则）都已经搭好，A 是把它们真正绑在一起的工程——让"用了一年的宠物"和"刚装上的宠物"在 prompt 层面有可观测的差别。其他路线（表情、隐私、跨设备、记忆 UI）都是边际优化。

## Iter 99 设计要点（已实现）
- **stats card vs chip strip 是不同职责**：StatsCard 是首屏"大数字 + 一个 badge"形态——核心数据高 typographic emphasis；ChipStrip 是细分指标的水平阵列——多 chip 紧凑显示。两者视觉密度不同，分两组件清晰。
- **ToneStrip 自带 null guard**：`if (!tone) return null;` 让外层 `<PanelToneStrip tone={tone} />` 无需 conditional 渲染。组件契约："给我 tone，可以是 null"——内部决定怎么处理。比外层 `tone && <ToneStrip />` 干净。
- **不再在 props 里传 NATURE 等字典**：StatsCard 和 ToneStrip 都不依赖 PROMPT_RULE_DESCRIPTIONS。如果将来要在 ToneStrip 里加 rule-aware 着色（比如按 active_prompt_rules 数量调整 mood 色），再 import 就好——目前不必要。
- **保留 IIFE 风格的派生计算**：StatsCard 内对 restraining/todayColor/todayTitle 的派生从原来的 IIFE 简化为顶层 const——组件作用域已经局部化了，不再需要 IIFE 隔离作用域。代码更平。
- **PanelDebug 还能继续拆吗？**：剩余的几个块（prompt-hints 展开 / decisions list / recent speeches / reminders / logs view）都和 state 耦合较紧（showPromptHints toggle、scrollRef 自动滚动、过滤展示）。继续拆需要 props 数量增加，性价比下降——目前拆到这个粒度合适。
- **依赖关系**：panelTypes（数据契约）→ PanelStatsCard / PanelToneStrip / PanelChipStrip（pure presentation） → PanelDebug（编排 + state）。三层清晰单向。

## Iter 98 设计要点（已实现）
- **打破组件循环依赖**：Iter 97 把 ChipStrip 抽成 PanelDebug 子组件，但类型定义还在 PanelDebug 里——ChipStrip import PanelDebug 类型，PanelDebug import ChipStrip 组件。这是循环依赖（虽然 TS 不报错因为 ChipStrip 只 import type）。Iter 98 把类型搬到独立 panelTypes.ts，两个组件都从中性第三方 import，依赖图变成 Y 字而非环形。
- **`.ts` 而非 `.tsx`**：纯类型 + 数据无 JSX。`.ts` 后缀让导入者一眼知道这是 data-only 模块。如果将来加面板专用 hooks 或非组件的辅助函数，也可以放这里或并列建 `panelHooks.ts`。
- **PROMPT_RULE_DESCRIPTIONS 包括 nature 字段**：dict 现在三字段（title/summary/nature）。从 panel UI concern 而言，nature 是展示分类——同位置维护。如果将来要按 nature 做 backend prompt 行为决策，再考虑往 backend 倒。
- **cargo 测试更新只改路径不改逻辑**：parser 仍按 `<key>: {` 模式扫，对目标文件位置无关。一行 path 改动 + 三处文案修正即可，Iter 89/90/91 测试逻辑保持不动。这就是抽 helper 的好处——单一锚点改完所有依赖跟随。
- **PanelDebug 体积降低 ~30%**：770 → 590 行。剩下的全是 component logic（state、effect、handler、JSX），更容易跟踪 panel UI 行为。每个对应职责清晰：panelTypes.ts = 数据契约，PanelChipStrip.tsx = 数据展示，PanelDebug.tsx = 状态编排 + 主布局。
- **不抽其他面板（PanelChat / PanelMemory / PanelSettings）的类型**：那些组件目前是相对独立的（chat 用自己的 ChatMessage 类型，memory 用自己的 MemoryItem）。只有 panel debug + chips 共享 type，所以 panelTypes 取名并不强制覆盖整个 panel/ 目录。如果将来出现跨面板共享需要，再考虑提取到 sharedTypes.ts。

## Iter 97 设计要点（已实现）
- **纯展示组件 + state 留在 parent**：PanelChipStrip 不持有任何 useState，全部 state 还在 PanelDebug。组件接收 props（stats / handlers）输出 JSX——单一职责清晰。如果将来想做 Storybook 测试或单独渲染 chip，组件签名就是契约。
- **导出 types + 字典而非新建 shared file**：本可以建 `src/components/panel/types.ts` 把 6 个 interface + PROMPT_RULE_DESCRIPTIONS 都搬过去。但现有 cargo 对齐测试（Iter 89/90/91）扫的是 PanelDebug.tsx；改动结构需要同步更新测试路径。直接 `export` 既有 const 是最小变更——TS import 可工作，cargo 测试只需要识别 `export const` 前缀（一行代码改动）。
- **chips 上方而非下方**：原 toolbar 是 panel 第一行，chips 嵌在右侧。把 chips 提到 toolbar 之上意味着用户打开 panel 第一眼看到的是数据状态（"现在 prompt 倾向 60% 克制"），其次才是动作按钮。"诊断"用例（占 panel 主要使用场景）优先级 > "操作"用例，所以 data-first 排序合理。
- **expansion 仍跟在 toolbar 下方**：理论上 prompt-hint 展开应该紧贴 chip 行（trigger 在那）。但展开是临时审视行为，每次出现尺寸 ~120px 高，把它放 toolbar 之上会让 toolbar 在用户审视规则时跳出视野。妥协：展开放 toolbar 下方，与 trigger 视觉距离稍远但 toolbar 位置稳定。
- **resetBtnStyle 抽常量**：5 处 chip 都有 "重置" 按钮共享同一套 10 行样式。原本散落 5 份，组件内提取成 `resetBtnStyle` const。这是抽组件的"副产品红利"——以前在大文件里重复因为重构成本高，搬进新组件的 fresh slate 自然可以做这种小整理。
- **flex-wrap 应对多 chip**：6 个 chip + 重置按钮在小屏可能超过宽度。`flexWrap: "wrap"` 让超出部分自动换行成第二行，`gap: "12px"` 保证行内行间间距一致。比之前 toolbar 单行硬挤更耐受窗口缩放。
- **alignment test 改最小化**：Iter 89/90/91 的 parser 只判 `starts_with("const PROMPT_RULE_DESCRIPTIONS")` → 加一个 `|| starts_with("export const ...")`。一行变两行，覆盖现状。如果未来有更激进的语法变化（如 `export const PROMPT_RULE_DESCRIPTIONS satisfies ...`）再升级 parser，但目前不必要。

## Iter 96 设计要点（已实现）
- **4 bucket 互斥求和=N 而非各自独立累加**：本可以简单两个 atomic（restraint_count_total / engagement_count_total），看到 R=12 E=4 推断"克制主导"。但单 Run 可能有多条 restraint 规则，求和会高估发生频率。每 Run 单一分类 bump 互斥 bucket，4 个 bucket 加起来 = Run 总次数，比例直接等于"那一类 dispatch 的占比"。
- **classification 与 panel badge 完全一致**：Iter 95 badge 颜色 = `restraint > engagement ? red : engagement > restraint ? green : (R+E==0 ? purple-neutral : purple-balanced)`。record_dispatch 完全镜像这个判断——保证"长期 chip 显示克制 60%"和"打开 panel 时 badge 是红色"两个观察是相同事实的两个时间尺度展示。
- **只 Run 派发计数，Skip/Silent 不计**：Skip 表示 gate 拦截（用户活跃 / 安静时段），Silent 表示 disabled——这两种情况虽然也"算了一次 active_labels"，但 prompt 没真的发给 LLM。计入会让 idle 用户的 12 次/小时 Skip 把统计淹没成"60% restraint"——其实根本没派发。所以只在 Run 路径计数，反映 prompt 真正在工作的那些时刻。
- **dominant chip 的 4 路 reduce**：用 `reduce((best, b) => t[b.key] > t[best.key] ? b : best)` 选最大 bucket。tied 时 reduce 保留先入者——按 buckets 数组顺序：restraint > engagement > balanced > neutral 优先级。这个 tie-breaker 选择是设计判断："如果 restraint 和 balanced 持平，更倾向报告 restraint"——因为 restraint 更值得用户警觉。
- **总数 0 时不渲染**：和其他 chip 同策略。新启动 panel 上不会冒出空 chip 干扰。Run 一次后立刻 1/1=100% 主导某类——预期外的"100% chatty 主导"瞬间值不会持续误导（很快被后续 Run 稀释）。
- **不持久化跨重启**：和其他 process_counters 一致。这是"调试 prompt 时看效果"的指标——重启清零让用户能针对一段使用窗口测量。如果将来想看"过去一周倾向"，需要走 speech_daily 类似的文件分桶（不是这个 iter 的范围）。
- **buckets 数组用对象数组而非分段 if**：`{key, label, color}[]` 让 chip 渲染逻辑统一——选 dominant 后直接拿 label/color，无 switch。新增 bucket 类型只需加一行数组项目。

## Iter 95 设计要点（已实现）
- **只数 restraint vs engagement，忽略 corrective/instructional**：badge 颜色应该传递"宠物现在被压还是被激发"。corrective（"过去做错了，注意改"）和 instructional（"做事时按某种格式"）都不直接影响"开不开口"，纳入计数会污染倾向信号。例如纯 instructional 规则一堆活跃，红色或绿色都是误导——保持紫色（中性）正确。
- **strict > 而非 ≥**：tilt 判断用严格大于。1 vs 1 仍然紫色；2 vs 2 也紫色。"平衡"也是一种状态，不应该归到任何一边。如果用 >=，1==1 时会被随便归到 restraint（因为先判），破坏对称性。
- **bg 和 bgOpen 分开两色**：closed 状态用中度色（#dc2626），open 状态深一档（#991b1b）。不用单色 + opacity，因为 opacity 在白色背景上会让 badge 文字掉对比度（白字 + 半透明红 → 文字可能变灰）。两套硬编码颜色稳定。
- **tooltip 4 种文案分支**：根据具体情况给精确描述，避免"current tilt: restraint" 这种英文 + 模糊。文案直接说"偏克制（克制 × 3、引导 × 1）"——既给类别又给数字，用户不需要展开就能猜出 prompt 长什么样。
- **闭合 IIFE 而非 useState 派生**：badge 颜色完全派生自 ToneSnapshot.active_prompt_rules，不需要 state——直接 IIFE 里算好返 JSX。useMemo 也不必要——active_prompt_rules 是 backend 字符串数组，每秒 refresh 一次，每次 10 条以下规则做 reduce 几乎零成本。
- **不引入 "warning" / "alert" 颜色**：本可以让 restraint ≥ 5 时切橙红 / 暗红进一步分级。但 5 条 restraint 已经是相当严重的克制状态，红色已经传递这个意思——再分级反而过度。简单二值（restraint vs engagement）最清晰。
- **badge 是 "prompt 整体心电图"**：和 panel 既有 chip strip 配合，badge 报告 prompt 顶层倾向，chip 报告各类细分指标（cache hit、tag 命中率、env 感知率、LLM 沉默率），点开 badge 后看到的是规则列表细节。三层信息密度：粗 → 中 → 细。

## Iter 94 设计要点（已实现）
- **4 类而非 2 类**：本可以二分 restraint/engagement，但 corrective（处理过去模式）和 instructional（具体操作步骤）都不是单纯的"压制"或"激发"。corrective 是"过去做错了，未来这样做"——半教训半行动；instructional 是"做这件事的时候按某种格式做"——技术细节。强行塞入 restraint/engagement 二元会丢失这两类的特殊价值。
- **配色对应情绪谱系**：克制 = 红（停一下）、引导 = 绿（前进）、校正 = 橙（注意）、操作 = 青（中性技术）。和 panel 既有色系不冲突——quiet-soon 用红、Spoke 决策日志用绿、克制模式 badge 用橙、cache stat 用青——四个 nature 复用既定语义。
- **聚合行 + 行内 badge 双层冗余**：聚合行让用户扫顶部就知道整体（"5 条里 3 条是克制"），行内 badge 让用户眼睛沿着列表向下移动时不丢上下文（每行哪类一目了然）。看似冗余，但分别服务"整体感知"和"逐条审视"两种使用模式。
- **nature 在前端而非 backend**：nature 是 UI 概念（用于展示分类）。backend 关心"哪些规则活跃"，UI 关心"如何呈现"。和 title/summary 一样放前端字典，未来加多语言或重新分类不动 backend。如果将来 backend 需要 nature 做决策（比如"如果克制规则 ≥ 3 条则强制 silent"），再考虑迁到 backend。
- **不引入第 5 个 nature**：考虑过 "encouragement"（区别于 engagement）、"informational"（区别于 instructional），但当前 10 条规则都能干净落入 4 类。增加分类容易把"分类"变成"为了分而分"。
- **每条规则的 nature 选择需要思考**：
  - icebreaker 表面上是 instructional（"问什么样的问题"），但核心精神是"避免 info-dense 话题"——是 restraint。我把它归到 restraint。
  - wake-back 提示要 "简短克制" 是关键词，restraint。
  - reminders 是"传达 + 删除"，二者都是具体操作，instructional。
  - 如果将来某条规则同时具备多个 nature（如 chatty 又克制又教如何说），可以引入数组类型，但现在还没必要。
- **不加 cargo test 守护 nature 字段**：Iter 89/90/91 已经守 label 对齐和 match arm，nature 是字典 metadata，缺失只影响 UI 展示美观度（fallback 为 "?" badge），不影响 prompt 行为。如果未来想严格守护，可加扫 PROMPT_RULE_DESCRIPTIONS 看每个 entry 是否有 nature 字段——但当前规模不必要。

## Iter 93 设计要点（已实现）
- **None == long-idle 的语义选择**：从未开口（None）当作 long-idle 处理。理由：fresh session 时用户面对一个完全没说话过的宠物，宠物自己应该开第一口；如果 None 当作"未知，不触发"反而把 first-session 用户排除在外。这条规则的精神是"沉默太久"——None 是"沉默无穷大"，应该最满足条件。
- **三参数门槛设计**：long-idle && under_chatty && !pre_quiet。三者都得满足才积极开口——单 long-idle 触发会跟 chatty / pre-quiet 冲突（已经聊够了 / 该睡了），过于鲁莽。三因素叠加才是真正的"安全开口窗口"。
- **chatty / long-idle 互斥与 pre-quiet / long-idle 互斥的处理**：测试上没法 single inputs 同时触发所有 10 label。改成 fingerprint 测试两 scenario combined coverage——这是"测试设计跟随领域设计"。如果硬塞 single inputs 通过测试，反而是把不可能的逻辑组合写进 production 代码。
- **rule 文本特意反 "问候 / 问感受"**：long-idle 规则明确说"不是问候、不是问感受，是真的「看到 ta 在做 X 想到 Y」"。observation: 沉默已久后开口最容易退化成"还好吗"这种无意义模板；规则强制 LLM 调 active_window 看出真实 context 后再开口，杜绝 generic 问候式打扰。这是 prompt 设计上"明确反例"的力量。
- **数字字段补 cadence_hint 字符串的不足**：cadence_hint 是文本（如"刚说过话，话题还热"），LLM 解析它需要对中文做语义理解。数字字段允许规则本身做 deterministic 比较（`>= 60`），LLM 不需要做模糊匹配。两者互补：人读字符串方便，规则用数字精准。
- **测试 base_inputs 默认 Some(8) 而非 None**：默认值要让现有测试不受新规则影响。Some(8) 表示"刚说过话"——和 cadence_hint 的默认文本对齐，且 < LONG_IDLE_MINUTES 不触发。如果默认 None 反而会让所有现有测试都激活 long-idle，违反"添加新功能不破坏老测试"。

## Iter 92 设计要点（已实现）
- **从单向限制到双向引导**：前 8 条规则全是"在 X 条件下宠物应该克制 / 校正 / 按某种方式说话"。Iter 92 第一次出现"在 X+Y 复合条件下宠物**应该开口**"，反向用 prompt 系统鼓励主动行为而不是只压制。"复合规则"是合理的第三类——单一信号可能不够强，复合信号可以解锁不同语调。
- **三类规则架构**：environmental（瞬时状态触发）/ data-driven（统计驱动）/ composite（多信号合成）。每类有自己的 helper，三者 chain 为 active_prompt_rules。这个分类不是为了好看——是把"什么样的输入触发什么类型的引导"拆成可独立扩展的轴。
- **wake-back 和 engagement-window 表面上冲突**：wake-back 说"问候要简短克制"，engagement-window 说"积极用这个时机带 plan"。设计上是互补的——LLM 看到两条会综合："先简短关心 + 简短点一下 plan"。规则间需要的不是强 disjoint，而是 LLM 能合成的指导面。
- **不让 engagement 同时排除 chatty**：本可以让 engagement-window = wake_back && has_plan && !chatty_active。但那增加耦合度，且实际上"今天聊得不少 + 用户刚回桌 + 有 plan"也可能是合理时机（plan 进展是新话题，不算重复闲聊）。让两条同时活跃，LLM 自己平衡更灵活。
- **复合规则只放需要"两个信号才行"的**：单一信号（has_plan、wake_back）已有自己的规则——只有合成才解锁的指导才放 composite。如果未来要加"3 个信号合成"的规则，仍走 composite helper 同样的模式。
- **fingerprint 用动作短语而非状态短语**：选 "此刻是开新话题的好时机" 作 fingerprint，不选 "复合时机" 之类抽象词。动作短语更难被其他 arm 复制（"好时机"几乎只在 engagement 出现），抽象词容易在通用规则文本里碰到。
- **frontend title "积极开口" 4 字**：和 chatty=今日克制 / pre-quiet=近安静时段 / icebreaker=破冰阶段 等同长度，dict 渲染整齐。"积极"对仗"克制 / 安静 / 破冰"——情绪谱系上明显是另一极，让用户在 panel 一眼看到 prompt 当前是被压制还是被激发。

## Iter 91 设计要点（已实现）
- **fingerprint 而非 length 检查**：本可以只断言 `rules.len() == base + len(labels)`——但那会被"两个 arm 互换"的 bug 蒙混过关（数量不变但文本错位）。fingerprint 表锁定每个 label 的文本特征，要求 arm 内容真实匹配 label 含义，捕获更细致的退化。
- **fingerprint 表的元-元覆盖检查**：测试自己也守门——如果 backend 加 label 但 fingerprint 表没补，`backend_labels.iter().filter(!fingerprint_labels.contains)` 会列出缺失。让测试不能因"作者漏改"而假阳性通过。这是"测试代码本身的可维护性"防线。
- **fingerprint 用最 unique 的 prompt 中文短语**：每个 arm 的 markdown 加粗段（`**...**：` 后内容）天然适合做 fingerprint——句首独特词组，几乎不会与其他 arm 撞。如果将来 prompt 改写，更新 fingerprint 比 wholesale 重写测试简单。
- **"规则文本待补" double assert**：单独检查 fallback 字符串 + 每个 fingerprint 单独检查。两个角度互补：fallback 检查捕获"完全 match 失败"，fingerprint 检查捕获"match 命中错的 arm"。理论上 fingerprint 检查能捕获 fallback 场景（label 无对应 fingerprint → 找不到 → fail），但 explicit 的 fallback 检查 panic message 更直白。
- **三层守护闭合的本质**：backend label → 前端 dict、前端 dict → backend、backend label → proactive_rules arm。任意一对漂移都被覆盖，形成 ABC 三角约束。新加规则的标准流程现在是固定的：(1) backend helper 加 label (2) proactive_rules 加 arm + 测试 fingerprint (3) 前端 dict 加 entry——三步都有 cargo test 守护。
- **不抽 trait/macro**：本可以用宏让"添加规则"成为单一声明。但 8 条规则的当前规模下，三处独立维护比一个庞大的 macro_rules 易读得多。等规则数量超过 20 再考虑。

## Iter 90 设计要点（已实现）
- **共享 parser helper**：Iter 89 用 substring contains，Iter 90 需要枚举 keys——共用一个解析函数让两个测试都看同一个真相。Iter 89 的 contains 模式有 false-positive 风险（label 名字出现在 comment 里），key parse 则严格只承认对象字面量的 key。重构 Iter 89 复用 helper 顺带提升它的严格度。
- **bare key 检测从 indent depend 改为更通用**：原 Iter 89 的 `"\n  plan:"` 模式硬编码两空格 indent。helper 改用 trim + `find(": {")` 模式，缩进无关——TS prettier 配置改成 4 空格也能工作。
- **HashSet 双向比对**：Vec → HashSet 转换 O(n)，N=8 时几乎免费；让 contains 是 O(1) 而不是线性扫，且测试逻辑更可读。
- **三种漂移场景全覆盖**：(a) backend 加 label 忘改 TS、(b) TS 加 ghost key 没 backend 对应、(c) backend 重命名 label。前两种各 fail 一个测试，(c) 因为旧 key 仍在 TS 但 backend 不再产 → 触发 ghost test fail；同时新 label 无翻译 → 触发 coverage test fail。两个测试合在一起捕获全部漂移类型。
- **不是 IndexMap 顺序检查**：本可以也断言 frontend 字典 key 顺序匹配 firing order。但 firing order 是 backend 决策概念，UI 展示顺序是另一种关注（panel 已按 active_prompt_rules 顺序渲染），dict 写入顺序无关紧要。增加这个约束反而限制开发者自由排序字典。
- **panic message 给修复指引**：失败时输出 `"...要么删了，要么补 backend label"` 中文提示——开发者一眼知道两条路径选哪条。"被动写错误信息"也是 API 设计，让测试失败比 silent 更友好。

## Iter 89 设计要点（已实现）
- **跨语言对齐用 Rust test 而非前端 test runner**：项目还没有 vitest / jest 之类前端测试基础设施。引入只为这一个 invariant 不划算。Rust 已有 cargo test 跑得起来，IO + string scan 能覆盖此场景，零新依赖。
- **literal 字符串扫描而非 TS 解析**：tree-sitter / oxc 之类能 robust 解 TS object literal，但是 over-engineering。当前 8 条 label 都是字符串字面量 + kebab-case，contains check 误判概率 ≈ 0。如果未来 label 集合膨胀或者命名碰撞，再升级为 oxc parser 一次性投入。
- **覆盖 quoted 和 bare 两种 JS key 形式**：对象字面量的 key 在合法标识符（`plan`、`icebreaker`、`reminders`、`chatty`）下可省引号；非法标识符（`wake-back` 含 `-`）必须加引号。两个 substring 模式合起来 OR 覆盖。
- **CARGO_MANIFEST_DIR + 相对路径**：`env!("CARGO_MANIFEST_DIR")` 是 cargo test 必有的，避免硬编码绝对路径。`../src/components/panel/PanelDebug.tsx` 跟随当前 monorepo 结构；如果将来重排，测试 panic 信息会显式提示 path 错位。
- **sanity check 字典存在**：如果未来重构把字典名改了或删了，单看"每个 label 是否能找到"会全部找不到，错误信息没头绪。先 assert 字典名出现，让"字典本身没了"和"少几条 label"两种失败模式区分清楚。
- **all-true / extreme inputs 触发全集**：`active_environmental_rule_labels(true,true,true,true,true)` + `active_data_driven_rule_labels(0, 999, 1, 999, 0)` 显式凑参让两边都返完整 label 集。这是测试的"输入选择"——不是 prod 场景，但 prod 也不会一次性触发全部 8 条；测试目的是覆盖 label 全集。
- **panic 信息列举 missing labels**：`assert!(missing.is_empty(), "missing: {:?}", missing)`——开发者看到失败信息直接知道要加哪几行 dict entry。比 `assert_eq!(left.len(), right.len())` 那种数字断言对调试友好得多。
- **不让测试自己修复**：测试只检测，不自动给 PanelDebug.tsx 写默认 entry。失败时让 dev 显式做"加 title + summary 中文"的本地化决策——title/summary 文本是设计选择，不是机械填充。

## Iter 88 设计要点（已实现）
- **summary 字典在前端而非 backend**：原 TODO 提议 backend 返 `Vec<{label, summary}>`。但 summary 是面向用户的中文 UI 文案，应当和其他 UI 文案一起在前端维护——backend 关心"哪些规则活跃"（数据），UI 关心"怎么呈现"（文案）。分层清晰。如果将来想多语言，前端字典可改成 `Record<string, {title_zh, title_en, summary_zh, summary_en}>` 不动 backend。
- **fallback 路径明确**：lookup 失败显示 `(label "xxx" 暂无中文描述)`，让用户立刻知道"哪个 label 在 backend 出现但前端字典没补"——而不是让缺失静默成空字符串。和 backend 的 `(规则文本待补)` fallback 同理：缺失要可见。
- **button 而非 span+onClick**：button 自带 keyboard accessibility（tab + enter/space）。默认 button 在 chrome 上有边框/背景，得 `border: none` reset。`background-color` 切换替代 `:active` 状态——不写 CSS-in-JS pseudo-class，简单 stateful 颜色就够了。
- **▾/▸ chevron 而非 +/-**：chevron 三角更直观表达"列表展开方向"，加号减号在中文界面里更像加减运算。`fontSize: 9px` + `opacity: 0.85` 让 chevron 比标签略小不抢视觉。
- **default collapsed**：badge 默认是收起状态。新装用户看到 "prompt: N 条 hint" 时不会被一堆中文规则块淹——好奇了再点开。已经有的 hover tooltip 还在，给只想 quick-glance 的用户。
- **不持久化展开状态**：不存到 localStorage 等。展开是临时审视的"我现在想看"动作，不是配置；下次打开 panel 默认收起最干净。
- **84px title 列宽**：5 个汉字 ≈ 80px (12px font × 1.3 字宽 + buffer)。固定宽度让所有标题左对齐成一列，summary 也同位置开始读，扫读流畅。
- **背景 #faf5ff 浅紫**：和 badge 紫色同 family 但极淡，视觉上"badge 拉出了一个紫色区域"。如果用 #f8fafc 中性灰，badge 和展开区色调断裂。

## Iter 87 设计要点（已实现）
- **label-driven 而非 condition-driven**：原 `proactive_rules` 把"是否触发"和"触发后说什么"绑在一起。拆开后 helper 负责前者（"哪些 label 活跃"），proactive_rules 负责后者（"label 翻译成什么规则文本"）。两份职责各司其职。
- **保留 unknown fallback 而非 panic**：`match *label { ... other => format!(..."规则文本待补"...)}` 让"label 加了但翻译没加"成为可见但非致命的 bug。`(规则文本待补)` 字符串明显异于正常规则，测试 + 实机日志都能捕获。如果 panic，prompt 就构造失败，宠物彻底沉默——比展示降级文本糟糕。
- **测试用 strings.contains() 仍稳定**：`proactive_rules` 重构 push 顺序 / 措辞都没变（match arm 直接拷贝原 if-block 的字符串）。所以 18+ 个 contains 测试零修改通过，是好的"无关行为不变"信号。
- **for chain(env, data) 顺序锁定 firing 顺序**：env 在 data 之前。如果未来想插入"between env and data"的新分类，要么在某 helper 里加新 label，要么在 chain 里加第三个 helper——结构清晰可扩展。
- **`for label in env_labels.iter().chain(data_labels.iter())`**：iter 取 `&&str`，`*label` 解到 `&str` 给 match。`*label` 看似多余但 match arm 用字符串字面量比较时 `&str == &&str` 不行，得 deref 一次。
- **不抽 `(label, format_args)` 表驱动**：理论上可以 `[("icebreaker", |inputs| format!(...)), ...]` 全表存储。但每条规则的 format 参数不同（icebreaker 只用 history_count，chatty 用 today_count + SILENT_MARKER），强行抽闭包表反而更复杂。match 直白足够。
- **5 always-on rules 保留 push**：本可以也走 helper 模式（"always" 总返"always"label），但那 5 条永远触发，没数据驱动条件，跑 helper 是空操作。直接 push 简单。

## Iter 86 设计要点（已实现）
- **拆两个 helper 而非一个胖函数**：本可以让 active_data_driven_rule_labels 接 10 个参数同时返 8 个 label。但 data-driven（依赖 counter / 历史）和 environmental（依赖瞬时 state）是两类信号——拆开后调用者能根据需要单独使用，比如未来"只统计 prompt 里的纠偏规则数量"还能直接用 data_driven helper，不用再切片。
- **chain + collect 而非 mut Vec push**：`env.iter().chain(data.iter()).copied().collect()` 一行表达组合意图。两边都是 Vec<&'static str>，链式拼接零拷贝直到 collect。
- **wake_back 从 wake_ago<=600 派生**：阈值 600 秒（10 分钟）是 wake_hint 构造的硬编码值。本可以提取常量，但 wake_hint 在 run_proactive_turn 里也只用一次——重复 600 在两处比抽常量+import 简单。如果将来要再用第三处再考虑提取。
- **first_mood = mood_text empty/None**：mood_text 在 ToneSnapshot 里已是 `Option<String>`。`map(empty).unwrap_or(true)` 一行覆盖两种 first_mood 情况：从未写过 (None) 或文本为空。
- **reminders / plan 走 build_xxx_hint 而非 memory_list 直接读**：build_xxx_hint 已经包含解析 + 过滤过期 + 构造文本逻辑。把"是否非空"作为"规则是否会触发"的代理，逻辑和 proactive_rules 严格对齐——避免重复实现导致漂移。代价是这两次调用每次 panel poll（1Hz）都跑——memory IO 但 yaml 文件极小，可忽略。
- **dispatch 重新计算 mood/wake/reminders/plan 而非传 ToneSnapshot 进来**：dispatch 早于 get_tone_snapshot 被调用（不同 entry path），共享一个"已计算的 ToneSnapshot"会需要不小的耦合。重新算的成本和单次 ToneSnapshot 一样，可接受。
- **保持 firing 顺序的设计**：proactive_rules 内是 wake → first_mood → pre_quiet → reminders → plan → icebreaker → chatty → env-awareness。labels 顺序严格匹配。如果将来加新规则到 proactive_rules，更新对应 helper 的 push 顺序，单测会捕获漂移。
- **未来想加 settings 控制 badge 显隐**：现在 8 条全显示——可能让"prompt: 5 条 hint"过于频繁出现。如果用户感觉吵，可以加 `panel.show_prompt_rules_badge_threshold`（默认 1，调高让 badge 只在更多规则时出现）。先看实际使用感受再决定。

## Iter 85 设计要点（已实现）
- **dispatch 时一次性算 labels，所有 push 复用**：active_data_driven_rule_labels 调用时机有两个候选：dispatch 前（与 prompt 实际计算同步） 或在 run_proactive_turn 里返回。前者意味着 dispatch 自己读 atomic + speech_count；后者要把 labels 加进 ProactiveTurnOutcome。选 dispatch 时算的好处：Skip / Silent / Run / outcome 全分支统一，不依赖 LLM 是否被调用。
- **append_tag 内联函数处理 "-" 占位**：reason 起始要么是 "chatty=..." 要么是 "-"。直接 `push_str(", rules=...")` 会得到 `"-, rules=icebreaker"` 不优雅。append_tag 检查若仍是 "-" 占位就先 clear——结果变成 "rules=icebreaker"。前端 strip "-, " 已经能 backwards-compat 处理两种格式。
- **"rules=" 顺序在 chatty 之后、tools 之前**：Spoke reason 形如 `chatty=N/M, rules=A+B, tools=X+Y`。chatty 是状态指标（"我现在多忙"），rules 是 prompt 规则集（"我现在受多少 hint 影响"），tools 是结果（"LLM 用了哪些"）。三者从输入→规则→输出递进，读起来像一条因果链。
- **labels 来自 active_data_driven_rule_labels 而非重复条件**：单一事实源——同一函数同时给 ToneSnapshot.active_prompt_rules 和 decision log 用。如果将来加新规则到该函数，两处自动同步。原本就是 Iter 84 抽出来这个 helper 的目的。
- **LlmError 把 tag 塞括号里**：现有格式 `format!("{} ({})", e, chatty_part)`——保留这个"错误信息 + 上下文"形态，只在括号内累加 tag。前端 localizeReason 对 LlmError 的 case 没改，因为它就是简单 passthrough "LLM 调用失败：${reason}"——括号里的 chatty/rules tag 自然在 reason 字符串里展示。
- **不在 Skip/Silent 里加 rules tag**：理论上 gate 拦截时 prompt 也"会"是这些 rules，但 LLM 没看到。把 tag 限定在"prompt 实际生效过"的事件（Run + outcome）更准确。Silent 进的 reason 是 gate 名字（"disabled" / "quiet_hours"），加上 rules tag 反而混淆——那次 prompt 根本没构造。
- **panel 不动**：localizeReason 已经处理 reason 字符串中可能存在多个 tag 的情况（用 strip + display 模式）。新增 rules tag 自然 fall through 到"宠物开口（...）"里展示完整字符串，无需特殊代码。

## Iter 84 设计要点（已实现）
- **只统计 data-driven 规则**：原 TODO 措辞是"任意 prompt 自动纠偏规则正在触发"。但实际 proactive_rules 有 8 条 contextual rule，前 5 条（wake/first_mood/pre_quiet/reminders/plan）是环境/状态触发——panel 已用 chip 展示对应输入。再用一个 badge 重复计数会让它和已有 UI 冗余冲突。后 3 条（icebreaker/chatty/env-awareness）才是基于聚合数据驱动 prompt 的，badge 单专门体现这层。
- **labels 返回 Vec<&'static str> 而非 Vec<String>**：所有标签是编译期常量。`&'static str` 零分配，调用者才转 String 入 ToneSnapshot（serde 需 owned）。多余拷贝最少。
- **labels 顺序匹配 firing 顺序**：proactive_rules 里 icebreaker 先 push 然后 chatty 然后 env-awareness。labels 函数严格同序。如果未来新增规则，单测 `combine_in_firing_order` 会捕获顺序漂移。
- **get_tone_snapshot 加 ProcessCountersStore state**：本可以让前端拿 env_tool stats 后自己派生标签。但派生逻辑（threshold、min_samples）藏在 backend，复制到 frontend 会破坏单一事实源。让 backend 一次算清楚 ship over 即可。一次额外 atomic load + 一次 today_speech_count IO，几乎零成本。
- **紫色 pill 而非小数字 chip**：badge 是身份标识"prompt 现在不在 default 状态"，区别于 cache/tag/silence 等数据 chip（mono 字 + 数字）。pill 形状 + 紫色（与已有"克制模式"badge 同 family，但更亮 #7c3aed 标识 prompt 层）让它在工具栏 visually 跳出。
- **空时不渲染，零干扰**：neutral state 工具栏不出现 badge。新装 + 用过几次的用户从不出现 → 突然出现说明"prompt 被多个规则影响了"，本身就是有用信号。如果常驻显示 "prompt: 0" 反而成为视觉噪声。
- **不直接显示完整规则文本**：tooltip 只列规则名（短），不复述 prompt 文本。规则名足够用户理解发生什么；要看完整 prompt 用户可以在 panel 别处或日志里翻——badge 是 dashboard 不是 inspector。

## Iter 83 设计要点（已实现）
- **数据闭环**：Iter 80（LLM沉默率）→ Iter 81（tool tags）→ Iter 82（聚合 atomic）→ Iter 83（数据回流 prompt）。这是连续 4 次小迭代的连贯方向：先给 LLM 行为打标，再聚合数据，再用数据自动改 prompt。每步都可独立 ship + 验证，避免一次性大改。
- **整数比较 `with_any * 100 < 30 * total` 而非浮点除法**：避免 `f64` 精度边界。100% 准确：3/10 = 30%（>= 30%，不触发）；2/10 = 20%（< 30%，触发）。如果用 `(with_any as f64 / total as f64) < 0.3` 在某些数字下因浮点表示可能产生意外。
- **min_samples 防早期噪声**：Spoke 计数从 0 开始，前几次结果方差大。比如刚启动 1/2（50%）和 0/3（0%）都是少样本噪声。10 是经验门槛——足够 stable 又不至于让纠偏永远不触发。
- **不持久化 env_tool counters → 规则会自动愈合**：Iter 82 的 atomic 是 process-level，重启清零。这给纠偏规则一个自然冷却：用户调好 prompt 后重启或手动重置统计，规则需要重新积累 10 次才再次触发。如果新 prompt 真的有效，环境感知率上去了，规则永远不再触发；如果还差，又能稳定回归。
- **不在规则里直接说"低于 30%"模糊化**：把 "12 次 / 2 次 / < 30%" 三个具体数字都塞进规则文本。LLM 看到"12 次只有 2 次调用"远比"较少"更具体——格式锚定让 prompt 更难被忽略。
- **建议 `get_active_window` 而非 4 个工具混合**：4 个 env 工具里 active_window 是最 universally 有用的（任何场景都能拿到信息），weather/events 在凌晨/无日程时会返空。规则里建议单一具体工具好执行；如果列出 4 个让 LLM 自由选，反而容易选最便宜的（=不调用）。
- **不用 settings 暴露阈值**：本可以让 30% 和 10 走 settings.proactive。但这俩参数都是 prompt 调优内部决策，不是用户偏好——普通用户看不出差别。如果未来发现需要按用户场景调（如弱网用户希望降阈值减少 IO），再升级到 settings。
- **新规则放最后**：rules 顺序 = LLM 阅读顺序。新规则放整个块末尾不打断已有的"silence/speak/single-line/tool-mention"基调；它是"额外纠偏"性质，最后看一眼最自然。

## Iter 82 设计要点（已实现）
- **`record_spoke` 集中 match 而非分散写**：本可以让调度处自己写 `for tool in tools { match tool ... bump }`。但工具白名单（4 项）是 EnvToolCounters 的关注域，未来加 `get_now_playing` 类工具，"哪些算 env-aware"应该和数据结构在同一处定义。把 match 放进 impl 让调用处一行：`record_spoke(&tools)`。
- **per-tool 字段而非 HashMap**：本可以 `tool_counts: HashMap<String, AtomicU64>` 通用化。但 4 项固定 + 不预期高频增长，atomic struct 字段的访问 O(1) 且无锁；HashMap 要 lock 或 dashmap，复杂度反而高。如果将来 env 工具增加到 10+ 项再考虑容器化。
- **spoke_total != llm_outcome.spoke**：两个计数在不同地方累计但应当同步。spoke_total 在 `record_spoke` 内 +1，llm_outcome.spoke 在 dispatch 同分支 +1——两者放紧邻代码块互为校验。如果未来重构破坏对齐，panel 上的两个 chip 比例对不上会是肉眼可见的回归信号。
- **50% 临界点**：与 LLM 沉默 chip 对称的"半数门槛"。低于 50% 表示"大多数开口都没看环境就说话"——prompt 工具引导没起作用。本可以用更严格的 30% 或更宽松的 70%，但 50% 是直观的"主流 vs 少数"分界。
- **不持久化**：env_tool 是 process-level atomic，重启清零。和 cache / mood_tag / llm_outcome 一致，是 session 内 prompt 调试的快速反馈。如果将来要看"上周环境感知率"，可以加 daily 文件，但当前需求是即时调优。
- **不加 spoke_no_tools 字段**：派生为 `spoke_total - spoke_with_any` 在前端就行；序列化时只送原子值，前端做减法。避免冗余字段、避免一致性校验负担。
- **chip 渲染条件 `spoke_total > 0`**：和其他 chip 一样首次启动不渲染，避免显示 "0/0" 除零。`Math.round(... * 100)` 在 frontend 也保护 NaN 不出现，因为分支已判过非零。

## Iter 81 设计要点（已实现）
- **opt-in collector via ToolContext 而非改 run_chat_pipeline 签名**：4 个 callers（chat / proactive / consolidate / telegram）只有 proactive 需要 tool tags。改返回类型迫使所有 caller 解构 `(reply, tools)` 或 ignore；通过 ctx 加 optional collector 让不关心的 caller 零改动。这是"添字段不破坏既有调用者"的标准模式。
- **mutex 而非 atomic / channel**：tool names 是 `Vec<String>`，原子追加需要 lock-free queue（复杂）或 channel（异步生命周期繁琐）。`Arc<Mutex<Vec<String>>>` 同步锁简单可靠，pipeline 末尾一次性写入，dispatch 读一次——锁竞争不可能成为瓶颈。
- **registry 自己也持有 called_tools (TokioMutex)**：本可以让 execute() 直接写到 ctx.tools_used。但 registry 自己有这个数据更内聚——未来其他 caller 想拿 tool names（例如统计页 "本会话用过哪些工具"）能直接 `registry.called_tool_names()` 而不需要 ctx 介入。pipeline 末尾的拷贝是显式的"出口"。
- **cache hit 也算 called**：意图角度：LLM 想着调 tool X（哪怕命中缓存），算"它用了 X"。如果只算 miss，cache 优化越狠 tool tag 越假——明明 LLM 调了 3 次 weather 都用，tag 缺。本目标是 prompt 调试反馈，关注 LLM 心智模型而不是 IO 实际数。
- **sort+dedup 在 read 而非 write**：每次 push 不去重，读时 sort+dedup。优势：write 路径零成本（无锁内查找）；劣势：内存稍多但同一 turn 工具调用很少（< 10），可忽略。
- **partial/error 路径不写 collector**：tool collector 在 pipeline final response 分支才 populate。如果 turn 中途 fetch 失败、loop 中断、或被 cancel，collector 保持空。这避免了"看到 tools 但没看到 reply"的迷之状态——always-correlated。
- **ProactiveTurnOutcome struct 而非 tuple**：`Result<(Option<String>, Vec<String>), _>` 也能用，但带名字的 struct 让 caller 写 `outcome.reply` / `outcome.tools` 自描述，未来加第三个字段（如 `tokens_used: u64`）零摩擦。
- **`tools=X+Y` 编码而非 JSON 数组**：decision log 是字符串域，纯字符串拼接最简单。`+` 是分隔符避免和工具名内含的 `_` 冲突。前端读到后无需解析，直接拼进展示文案。如果未来想结构化（带 latency 等），再 split。

## Iter 80 设计要点（已实现）
- **复用 ProcessCounters container pattern**：第 3 个 sub-struct（cache/mood_tag/llm_outcome）用同一模式：AtomicU64 + 工厂 + Stats serde 结构 + get_/reset_ 命令对。每加一个新指标，机械地复制粘贴改名字即可。这种"同形复制"反而比抽象出 trait 更易读——具体类型里能直接看到字段含义。
- **bump 在 dispatch 处，不在 run_proactive_turn 内部**：`run_proactive_turn` 不知道 ProcessCounters 的存在（它接 AppHandle 但通过 state 访问也行）。但放在 dispatch 处的好处是：它已经在做完全相同的 outcome 分类（`Ok(Some) / Ok(None) / Err`）来 push decision log；同位置 fetch_add 让 atomic 和 decision log 永不分叉，未来想加新 outcome 状态（如 `LlmTimeout`）一处改全到位。
- **沉默率而非开口率**：UI 显示 "LLM沉默 N/M" 而非 "LLM开口 N/M"。两者数学等价但语义重点不同：用户关心的是"为什么这么沉默"——沉默是异常事件，开口是默认期望。沉默数字直接出现在 chip 上比 100% - 开口率% 心理算账少一步。
- **临界点切橙色 = silent+error > spoke**：即沉默和失败合起来超过开口数。这是个朴素阈值（50%）。本可以用 30% 或 60%，但 50% 是"已经不正常"的最直白门槛——一半以上的 LLM 调用没换来对话，prompt 必然有问题。
- **error 也算入沉默率分母**：error 是 LLM 调用失败（网络/API），技术上不是"主动沉默"。但用户视角"宠物没说话"——失败和沉默都是同样后果。把 error 算进总数让 chip 不需要分两个比例。
- **chip 仅在 total > 0 才渲染**：与 Cache / Tag chip 同策略。否则首次启动每个 chip 都显示 0/0% 拥挤工具栏；发生过即出现，零次时藏起来。
- **不持久化跨重启**：与 cache/mood_tag 一致，atomic 是 process-wide。如果用户重启就清零；这不是 lifetime stat（那个 speech_count.txt 是文件持久），是 session 内的 prompt 调试反馈。重启清零等于"开始新 session 看 prompt 现在表现如何"，符合调优场景。

## Iter 79 设计要点（已实现）
- **bump 而非按 kind 归档**：本可以让 Run + outcome 合并成一个 entry（用 `outcome: Option<String>` 字段后填）。但那破坏了 Iter 78 的"Record before dispatching"时序——in-flight 的 Run 对 panel 不可见。简单 bump CAPACITY 是对 Iter 78 设计的最小妥协。
- **16 而非 20**：每 Run 占 2 行，10 → 16 给出 8 完整 cycle。20 给 10 完整 cycle 也行，但 panel 即使升 maxHeight 也不要"决策列表喧宾夺主"——它是"为什么沉默"的辅助信息，主屏要留给 toolbar/stats/tone strip。
- **U+2514 而非缩进 padding**：本可以给 outcome 行加 `paddingLeft`。但缩进对 mono 字体的对齐感不好，时间戳也跟着错位。`└ ` 是 mono 字符占 1 列，时间戳列宽度不变，连接关系靠字符语义而不是位置——更稳。
- **maxHeight 120 → 200**：粗算每行 ~17px，120 显 ~7 行，200 显 ~12 行；新 cap 16 仍可能偶尔触发滚动（事件爆发期），但 200 足够覆盖正常使用。再大就开始挤压下面的 stats/reminders 区。
- **测试不动**：3 个 decision_log 测试都通过 `CAPACITY` 常量参数化。我故意不去硬编码 10/16，让 cap 调整时测试零成本跟随。这是设计的好处之一。

## Iter 78 设计要点（已实现）
- **post-LLM 第二条决策而非塞进 Run**：本可以延迟 Run push 到 LLM 返回后，把 idle+chatty+outcome 拼成一条。但那破坏了"决策记录在 dispatch 前完成"的现有模式（注释明确说"Record before dispatching"），且会让 panel 看不到正在等 LLM 返回的 in-flight Run。改成两条独立 push 保留时序信号——用户能看到 Run 触发时间和 outcome 时间分别（隐含 LLM 用了多久）。
- **CAPACITY=10 → 一次 gate 通过吃 2 行**：从 1 行涨到 2 行意味着可见决策窗口从 10 次 gate 评估变成 ~6.5 次。10 已经是 ring buffer cap，不会无限增长；6.5 次 gate 评估的窗口对调试而言够用（默认 5 分钟一次评估即覆盖最近半小时）。如果将来发现窗口不够再调 CAPACITY。
- **chatty_mode_tag 抽成 pub fn**：本可以 inline 在 dispatch 里。但纯函数 Option 返回值清晰、好测，且未来如果 prompt 还有别的"软规则触发标签"（icebreaker / pre-quiet）想往 decision log 走，可以仿照模式扩展。
- **"-" 占位 vs 不传 reason**：post-LLM push 的 reason 从来不为空——非活跃时填 `"-"` 单字符。这样前端 localizeReason 永远不需要判 empty，逻辑两支：`reason === "-"` vs 含 chatty 字符串。如果用空串前端要再判 `!reason || reason.trim() === ""` 啰嗦。
- **三色 outcome**：`Spoke=#16a34a 深绿`（Run=#22c55e 浅绿的"成熟版"，表示已开口落地）/ `LlmSilent=#a855f7 紫`（mood/motion 配色家族，与紫色 motion chip 暗示"内部状态"概念）/ `LlmError=#dc2626 红`（与 quiet-soon 共用红色，表示"异常需注意"）。三色和已有 Run/Skip/Silent 都不冲突。
- **不破坏现有 Skip/Silent kind**：所有 gate 拦截的 kind 名字保持不变（disabled/quiet_hours/awaiting/cooldown/macOS Focus/idle_below_threshold），只是在 dispatched Run 后增加 LLM-outcome 行。前端 localizeReason 老 case 不动，新 if-block 在 Skip 之前加一组三个 kind。

## Iter 77 设计要点（已实现）
- **复用 ToneSnapshot 而非新加 command**：本可以加 `get_chatty_day_threshold` 单独命令。但 ToneSnapshot 已经在 fetchLogs 的 Promise.all 里被调，加字段几乎零成本。设计哲学是 "信号同源"：宠物决策依赖的全部信号 → 一个 snapshot → 同时给 LLM(prompt) 和给用户(panel)，两边视图永不分叉。
- **派生在前端而非后端**：本可以在后端算 `restraining: bool` 直接给前端。但前端要 threshold 数字本身（写进 hover 文案 "已超过 5"），所以 raw threshold 必须传。既然传了就 derive 在前端——后端不背业务展示概念。
- **互斥而非叠加显示**：原来"破冰阶段"+"今日聊得多"理论上可以同时存在（破冰期一天突然爆出一堆主动开口），但破冰是 `lifetime < 3` 维度，克制是 `today >= threshold` 维度，两个 badge 同时挂在右边视觉拥挤。约定优先克制（更紧迫，行为正在被改），破冰让位。
- **pill 形而非纯色文字**：克制模式比破冰更 actionable（用户看到马上理解"我可以去 settings 调"），用 pill + 边框让它更接近"提示框"质感而不是普通 label。色用 #ea580c（橙）+ #fff7ed（浅橙背景）+ #fed7aa（中橙边）三色构成，是 tailwind orange-500/50/200 标准搭配，确保在白底上对比度过关。
- **fallback=5 与 run_proactive_turn 同步**：两处必须一致——否则 panel 显示 "克制模式" 但 LLM 实际没看到该规则（或反之）。设了硬编码 5 两处都是同一个数字，未来如果改默认值要两处一起改。可以提取共享常量但目前只两处复制成本低、明显，留着不重构。
- **IIFE pattern**：`{(() => { ... })()}` 直接在 JSX 里跑函数派生本地状态。本可以提取成 `restraining` const 在组件 body，但那个值只在卡片 JSX 里用一次，IIFE 把作用域局部化更紧凑——读者看 JSX 就能找到逻辑。

## Iter 76 设计要点（已实现）
- **从 const 升级到 settings 字段**：Iter 75 的 `CHATTY_DAY_THRESHOLD = 5` 是占位常量，标记位置。Iter 76 拆出来后 const 完全删除——不留死代码。Threshold 现在两路：用户从 panel 改 / serde default 兜底。
- **0 显式关闭语义**：`threshold > 0 && today_count >= threshold` 而非简单 `today_count >= threshold`。如果只走第二个，当 threshold=0 时 today=0 也会触发——首句话就被骂"今天已经聊了 0 次"，nonsense。0 表示"我永远不希望这个克制规则触发"，明确写进 PromptInputs 的 doc。
- **default 5 vs 取消默认**：本可以让 default = 0（默认关闭新规则）。但这破坏了 Iter 75 已经在跑的行为。保持 5 = "新功能默认开启与上版本一致"，用户嫌啰嗦再去 panel 调高或调 0。
- **测试改用 inputs 字段而非读 const**：原 chatty_day 测试 `inputs.today_speech_count = CHATTY_DAY_THRESHOLD` 直接捏死了常量值。改成 `inputs.chatty_day_threshold` 后测试是 self-contained——不受未来默认值变化影响。
- **PanelNumberField 单独一行**：本可以塞在 quiet_hours 那行的 grid 里。但 quiet_hours 是"硬性时间窗"，chatty_day 是"软性数量阈值"，语义不同；放新行下方避免视觉混淆，且 label 文案较长（"今天主动开口达到此数后变克制（0 = 关闭）"），grid cell 装不下。
- **fallback 在 run_proactive_turn 而非 trigger 全链**：`get_settings().ok().map(...).unwrap_or(5)`。如果 settings 读失败（极罕见，通常是文件锁竞争或权限），fallback 到 production default 5；不要 `unwrap_or(0)`（默默关闭规则）也不 panic。

## Iter 75 设计要点（已实现）
- **跳过 Iter 74 的视觉扩展先做行为**：Iter 74 是 stats 卡的"本周/sparkline"——纯视觉，对宠物行为零影响。Iter 75 让今日数据真正改变 prompt，把 Iter 73 的数据基础变成可观察行为。先做有行为副作用的迭代，视觉迭代留到 todo 末尾。
- **阈值 5 而非 3 或 10**：3 太低（用户正常一天主动响应几条都可能触发，让宠物从中午就开始装哑），10 太高（每日很少能到）。5 是观察值——typical session 在 idle 周期 + 各种 gates 之后大约会主动开 2-4 次，5 代表"今天异常活跃"的软退避点。如果将来有用户行为数据可以再调。
- **建议保持安静而非硬封闭**：本可以在 backend 直接 gate（`today_count >= N` 就跳过 LLM 调用）。但这会出错——如果今天恰好有用户刚醒来的窗口/到期提醒，LLM 自己有上下文判断该不该说，比 gate 拍死强。所以走"prompt 软规则"路线，把决定权给 LLM 但写明"除非有真正值得说的新信号才开口"。
- **借用现有 SILENT_MARKER**：规则里直接 `format!` 拼出当前的 silent marker 而不是再造一套机制。如果将来 marker 改了，这条规则自动跟随，零维护。
- **数字塞进规则文本**：和 icebreaker 规则同样设计——不写"已经很多次"模糊形容，而是 `今天已经主动开过 N 次口了`。LLM 看到 5 vs 8 vs 15 会有不同力度的克制，模糊形容会被一刀切。
- **不在 PromptInputs 加 `is_chatty_day: bool` 派生字段**：本可以在 `PromptInputs` 加个 bool 让 rules 直接读，但那等于把阈值逻辑重复在两处。直接把原始 count 传给 rules，rules 自己 `>= CHATTY_DAY_THRESHOLD` 比较——单一事实源，阈值改动一处生效。

## Iter 73 设计要点（已实现）
- **JSON map 而非 SQLite/CSV**：当前需求是单天 key→count 反查，JSON map O(1)；如果未来要带"分小时"、"分类型"或多列查询，再迁 SQLite。CSV 必须扫全文件，对 90 天数据其实差不多但缺省可读性。serde_json 已有依赖，零成本。
- **BTreeMap 而非 HashMap**：序列化输出按 key 排序后 file diff 友好（手动看 `~/.config/pet/speech_daily.json` 也按日期排序）。读 path 几乎不影响性能，90 行数据级别可忽略。
- **prune 策略：lex-compare YYYY-MM-DD**：本可以 parse 每个 key 成 NaiveDate 再比较 Duration。但 ISO 日期字符串字典序 == 日期序，直接 `k.as_str() >= cutoff_str.as_str()` 一行搞定，避免每个 key 走 chrono parse。non-parseable key 保留是显式选择——不是这模块写的就别动它。
- **三段 best-effort IO**：speech_history.log 写完 → lifetime bump → today bump。lifetime 失败不影响 today，反之亦然。理论上可以并行 join，但顺序 IO 保证: 用户看 panel 时若 today 比 lifetime 多代表"刚刚 bump 完 today 还没回到 lifetime"——窗口极短，比并行带来的写竞争可控。
- **DAILY_RETAIN_DAYS = 90**：超出当前面板需要（只用 today 1 项），但写入端 prune 一次定下来 retention 上限，未来面板要"过去 30 天 sparkline"或"周报"时不用追溯写时机。90 行 JSON ≈ 2KB，零成本。
- **20px/28px 大小对比**：数字读者最关心"今天 vs 总累计"，"今天"是 daily refresh 的瞬时态，"累计"才是 brand-defining 长期 number。对比 8px 大小让累计仍然主导视觉但今日清晰可读。
- **测试只覆盖纯函数**：bump_today_count / today_speech_count 是 IO，依赖系统时区和 config_dir，难脱离副作用。parse_daily 和 prune_daily 是 pure，5 个单测覆盖 empty/malformed/valid/cutoff 边界/non-parseable key 保留/retain=0 含义；这是最小集合让设计意图被钉死。

## Iter 72 设计要点（已实现）
- **薄封装而非 reuse get_tone_snapshot**：`get_tone_snapshot` 已经返了 proactive_count，理论上前端可以直接 `tone?.proactive_count` 渲染大数字。但这绑定了"想看累计数 = 必须先拿到完整 ToneSnapshot"——一旦未来 ToneSnapshot 加重字段、或者出现想脱离 tone 单独看 stats 的场景（如启动画面、Telegram /stats 命令），就要重新拆。先做单值 command 让职责清楚。
- **大卡片 + chip 双重显示，不删 chip**：表面上看冗余，但读者场景不同。chip 在 tone strip（`fontSize: 11px` 条带）里是"我顺便扫一眼当前所有信号"——大数字混进去会被周边压扁。大卡片放工具栏正下方是"我点开 panel 第一眼想知道宠物存在感"。两者都保留实际花费几乎是 0（多渲染一行），收益是不同心理路径都得满足。
- **背景渐变而非纯色**：panel 里其他段都是 `#fdf4ff`/`#f1f5f9`/`#fff7ed` 等单色背景区分用途。这个 stats 块要"亮眼"但不喧宾夺主，用一道淡紫到淡蓝 135° 渐变就够 weight 了——和紫色数字呼应又不挤压可读性。
- **破冰期标签靠右浮动**：`marginLeft: "auto"` 把"破冰阶段"推到右边。如果直接放数字旁边，0/1/2 三个数字时整行会变成"0 次主动开口（破冰阶段）（持久累计...）"——括号嵌套读着累。靠右单独一格让它像个 badge。
- **不为这个加单测**：纯透传命令 + 前端 state 串联。已有 `lifetime_speech_count` 测试 + cargo type check + tsc 兜底。

## Iter 71 设计要点（已实现）
- **从 A 反转到 B**：Iter 70 选了 frontend-only 截断指示（A 方案，"50+"），Iter 71 走的是当时被推迟的 B 方案——独立持久 counter。两次决定不矛盾：A 适合验证用户在不在乎；现在已上线第三轮 panel chip 演化（69 加 chip → 70 加饱和提示 → 71 升级为真实累计），把"长跑用户能看到精确数"这个底层能力补全。
- **文件 sidecar 而非 ProcessCounters atomic**：counter 必须跨重启活下来，否则用户每次开机都看到从 0 涨——比 50+ 还差。ProcessCounters 是进程内 State，重启清零，不达标。文件方案虽然不优雅但和 speech_history.log 同位置同 IO 模式，复杂度增量最小。如果将来加更多持久 counter，可以一次把它们迁到一个 .json 或 sqlite。
- **bootstrap from count_speeches**：升级用户首次启动时 sidecar 不存在，回退读 speech_history.log 行数。意味着从 Iter 70 升级上来的用户能继续在原有 ≤50 基础上累计，而不是从 0 开始。第一次 bump 后 sidecar 永远存在，bootstrap 只生效一次——零长期开销。
- **bump 在 record_speech_inner 末尾且 best-effort**：失败不影响 speech 写入。两条 IO 顺序是先写 speech_history（用户期望持久），再写 count（衍生信息）；若 count 写失败，下次启动 bootstrap 会重新对齐到 speech_history 行数（≤50 时仍准确，>50 时差额永远丢失但不致命）。
- **删 proactive_count_capped 而非保留**：lifetime counter 不会饱和，flag 永远 false——前端死代码 + 多一个序列化字段。直接删 ToneSnapshot 字段 + interface 字段 + 渲染分支，比留着更干净。tooltip 改写为"持久化在 speech_count.txt"让用户知道这是真实累计，不需要解释饱和。
- **不加新单测**：纯 IO 代码（read/write 单整数），逻辑量极少；rust 类型系统 + tsc + 既有 speech_history 测试 + cargo check 兜底。如果将来 bootstrap 或 bump 出 bug，加一个临时文件 round-trip 测试即可。

## Iter 70 设计要点（已实现）
- **A vs B 选 A**：(A) `50+` 显示是 frontend-only 提示截断，零额外 state；(B) 独立 atomic 是 source-of-truth fix，多一组持久化考虑（不持久化重启就清零，反而误导）。当前用户最可能在前几次破冰阶段就放下来或换设备，长跑用户的精确累计需求弱。简单版本足够。如果将来真有人想用 lifetime stats 当成就，再走 B。
- **bool 而非 sentinel value**：本可以让 backend 返 `Option<u64>` (None=capped) 或负数。但 `proactive_count_capped: bool` 字段名自解释；前端 `count + (capped ? "+" : "")` 一行渲染干净。
- **SPEECH_HISTORY_CAP 升 pub**：原是 file-private const。现要 ToneSnapshot 比较，最低权限升级——只让模块内 const 公开为模块级 pub，不引新接口。
- **`>= cap` 而非 `== cap`**：理论上 trim 保持 ≤ cap，但用 `>=` 是防御性写法——如果 future 改了 trim 逻辑（如不那么严格），这个比较仍然正确。
- **tooltip 三档**：< 3（破冰）/ capped（饱和说明）/ 默认（基于 log）。每档用户场景不同，文案对应。
- **不写新单测**：本质是 bool 派生 + ToneSnapshot 字段添加 + 前端渲染条件。runtime 行为靠现有 count_speeches 测试 + cargo type check 兜底。

## Iter 69 设计要点（已实现）
- **chip 在 count=0 也渲染**：其他 chip（cache/tag/wake/mood）都用「值不存在/为零就藏起来」逻辑。但破冰阶段的核心展示就是 "0次/1次/2次"——藏起来反而失去信号。所以 proactive chip 无条件渲染，只是颜色随破冰状态变化。
- **琥珀色（#d97706）作 warning 而非 alert**：red 给 quiet-soon、green 给 trigger 成功、紫 给 mood/tag——琥珀是 "soft warning / heads-up"。"破冰阶段"不是问题，只是个状态告知，琥珀比红色合适。
- **🤝 emoji 选择**：握手 = 初识 / 介绍。和其他 emoji 一样作 type discriminator——time/cadence/wake/motion/mood/quiet/handshake，都不重复。
- **不在破冰外完全隐藏**：count > 3 时仍然显示「已开口 N 次」（灰色），让用户随时能看到累计——是 lifetime stat，不是临时态。
- **50 行 cap 的暴露问题**：count_speeches 受 SPEECH_HISTORY_CAP=50 限制，超过后永远显示 50。这个截断 panel 上没法体现，写进 tooltip "受 speech_history.log 50 行 cap 影响"。如果用户介意可以走 Iter 70 加独立 atomic 真实累计。

## Iter 68 设计要点（已实现）
- **用 speech_history 行数作 proxy 而非新 atomic**：本可以加 ProcessCounters.proactive_lifetime_count + bump 在 record_speech 处。但 speech_history 文件本身就是"宠物每次开口"的真相源，count 它的非空行数和 atomic 等价、自动持久化、零新状态。SPEECH_HISTORY_CAP=50 足以判断"前几次"，超出后总是返 50；对于 < 3 的 threshold 完全够用。
- **threshold = 3**：经验值。1 太少（用户连"宠物长啥样"都还没适应）；5+ 太多（让宠物长期处于"问问题"模式让人嫌唠叨）。3 给一天（默认设置 5min interval × 3 ≈ 15min）的破冰窗口。
- **rule 在 plan 之后**：plan 是"宠物自己有意图"，icebreaker 是"宠物没经验"。两者可能并存（用户初次安装就问宠物今天有啥计划）。先 plan 后 ice-breaker 让 LLM 看到：先看自己的目标，再看自己的经验状态——更符合"先意图后克制"的思考链。
- **数字直接写入 rule**：用 "你之前主动开口过 N 次（< 3 次的破冰阶段）" 让 LLM 知道精确进度。如果只写"还不熟"模糊，LLM 第 1 次和第 3 次都会同样克制。
- **base_inputs 默认 100**：之前 5 个 conditional rule 都用 false/empty 作 default 不触发，新加的 `usize` 没有"empty"概念。100 远超 threshold 让现有 rules_count_and_format 等测试 unchanged。
- **不写 count_speeches 单测**：纯 file read + line count + filter，逻辑被 parse_recent 同 mod 的测试覆盖（用 std::fs::write 验证 round-trip）。新 count_speeches 实现接近 trivial，cargo check 抓签名错。

## Iter 67 设计要点（已实现）
- **plan vs reminder 各自独立的 cutoff**：考虑过共用一个 `stale_memory_hours`，但两类语义不同——reminder 是用户挂的，错过窗口算"凉了"；plan 是宠物自己定的目标，错过 24h 表示"昨天的目标，今天该重新看看了"。各自配置可让用户分别调（"我经常出差，48h 才算 plan 过期，但 reminder 我希望严格 12h"）。
- **`as_ref()` 让 Result 可重用**：`get_settings()` 是 fallible 调用，本来调一次就消耗。`cfg_settings.as_ref().map(...).unwrap_or(24)` 让两次读 settings 共用同一个 Result，避免双调。
- **parse_from_rfc3339**：memory.rs 的 `now_iso()` 写 `"%Y-%m-%dT%H:%M:%S%:z"` 是 RFC3339 格式 — chrono 内置 parser 直接吃。如果 someday 格式变了 parser 会 fail 而 sweep_stale_plan 返 false（fail-safe），不会误删。
- **best-effort delete**：和 sweep_stale_reminders 一样，`memory_edit.is_ok()` 不吐错。Plan 文件读不到、updated_at 解析不了、删除失败——任何一步出错都不阻塞 consolidate 主流程。
- **bool 返回 vs usize**：sweep_stale_reminders 返 usize（多个），sweep_stale_plan 返 bool（单个）。daily_plan 是 singleton（只能有一个 entry），用 bool 准确表达"删了吗"。
- **不写新单测**：plan 删除靠两个 chrono 操作 + 一次 memory_edit。chrono 的 parse_from_rfc3339 是 stdlib 行为；memory_edit 是 Tauri 命令本身有 path coverage；Duration 比较显然。Iter 60 sweep_stale_reminders 也没写专门 test 就是同理由。

## Iter 66 设计要点（已实现）
- **format 不限定，让 LLM 自由发挥**：考虑过定义结构化 plan 格式（JSON / 严格 bullet schema），但 plan 的本质是 self-instruction，越约束越机械。Description 当 free-form 给 LLM——只在 prompt 给 suggested 格式（`[done/total]` 进度标记），不在 Rust 端 parse。代价：进度计算靠 LLM 自觉，可能漂移；收益：跨 turn 灵活性大。
- **复用 ai_insights 类别**：current_mood 已经在那里。daily_plan 同样是 pet's own state——同 category 一致。如果未来"pet's state" 项太多再考虑分子类别。
- **"优先推进"非"必须推进"**：rule 里写"看时机自然"。如果 user 刚好在做完全无关的事，硬套 plan 就尴尬。LLM 自己判断 fit。
- **完成的项要删除**：和 reminder 一样，避免 plan 永远停在已完成状态污染下次。删除责任仍在 LLM——"自我管理"是 plan 概念的一部分。
- **不写专门的 stale sweep**：daily_plan 不像 reminder 有时间锚。如果用户休 3 天再开 pet，旧 plan 还在。Iter 67 列了——加 updated_at-based 清扫。
- **inject_mood_note 第三 section**：mood / reminder / plan 三段结构。每段 self-contained 不互引——LLM 看哪段相关就用哪段。`format!` 拼三段比之前两段更适合走 builder 模式（如果第四段出现就该重构）。
- **panel 暂不显示 plan**：plan 在 prompt + memory 都看得见；toolbar 显示又一段会让 panel 拥挤。等用户实际用上后视情况扩。

## Iter 65 设计要点（已实现）
- **`Option<String>` 而非两 enum**：`enum TurnOutcome { Silent, Spoke(String) }` 也行，但 `Option<String>` 让 None=Silent / Some=spoke 一一对应——类型本身已经够表达，无需新枚举。`is_some()` / `match` 都自然好用。
- **spawn 自动适配类型变更**：`if let Err(e) = ...` 只关心 Err 变体，对 Ok 子类型变化无感。这是 Rust 类型系统的优雅之处——单点改返回类型，多个 callsite 自动通过编译。
- **silent 也算成功**：`Result<Option<String>, String>` 把 silent 放在 Ok(None) 而不是 Err("silent")。silent 是合法决策，不是错误——错误（API 失败 / config 缺失）还是走 Err 通道。
- **trigger 显示真实 reply**：截断到 toolbar 的 ellipsis + tooltip 完整。如果 reply 超长（极端情况几百字），ellipsis 截断不会让 toolbar 错位；hover 看全文也是开发期 demo 的合理交互。
- **format 含三段（耗时 / idle / 文本）**：耗时让用户判断 LLM 调用速度；idle 让用户对比 prompt 中其他 hint；文本是核心。一行装下三种语义信号是 status 设计的小巧思。
- **不更新 useChat session**：trigger_proactive_turn 走的是 run_proactive_turn 路径，里面的 `app.emit("proactive-message", payload)` 仍然会触发 useChat 的事件监听，session message 自动更新。trigger 命令本身不需要再单独管 session。

## Iter 64 设计要点（已实现）
- **共用一个 state 显示成功/失败**：本可以两个 state 分别表示。但 success/failure 是同一行 UI 元素的两种内容，单 state 加 `startsWith("触发失败")` 判断颜色更紧凑。代价：失败信息和成功信息互相覆盖；但用户基本不会同时关心两者。
- **8 秒自动清**：手动调过 setInterval 见过用户被 stuck status 困扰。8 秒既给用户读完，也让 status 不会永远卡在那。比 5 秒留点缓冲，比 15 秒不至于太久。
- **绿色 #059669（成功）/ 红色 #dc2626（错误）**：标准 success/danger 配色，与 panel 其他色系（蓝/紫/橙）区分。Status 是临时态，颜色越鲜明读越快。
- **max-width 260px + ellipsis**：toolbar 紧凑，长 status 会让按钮挤压。截断后整体布局稳定，hover tooltip 给完整信息。
- **失败也保留 console.error**：DevTools 用户期望错误能在 console 看到完整 stack。state 显示是中文友好版，console 是原始版。两者并存。
- **不加给 consolidate 的同种反馈**：PanelMemory 已有 `message` state 显示 trigger_consolidate 的状态。两个 panel 各自独立。如果未来想统一一个全局 toast 系统再考虑合并。

## Iter 63 设计要点（已实现）
- **绕过 evaluate_loop_tick 直调 run_proactive_turn**：手动 trigger 的语义就是"我现在要它开口"，跑一遍 gate 然后 silent 是徒劳。所以直接 skip evaluate，从 run_proactive_turn 起步。代价：手动触发的 turn 不会被 decision_log 记录（因为那是 spawn loop 在 evaluate 后做的）；但 LogStore 会记 "Proactive: speaking ..." 行，可追溯。
- **保留真实 idle/input_idle 值**：本可以传 0 和 None（"假装用户刚活跃"），但保留真实值让 prompt 看到的 cadence_hint / input_hint 是真的——demo 时用户能看到真实状态，调 prompt 时也是真实输入。仅 gate 被绕过，prompt 内容仍真实。
- **绿色按钮配色**：DevTools 橙、整理紫、开口绿。三按钮三色让"运行 X 操作"的视觉模式互不混淆。绿色暗示"开口/说话"是日常正向 action（橙色暗示"调试"，紫色暗示"重活"），有色彩心智暗示。
- **不在 toolbar 显示 status**：返回的 "finished in N ms" 暂时丢掉。Iter 64 列了让它显示——这次只先打通触发链路，避免一次性改太多。
- **不写新单测**：trigger 是 thin wrapper；run_proactive_turn 调 LLM 不便单测；前端只是 invoke + state，cargo + tsc 抓签名错。
- **手动 vs 定时 turn 的统计影响**：trigger 触发的 turn 也会更新 cache_counters / mood_tag_counters（因为 run_proactive_turn 末尾还是会 read_mood_for_event + log_cache_summary）。这是有意的——"手动触发"也是真实 LLM 调用，理应纳入统计。如果要排除，得加一个"is_manual"标记往下传，复杂度暴增。先不做。

## Iter 62 设计要点（已实现）
- **绕过 min_total_items gate**：定时触发会检查 `total < cfg.min_total_items` 跳过空索引，但手动触发一定是用户故意要跑——可能就是想验证当前 prompt 工作。所以 `trigger_consolidate` 直接调 `run_consolidation` 不走 gate。
- **不绕过 cfg.enabled 检查**：实际上用户即使 disabled = true 想手动跑也合理 (debugging without 留 cron 跑)。当前实现也不检查 cfg.enabled——`run_consolidation` 本身不依赖那个字段。OK by side effect。
- **status 字符串而非 ()**：Tauri 命令返回 `Result<String, _>` 让 panel 能直接 setMessage(status)。比起返 `()` + 让前端写死 "完成"，更准确显示真实耗时（用户能看到"6800 ms"知道 LLM 调用花了多久）。
- **整理后 loadIndex() 刷新**：consolidate 改了 memory，panel 上展示的 cached index 会过期。reload 是 ms 级开销，立即给反馈值。
- **紫色按钮**：与"重连 MCP"使用 `#8b5cf6` 一致——都是"运行某个长操作"的紫色 action。颜色 tongue 一致让用户视觉模式识别更稳。
- **tooltip 解释"做啥"**：用户看到"立即整理"可能不知道具体涵盖什么。tooltip 写"合并重复 / 删过期 todo / 清 stale reminder"让他们决策时知道边界。
- **不写新单测**：trigger_consolidate 是 thin wrapper 调 run_consolidation；后者本身没有便利的测试路径（需要 mock LLM）；前端是 invoke + setState 链路，cargo + tsc 兜住语法错。

## Iter 61 设计要点（已实现）
- **归属 MemoryConsolidateConfig 而非 ProactiveConfig**：原 TODO 写 ProactiveConfig 但反思下来 sweep 是 consolidate 阶段做的、和 consolidate 的 enabled / interval_hours 等同源。把它放 ProactiveConfig 会让 settings.yaml 里 reminder 相关配置散落两处。"功能在哪跑，配置就在哪" 是更稳的归属规则。
- **default 24 与硬编码同值**：升级用户的 config.yaml 没有 `stale_reminder_hours` 字段时 serde default 给 24 = 之前行为。零意外升级。
- **fallback to 24 on settings error**：`get_settings().map(...).unwrap_or(24)` 让 settings 文件出问题时仍能 sweep——consolidate 整体功能不该被 settings 解析失败彻底关掉。
- **panel 加说明字段**：模态太挤就不放说明（用户看 label "清理过期 reminder (小时)" 大致能懂）；panel 视图宽，加一句中文说明区分 HH:MM vs YYYY-MM-DD 两种 reminder 行为。这种 modal-vs-panel 差异化跟 Iter 37 的 chat trim 设置一致。
- **不写新单测**：is_stale_reminder 已经接受 cutoff 参数测过；新设置字段只是把"24"换成"settings.stale_reminder_hours"——类型 + cargo check 抓 plumbing 错。Tauri 命令+settings 套路稳定后这种改动单测价值低。

## Iter 60 设计要点（已实现）
- **deterministic sweep 而非 prompt rule**：原 TODO 说"加一条规则" — 但用 prompt rule 让 LLM 删除 stale 是不可靠的（LLM 可能漏看、可能误删非 reminder 的 todo）。Rust 端按规则扫一遍是确定的。"consolidate 帮兜底"恰恰是确定性兜底的语义。
- **TodayHour 永远不 stale**：Recurring 语义。比如"23:00 吃药"用户可能希望天天提醒，让 consolidate 第二天就删了违反预期。如果用户想单次，让他用 Absolute 格式。这种"shorthand 是 recurring，long-form 是 one-shot"的语义靠 enum 拆分明确。
- **collect titles 再删**：iterate 时直接 mutate 会触发 memory_list 内部状态飘移（每次 memory_edit 都重写 yaml）。先收集要删的 titles，循环结束后再调 memory_edit，避免 race。
- **24h 硬编码**：当前 cutoff 写在 sweep call site `sweep_stale_reminders(now, 24)`。Iter 61 列入了 settings 化。这个 magic number 在调用站显式而非藏在函数默认值里——读 run_consolidation 的人一眼能看到"24h cutoff"。
- **沙盒前调 sweep 而非 LLM 之后**：把 sweep 放 LLM 调用之前，意味着 LLM 看的 index 已经干净，不会"花功夫思考要不要删过期 reminder"。少一次推理。
- **best-effort delete**：sweep_stale_reminders 用 `.is_ok()` 累计，删除失败（罕见）忽略不抛错。consolidate 主流程不该被一个 todo 删除失败打断。
- **测试位置**：`is_stale_reminder` 测试放在 `mod reminder_tests` 里（与 parse / due 同 mod），保持 reminder 相关行为统一审视。`sweep_stale_reminders` 不测——它的逻辑就是"调 is_stale 过滤 + 调 memory_edit delete"，每个组件已测。

## Iter 59 设计要点（已实现）
- **enum 而非 Option<NaiveDate>**：原本想用 `Option<NaiveDate>` 配 `(u8, u8)` —— 表示"无日期=今天，有日期=绝对"。但 enum `TodayHour` / `Absolute` 显式语义两态，match 时 caller 必须想清楚两种情况，避免 forget Some/None 心智负担。
- **TodayHour 仍保留 wrap-midnight**：用户用简短形式时通常是"今晚的事"，"23:55"在凌晨 00:05 仍想触发是合理预期。Absolute 不 wrap——既然指定了具体日期，就不该越界。这两种语义差异写在 doc + 测试 (`absolute_does_not_wrap_midnight`) 里。
- **不引入 [remind: +30m] 字面格式**：LLM 现在每次都看到 prompt 头部 "现在是 YYYY-MM-DD HH:MM"，让它做加法是合理责任分配。如果允许 `[remind: +30m]` 字面存储，每次 panel/prompt 读取还得算"这条是什么时候写的"——`created_at` metadata 可以提供，但增加了 parse 复杂度。让 LLM 在写入时换算成绝对时间是更简单的契约。
- **NaiveDateTime 而非 DateTime<Local>**：本地时区变化时 NaiveDateTime 不会自动调整，但 reminder 是"绝对一个时刻在那儿"，时区问题不是考虑重点（本地 chrono::Local::now().naive_local() 转一下 caller 用 wall clock）。如果未来要跨时区考虑，再加 timezone 字段。
- **提示词描述三种场景**：今天/跨天/相对，列了具体例子。LLM prompt 的清晰度更多靠"举例"而非长篇解释——"`[remind: 2026-05-04 09:00] 项目早会`"比"如果是某天早 9 点开会，使用包含日期的格式..."更直接。
- **format_target 拉出来公用**：build_reminders_hint 和 get_pending_reminders 都需要"把 ReminderTarget 渲染成一行字符串"。抽出 helper 避免两处实现飘移。前端 panel 不再自己 split timestamp（之前是 ISO 字符串），直接渲染后端给的 `r.time` 就好。

## Iter 58 设计要点（已实现）
- **复用 parse_reminder_prefix + is_reminder_due**：和 build_reminders_hint 同一函数，确保 panel 显示和 prompt scan 用同一种判断。"prompt 看到的是哪些 / panel 显示的是哪些" 这两个集合若用两套实现容易飘移。
- **同时返回 due 和未来未 due**：build_reminders_hint 只给 due 的（要进 prompt），但 panel 想看完整列表（包括"已设但等几小时才到"的）。所以 get_pending_reminders 返全部解析得通的 reminders + 一个 due_now 标志，让前端决定怎么显示。
- **橙色背景 #fff7ed（reminder 维度）**：颜色编码继续扩展——Cache 蓝 / Tag 紫 / Speech 紫 / Wake 蓝 / Reminder 橙 / Decision 灰白。橙色和"待办" / "时钟"心智模型对应，且与已有色系不冲突。
- **due_now 加粗 + 更深橙**：同一段两种颜色避免 due 和未 due 看起来一样。深橙 (#ea580c) 抢眼比浅黄 (#a16207) 多。加粗强化"现在该提醒"的紧迫感。
- **不写后端测试**：get_pending_reminders 是 thin wrapper —— parse_reminder_prefix / is_reminder_due 已分别测过；memory_list 是 Tauri 命令本身有覆盖。再写 plumbing 测试只是验证 wrapper 链接没断，cargo check 抓得住。
- **panel 渲染区段顺序**：toolbar / tone / decisions / speech / reminders / log。reminders 放 speech 之后是因为两者都跟"宠物未来要做什么"相关（speech 是过去说啥，reminders 是未来要提啥），相邻显示更连贯；放 log 之前因为这是结构化数据 strip，log 是流水。

## Iter 57 设计要点（已实现）
- **拆 mood_section / reminder_section 而非内联**：单一长 body 也行，但拆成两段命名变量让代码更易读、未来想加第三段（"如果用户问明天日程"等）可以追加新 section + format!。这是把 mood note 也演化到 builder 模式的早期形态。
- **格式约定写在 prompt 里 vs 写在 SOUL.md**：SOUL.md 是宠物的 persona 设定（性格），不该塞工具/格式约定。inject_mood_note 是工程性 system 提示，正合适 — 它已经在做 "教 LLM 怎么写 mood format"。新增 reminder format 自然延续。
- **明确反例**："我说今晚要去吃饭"是闲聊不是提醒。如果 LLM 把每句"X 时间"都建 todo，会刷出几十条无效提醒。给反例 = 给 LLM 一个判断锚。
- **不写测试**：纯字符串模板加段 + cargo 编译通过 + Iter 56 已经测了 parser 和 due 检查 + 56 的 builder 测试也覆盖了 hint 注入路径。再写一个"check inject_mood_note 输出含「[remind:」"是测试 string literal 自身存在，价值低。
- **ASCII vs 中文引号坑第三次**：Iter 29 / 39 / 57 都中过同一个雷。下次写中文长字符串文本里若需要嵌引号，第一反应应该是「」`「」`，不是 `"..."`。在 IDEA 里写下来当 anchor。

## Iter 56 设计要点（已实现）
- **`[remind: HH:MM]` 前缀约定，复用 todo 类别**：考虑过新建 `reminder` memory 类别，但那要改 memory.rs 的常量并不增加多少清晰度。复用 `todo` 类别，用 description 前缀做识别——和 `[motion: X]` mood 前缀同款思路（Iter 10 / 22）。memory_edit 已经能创建 todo，无需新工具。
- **due window = 30 min**：宠物每 5 min 一次 proactive tick，30 min 给 6 个机会能命中。如果 < 5 min，主动开口的其他 gate（cooldown/idle）很容易让宠物错过；> 30 min 又会让早起报错过的提醒在中午冒出来诡异。30 min 是经验值，settings 可暴露但当前不暴露。
- **跨午夜处理与 quiet hours wrap 同一思路**：`+24×60`。但 due 检查只想认 "now 是 target 之后但不超过 window" 这一种 due——所以 `delta < 0` (target 在未来) 不应直接 wrap 当作 due。仔细看代码：`+24*60` 后 delta 可能很大，再用 `< window` 过滤；只有"刚刚跨过 target 时刻"才 wrap 后变 small delta。例：target 23:55 / now 00:05 → 原 delta = -23×60-50 = -1430，+1440 = 10，< 30 → due。target 12:00 / now 11:55 → 原 delta = -5，+1440 = 1435，> 30 → 不 due。Wrap 逻辑天然只允许小 wrap 通过。
- **rule 强调"最相关的一条"**：实际场景下用户可能积累多条 todo（吃药、开会、买菜...）。如果让 LLM 一次全念出来对话会僵——明确 instruct "挑最相关一条" 减少机器感。
- **delete after 提醒**：让 LLM 在开口后删掉那条 todo，避免下个 30-min 窗口里 reminder 重复。这是单次提醒语义；如果用户想周期性提醒，让他们重新加一条（或后续 Iter 加 "周期" 标记）。
- **scan async 还是同步**：memory_list 是同步函数（直接读 yaml）。`build_reminders_hint` 用同步签名即可，不需 async。
- **测试拆 mod 而非全塞 prompt_tests**：parse / due 的测试纯数学，逻辑清晰，单独 `mod reminder_tests` 让 test 列表读起来按主题分组。

## Iter 55 设计要点（已实现）
- **复用 minutes_until_quiet_start**：Iter 54 写好的纯函数直接 reuse，不重写。`get_tone_snapshot` 和 `run_proactive_turn` 都调它，保证 panel 显示和 prompt 看到的一致——单一数据源。
- **红色 🌙 颜色**：tone strip 现有 Cache 蓝、Tag 紫、wake 蓝、period 灰。新加的 pre-quiet 用红色 / 月亮 emoji 区分"快到了"的紧迫感。颜色编码越多越要谨慎，但目前只有 5 个独立段落，红色 alert 仍可读。
- **不写新单测**：minutes_until_quiet_start 已被 7 个 case 覆盖；ToneSnapshot 字段添加是数据 plumbing，cargo check 抓签名错；前端是 ts 类型对齐，tsc 抓拼写错。"加新字段"性质改动靠类型系统兜底足够。
- **plumbing 进度**：从 prompt 加 hint → builder 加 input → 命令 expose → panel 渲染，这个 4 步链路其实从 Iter 50 起就建立了。新加一种 tone signal 已经稳定走这个 pattern，第 N+1 个 signal 会几乎"配方化"。

## Iter 54 设计要点（已实现）
- **跨日 wrap-around 用 `+ 24 × 60`**：和 in_quiet_hours 同一思路。如果 quiet_start 今天已过（比如 quiet=8-22 + now=23:00），下次 quiet_start 是明天 8:00 = 24×60-23×60+8×60 = 540 分钟。look_ahead 远超 15 → 自然 None。简单且对所有分布通用。
- **strict `<=` 而非 `<`**：测试 `at_window_edge_15_min` 显式约定 15 分钟时仍触发。设计上"恰好到 look_ahead"算"快到了"更自然。如果改 `<` 会让 22:45 这种正好阈值的场景反复落入"刚错过窗口"的不一致状态。
- **15 分钟硬编码**：`look_ahead_minutes` 是函数参数（让单测能注入不同值）但 caller 写死 15。理由：(a) 这是 conversational rule，用户调它的预期低；(b) 如果有人想自定义，加 settings 字段比加 UI 控件更便宜——等真有需求再做。
- **跨日 + look_ahead 关系**：若 look_ahead 跨过 24h（设 1500），算上 wrap 就需要更复杂处理。当前 look_ahead 远 < 1440（一天分钟数），无歧义。注释里没强调但代码上 wrap 后 `delta as u64 <= look_ahead` 比较是单调的。
- **不修改 quiet_hours 那张 gate 决策表**：临近规则只影响 prompt，不影响 gate。这条 line 之间的边界是有意的——gate 决定 "fire 还是 silent"，prompt 决定 "fire 时该说啥"。让 24:00 用户能听到一句"晚安"再静音，比 22:59 还在聊天 23:00 突然冷处理体验更连贯。
- **测试 `past_today_uses_tomorrow` case**：07:00 早晨 quiet=23-7 → not in quiet（end 是 exclusive 7），quiet_start 今天 23:00 还有 16h，远超 look_ahead → None。这个 case 验证早晨刚出 quiet 不会 trigger pre-quiet rule。原本以为 wrap-around 可能让我误算成"距下次 quiet 16 小时"反而触发，写 test 帮我提前约束逻辑。

## Iter 53 设计要点（已实现）
- **wake_hint 非空作为 wake-recent 信号**：本可以再加一个 `wake_recent: bool` 字段。但 wake_hint 已在 PromptInputs，且其非空恰好对应"在 grace 内"——派生信号不重复携带，DRY。代价：rules 函数读 hint 字符串而不读结构化 bool，但这只是 in-Rust 的小耦合，上下文清楚。
- **is_first_mood 显式 bool 而非检 mood_hint 字符串**：mood_hint 在 first time 时是 "（还没记录...）"。可以 `mood_hint.contains("还没")` 检测。但那是脆弱耦合（有人改 hint 措辞会断），显式 bool 是契约。
- **rule 添加而非替换**：考虑过让 wake context "用户刚回来，先简短问候" 替换基础 rule "只说一句话"。但替换会让"基础 6 条"语义飘移；追加更稳——LLM 看到所有适用规则，自己解决冲突（这两条本就一致）。
- **Vec 容量预估为 8**：`Vec::with_capacity(8)`，base 6 + 最多 2 个 conditional。上限准确避免 reallocation；后续加新 conditional rule 要跟着调整 capacity 或忽视（reallocation 成本可忽略，但 with_capacity 是 documentation as code）。
- **测试 baseline 锚点**：`no_context_rules_with_default_inputs` 验证 6 条这一基准——任何 base rules 增减都立刻让其他 4 个 conditional 测试同步打破。三层防护让"加新 rule 必须更新对应 count assertion"成为强制。
- **不引枚举 / 不引 trait**：完全可以做 `enum RuleSource { Base, Wake, FirstMood, ... }` 让规则各自标 source。当前 2 条 conditional 的复杂度不值得。等 5+ 条时再考虑。
- **PromptInputs 字段从 9 到 10**：每加一个 conditional 维度就多一字段。这种 struct 扩展是 builder 模式自然代价；好在加字段唯一影响是 `base_inputs()` 测试构造器多一行。

## Iter 52 设计要点（已实现）
- **`Vec<String>` 而非 `Vec<&'static str>`**：原 TODO 想用 `&'static str` const 数组省分配。但其中 3 条规则用 `format!` 插值（SILENT_MARKER / MOOD_CATEGORY / MOOD_TITLE），编译期不可能形成 `&'static str`。混用 owned 和 borrowed 反而复杂；统一 `Vec<String>` 简单。每秒 < 1 次调用，6 个 String alloc 无足轻重。
- **行 by 行 push 而非 vec! macro**：本可以用 `vec![format!(...), "…".into(), ...]`。但分批 push 调试更直观——加新规则时 `git diff` 显示一行 push，而 vec! 内插一行会让整段被识别为 "全改"，git blame 也更细。
- **assert rules.len() == 6**：count assertion 是 anchor。未来 ladder 改动（加 / 删一条）必须显式更新测试，避免悄悄 drift。配合"每条以 `- ` 起头"约束 + 关键词存在性测试，三层防护。
- **关键词测 SILENT_MARKER / MOOD_CATEGORY / MOOD_TITLE / motion tags**：这些是规则有效性的最低门槛——LLM 看不到这些标识就不知道该写啥。测它们存在比测完整字符串稳健（措辞调整不破坏测试），比不测更能阻止误删（直接复制/粘贴时漏掉常量）。
- **rules_appear_in_full_prompt 是回归测试**：未来若有人 refactor `build_proactive_prompt` 不小心忘 extend rules，这条 test 立刻失败。"每个被抽出来的子组件都有"还在主路径里"的测试"是 builder pattern 的标配。
- **不在 rules() 里读 PromptInputs**：现在签名 `proactive_rules() -> Vec<String>`，无依赖。Iter 53 想动态调整时可以无痛改成 `proactive_rules(&PromptInputs)`——就算 callers 多，因为内部使用，改两处即可（builder 自身 + tests）。

## Iter 51 设计要点（已实现）
- **`Vec<String>` 而非 `String::push_str`**：考虑过 `let mut s = String::new(); s.push_str(...); s.push('\n');`。Vec + join 优势：(a) push_if_nonempty 可单独跳过；(b) 调试时 `dbg!(&sections)` 看 layout 一目了然；(c) 不用手写 newline，join 帮忙。代价是分配多一些（每段一个 String alloc），prompt 调用一秒级别频率，可忽略。
- **PromptInputs 而非 9 个独立参数**：单一函数签名 9 个 borrow lifetime 让调用方读起来糟糕。Struct 把它们打包，调用站显式 `PromptInputs { ... }` 字段写法 self-documenting。`'a` lifetime 显式标注让编译器把 borrow 关系算得清楚。
- **build_proactive_prompt pub for tests**：为了让 prompt_tests mod 能直接调，把 builder 升 pub。Tauri command 没暴露，只是 mod-level 可见。如果担心暴露过度可以加 `pub(crate)`，目前 `pub` 一致最简。
- **mood_hint 必出 vs focus/wake/speech 可选**：mood 在 bootstrap 时给 fallback 文案"还没记录过..."，永远非空。focus/wake/speech 在 inactive/无 wake/无历史时返回空 string。`push_if_nonempty` 显式区分这两类。
- **测试只比 contains 不比完整字符串**：prompt 完整字符串 ~1.5KB 中文。逐字 assert 太脆弱（任何措辞调整都让测试爆炸）。`assert!(p.contains("xx"))` 检关键内容存在即可，未来局部调整 prompt wording 不必更新测试。
- **避免 `\n\n\n` regression test**：原 format! 里 `{focus_hint}\n` 在 focus_hint 为空时会留下空行。本 builder 的 push_if_nonempty 跳过 → 不会有空行。专门写 `assert!(!p.contains("\n\n\n"))` 让未来若有人误用 push 跳过断言会 fail。
- **不动 inject_mood_note 等其他 prompt**：reactive chat / consolidate 各有自己的 prompt 构造，结构更简单（1-2 段），抽 builder 收益不显著。本次只动 proactive。Iter 52 之后若 reactive prompt 也膨胀再考虑。

## Iter 50 设计要点（已实现）
- **一个命令而非多个**：本可以让 panel 调三个命令（cadence、wake、mood）独立拉。但 ToneSnapshot 把它们打包成一次 IPC，更原子（多个独立调用之间状态可能漂动）、更便宜（一次轮询 vs 三次）。代价：加新信号要改 struct + 命令 + 前端 interface 三处——但每秒 1 次调用，单调用便宜，权衡好。
- **复用而非重新算**：`get_tone_snapshot` 直接调 `period_of_day` / `idle_tier` / `read_current_mood_parsed` / `last_wake_seconds_ago` —— prompt 也调这些。"两个消费者用同一份数据"是设计目标，避免 panel 显示一个值、prompt 用另一个的尴尬。
- **emoji 作为 type discriminator**：`⏱` time、`💬` cadence、`☀` wake、`★` motion、`☁` mood。比"period:" / "cadence:" 这种文字标签紧凑且自带语义。代价：emoji 渲染依赖系统字体（macOS / Windows / iTerm 都没问题，远程终端可能不行），但 panel 是 GUI 不是 terminal。
- **wake 仅在 ≤600 内显示**：和 Iter 48 / 49 的 600s grace window 对齐——超过就是"早就 wake 过的事"，UI 不再炫耀。
- **mood text 截断显示**：mood 可能很长（"看用户在写代码替他高兴，但担心他没吃午饭..."），整段塞 panel 一行会让其他段挤掉。`flex: 1 + ellipsis` 让它自适应宽度，hover tooltip 看完整。
- **不写测试**：纯组装函数，没条件分支，依赖的 4 个 helper 都各自有测试。tsc 抓字段名错；cargo check 抓签名错；剩下的"渲染好不好看"是肉眼活，单测帮不上。

## Iter 49 设计要点（已实现）
- **软化哪些 / 不软化哪些**：核心设计决策。cooldown 和 idle threshold 都是"避免打扰"的时间约束——而 wake 已经标记"用户离开过桌子大概率回来了"，这两个约束的本意不再适用，软化。awaiting / quiet_hours / focus_mode 是用户偏好或社交礼貌（"我没回应你，你别接着说"），wake 不该绕过它们。决策表：
  | gate | 软化 | 理由 |
  |---|---|---|
  | enabled | ✗ | 用户显式关掉 |
  | awaiting | ✗ | 礼貌/响应等待，与时间无关 |
  | cooldown | ✓ | 避免连续打扰，但 wake 后状态已变 |
  | quiet_hours | ✗ | 用户睡觉时间偏好，wake 偶发不应突破 |
  | focus_mode | ✗ | 用户正在专注，wake 不暗示该打扰 |
  | idle threshold | ✓ | "用户该静一会儿"前提是用户在桌前；wake 推翻前提 |
  | input_idle | ✗ | 用户活跃在键盘 = 不该插话，wake 已恢复无关 |
- **idle 减半而非清零**：清零（threshold=0）会让用户开盖瞬间宠物就喊"欢迎"——可能用户只是查个时间又关上。减半到 ≥60s 给用户至少 1 分钟"重新进入工作"时间，再开口。
- **floor 60s 是 idle gate 自带的**：原 gate 已 `cfg.idle_threshold_seconds.max(60)`。软化时再 `(raw / 2).max(60)` 重申一遍，避免用户调 idle=120 时 wake 让它变 60（可接受）vs 调 idle=30 时 wake 让它变 30 (=15 max 60 = 60，本来就被原 max 拉上来，这里再加防御)。
- **wake_recent 用 matches! 而非 if let**：matches! 一行表达"在窗口内"，可读性更好。`Option<u64>` + 上限比较是这种模式的典型用法。
- **测试用 grace 边界 600/700**：选 700 而非 601 测试 "刚出 grace"，让 boundary 假设不依赖严格 strict-vs-inclusive 的精确边界（grace_recent 是 `<=`）。
- **6 case 覆盖每个软化 + 不软化**：3 软化测试（cooldown 跳/不跳、idle 减半、idle floor）+ 3 不软化测试（awaiting / quiet）。每个决策矩阵格子都有测试 keep us honest if 未来 someone 想"也软化 quiet_hours"。

## Iter 48 设计要点（已实现）
- **心跳间隔推断 vs NSWorkspace**：原 TODO 提到可走 NSWorkspace 通知或 Swift sidecar。心跳推断的优势：(a) 跨平台（Linux suspend、容器调度器同样工作）；(b) 零 macOS-specific 代码或 plist 配置；(c) 一个纯函数 + Mutex 可全测。代价是阈值需要调，且热挂起+冷恢复 < 阈值的事件会漏。但宠物业务逻辑能容忍漏检，强信号准确比检全更重要。
- **阈值 = 2× 正常 sleep**：proactive 默认 interval 300s。阈值设 600s = 2× 给 jitter 余地。如果哪天用户把 interval 改到 600s+，阈值需要相应提升——目前没做动态阈值，写死保持简单。Iter 49 如果要根据 wake 调 gate，可能要顺手让阈值跟 settings 联动。
- **Instant 的 `checked_sub` 测试技巧**：`Instant::now() - Duration::from_secs(N)` 在大多数运行时是 valid（boot time 远早于 600s 前）。但安全起见用 `checked_sub` + `expect`。这让单测能控制"prev"和"now"两个时间点而不需要 thread::sleep。
- **wake_hint 用秒数描述**：「大约 N 秒前刚从休眠唤醒」让 LLM 知道时间感（10 秒前 vs 8 分钟前的招呼语调不同）。但秒数粒度对人不友好——如果是 350 秒，可能要换成"5 分钟"。当前不做格式化，让 LLM 自己解读 raw 数字；以后嫌不自然再加 humanize 函数。
- **observation 在 spawn 顶部**：放在 `let settings = ...` 之前还是之后？放之前能在 settings 错误重试时也心跳，但 wake 通常不发生在那里。放 settings 之后、evaluate_loop_tick 之前最安全——确保每次 normal tick 都心跳一次，且不被 settings 错误干扰。最终选了放最靠近 evaluate 处。
- **不影响 gate**：仅 informational。理由：(a) "刚 wake" 不一定 == "用户回来了"——也可能是闭盖在沙发上手抖按了一下；(b) gate 改动会让 wake 后宠物立刻发声，频繁 wake 用户（如手提开合）会被打扰。把 gate 升级留给 Iter 49 慎重决定。

## Iter 47 设计要点（已实现）
- **rule of two 触发抽取**：通常我等 rule of three，但这里特殊——focus_tracker 的 rotation 已经写得很泛化（path + max_bytes 两个参数，没什么 module-specific 的逻辑），且 speech_history 的需求是字面同款。第二个 caller 出现就是把它抬上来的最佳时机；等第三个出现时，rotation 已经是 well-known util，新 caller 的开发者会期待它存在。
- **测试搬家而非复制**：focus_tracker 的 6 个 rotation 测试整体迁到 log_rotation，原位置删除留 comment "搬家了"。这种"测试随实现走"是 Rust mod system 的自然结果；测试覆盖度没变，单一来源更易维护。
- **speech_history 加 size 上限是 defense in depth**：原来 50 条 line cap 在 LLM 守规矩时足够（正常一句话 ~50 字符 → 整个文件 < 5KB）。但 LLM 抽风（譬如 hallucinate 一个 5MB 的 JSON）一条就把文件撑爆。size cap 100KB 给"50 行 × 平均 2KB/行"留余地，正常使用永远不触发，异常时立刻兜底。
- **rotate before read**：`record_speech_inner` 先 rotate 再 read 现有内容。如果反过来（先 read 再 rotate），oversized file 会被读进内存才被 rotate。先 rotate 让 read 看到的是空文件（rotation 后 path 不存在 → unwrap_or_default → 空字符串），少一次大读 IO。
- **不为 speech_history 写新 rotation 测试**：log_rotation 的 6 个 case 已经把 rotation 行为测得很彻底。再写一份 speech_history-specific 的"我有调用 rotate"测试只是验证 plumbing，cargo check 已经把那点抓住。
- **保持 net-zero test count 是良好信号**：单纯重构不该减测试覆盖。这次 -6 + 6 = 0，证明搬家完成度高。

## Iter 46 设计要点（已实现）
- **timestamp 切片在前端做**：本可以让后端命令直接返 `{ time, text }` 结构。但保留 raw line + 前端 `slice(11, 16)` 取 `HH:MM` 让接口最简单（unstructured array），UI 改显示格式（比如想要相对时间 "刚才"/"5 分钟前"）也只是前端事。
- **紫色与 Tag 同色系**：Cache 蓝色（外部 cache 维度）、Tag 紫色（mood/personality 维度）、Speech 紫色背景（mood/personality 维度）。颜色是 panel 里的"信息维度索引"——同色系意味着语义相近，用户视线扫一遍就能 group。
- **HH:MM 而非完整 ISO**：长 timestamp 在窄 panel 里换行难看。`HH:MM` 5 字符足够区分"几分钟前 / 几小时前 / 跨日"。如果用户需要详细时间，hover tooltip 可以展开（暂未做）。
- **fetchLogs 五路 Promise.all**：每秒 5 个 IPC 调用看起来多但都是廉价的（log array、几个 atomic、少量文件读）。改成 batch 命令 `get_panel_state()` 也是个选项，但目前每个 invoke 命令都对应单一 reader 概念，分开更清楚——加新 stat 直接加 invoke + state 一致。
- **不加 reset/clear 按钮**：speech_history.log 本就是 trim-on-write 自我管理的，且用户清掉等于让宠物失忆——不该轻易做。如果要支持清零，应该在 Iter 47 顺手加一起处理（与 rotation 配套）。

## Iter 45 设计要点（已实现）
- **独立文件而非 memory 条目**：考虑过把"最近发言"做成 `ai_insights/speech_history` 之类的 memory 条目让 LLM 自己 memory_edit 维护。但这是 deterministic 记录——每次说话就追加，不需要语义判断，不该让 LLM 决定。后端 owns 它，简单可靠，且不污染 memory 索引（用户面板看 memory 时不必看到一堆"我说过的话"）。
- **跟 focus_history.log 同款架构**：append-only + size cap + parse 纯函数 + 公共目录路径。Iter 23 + 25 已经把这个模式调好；本次复用，时间预算大量花在 prompt 注入而不是基础设施。
- **trim on-write 而非 rotation**：focus_history 用 1MB rotation 到 `.1`。speech_history 写频率更低（每次主动开口一次），且只关心"最近 N 条"，不需要保留 rolling 多份历史。每次 write 前 trim 到 50 条更简单且 always-bounded。
- **`SPEECH_HISTORY_CAP=50` vs `RECENT_HINT_COUNT=5`**：保留 50 是给未来预留——比如 panel 想显示最近 20 条、或 consolidate 阶段想分析"宠物总说啥"。当前 prompt 只用 5。如果只为 5 把 cap 设到 5，将来扩展功能要回来改。
- **strip_timestamp 把 ts 从 prompt 显示中剥离**：cadence_hint 已经给了"距上次主动 N 分钟"，再让 LLM 看到每条的 ISO 时间是冗余 noise。让 prompt 里只显示纯文本 bullets。
- **空 history 不渲染**：第一次 / 文件丢失时 speech_hint 是空串，prompt 不增加无意义占位。LLM 不会被"（你最近什么都没说）"这种空陈述分心。
- **best-effort write**：record_speech 吞 IO 错误。原因和 focus_tracker 同款——这个记录是 "nice to have"，宠物的核心说话流程不该因为 disk full 而断。
- **测试先行**：parse_recent 7 个 case 是这次开发最先写的部分（无 IO 简单）；之后写 record_speech_inner 时心里有底——读出去的形状是已知的。

## Iter 44 设计要点（已实现）
- **5 档而非 3 档**：考虑过简化到"刚才 / 一会儿 / 很久"。但 60–360 分（一两小时到大半天）和 361–1440 分（半天到一天）实际感觉差异挺大——前者还能直接接话题，后者需要"重新打招呼"。多一档没什么成本。1440+ 单独一档则覆盖跨日情形，让宠物有"昨天那个事..."的开场可能。
- **idle_minutes 与 since_last_proactive 都给**：本可以替换 idle_minutes，但两者语义不同，都给 LLM 让它自己判断重要性更稳。例：用户主动找过宠物聊天（idle_minutes=2），但宠物自己上次主动开口是 4 小时前——cadence 还是「几小时没说话」基调，与 idle 一致；但如果反过来用户活跃宠物却很久没主动开口，cadence 也能反映出来。
- **clock.snapshot 二次调用**：spawn 已经调过一次，run_proactive_turn 里又调一次，相当于读两次锁。但两次读之间用户可能正好交互过，second snap 更新——这正是我们想要的实时数据。Mutex 锁极便宜（μs 量级），不优化。
- **不改 LoopAction::Run 透传 since_last_proactive**：传参方式让 spawn 决定的 snapshot 和 run_proactive_turn 用的 snapshot 时序一致；二次取则各取所需。后者更简单（少一处签名变化）也更准（用最新值）。
- **测试覆盖每档 + 每边界**：6 个 happy path + 8 个 boundary（4 个跳变点 × 2 侧）。idle_tier 是数值范围匹配，最容易 off-by-one；boundary 测试让"15 还是刚说过 / 16 是聊过一会儿"这种规则刻在 binary 里。
- **first-time 单独文案**：snapshot.since_last_proactive_seconds 为 None 表示"this proactive 是第一次"。给一句"你还没主动开过口，这是第一次"比让 LLM 看到 None / 0 自己脑补更直接。

## Iter 43 设计要点（已实现）
- **直接给中文时段词，不给 morning/afternoon**：原 TODO 设想英文标签（"morning/evening/..."），但 prompt 整体是中文，混语会让 LLM 多一次内部翻译。直接给"清晨/上午"等让模型语义抓手最近。代价：英文使用者看不懂——但这个项目的 SOUL.md / 工具描述都是中文，本来就是中文 first。
- **边界选定按对话直觉**：5 起算清晨而非 6（早起的人 5 点醒了，听到"深夜"会困惑）；11 进中午而非 12（11:30 已经在准备午饭）；17 到傍晚（北京冬天 17 点已天黑）。这些都是在脑子里走一遍 "如果是用户此时收到这条消息，他会觉得宠物说对了吗" 决定的。
- **22:00 算深夜**：和 quiet_hours 默认起点 23:00 错开 1 小时是有意的——quiet_hours 是"不打扰你睡觉"，period_of_day "深夜" 是"很晚了"的对话氛围。22:00 用户可能还醒着但是该营造夜的氛围，proactive gate 还允许说话。
- **不与 quiet_hours 联动**：可以让 period_of_day 直接复用 quiet_hours 边界，但那把"用户配置（什么时候不打扰）"和"对话氛围（什么时候叫晚上）"耦合了。两个独立维度：quiet 决定要不要开口、period 决定开口时怎么说。
- **测试覆盖每个跳变点两侧**：不光测 happy path（每个时段一个代表 hour），还专门测每个 boundary（4/5、7/8、10/11、12/13、16/17、18/19、21/22、23/0）。time-of-day 这种规则一年用 365 天，bug 可能要等到某个特定 hour 才显形——cheap 测试覆盖换来强信心。
- **不动反应式 prompt**：proactive 是"主动找话题"，time-of-day 给找话题人提示；reactive 是用户驱动话题，模型再注入"现在是傍晚"反而冗余、抢戏。

## Iter 42 设计要点（已实现）
- **嵌套 struct 而非 newtype**：本可以让 ProcessCounters 是各种 atomic 的扁平堆叠（`pub turns: AtomicU64` / `pub hits: AtomicU64` / `pub mood_with_tag: AtomicU64` ...）。但这会丢失"cache 维度"、"mood_tag 维度"的语义层级——日后再加 third 组 metrics 时，扁平命名会冲突或啰嗦。嵌套子 struct 的代价是访问稍长（`counters.cache.turns` vs `counters.cache_turns`），收益是分组语义清晰。
- **暂留旧 type alias 给测试**：完全删 `CacheCountersStore` / `new_cache_counters()` 也行（测试改用 `new_process_counters().cache`），但要重写 5 个测试。`#[cfg(test)] pub` 是更小的改动——production 不见、测试可见、零 warning。规模到时（Iter 50+ 出现第三个 counter group 时）再考虑彻底删。
- **counters 默认初始化全 0**：`Default::default()` for ProcessCounters 自动给 cache / mood_tag 都 zero AtomicU64。不需要写显式构造器。
- **Tauri 命令签名统一**：4 个 stats 命令现在都 `State<ProcessCountersStore>`，前端只需要一个 invoke 类型——以后加新 stats 命令也走这条 State。前端 fetchLogs 的 Promise.all 还是分别调 4 个命令；如果想要一个 mega-stats 命令也可以，但现在分开让 RPC 边界对应 UI 边界更直观。
- **5-callsite plumbing 是真问题不是想象**：Iter 34（cache）和 Iter 40（mood_tag）两次都重做完全相同的 11 文件改动，第三次（如果是 token_usage）会让我开始 reflexively 抗拒加新 counter。这次合并后第三组 counter ≈ 5 行：1 sub-struct + 1 default + 1 get 命令 + 1 reset 命令 + 1 panel 渲染。值。
- **每次 LLM turn 这里没新 IO**：reorganize 不引入 perf 退化。`Arc<ProcessCounters>` clone 等同于之前两个 Arc clone 之和（指针 + ref count），不慢不快。

## Iter 41 设计要点（已实现）
- **复制 cache reset 模式**：Iter 35 已建立 reset 按钮的 UX 标准（小号低对比、乐观更新前端 state、tooltip 解释）。本次直接复用——保持两个 reset 按钮在工具栏旁同等视觉权重，让用户一目了然知道两条统计可独立重置。
- **不抽公共 reset 组件**：考虑过把"reset 按钮 + state"抽成 `<ResetableStat />` 组件，现在两份逻辑几乎重复。但 cache 和 mood_tag 的渲染细节（颜色、tooltip 文案、显示格式）差异让抽象需要太多 props，得不偿失。第三个 reset 出现时再考虑——这是 Iter 42 的合并方向自带的优化机会。
- **inline-flex 包裹规律**：button 紧贴 stats span，display: inline-flex + gap: 6px。和 Cache 那段一字不差——刻意保持视觉对仗，让 panel 看起来"对称且整齐"。

## Iter 40 设计要点（已实现）
- **取代 Iter 12b 的"实机交互测试"**：12b 一直挂着无法在自动化会话中完成。把"格式遵守率"做成 panel 一直可见的指标后，这个统计随着每次 LLM turn 自动累计，用户实机跑应用时打开 panel 就看到——比专门做一次"测试"更可持续，也消除了 12b 的存在意义（合并进 12b TODO）。
- **三档统计而非两档**：除 with_tag / without_tag 还加 no_mood，因为"还没记录过 mood"是真实存在的常态（首次启动、第一次 proactive 之前）。这一档不参与 ratio 分母，避免初期"100% no_mood = 0% 命中率"的误导。
- **read_mood_for_event 签名改为接 &ToolContext**：之前接 (&LogStore, &str)。改为 ctx 后函数能拿到所有它需要的东西（log store + counters），调用站省两次 inner().clone()。这是"小函数应该接它需要的全部 context"原则的应用。
- **重新走一遍 ToolContext field 加字段流程**：和 Iter 34 添加 cache_counters 同款 6 步——struct 字段 / new / from_states / for_test / 5 callsite / 反向 plumbing 通到 lib.rs。每一步都是机械的，但加一遍仍然要碰 11 个文件。这种"reusable plumbing pattern"如果再来一次（比如下次加 token_usage_counters），考虑是不是要把这些 counter 都装进一个总的 `ProcessCounters` struct 减少散布。Iter 41 / 42 时再决定。
- **不写复杂集成测试**：read_mood_for_event 依赖 disk read，单测要 mock memory 系统重。我满足于：(a) atomic counter 单测覆盖低层；(b) 现有 mood::tests 覆盖 parse 逻辑；(c) cargo check 把 plumbing 错误兜住；(d) 实机用 panel 实时看真值更有说服力。

## Iter 39 设计要点（已实现）
- **前端映射 vs 后端中文文案**：选前者。原因：(a) 后端 reason 现在是稳定的语义 key（"disabled" / "quiet_hours" / ...），改成中文文案就把 UI 语言耦合进协议；(b) 加新语言、做 i18n 时只动前端表；(c) 后端日志依然英文便于 grep；(d) reason 字符串可以同时作 enum 用（panel 旁的"按 reason 过滤决策"功能就靠英文 key）。
- **分层翻译策略**：Silent 是 enum-like → 一对一 switch；Skip 是 prefix + 动态参数 → startsWith 匹配 + 替换前缀保数字；Run 已经结构化无需翻译。每个 kind 一种翻译策略，简洁。
- **未识别 fallback to 原文**：`default: return reason` 让未来后端加新 Silent 值（比如 `respect_focus_mode 关`）UI 不会突然空白，而是显示英文 key——降级体验合理。
- **剥离 "Proactive: skip — " 前缀**：后端日志里这个前缀有用（grep 时能锁定来源），但 UI 已经用颜色 + KIND 列标识"这是 Skip"，重复信息只是噪音。前端独立优化呈现，不需要改后端。
- **不写测试**：纯字符串映射，没有边界 / 分支风险，cargo 也无新东西要验。tsc 通过 = 类型/语法对，已经是足够防线。

## Iter 38 设计要点（已实现）
- **`Silent { reason: &'static str }` 而非 String**：silent 的原因都是固定枚举值（"disabled" / "quiet_hours" / "idle_below_threshold"），用 static str 零分配。Skip 才用 String 因为它有动态信息（cooldown 还差几秒）。这种"按需选择存储成本"细致但值得。
- **决定记录在 dispatch 之前**：先记录、再 dispatch。如果先 dispatch（特别是 Run 路径会跑 LLM 几秒），到记录时 timestamp 就漂了；记录失败也可能让 dispatch 提前继续。先记录顺序更可靠，对 Silent/Skip 也无延迟。
- **VecDeque + push_back / pop_front**：ring buffer 标准实现。`while len > CAP { pop_front }` 在 push 后判，比 `if len == CAP { pop_front } push_back` 更不容易踩 off-by-one。
- **CAP=10 而非 5**：原 TODO 说 5。但 ring buffer 在 panel 上只占小高度（120px），10 条提供更多上下文（一小时左右数据 / 看到 cooldown 后等多久 Run）。代价 ≈ 10 * 100 字节 = 1KB，可忽略。
- **kindColor 三色编码**：Run 绿（"成功打通了！"），Skip 橙（"有原因不说"），Silent 灰（"安静"）。颜色直接映射到"用户最想关心的程度"——Skip 中的 reason 通常是用户能配置的（cooldown 等），Run 是用户期望的，Silent 是常态。
- **未给前端做中文映射**：reason 字符串原样显示。`disabled` / `quiet_hours` 对中文用户不那么友好。Iter 39 列了，但需要决定：(a) 后端直接给中文；(b) 前端建一份 mapping。后者更灵活但翻一份重复，前者简单但耦合。
- **不修改 LogStore，不与 logs 重叠**：决策 buffer 是独立的 ring，跟 LogStore 完全平行。LogStore 是 5000 行流水（详细但要 grep），决策 ring 是 10 条精炼（一眼即懂）。两个不同读者用例。

## Iter 37 设计要点（已实现）
- **空白占位让两列网格不踩空**：单字段套两列网格 (`twoColRow`) 看起来奇怪——左边一列右边一列空。用 `<div style={{ flex: 1 }} />` 占位让 NumberField 不被拉满整行，保留与上面 ProactiveConfig 网格的视觉对齐。这种"留白也是 layout 决策"的小用心。
- **0 = 不限**：约定继承自 trim 后端实现（`max == 0` 早 return）。Label 必须把这点直说，否则用户会以为 0 = "禁用 chat 历史"，意思相反。
- **panel 视图加说明文字**：modal 视图空间紧不放说明，但独立 panel 窗口宽度够，加一行 11px 浅灰说明文字（"桌面 chat 和 Telegram 都按此上限裁剪"）就解释清楚了 trim 的影响范围。这个差异化做法跟设备形态匹配 — modal 给老手快速调，panel 视图教新手。
- **复用 NumberField，零新组件**：Iter 27 抽出来的 NumberField 在这里收益体现——加一个新字段 ≈ 8 行 JSX，没新样板。如果当时不抽就是 17 行复制。
- **不接 reactive UI 提示当前 history 长度**：考虑过显示"当前会话已有 N 条历史，将裁剪到 M"，但这是 reactive 数据需要 polling，且对配置场景过度——用户配置时不想看运行时数据。

## Iter 36 设计要点（已实现）
- **trim 在后端而非 frontend**：让 useChat 保留全量历史用于 UI 展示（用户能滚回看完整对话），但发给后端时 backend 自己截断。这样"显示" vs "上下文" 解耦：前者是 UX，后者是经济性。
- **保留 N 条 + 头部 systems**：前导 system 消息（SOUL.md + 任何 mood/policy 注入）必须留——它们是人格基础。trim 只动中间的 user/assistant。"前导"定义为"从 0 开始连续的 system"，第一条非 system 之后再有 system 也算 history（telegram bot 的 inject_mood_note 就是这种情况，但 inject 是 trim 之后做的，所以测试不需要覆盖这种）。
- **跨 desktop/telegram 共享 trim_to_context**：先把 telegram 切片逻辑泛化到 `Vec<ChatMessage>`，让两条路径都调同一函数。代价：telegram 多一次 Value→ChatMessage 转换，但 50 条左右数据量可忽略。收益：以后再加新对话入口（discord、web 等）零成本接入。
- **0 == 关闭**：和 quiet_hours 同款约定。`max=0` 有意义——用户偏好"我自己控制 frontend 历史长度，后端别动"。比加 `enabled: bool` 字段干净。
- **AiConfig 加 max_context 而不是 chat 命令直接读 settings**：AiConfig 是"跑 LLM 需要的全部参数"的集合。把 trim cap 也归为这一层，后续 telegram / consolidate / proactive 任何地方建 AiConfig 都自动拿到，不需要每条路径都从 settings 单独读。
- **测试结构 5 case**：每个测试一个语义，不堆叠 assertion。`trim_zero_disables_gate` / `trim_below_cap_is_no_op` / `trim_drops_oldest_history_keeps_system` / `trim_preserves_multiple_leading_systems` / `trim_with_no_system_messages`——读 test 名字就能 derive 行为。
- **UI 拆 Iter 37 单独提交**：本 commit 已经动了 settings struct + 2 处后端 + 2 处前端类型，再加 UI 控件会让 diff 太杂。后端就位前提下，UI 是简单 NumberField + 一行 wiring。

## Iter 35 设计要点（已实现）
- **乐观更新前端 state**：`handleResetCacheStats` 调 invoke 后立刻 `setCacheStats({0, 0, 0})`，不等 1 秒 polling 间隔。这是常见 UX：用户点重置看到数字归零，否则会怀疑"按钮坏了？"。Tauri 命令返回 ok 后下次 polling 会重新读，校对一致——零风险乐观更新。
- **按钮和统计共生于 inline-flex**：把按钮放进与 Cache span 同一个 inline-flex 容器，间距 6px。这样按钮自然"属于"那段统计，而不是漂在工具栏里。重置按钮只有 cache 显示时才出现（已有 `total_calls > 0` 守卫覆盖整段）。
- **小号低对比按钮**：fontSize 10 / 浅边框 / 灰色文字。重置 cache stats 是 nuanced 操作（不应该常做），按钮压低视觉权重防止用户手滑误点。和"清空"按钮（13px、灰底）的视觉级别不同——清空日志反而更日常。
- **测试只验证语义不验证 Tauri 路由**：`cache_counters_can_be_reset_to_zero` 直接对 atomic store(0) 验证。Tauri 命令本身只是 plumbing（参数注入 + 调用函数），如果 plumbing 错了 cargo check 会先拦截。
- **counters 用 `store` 而非 `swap`**：reset 不关心旧值。`store(0, Relaxed)` 是最便宜的写。`swap(0)` 会返回旧值——这里没人需要。
- **UI 文字"重置"而非"清零"**：两者都行；"重置"听起来更"无副作用"，"清零"听起来更"破坏性"。前者更准确——这只是让计数器重新开始，不会影响其他状态。

## Iter 34 设计要点（已实现）
- **删 parse_cache_summary 而非保留作 fallback**：考虑过把 atomic 当主路径，log 解析当 fallback。但两条路径意味着两套测试、两份语义对账，长期负担大。彻底切换 + 删旧路径，简单。Iter 17 那条"dead code 该删"原则的同款落地。
- **field 加在 ToolContext，不引另一种参数传递**：本可以让 pipeline 多一个参数 `cache_counters: &CacheCountersStore`，避免改 ToolContext。但 4 个 caller 都要改 + 5 个 trait method + pipeline 签名 → 数百行 diff。把 counters 装进 ToolContext 才是真正"改一处管 5 处"。
- **`#[cfg(test)] for_test` 减小测试摩擦**：测试用 ToolContext 不需要 Tauri State。让 `for_test(log, shell)` 内部自动构造 fresh counters 比每个测试手动 `new_cache_counters()` 后传入更省事，且让 production 接口保持显式。
- **Relaxed ordering 仍然够**：counters 没参与任何同步关系（reader 是 panel UI，writer 是 pipeline 末尾，没人靠 counter 状态做后续决策）。Relaxed 是最便宜的内存序。
- **summary 0 case 不 bump**：和 Iter 30 同款决定——0 的 turn 不是真"有 cache 行为"的 turn，纳入会污染分母（"100 个 turn 50% 命中率" vs "30 个 turn 70% 命中率"，前者把没有缓存调用的 turn 也算上误导）。
- **counters 永不 reset**：当前没 reset 接口。pet 重启会清零；运行期间无法手动归零。Iter 35 计划加按钮——用户长期跑会想看新窗口的统计。

## Iter 33a 设计要点（已实现）
- **TODO 描述错了，先纠正**：原 TODO 说 LogStore 是 unbounded，但读代码发现已有 500 行硬限。这种"基于记忆而非阅读源码"的 TODO 错误偶尔会出现。修正记录在 DONE.md 让以后看 TODO 流水的人不会困惑。
- **常量化魔法数**：5000 直接换 500 不算改进；命名 + doc comment 才是。`MAX_LOG_LINES` 让阅读者一眼明白意图，doc comment 量化说明 "5000 ≈ 几百个 turn"。
- **5000 不是 10000**：原 TODO 建议 10000。但 `Vec::drain(0..n)` 是 O(n+m)（n 是 drain 数，m 是剩余），cap 越大单次溢出越大也越贵。5000 平衡内存（~几 MB）和裁剪 cost。
- **on-disk 不限制**：app.log 是磁盘文件，os 层面不会因为它涨到几百 MB 就 OOM；用户也可以 `tail -f` 看完整历史。in-memory cap 主要保护进程 RSS，不该把磁盘也限。
- **拆分 Iter 33**：原 TODO 包了两件事——cap + cache 累计独立。后者要改 ToolContext 签名 + 4 个 caller，太大单 iter。拆 33a/34 让每个 commit 单一职责。
- **drain 测试覆盖边界**：`MAX_LOG_LINES + 50` 这种刚溢出的情况比 +1 更能暴露 off-by-one。验证 newest 和 oldest 分别正确，比"len == cap"更有信息量。

## Iter 32 设计要点（已实现）
- **复用现有 polling 周期**：PanelDebug 已经每 1 秒 fetch 一次日志，`get_cache_stats` 也搭这个频率不需要单独的 setInterval。`Promise.all` 让两个 IPC 并行而不是串行——同样的整体延迟。
- **total_calls=0 时不渲染**：UI 初始打开 + 还没跑过任何 LLM turn 时，渲染 "Cache 0/0 (NaN%)" 既丑也无信息量。`{total > 0 && <span>...</span>}` 一行守卫掉。
- **等宽字体 + 蓝色**：把统计跟旁边的"日志条数"在视觉上区分开。等宽是因为数字会跳变（0/0 → 1/3 → 5/9...），等宽避免每次更新让旁边内容抖动。
- **tooltip 写口语而非缩写**：`Cache 5/9 (56%) · 3 turns` 是技术性短表达；hover tooltip 写 "3 次 LLM turn 中累计触发了 9 次环境工具调用，其中 5 次命中缓存" — 让不熟悉术语的用户也能 figure out 含义。
- **不在 chat / settings panel 显示**：cache 统计是 debug 性质的信息，放在 PanelDebug 最合适。primary chat 路径用户不该看到工程指标。
- **后续陷阱预演**：Iter 33 提到 LogStore 无 size cap。当前 stats 完全靠日志解析，如果日志被截尾旧的 summary 行就消失。短期不是问题（LogStore 在 RAM 里，`clear_logs` 是用户主动），但长跑会渐进失真。Iter 33 可能要考虑把 turns/hits/calls 搬到 LogStore 旁边做 atomic 累计——cache_stats 解析当 fallback。

## Iter 31 设计要点（已实现）
- **解析 log 行而非外部 atomic 累计器**：另一个选择是给 ToolRegistry 加全局 atomic（cross-turn 累计）。但 registry 是 per-turn 重建的，跨 turn 累计需要把状态搬到 app state 层级——引入新的 `Mutex<CacheCounters>` Tauri State。解析现有 log 行避免新状态：(a) 利用了 LogStore 已经存在的"按 turn 分行"结构；(b) clear_logs 自然清空累计；(c) 新加 turn 不需要任何代码改动来纳入。
- **parser 容错且严格**：`parse_cache_summary` 不匹配 → 返 None 而不是默认 (0, 0)。这样统计不会被无效行污染。测试里 5 个 case 4 个是 negative，确认 parser 不会被相关但不匹配的行（"Tool call: ..."）误吃。
- **`turns` 字段定义为"含至少一次 cacheable 调用的 turn 数"**：因为 log_cache_summary 在 total=0 时不打印，所以解析端看到的"行数"自然只算非空 turn。这个语义对用户更有用——"宠物有过 N 次环境感知决策，命中率 P%"——比"系统跑了 N 次 LLM turn"更直观。
- **CacheStats 三字段而非 derived hit_ratio**：本可以再加 `hit_ratio: f64`。但前端拿到 hits/total 自己除一下更灵活（精度、显示格式都由 UI 决定）。Rust 侧只送原始数据。
- **保 string-based parsing 简单**：用 `split("Tool cache summary:")` 找 marker，再 `split('/').nth()` 取 H/T，没引正则库。日志格式我们自己控制，正则反而过度。如果将来格式漂移，测试会先失败。

## Iter 30 设计要点（已实现）
- **AtomicU64 + Relaxed**：cache 计数器不参与任何同步——它们只用于事后统计。Relaxed ordering 是最便宜的内存序，性能影响可忽略。Acquire/Release 这种关系性 ordering 在这里是无意义的负担。
- **0/0 不打日志**：silent proactive tick（gate 先 short-circuit 没有跑 LLM）就完全不会走到 pipeline，那条路径根本碰不到 summary 行。但 LLM 跑了但模型没调任何 cacheable 工具的情况存在——这时也跳过 summary，避免日志噪音。
- **summary 在成功分支而非 finally**：错误路径（pipeline 抛错）不需要 summary——错误本身就是日志重点，再叠一行命中率反而干扰。Final response 分支是"正常结束"的唯一返回点，加在那里覆盖正常路径就够。
- **覆盖全调用者无需各自改**：`run_chat_pipeline` 是所有 4 条 LLM 路径的公共底座。改一处底座就够。这是 Iter 18 那种"guard 列表 + 单一 sleep"工程模式的同类好处——把横切关注点集中在 hub。
- **cache_stats() pub 是为以后铺路**：现在没有 caller 用 `cache_stats()`，但暴露出来不增加 API 表面成本，且 Iter 31 设想的"面板可视化"会直接读它。比写完面板再回来加访问器更顺。

## Iter 29 设计要点（已实现）
- **行为指引而非实现细节**：原 TODO 措辞是"告诉 LLM 重复调用会被 dedupe"，但 LLM 不需要知道我们怎么做的——它只需要知道做什么。改成"相信首次返回值"对模型更直接、不让它分心去推理工程层。这是 prompt 工程一个反复出现的原则。
- **不在 reactive chat 同样加**：`inject_mood_note` 是反应式聊天的注入，那里用户可能间隔几分钟分两次问"现在天气怎样"——cache 已经过期、LLM 也理应 re-query。不加这条规则，让反应式 chat 保持灵活。
- **半角引号陷阱**：用 ASCII `"再确认一下"` 在 Rust format!() 中等于"提前关闭字符串"。换成中文全角「」既符合中文排版习惯，又避开了语法陷阱。这种坑代码 review 时不容易发现，cargo 立刻 fail 是好事。
- **cache 默认无副作用**：本迭代纯 prompt，没改任何 Rust 行为。Iter 28 的 cache 已经在跑，无论 LLM 是否守这条规则，重复调用都不会真的产生 IO。这条 prompt 是给 LLM 内部推理压力减负——它若按规则做，就不必为每次工具调用都"思考要不要再确认"了。

## Iter 28 设计要点（已实现）
- **白名单 opt-in 而非 opt-out**：缓存默认应该是关——任何"被默认缓存"的工具都需要显式判断它是否真的幂等。把 `CACHEABLE_TOOLS` 写成短列表 + 注释强调"never add mutating tools"，让加新缓存工具变成需要刻意决策的动作。
- **registry-scoped 而非全局缓存**：`ToolRegistry` 在每次 `run_chat_pipeline` 里 new 一遍 → 缓存自动 per-turn。如果做全局 LRU 缓存反而要操心 invalidation（"用户 30 秒后再问一次天气，旧值还该用吗？"）。当前设计 0 invalidation 复杂度。
- **测试用 CountingTool mock 而不是真工具**：真 `get_weather` 要打 wttr.in 网络，单测不该；引用 `httpmock` 等 dev-dep 又重。手写 5 行的 CountingTool 内部测试用，最轻量。
- **with_tools 私有而非 pub**：考虑过 `pub fn with_tools(...)` 让外部也能定制工具列表，但现在没有这种调用者。`#[cfg(test)]` + 同 mod 自由访问 = 最小 API 表面。哪天真有人需要再升级到 pub。
- **cache_key 用 `name|args` 字符串而非 (name, args) tuple**：Tuple key 类型签名更精确，但 `HashMap<(String, String), String>` 多两个 String 分配。单字符串 key + `|` 分隔同样可靠（工具名不含 `|`），更便宜。如果哪天工具名能含 `|` 再换。
- **缓存值是 result string**：result 是工具的 JSON 字符串输出，缓存它就够了。不是缓存执行——重要边界，因为 `execute(arguments, ctx)` 里包含 `ctx.log(...)` 副作用。"被 cache 的工具调用不再写日志"是预期的——首次已经 log 过了，后续 hit 单独记 "cache hit" 行就够了。
- **未来可能的扩展**：(a) 给 cache 加 size cap 防 LLM 疯狂尝试不同 args 把内存撑爆；(b) per-tool TTL（天气数据 1 小时之内有效跨 turn 也行）；(c) MCP 工具白名单——但需要工具自报"我是只读"。Iter 28 这版是最小 viable。

## Iter 27 设计要点（已实现）
- **wrapper 模式 vs 直接传 props**：抽共享组件最常见的失败模式是"call site 比之前更啰嗦"。如果让每个 NumberField 调用都写 `labelStyle={labelStyle} inputStyle={inputStyle}`，8 处 + 2 处 = 16 处样板重复——比抽之前还差。本地 wrapper 把样式绑定一次，call site 完全不用改。这是"trade DRY in styles for DRY in calls"的典型选择，前者重要性低（一个文件内 const 引用）。
- **labelStyle/inputStyle 作 props 而非 fixed**：起初想直接在 SharedNumberField 里硬编码 inputStyle。但两个 panel 实际样式差异有几处（边框颜色、字号），强行统一会破坏视觉连贯性。让样式可注入是设计开放原则的实例：组件知道"这是个数字字段"，不该越权决定外观。
- **顺手清理冗余 TODO**：发现 TODO.md 里有一条"PanelSettings.tsx 接 Proactive/Consolidate"上一轮已经做完但忘了删。这种 stale 项每过几天就会让人怀疑"我是不是漏了什么"。看到就清。
- **确认 tsc 而非加测试**：UI 重构无逻辑变化，类型系统就是最好的回归。type-check 通过 = 调用 site 至少没看错 prop 名字。

## PanelSettings 补 Proactive/Consolidate UI 设计要点（已实现）
- **跳 Iter 26 选这个**：Iter 26 是给 IDEA 写一段 known-limitation 文档 + 加一个验证启动时 active 行为的测试。但 `first_observation_active_logs_on` 测试已经在 Iter 23 覆盖了启动行为；只缺一段说明文字而已——价值低于"补全 panel 形式视图"这个真实的功能缺口。把 Iter 26 标记为 obsolete，留给 IDEA 章节补一句即可。
- **复制而不抽公共组件**：第一反应是把 `NumberField`（小窗）和 `PanelNumberField`（panel）合一。但两者样式上下文略有差异（小窗的 inputStyle 字号 13 / 边框 #ddd；panel 的字号略小、整体浅色阴影更深）。强行合并要么引入 props 复杂度，要么破坏一边的视觉。先复制，等真需要第三处再抽。Iter 27 列着待办，下次有触发再做。
- **panel 视图 vs modal 视图**：项目有两套设置 UI——小窗右键弹的 SettingsPanel modal（轻便、260–300 px 宽）+ 独立 panel 窗口的 PanelSettings（重量、能编辑 MCP/Telegram 这种长字段）。共享 `useSettings.ts` 的 `AppSettings` 类型保证字段不漂移；UI 形态可以独立演化。这次让 panel 视图也 catch up 到 modal 已有的字段。
- **顺手覆盖 Iter 21+22 加的字段**：原 TODO 只说"接 Proactive / Consolidate"。但 Iter 20 的 quiet_hours、Iter 21 的 respect_focus_mode 也都还没在 panel 视图露出过——一并补完，避免未来发现"诶这个字段在 modal 有 panel 没"。

## Iter 25 设计要点（已实现）
- **size-based 而非 time-based**：本可以"每月 1 号滚动一次"。但 size-based 有几个优点：(a) 实现简单（一次 metadata 调用比时间窗判断稳）；(b) 对低使用率用户友好（一年都没满 1MB 就不滚动）；(c) 高使用率用户也不会丢得太快（30k 行约一年）。time-based 适合"日志按月归档查阅"场景，本项目是给 LLM 看模式不是给人翻档案。
- **`with_extension` 陷阱**：`PathBuf::from("focus_history.log").with_extension("log.1")` 会得到 `focus_history.log.1`——但这是利用了 `with_extension` 的实现细节（"log.1"被当成新扩展，附在去掉旧扩展 "log" 后的 stem 上）。换成 `focus.txt` 就不灵了。直接 `OsStr::push(".1")` 是对路径文本追加，最稳。专门写测试 `rotated_path_handles_no_extension` 验证。
- **best-effort rotation**：`append_event` 里 `let _ = rotate_if_needed(...)`，吞掉错误。原因是 tracker 跑在后台，rotation 失败不该让 transition 丢失——大不了文件继续涨一会儿，下次 polls 再尝试。append 写本身的错误倒是 propagate 出去，因为那才是数据丢失。
- **只保留一代**：`.1` 之外不再有 `.2/.3`。设计假设是 LLM 周期性 consolidate 把"长期模式"提炼到 user_profile memory，原始日志只是给最近的 read_file 服务。一年前的具体 transition 时刻没价值。如果未来 Iter 有"年度复盘"需求再加多代。
- **覆盖 `.1` 是合规的**：测试 `rotation_overwrites_existing_dot_one` 显式验证。`tokio::fs::rename` 在目标存在时直接替换（POSIX 语义），不需要先 remove。
- **不引 tempfile**：用 `std::env::temp_dir() + nanos` 自建临时目录，节省一个 dev-dep。代价是清理靠 `let _ = remove_dir_all` 而非 RAII，偶尔可能残留——但 /tmp 本来就是 OS 周期清理的，不是问题。

## Iter 24 设计要点（已实现）
- **存在性检查决定是否注入**：consolidate prompt 是有限注意力。让 LLM 看到一段"读这个文件"的指令，但文件其实不存在，模型只会困惑——可能会去 read_file，得到空内容或错误，然后在总结里写一句"focus 数据不足"，浪费 tokens。`focus_history_hint()` 用 `path.exists()` 短路返空串，让 prompt 在新装环境保持简洁。
- **绝对路径而非 `~`**：tilde 由 shell 展开，但 read_file 是 Rust 端调用，不会走 shell。给 LLM 看实际可用的绝对路径（如 `/Users/moon/Library/Application Support/pet/focus_history.log`）能减少一次试错。
- **建议而非强制**：prompt 用"建议你用 read_file"、"如果数据足以总结...就 memory_edit"、"数据太少就先放着"这种条件化语言，给 LLM 判断空间。强制读+总结会让早期数据稀疏时也产生信息量低的 memory 条目。
- **明确价值取向**：`"一条结论性 memory 比一千行原始日志更有用"` 是给 LLM 的目标函数。不写它，LLM 可能把整段日志原样塞进 detail_content，反而让 memory 系统膨胀。这种"教 LLM 怎么判断"的 prompt 工程比纯描述任务更值。
- **路径计算用 `dirs::config_dir`**：与 focus_tracker 写入路径同源，保证一定能匹配。如果两边写不同 path，Iter 24 会指向一个永远不存在的位置——所以两边都用同一个库函数最稳。
- **闭环 Iter 23+24**：原始事件流 → 周期性总结 → 结论性 memory。这是个常见模式（log + summarizer），未来如果加更多事件流（active_window 历史、interaction 频率），可以套用同一架构。

## Iter 23 设计要点（已实现）
- **磁盘日志而非 memory 条目**：本来想用现有 memory 系统（`general/focus_history` 条目，detail_path 文件追加）。但 memory_edit 的 update 是整文件覆写不是追加，每次写 100 KB 历史不合适；而且 memory 索引应该是"宠物已经知道的事实"，不是"原始事件流"。日志文件 + 一条总结性 memory 条目（Iter 24 由 consolidate 写入）的两层结构更干净。
- **append 不读旧内容**：tracker 只关心 prev 和 curr 两个状态，不需要回放整个历史。这意味着重启后 last 是 None，第一次观察可能丢一个连续状态点——但 `first_observation_inactive_logs_nothing` 这条规则刚好让"启动时没开 focus"不留无意义记录；如果启动时正在 focus，会写一条 `on:xxx` 算作"我重启时这个状态在持续"，可接受。
- **classify_transition 是纯函数**：状态机就 4 种 case，写成纯函数后单测覆盖每条 + 空 name 退化共 7 case。日后调整规则（比如想忽略 < 30s 的瞬态切换）只动这一个函数 + 加测。
- **POLL_INTERVAL 60s 是平衡点**：1s 太频（每天 86400 次 IO 浪费）；5min 又会丢短时切换。60s 一天 1440 次 polls，对本就闲置的 tokio runtime 完全可承受。
- **不加 enabled 配置**：tracker 的 IO 成本 = 每分钟一次小 JSON 解析 + 至多一行写入。除非用户极度在意日志文件存在（隐私担忧），否则不需要开关。和 proactive/consolidate 那种会调 LLM 的 opt-in 不同。
- **后续 Iter 24 衔接**：本迭代只产数据。让 consolidate 主动读取并总结是分开的工作——保持每个 iter 单一职责，方便回滚。

## Iter 22 设计要点（已实现）
- **拆 IO 与解析**：和 mood/gate 同一套路。`parse_focus_status(&Value)` 是纯函数（输入 Value 输出 FocusStatus），`focus_status()` 是异步外壳负责读盘。这样测试 6 个 case 不用 mock 文件系统。
- **嵌套 and_then 而非 unwrap**：JSON 路径 `data[0].storeAssertionRecords[0].assertionDetails.assertionDetailsModeIdentifier` 5 层深，每层都可能缺失（macOS 版本差异）。`Option::and_then` 链是最简洁的"任一层失败就降级到 None"方式。
- **`rsplit('.').next()`**：identifier 形如 `com.apple.donotdisturb.mode.work`，要的就是最后一段。`rsplit` 反向迭代，`next()` 拿到第一个 → 也就是最后段。语义直白，无需正则。
- **focus_hint 在 mood_hint 之后**：模板里位置选择不是随便的——mood 是宠物自身状态，focus 是用户当前状态。先自己后用户的顺序更符合"我现在心情如何 → 用户在做什么 → 我该不该说话"的思考链。
- **`respect_focus_mode=true` 时这条注入不会触发**：这是有意的耦合。默认配置下 focus active → gate skip → 不到 run_proactive_turn。只有用户主动 opt out gate 才会让 LLM 看到 focus 名字。这种"用户可以渐进解锁更精细的行为"是温和的设计。
- **保留 `focus_mode_active` 作为薄 wrapper**：本来想全部迁到 focus_status，但 gate 代码只关心 active 不关心 name。让 focus_mode_active 继续存在 + 内部调 focus_status 是 Sequencially Better Patterns 教科书做法（旧 API 不破坏，新 API 更丰富）。
- **未实测**：和 Iter 21 一样依赖用户实机的 Focus 文件结构。代码层面 36 个 tests + cargo check 全过；解析逻辑保守 fail-soft，最坏情况是 name=None 不影响 prompt 整体可读。

## Iter 21 设计要点（已实现）
- **读 Assertions.json 而不是 osascript / shortcuts**：考虑过 `osascript -e 'tell application "System Events" ...'` 但 System Events 没有 focus 状态字段；考虑过 `shortcuts run "GetFocus"` 但需要用户先创建 shortcut；最终选 `~/Library/DoNotDisturb/DB/Assertions.json`，是 macOS 自己写的真相源，read-only 一次足够。代价：(a) Sonoma 之前路径或格式可能不同；(b) 文件可能无权限读（但极少见，通常用户级访问没问题）。
- **`Option<bool>` 三态而非 bool**：`Some(true)`=肯定 active；`Some(false)`=肯定不在；`None`=不知道（非 macOS、文件缺失、解析失败）。让 gate 逻辑能区分"不确定"和"确定不"——前者必须 fail open（不阻塞），不然非 macOS 用户永远卡死。和 input_idle 的 None 处理思路一致。
- **respect_focus_mode 默认 true**：用户的"勿扰"是非常明确的信号，宠物默认就该尊重。比 quiet_hours 默认值更确定——quiet_hours 是猜的（用户可能是夜猫子），focus 是用户主动按下的。
- **Skip vs Silent**：focus 通常不会持续整夜（用户用完会关），所以频率没夜里那么高，日志记录有审计价值（"哎，那个时间段宠物为啥没说话？哦原来开了 focus"）。和 quiet_hours 用 Silent 形成对照。
- **懒读文件**：`if cfg.respect_focus_mode { focus_mode_active().await }`，关闭设置时跳过文件 IO。这种"只在需要时检查"的优化在 evaluate_loop_tick 里有意义，因为它每 5 分钟左右跑一次。
- **测试覆盖三态 × 两设置**：4 case 完整覆盖（active+respect / active+!respect / inactive / unknown）；fail-open 行为是测试里最重要的不变量。
- **未实测**：跟 calendar 一样无法在本会话拿用户真实 focus 状态。代码层面 cargo + tests 通过；运行时验证留待用户实机。

## Iter 20 设计要点（已实现）
- **`hour` 注入而非函数内取**：`evaluate_pre_input_idle` 加参数 `hour: u8` 而不是内部 `chrono::Local::now()`。原因：测试要能控制时间。也避免 evaluate 函数变成"impure"（调系统时钟相当于隐式 IO）。`evaluate_loop_tick` 真要跑时再取小时。
- **`start == end` 表示关闭**：避免再加一个 `enabled` 布尔。约定上"00–00"是空区间，自然代表"无安静时段"。这是个能学的 UX 约定，且测试可见。
- **Silent 而非 Skip**：晚 11 点到早 7 点用户基本在睡觉，每隔几分钟 evaluate 一次都触发 quiet 分支。如果用 Skip 就一夜下来日志几百行噪音。Silent 直接静默，匹配 idle-below-threshold 的处理思路。
- **u8 而非 u16/枚举**：本可以用 `enum QuietWindow { Disabled, Active(u8, u8) }`，但 settings.rs 要 serde 序列化、前端要传 number 字段，多一层枚举抽象会让 TS 端跟着麻烦。两个 u8（0–23）+ "相等=关"约定 = 性能/可读/序列化都够好。
- **wrap_around 是 quiet hours 的核心边界**：默认值 23–7 必然 wrap。专门写了 `wraps_midnight` 测试覆盖 23/0/3/6/7/12/22 七个时间点，未来重构这条逻辑时一秒内就能验证回归。
- **NOON=12 在 hour 注入后变成必备**：原有 12 个 gate 测试都在 quiet 窗口外的"日间"运行。把 12 提为常量是表意改进——不写 `, 12)` 而写 `, NOON)`，读起来知道意图是"测试不关心时间"。

## Iter 19 设计要点（已实现）
- **拆 sync/async 而不是引 trait**：Iter 18 已经预留了"等真要测时再决定要不要 trait"。这次评估发现：4 道 gate 是纯数据，1 道有 IO。把数据 gate 抽成同步函数，IO gate 接 `Option<u64>` 由 caller 喂——比起引一个 `trait InputIdleProvider` 简单太多，测试也不必 mock 任何东西。
- **Result<(), LoopAction> 表达"短路 vs 通行"**：因为前段 gate 要么失败终止（返回 LoopAction）要么通过继续，用 `Result<(), LoopAction>` 直接对应这两种状态，比 `Option<LoopAction>` 语义更清晰（None vs Some(...)读者要解释含义，Ok/Err 自带方向）。
- **derive PartialEq + Debug 在 LoopAction**：本以为不需要，但 `assert_eq!(action, LoopAction::Silent)` 直接比 `match` 简洁。Debug 是为了 `panic!("expected Silent, got {:?}")` 失败信息用。两条 derive 一行搞定。
- **测试覆盖隐含规则**：`idle_threshold_seconds.max(60)` 这种 clamp 是个容易被未来重构者误删的东西。专门写一个 `idle_threshold_clamped_to_60_minimum` 用 30 配 10 的输入测出来，回归就稳了。
- **测试不要全是 happy path**：12 个 case 有 8 个是 negative path（短路 / Skip / clamp 等）。这些就是宠物"安静"的关键路径——主动开口逻辑的 bug 多半出在"该闭嘴时却开口"，必须先把 negative 测好。

## Iter 18 设计要点（已实现）
- **三态 enum vs 二态 Option<String>**：本来想把 `Silent` 和 `Skip` 合成一个 `Option<String>`（None=Silent, Some=Skip with reason）。能省 enum 但语义糊：Silent 和 Skip 在主循环里行为分支不同（一个不日志一个日志），enum 让分支显式更可读。
- **evaluate 函数纯而不副作用**：不直接写日志，把 reason 作为 String 返回让外层处理。这是为了下一个 iter 能直接对它写表驱动测试——纯函数测试零成本，函数内部 write_log 测起来就要 mock LogStore。
- **idle 不达标用 Silent 而不是 Skip("idle short")**：原代码也没在这个分支写日志——大多数 tick 是这个状态，每秒/每几分钟一行"现在还没到 idle 阈值"是日志噪音。Silent 显式表达了"这是预期常态，不必发声"。
- **单一 sleep 收尾**：原本 4 个 `tokio::time::sleep + continue` 散落在 if 分支里，每加一道 gate 都要复制粘贴这两行，是 bug 多发区（漏掉 sleep 就忙循环了）。统一到外层后多加一道 gate 只关心 LoopAction 怎么返回。
- **log_store 懒取**：只在 Skip 分支取 LogStore。原版每 tick 都先 clone 一遍 Arc 备用，Silent 路径上是浪费。Arc clone 成本极低但风格更纯。
- **不强引入 trait/dependency injection**：本可以让 evaluate_loop_tick 接 trait `ClockSnapshot + InputIdle` 让单测更彻底，但当前 5 道 guard 有 4 道是纯数据 (settings + snap)，1 道有 IO (input_idle)。引入 trait 会让代码现在就过度抽象，IDEA 里把测试相关的设计推到 Iter 19，那时再决定是不是要 trait。

## Iter 17 设计要点（已实现）
- **删除而非 `#[allow(dead_code)]`**：能删的就删而不是抑制告警。allow 攻略让代码"看起来在用"，未来真的需要这接口时还得回来重新审视；删了反而清晰——再要时直接复原 git 历史。
- **告警基线归零的价值**：项目从来"两条无关 warning"很容易让心理上对告警麻木。归零之后任何新告警都立刻显眼，等于免费做了一道质量门。两分钟收益，长期回报高。
- **不是所有 dead code 都该立删**：这两条都是已存在挺久的"看起来像合理 API 但没人用"。如果是新加的还在演进的代码，dead_code 警告可能只意味着"还没接通"，留着合理。删除门槛是"功能边界已稳，且确实没人调用"。

## Iter 16 设计要点（已实现）
- **顶层模块而不是子模块**：放在 `crate::mood` 而非 `proactive::mood`。理由：mood 不是 proactive 的 sub-concept，是 4 条入口共用的横切关注点。挂在 proactive 下面会让 chat / telegram / consolidate 的 import 路径暗示不正确的层级关系。
- **常量从 private 升级 pub**：`MOOD_CATEGORY` / `MOOD_TITLE` 原来是 proactive.rs 内部 const，现在跨模块用就得 `pub`。这是必要的 API 表面增加；好处是任何想自查 mood 是不是被规则覆盖的代码都能直接 `if title == mood::MOOD_TITLE`，不用拼字符串。
- **测试位置随函数走**：`parse_mood_string` 的测试整块跟着搬到 mood.rs 里。Rust 习惯是测试和实现紧挨着，搬到新文件后路径变成 `mood::tests::*`——`cargo test --lib proactive` 这种过滤会突然找不到测试。这次换成 `cargo test --lib`（全跑）确认。
- **import 块的次生整理**：proactive.rs 里 `use` 块之前因为 helper 就地添加被切成两段，迁移时一并恢复成一整块。这种"清理趁手做"的小修很值得——下次再读这文件不会被乱序刺到眼睛。
- **没改外部行为**：纯重构。callsite 数量、调用形式、运行时表现完全不变。`cargo check` + 8 个 mood unit test + 二话不说就过 = 安全的搬家。

## Iter 15 设计要点（已实现）
- **helper 在 proactive.rs 而非新模块**：考虑过新建 `mood.rs` 把所有 mood 相关的东西（常量、parse、read、event helper）打包过去——更对称、更准。但那会一次改 4 个文件的 import 路径，又把 read_current_mood / read_current_mood_parsed 也连带搬走。本次目标是去重而非搬家，所以仍把 helper 放 proactive.rs，搬家拆为 Iter 16。
- **签名选 `&LogStore` 而非 `&Arc<Mutex<...>>`**：直接接 `&LogStore` 让调用方负责拿到引用，方法内部用 `write_log(&store.0, ...)` 即可。这样：(a) callsite 写法统一；(b) 不需要 telegram 那种手写 lock；(c) chat.rs 用 `State::inner()` 转换是单行，简单。
- **保留 consolidate 的 mood 删除监控**：helper 只处理"missing prefix"。consolidate 还有"mood 被删"这个独特检查，因此不能完全替换 — helper 抽公共部分，独有逻辑留在 callsite 旁边。这是合适的边界：DRY 但不强行 over-abstract。
- **第二次 log_store 拷贝在 proactive**：`run_proactive_turn` 在前段把 log_store move 进 ToolContext，到末尾用 helper 时只能再 clone 一次。一个 `Arc<Mutex<...>>` 的 clone 成本可忽略。如果在意可以重排：先记 log_store 引用，后期再用——但那要重写函数顶部，不值。
- **rule of three → four 触发重构**：Iter 14 已经记下这个信号。本次从 4 个复制点减到 4 个一行调用 + 1 个 helper 定义。第五个入口加进来时几乎零成本。

## Iter 14 设计要点（已实现）
- **保护 current_mood 在 prompt 而不是代码层**：本想在 Rust 里拦截"删除 current_mood"调用，但那意味着要给 memory_edit 加白名单，相当于把规则散到工具层、不优雅。Prompt 里加一句"绝对不要删"成本最低，且 LLM 大概率会遵守。代价：偶发违规需要靠日志报警发现——所以加了 WARNING 日志监控。
- **mood_before / mood_after 对比**：单纯读 mood 之后判断 None 不够——可能本来就是 None。要对比"之前有现在没有"才是真信号。Snapshot 模式简单可靠。
- **消极守护多于激进重写**：如果 LLM 真的把 current_mood 删了，本可以从 mood_before 自动重建。但那会让 LLM 觉得"反正会被还原"反复尝试删除。先不还原，靠日志告警 + prompt 强调，多次违规再考虑硬恢复。
- **第四条入口的 DRY 信号**：现在四处入口写几乎相同的"读 mood + 缺前缀日志 + emit chat-done"代码块。本次还容忍着复制，但已经到了 DRY 阈值——把这块抽成一个 helper 函数应该是下一个迭代（列入 Iter 15）。三次复制是 OK 的（Rule of Three），第四次复制就该重构了。
- **consolidate 的 emit 价值**：consolidate 只可能"refine" mood text 而不会改变核心情绪——大多数情况下 emit 出来的 motion 跟之前一样。但 emit 是状态对账机制：如果未来加了"重启时也跑一次 consolidate"之类的功能，这个 emit 就会让前端在启动后立刻同步到正确状态。

## Iter 13 设计要点（已实现）
- **复用 inject_mood_note 而不是搬到 chat pipeline 里**：又一次拒绝把 augment 塞进 `run_chat_pipeline`。理由还是相同：proactive 已自构造 mood 上下文，pipeline 内部加 inject 会重复。让"哪条入口需要 mood 注入"由调用者决定，每个调用者一行 `let msgs = inject_mood_note(msgs);` 比 pipeline 长出一个分支判断更清晰。代价是 telegram 自己也要写一遍这行——可接受。
- **Telegram 也 emit chat-done**：本来犹豫——Telegram 用户不在桌面前，desktop Live2D 动起来意义何在？想了想，意义恰恰在于："你在 Telegram 上跟宠物聊完 mood 变了，回到桌面看到的状态是连贯的"，而不是回桌面后宠物还停在两小时前的样子。所以照样 emit。
- **AppHandle 一路注入到 HandlerState**：Tauri 的 `AppHandle` 是 `Clone`，安全跨 spawn 边界。把它放进 Arc<HandlerState> 既共享给所有消息 handler 也避免每次 emit 时重建。
- **三条入口行为对称的代价**：现在有 4 处会读取/影响 mood（proactive / chat 命令 / telegram / consolidate）。consolidate 还没接缺前缀监控，列入 Iter 14。每多一条入口就要小心两类对称：(a) prompt 里给 LLM 的 mood 上下文；(b) 跑完后是否 emit 事件让动画跟上。第 (b) 项目前 consolidate 跑完没 emit——它批量改 mood 后理论上前端应该能感知，列入 Iter 14。
- **reconnect_telegram 也得改**：动态重连这条路径容易被忽略。如果只改 lib.rs 不改它，重连后的新 bot 实例就会 emit 不出 chat-done。这个 mistake 早期靠 `cargo check` 抓到了，是好运。

## Iter 12a 设计要点（已实现）
- **拆纯函数为了可测**：原 `read_current_mood_parsed` 把读盘和解析耦合，要测就得 mock 文件系统。把 25 行解析逻辑剥到 `parse_mood_string(&str)`，零 IO 依赖，加测一气呵成。代价就是多一层调用——可忽略。
- **测试覆盖边界而非 happy path**：标准格式只测一个，剩余 7 个全是边界（空 / 超长 / 未闭合 / 前导空白 / 空文本…）。这种解析函数最大风险是"模型胡写出诡异输入"，不是"格式正确解错"，所以测试结构刻意倒向 negative path。
- **监控点放 backend 而非 frontend**：缺前缀的 fallback 是前端做的，但日志在后端打。原因：(a) 后端有 LogStore 的现成基础设施；(b) 我们想监控的是 LLM 行为（写 mood 时是否守约）而不是前端 fallback 是否生效；(c) backend 更靠近事实源。
- **不写 metrics counter**：本来想加个 `AtomicU64` 累计两个数（has_prefix / missing），但 (a) Tauri 没有暴露 metrics 端点，(b) 用户最直接的方式还是 `grep "missing \[motion"` log file。计数器要等真有可视化需求再加。
- **Iter 12 拆成 12a 12b**：本次只做了能在无交互环境完成的部分（测试 + 监控钩子）。"实机跑一次看模型守不守约"是需要真用户 + 真 LLM 的事，单独留为 12b 让用户实机跑后判断。

## Iter 11 设计要点（已实现）
- **augment 在 chat 命令而不是 run_chat_pipeline**：本来想在 pipeline 里做，让所有调用者（chat / telegram / proactive）共享。但 proactive 已经手动构造 mood 上下文，再 inject 一次会重复；telegram 暂不在范围。所以放在 `chat()` tauri 命令这个最局部的位置，影响范围小。后续 Iter 13 可以把 telegram 也接上。
- **system 消息插在 SOUL 后**：放最前会破坏 SOUL 的"人格基准"地位；放最后某些模型对尾部 system 处理不一致。插在第一个非 system 之前最安全——SOUL 还在第 0 位，mood note 紧随其后，对话历史和最新 user message 顺位顺延。
- **不影响前端持久化**：useChat 自己持有 messages 副本并存盘，augmented 只在 Rust 内存里跑一次。这意味着同一会话在不同时间发起的请求都会拿到当前最新 mood，而不是被某次旧 mood 锁死——很重要的特性，因为 proactive 会在两次反应式之间偷偷改 mood。
- **明确告诉模型"没变就别更新"**：消极地"允许更新"会让 LLM 倾向于每次都改一下（让自己显得有进展）。不必要的写入会让 mood 漂移得太快，反而失去连续性。所以 prompt 里直接写"心情没变就不用更新"。
- **mood 4 组映射写在 prompt 里**：避免模型猜 group 名对应啥情绪。多写几个字换可靠性，值。

## Iter 10 设计要点（已实现）
- **`[motion: X]` 前缀而不是单独 memory 条目**：曾考虑用一个独立 memory 项 `motion_hint` 让 LLM 写"当前推荐动作"。但那要求 LLM 对每次主动开口都做两次 memory_edit（一次 mood，一次 motion），调用次数翻倍且容易漏掉一个。前缀方式：一次 memory_edit 同时承载语义和动作，自然耦合。
- **结构化 vs 自由 mood**：之前 mood 完全自由；现在加结构会不会让人格描述变僵硬？前缀只在 description 开头几个字符，自由文字部分仍然完全开放，不影响 mood 表达力。代价是 LLM 学这个约定要 prompt 强调一句。
- **保持向后兼容**：`read_current_mood_parsed` 在前缀缺失时仍返回 `(raw_text, None)`，前端 fallback 到关键词匹配。这样：(a) 旧的 mood 数据（已写入但没前缀）继续工作；(b) 模型某次"忘了"前缀也不致动作变怪；(c) 给将来切换不同模型预留缓冲。
- **VALID_GROUPS 白名单**：LLM 可能写 `[motion: Bow]` / `[motion: tap]`（大小写） / `[motion: 微笑]`。前端用 Set 严格匹配大小写敏感的合法名，否则降级到关键词。这把"模型胡写"的影响隔离在前端单文件里。
- **mood prompt 注入用 text 而非 raw**：注入 `[motion: Tap] 看用户在写代码，替他高兴` 整段会让下一轮 LLM 看到 `[motion: Tap]` 这种"meta 信息"，可能把它误解为对话内容。strip 掉后 LLM 看到的就是干净的"看用户在写代码，替他高兴"，更接近自然记忆。
- **未实测端到端**：和此前 UI 类的 iter 一样，本机不开 dev server，所以"LLM 是不是真的会按格式写"靠 prompt 强约束 + fallback 兜底。Iter 12 留作监控点。

## Iter 9 设计要点（已实现）
- **后端 emit 而不是前端拉**：选项 A = useChat 在 done 事件后调 memory_list 拉 mood；选项 B = chat 命令完成后从 Rust emit 一个事件。选 B 因为：(a) 与 `proactive-message` 对称，前端只关心"事件 → 动作"的映射；(b) Rust 已有 `read_current_mood` 函数可以复用；(c) 避免一次额外 IPC，且消除"前端拉之前 mood 又被改了"的 race。
- **chat 命令加 AppHandle**：Tauri 命令可以直接通过参数注入 AppHandle，无需 manage state。最小侵入。
- **反应式聊天暂不更新 mood，仅消费**：reactive chat 的 prompt 不要求 LLM 调 memory_edit 更新 mood。所以 mood 在反应式对话中 stale。仍然 emit `chat-done` 是为了"用户跟宠物聊天时角色也得动一动"——动作反馈是用户体验问题，不依赖 mood 是否最新。让 reactive 也能更新 mood 列为 Iter 11 单独做。
- **前端共用 triggerMotion**：把 motion 触发逻辑从内联 listener 提到模块级辅助函数，两个事件源调同一个函数。这样如果未来再加事件源（比如某种"开机动画"事件）也只是再写一行 listener。
- **mood 仍可能是 None**：第一次启动且 LLM 还没写过 mood 时，event payload 的 mood 是 null。`pickMotionGroup(null)` 会返回 Tap 作为 fallback，符合 Iter 8 已经定下的"宁可动也不要静默"原则。

## Iter 8 设计要点（已实现）
- **mood 随 message 一起 emit，而不是前端再查一次 memory**：本来想让前端收到 proactive-message 后再调 `memory_list` 查 mood，但那样：(a) 多一次 IPC 开销；(b) 存在 race——LLM 在下一 tick 刚好又改了 mood 怎么办？把 mood 嵌进 ProactiveMessage payload 里就锁死了"这条消息对应的当时 mood 状态"，时间一致性更好。
- **关键词列表保守且简短**：第一版只覆盖最常见的 mood 短语，宁愿漏匹配（fallback 到默认动作）也不要错配。LLM 写的 mood 是中文自由文本，正则/embedding 匹配都能做但都太重；硬编码列表是 80/20 解。后续 Iter 10 可以把"挑动作"职责丢给 LLM 自己做。
- **没匹配时也播 Tap 而不是 Idle**：默认 Idle 对用户感觉像 bug——主动开口了但角色没动。Tap 至少给出一个温和的可见反馈，比"主动说话却毫无动作"好。
- **miku 只有 motion 没 expression**：检查 model3.json 才发现这点。原 TODO 说"表情和动作"但模型不支持表情资源，所以本迭代退到 motion-only，并在 DONE.md 写明限制以便日后换模型时知道为什么。
- **priority=2 (NORMAL)**：让 motion 自然播完，但用户主动调出来的动作（如 tap 互动）还能覆盖它。priority=3 (FORCE) 会打断一切，过于霸道。
- **未实测视觉效果**：本机不开 dev server。这是已知风险，但 motion API 调用形式与 `pixi-live2d-display` 文档一致，且失败有 try/catch 兜底，最坏情况是不动而不是崩溃。

## 设置面板·Proactive/Consolidate UI 设计要点（已实现）
- **跳过 Iter 7c 选这个**：原优先级是 Iter 7c (系统通知) > Iter 8 > 设置面板。但 Iter 7c 需要 Full Disk Access + 私有 sqlite schema，单次迭代风险大；Iter 8 需要熟悉 Live2D model 的表情资源，调研成本高。设置面板是"已实现的功能首先要可用"——前几迭代加的 proactive/consolidate 现在只能改 config.yaml，普通用户不会用，把开关暴露出来才能让前面的工作真正落地。
- **NumberField 而非 slider**：滑条占垂直空间多，且数值范围跨度大（60s ~ 7200s），滑条精度低。`<input type="number">` 紧凑、可键入精确值、有原生 min 校验。代价是不直观——用文字提示来弥补。
- **两列网格而非单列**：4 个数字字段单列要 4 行 × 60px = 240px 垂直空间，两列两行少一半。模态最高 560px 也勉强。
- **PanelSettings.tsx 也要改初值**：第一次 commit 漏改，TypeScript 编译就会卡住。这反映出当前前端有两个 settings 入口（小窗 + 面板视图）共享 `AppSettings` 类型——一加字段就要两处都补。后续可以让两个视图共享一个 `defaultSettings()` 工厂函数，但不在本迭代范围。
- **未跑实际 UI 测试**：本次只做静态类型检查 (tsc) + 后端编译。dev server 没启 — 这是个限制，TS 通过不等于交互正确。后续 Iter 8 因为涉及 Live2D 视觉效果，必须 vite dev 本地试。

## Iter 7b 设计要点（已实现）
- **AppleScript over EventKit/sqlite**：本来想直接读 `~/Library/Calendars/*.sqlite`，但那是私有 schema 经常变；EventKit 走 Swift FFI 又会引入新的 build target。osascript + Calendar.app 是和 `get_active_window` 一致的 shell-out 模式，复用现成基础设施，代价是 Calendar.app 第一次调用会冷启动需要数秒——可接受，因为这工具不在主要互动路径上。
- **TAB 而非 unicode 分隔符**：试过 `‹|›`，但担心 osascript 在某些 locale 下对多字节字符处理意外。TAB 是单字节、AppleScript 内置常量、几乎不出现在日历标题里。退出码失败时也不会有半截 TAB 拼出乱七八糟字段。
- **不解析日期为 timestamp**：AppleScript 把日期格式化成 ISO 8601 比想象的麻烦（locale 依赖、需要 do shell script），而工具结果反正进 LLM context，模型完全能理解 "2026-5-2 9:7" 这种半结构化字符串。Rust 层不必做严格解析。
- **MAX_EVENTS = 20**：上限保护 LLM context budget。一周里如果有超过 20 个事件，宠物挑前 20 条聊就够了，剩下的 `truncated=true` 字段提示模型"还有更多但没列"。
- **prompt 强调"日程是私人内容不要原样念"**：这是隐私敏感信号最强的工具，宠物如果不假思索地读出标题和地点会让用户感觉被监控。和 `get_active_window` 同样的措辞策略。
- **未现场验证脚本**：因为这次会话里没法读 user 的真实日历（隐私边界），就只在 cargo check 层把语法保住。osascript 字符串字面量会在第一次实际调用时被 macOS 解析；如果有错就靠运行时 fallback（返回 stderr）。

## Iter 7a 设计要点（已实现）
- **拆分原 Iter 7**：原 TODO 把日历/天气/系统通知打包成一项，但实际复杂度差异巨大——天气是无密钥 HTTP，日历/通知都需要 macOS 权限和 AppleScript/数据库读取。先做最小、最低风险的天气，把日历和通知拆成 Iter 7b/7c 单独处理，避免一个 PR 又长又风险大。
- **wttr.in 而非 OpenWeather**：选 wttr.in 因为：(a) 不需要 API key，零配置；(b) `?format=4` 直接给出适合 LLM 用的一行人类可读字符串，不用解析 JSON；(c) IP 定位，不必硬编码城市。代价是 wttr.in 偶尔会被限速或返回 ASCII 艺术错误页，所以工具实现把 raw body 透传出去让 LLM 自己判断。
- **工具描述强调"不要原样念"**：和 `get_active_window` 同一个套路。LLM 容易把工具结果当事实陈述塞进回复里，但天气数据原样塞进来读起来很机械（"Beijing: ⛅ 🌡️+18°C"）。Prompt-level 提示能减小这种概率。
- **不加专门的天气配置**：本想加个 `WeatherConfig` 让用户固定城市，但 wttr.in 默认 IP 定位已经够用，且 LLM 可以记住用户城市存到 user_profile 里再传 `city` 参数。少一个配置项就少一个用户接触面。
- **proactive prompt 加注"偶尔用一次"**：天气是最容易被 LLM 滥用的工具——"打个招呼"很容易触发"看眼天气"。明确写出来能压低调用频率。

## Iter 6 设计要点（已实现）
- **独立模块而非塞进 proactive**：consolidate 与 proactive 周期完全不同（小时级 vs 分钟级），关心的信号也不同（记忆容量 vs 用户状态）。塞同一个循环里要么牺牲粒度、要么加分支，模块拆开更干净。
- **LLM 自己改记忆，Rust 不动**：和 Iter 4 mood 同一思路。Rust 端只构造 prompt + 给工具，由 LLM 通过 `memory_edit` 完成合并/删除。优点是规则永远是模型语义判断（"这两条是不是同一件事"），不会被简陋的字符串匹配规则拘束。代价是要花 token，所以默认关闭、有最小条目数门槛。
- **触发门槛 12 条**：经验值。少于这个数手动看一眼就能整理；多了再让 LLM 介入避免索引膨胀。后续可以暴露到设置面板。
- **强约束保守原则**：prompt 里反复强调"不确定就保留"、"已清爽就 noop"，避免 LLM 为了"完成任务"乱删乱合。这种偏振对 housekeeping 任务很重要——错杀比漏杀代价高。
- **记 before/after 条目数到日志**：方便事后判断这次跑有没有过度删除。如果某天发现一夜之间从 30 条变成 5 条，可以及时关掉这功能。后续 Iter 可以加 dry-run 模式或回滚机制。
- **风险**：当前没有"快照/回滚"机制——如果 LLM 删错了重要记忆，无法恢复。短期靠保守原则 + 用户可关闭来缓解；长期可以考虑在 consolidate 前自动备份 index.yaml。

## Iter 5 设计要点（已实现）
- **awaiting 闸 vs cooldown 闸**：两者解决不同问题，因此都要。
  - awaiting 是"被忽视就闭嘴"的伙伴礼貌——上一句没人理，再说一遍只会更尴尬，没有时间能消解这个状态，必须等到用户主动开口才解锁。
  - cooldown 是"刚说完话别马上又说"的硬下限——即便用户秒回了，宠物也不该立即再主动开口。1800s 默认是个相对保守值，避免开发期来回测试时被烦到，正式使用可以根据习惯调小。
- **不在 LLM 层做判断而在调度层做**：之所以在 Rust 闸门里直接 skip，而不是把 awaiting/cooldown 信息丢给 LLM 让它自己决定，是因为：
  - 直接节省一次完整的 LLM 调用（包括可能的工具调用）——cooldown 期间一律不该花钱。
  - LLM 更不容易判断"上次的话用户有没有真的看见"，规则化反而更可靠。
- **`mark_user_message` vs `touch`**：原 `touch` 在 chat.rs 被调用两次（请求前 + 请求后），第一次是用户消息进来，第二次是助手响应结束。第一次的语义其实是"用户回复了"，第二次只是"互动刚结束"。把第一次替换为 `mark_user_message` 才能把 awaiting 清掉，同时保留 `touch` 给那些不属于 awaiting 状态机的场景（如反应式回复完成）。
- **快照而非加锁多次读**：spawn 循环里要看 idle、since_last_proactive、awaiting 三个字段。如果分开调三次方法会多次锁同一把锁，并存在状态不一致风险（例如读到一半被 `mark_user_message` 改掉）。改成 `snapshot()` 一次性返回 `ClockSnapshot` 结构。

## Iter 4 设计要点（已实现）
- **存哪**：复用现有 memory 系统（`ai_insights/current_mood`），而不是新加状态文件。原因：(a) memory 已经有 list/search/edit 工具暴露给 LLM，零成本；(b) memory 面板会显示出来，用户可以肉眼检视宠物"心情"；(c) 避免引入又一个独立的状态来源。
- **谁写**：完全交给 LLM 写。Rust 端只读不写，连 bootstrap 都不做。这样保证 mood 内容是模型语义生成的（自然语言、跟人格一致），而不是 Rust 拼接出的"模板心情"。代价是第一次开口前 mood 是空的，但 prompt 里明确告诉 LLM "这是第一次"，模型会自己创建。
- **何时读/写**：每次 proactive tick 读，开口后写。沉默时不写——节省一次工具调用，且"没说话 = 心情没变化"是合理近似。
- **作用域只在 proactive**：reactive chat 暂不注入 mood，避免同时改两条链路、放大测试面。后续可加，但要先观察 proactive 路径下 mood 是否真的能稳定演化、不会跑偏。
- **风险**：(a) LLM 可能忘了调 memory_edit（prompt 里强约束 + 文字加粗以减小概率）；(b) mood 可能 drift 到很奇怪的地方（"今天好烦躁"持续好几个小时）——后续 Iter 6（记忆 consolidate）可以顺手修剪过期 mood。

## Iter 3 设计要点（已实现）
- 用 `ioreg` 而非 `CGEventSourceSecondsSinceLastEventType` FFI：避免拉 `core-graphics` 依赖；HID 路径不需要 Accessibility 权限，跟 `osascript` 一致都是 shell-out 模式，调试也方便。
- 双重门槛而非单一替换：原 `idle_threshold_seconds`（距上次对话）保留，新加的 `input_idle_seconds` 是"用户最近还在动键盘吗"。两条都过才允许 LLM 决定开口。这样能区分"用户离开座位 30 分钟" vs "用户在专注打字 30 分钟"——后者绝不打扰。
- 仍交给 LLM 决定要不要说：把 `input_idle` 也写进 prompt（"键鼠空闲约 N 秒"），让 LLM 综合判断。门槛只是硬下限，不是充分条件。
- 默认 60 秒：避免在用户正连续输入时跳出来；又不会把短暂停顿（看文档、想问题）算成"在工作"。0 = 禁用门槛，便于测试或非 macOS 平台。

## Iter 2 设计要点（已实现）
- 工具而非主动注入：让 LLM 在它认为有必要时主动调 `get_active_window`，而不是每个 tick 都把 app 名喂进 prompt。这样 LLM 自己决定要不要"看一眼"，也避免把噪音灌进上下文。
- macOS 用 AppleScript 而非私有 API：依赖 `osascript`，开箱即用，不用引入额外 crate；窗口标题取决于 Accessibility 权限，工具描述里明确把这点告诉 LLM。
- 工具描述明确提示"作为 hint 而非 authoritative，不要过度具体"——避免宠物把窗口标题原样念出来让用户觉得被监视。
- proactive prompt 里点名 `get_active_window` + `memory_search`，主动开口前优先看一眼当下，再决定话题。

## Iter 1 设计要点
- Rust 端后台 tokio 任务在 `setup` 中 spawn，每 `proactive_interval_seconds` 触发一次。
- 触发时查询"上次用户/宠物互动时间"，若大于 `proactive_idle_threshold_seconds` 则调用 LLM。
- LLM 输入：SOUL.md + 工具提示 + 一条 user 消息「（系统提示）现在是 X 时刻，距离上次对话已 Y 分钟。如果你想主动跟用户说点什么，直接说出来；如果不想打扰就只回一个特殊标记 `<silent>`。」
- 若回复非 `<silent>` 则推送给前端，前端在气泡显示并写入 session。
- 配置项：`proactive.enabled` / `proactive.interval_seconds` / `proactive.idle_threshold_seconds`。
- 前端通过 Tauri event 监听 `proactive-message` 事件。
- 默认 `enabled=false` 以免开发期打扰，开发完后可在设置面板勾选。

## 风险 / 注意
- LLM 反复主动调用可能很贵，要有最小间隔（默认 ≥ 5 分钟）。
- 用户正在打字时不要打断——后续 iter 加入键盘活动检测。
- session 写入并发：主动消息和用户消息可能同时写，需要锁或用 invoke 复用现有路径。
