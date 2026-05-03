# DONE

记录每次迭代完成的实质性变化（按时间倒序）。

## 2026-05-04 — Iter R19：speech length register 多样性 nudge（"偏长 / 偏短" 提示）
- 现状缺口：pet 主动开口的字数分布几乎是单 register —— LLM 默认产出一致长度。但**真朋友会混着说短"嘿"和长"今天有什么打算"**。同一长度 5 句话连发会让 pet 显得"机器化"。speech_history 已有数据但从未被用作"register variance" 的反馈源。R11 detect_repeated_topic 检测内容重复，R19 检测**长度重复**。
- 解法 — 纯统计 + 静默兜底：
  - speech_history.rs 加 3 个常量 `SPEECH_LENGTH_MIN_SAMPLES = 3` / `SPEECH_LENGTH_LONG_THRESHOLD = 25` / `SPEECH_LENGTH_SHORT_THRESHOLD = 8` + 纯函数 `format_speech_length_hint(lines: &[String]) -> String`。
  - 步骤：strip_timestamp 取 char 数 → filter 0-char（empty） → 检查 nonzero ≥ 3 → 全部 ≥25 → "偏长 + 试更短"，全部 ≤8 → "偏短 + 多花两句"，混合 → 空字符串。
  - PromptInputs 加 `length_register_hint: &'a str`；prompt assembler push_if_nonempty 在 active_app_hint 后。
  - run_proactive_turn 复用现有 recent_speeches(5) 绑定 — speech_hint + repeated_topic_hint + length_register_hint 三层从一次 fetch 出。
- 决策 — 三阈值"全或无"语义而非平均带宽：诱惑是用 mean ± std deviation 判断 variance。但 5 个样本 std dev 噪音大；"全部都长 / 全部都短" 是更稳的二元信号 — 哪怕 5/5 都偏长才提示。**simple gate > complex statistic** 是 R7 step function 也用过的纪律。
- 决策 — `chars().count()` 不 `len()`：中文 1 字 = 3 字节 UTF-8。25 char 中文 = 75 byte，bytewise 看会判成"非常长"。专门 test 钉住 chinese 30-char 触发"偏长" 不 panic on multibyte boundary。
- 决策 — empty stripped lines filter 后再判 sample 数：log 文件可能含损坏行（"<ts> <empty>"）。如果不 filter，empty 拉低 mean → 被误判"偏短"。filter 后如果 nonzero < 3 → return ""，**不带破损数据做强信号**。
- 决策 — mixed 静默不报：如果用户已经看到 "嘿 / 在吗 / 早上好今天计划是什么" 混合 register，pet 已经在做对的事，不需要 prompt nudge。**只在异常时干预**，正常时静默。
- 决策 — 25 char 阈值：经验值。日常 Chinese conversational 话语 5-30 字。25 是高端"偏长"边界。8 是低端"偏短"边界。可调，必要时改成 settings 字段（先不做，等用户反馈）。
- 决策 — `[N/M]` reuse pattern：不写 PromptInputs.recent_speeches 重复 fetch；run_proactive_turn 一次 fetch 给 3 个 hint 复用。R11 IDEA 已写过这个经济原则。
- 测试（8 新单测）：
  - returns_empty_below_min_samples（< 3）
  - fires_when_all_long（3 行 ≥ 25 char Chinese）
  - fires_when_all_short（3 行 "嘿 / 在吗？/ 吃了吗？"）
  - returns_empty_for_mixed_register（短+长+长）
  - handles_chinese_correctly（30-char 中文不被 bytewise 误判）
  - skips_empty_lines（1 empty + 3 long 仍触发）
  - returns_empty_when_too_few_nonzero（2 empty + 2 long → only 2 nonzero）
  - includes_sample_count_and_mean（"3 句" / "平均" 文案钉死）
- 测试结果：460 cargo（+8）；clippy --all-targets clean；fmt clean。第一轮 fixture 没数对字数，3 句 23 char 被判 mixed；改 fixture 加到 27/28/28 通过。
- 结果：proactive prompt 现在多一层"风格变化"信号。如果连续 5 次开口都是长 question / 长关心，第 6 次会带一句"试更短的关心"；反之亦然。让 pet 的语气更**会换register**，更像真实朋友交替说"在忙吗" 和"今天感觉怎么样我注意到你已经写代码两个小时了"。

## 2026-05-04 — Iter R18：抽取 read_ai_insights_item 共享 helper（refactor / 还 R16 IDEA 标记的债）
- 现状缺口：proactive.rs 有 5 个 helper（get_persona_summary / build_persona_hint / read_daily_plan_description / read_daily_review_description / daily_review_exists / build_plan_hint），consolidate.rs 有 1 个（sweep_stale_plan）— **6 处都做同一件事**：memory_list("ai_insights") → categories.get("ai_insights") → items.iter().find(|i| i.title == ?)。R16 IDEA.md 已记下这债："当 helper 数到 6 时强制 refactor"。R17 又新增了一个调用面，正好踩到阈值。
- 解法 — 单点 thin helper：
  - `commands/memory.rs` 新加 `pub fn read_ai_insights_item(title: &str) -> Option<MemoryItem>`：3 行 ok? + ?；返 cloned MemoryItem。
  - 6 个调用点全部精简：
    - get_persona_summary：`Option<MemoryItem> → PersonaSummary` 1 行 .map + 兜底 unwrap_or_else
    - build_persona_hint：let-else + 提前 trim 检查 + redact + format
    - read_daily_plan_description：1 行 .map(|i| i.description).unwrap_or_default()
    - read_daily_review_description：1 行 .map(|i| i.description)
    - daily_review_exists：1 行 .is_some()
    - build_plan_hint：1 行 .map + 1 行 format_plan_hint
    - sweep_stale_plan（consolidate）：let-else + RFC3339 parse + age check + delete
- 决策 — 返回 cloned MemoryItem 而非 description 字符串：诱惑是 `read_ai_insights_description(title) -> Option<String>` 直接给 description（5 个 caller 之 4 都只要 description）。但 get_persona_summary 要 updated_at（D5），sweep_stale_plan 要 updated_at + title。返 MemoryItem 让所有 caller 都能各自 take 想要的 field — 一个 helper 服 6 种 caller 模式。`.clone()` 成本可忽略（MemoryItem 字符串都是短的，每天调用次数级别）。
- 决策 — 命名"read_ai_insights_item"：考虑过 "find_ai_insights_item" / "get_ai_insights_item" / "lookup_ai_insights_item"。"read" 跟现有 read_current_mood / read_daily_plan_description 用词一致，最融入 codebase 语境。"get" 太空泛（跟 Tauri 命令的 get_* 命名空间冲突），"find" 容易让人以为返第一个（实际返 None / Some），"lookup" 罗嗦。
- 决策 — 不抽 `read_ai_insights_items_filter`（Pattern B）：consolidate.sweep_stale_daily_reviews 是唯一一处遍历整个 ai_insights category 的 caller。**单一调用点不抽抽象** 是 R12b IDEA 写过的纪律 ("late abstraction > early abstraction")。Pattern B 留 inline。
- 决策 — 不改 consolidate.rs 顶部 use 语句：原 `use crate::commands::memory;` 已经 import 整个 module；新 `read_ai_insights_item` 通过 `memory::read_ai_insights_item` 访问，无需重新 import。
- 决策 — 修复 build_plan_hint 的 borrow 模式：原本 `cat.items.iter().find().map(|i| i.description.as_str())` 借 cat。新版返 cloned MemoryItem 后 .map(|i| i.description) 拿 owned String，再 `format_plan_hint(&description, ...)` 借引用。语义不变，避免新版本嵌套 lifetimes。
- 决策 — fmt 把 build_plan_hint 改后的 closure call 重新换行（保 80-col）：`format_plan_hint(&description, &|s| { redact(s) })` 自动换行成多行。functional 不变。
- 测试：纯 refactor，不修测试。452 cargo 全过（无变化），证明语义保不变。clippy / fmt clean。
- 结果：proactive.rs 从 1700+ 行减到 1670 行（-30 行 net），代码密度提升。`memory_list(Some("ai_insights"))` 调用从 8 处降到 2 处（保留 sweep_stale_daily_reviews + 大概外部某处 panel call）。**重复模式抽象化是技术债 maintenance** — R-iter 路上每隔几次刻意还一笔，避免后期 refactor 大爆炸。

## 2026-05-04 — Iter R17：consolidate 自动清理 30 天前的 daily_review 条目（防 unbounded growth）
- 现状缺口：R12 / R12b 让 pet 每天 22:00 后写一条 daily_review_YYYY-MM-DD 到 ai_insights memory。一年 = 365 个 .md 文件 + 365 行 YAML index 条目。从未实现 retention，会无限增长 — 不仅磁盘空间，更糟的是 panel memory list 渲染会被几百行历史污染。
- 解法 — 复用 consolidate sweep 模式：
  - **settings.rs**：新字段 `stale_daily_review_days: u32`（默认 30，0 = 关闭剪枝）。default_stale_daily_review_days() helper + Default impl 增项。
  - **daily_review.rs**：纯解析器 `parse_daily_review_date(title) -> Option<NaiveDate>`：strip "daily_review_" 前缀 + chrono::NaiveDate::parse_from_str("%Y-%m-%d")。纯 staleness 函数 `is_stale_daily_review(title, today, retention_days)`：retention=0 短路返 false（pruning disabled），title 非 review 返 false（保护 mood/plan/persona），signed_duration_since 处理未来日期（clock skew → 负 delta → 不删）。
  - **consolidate.rs**：新 `sweep_stale_daily_reviews(today, retention_days) -> usize`：copy reminder/plan/butler sweep 模式（memory_list ai_insights → filter → memory_edit delete）。在 run_consolidation 加进 sweep 链，仅 swept > 0 时写 log（避免每次 consolidate 都"pruned 0"刷屏）。
- 决策 — title prefix 严格 match：scrub 只看 `daily_review_YYYY-MM-DD` 这个 schema。任何其他 ai_insights item（current_mood / persona_summary / daily_plan / 用户手写笔记）parse_daily_review_date 返 None → is_stale 返 false → 永不删。**defense in depth** 保护 protected items — sweep 不需要硬编码 protected list，schema-based filtering 自然排除。
- 决策 — `delta > retention_days`（严格大于）而非 `>=`：30 天前的 review 还**正好**在窗口边缘。delta == 30 是"今天的对面 30 天" — 还在 retention 内。这种"边界条件用 strict gt" 是给用户多 1 天 buffer 的友好默认。
- 决策 — retention=0 = 永不剪枝，不是 = 立刻全删：诱惑是 retention=0 表示"立刻清"。但用户配置 0 一般意味着"我不想这功能" — fail safe 默认应该是 *保留* 数据。R12 / R14 / R16 review 是 pet "成长" 的载体，删了不可恢复。"0 = disabled" 比"0 = aggressive" 安全得多。
- 决策 — 默认 30 天：对应"过去一个月的日记可以翻看"。90/365 也合理但 30 是 sweet spot — 平衡"够查最近上下文" 和"不让 panel 过载"。可改 yaml 调，前端 UI 暂缺（少用，先 yaml 编辑足够）。
- 决策 — `signed_duration_since` 不 panic on future date：手动改 yaml 把 review_2030-01-01 提前写入会让 today.signed_duration_since(future) 是负数 → num_days() 是负 → not stale → keep。系统不会因为脏数据 crash。这是 R12b "[1/0] graceful skip" 同源思路。
- 决策 — sweep 同步而非 async：consolidate 已有 sweep_stale_reminders（同步）+ sweep_stale_plan（同步）+ sweep_completed_once_butler_tasks（async，需写 butler_history）。daily_review 删除不需要 history 写入 — 同步即可，跟 reminders/plan 同样 boilerplate。
- 测试（9 新单测）：
  - parse_review_date_extracts_valid_dates / rejects_non_review_titles / rejects_malformed_suffix
  - stale_review_returns_false_when_retention_zero（disabled gate）
  - stale_review_returns_false_for_non_review_titles（protected items 安全）
  - stale_review_returns_false_for_today（边界 day=0）
  - stale_review_returns_false_within_retention_window（含边界 day == retention 不删）
  - stale_review_returns_true_past_retention（day > retention 删）
  - stale_review_handles_future_dates_gracefully（clock skew）
- 测试结果：452 cargo（+9）；clippy --all-targets clean；fmt clean。
- 结果：daily_review 不再 unbounded 增长。一个月后 panel ai_insights 列表稳定在 ≤30 个 review 条目 + 几个 protected items（mood / plan / persona_summary / butler 长期任务），可读性 + 性能都不会随时间退化。memory subsystem 的 retention 闭环也跟 reminder/plan/butler-once 三个 sweep 对齐 — 现在所有 time-bound memory 都自动 garbage collect。

## 2026-05-04 — Iter R1c：panel UI 区分 Dismissed vs Ignored（R1b 信号 surface 闭合）
- 现状缺口：R1b 加了 Dismissed kind 进 backend，但 panel UI（PanelToneStrip "💬 N/M" chip + PanelDebug 反馈 timeline）依然只显示"回复 / 忽略"二元 — Dismissed 被静默归到"忽略" 灰色 pill 里。用户没法 inspect 自己的 dismiss 行为是否真被记录、R7 cooldown 是否真的因此响应。**写了的信号 panel 看不见 = 半闭环**。
- 解法 — 三段 surface 升级：
  - Backend：FeedbackSummary 加 `dismissed: u64` 字段。build_tone_snapshot 多扫一遍 entries filter Dismissed，count 进 dismissed 字段。replied + dismissed + ignored = total（ignored 是计算项）。
  - PanelToneStrip：chip 文案不变（"💬 5/10"），dismissed > 0 时加紫色 "· 👋3" 后缀；hover title 升级到三段："过去 N 次：回复 X，被动忽略 Y，主动点掉 Z" + R7 阈值文案改成"负反馈率（忽略+点掉）> 60% → cooldown × 2"。
  - PanelDebug timeline：FeedbackEntry kind union 加 "dismissed"。pill 三色三 label：绿 / 灰 / 红 + "回复 / 忽略 / 点掉"。每个 pill 加 hover title 解释信号强度（"5s 内主动点掉 — 比被动忽略信号更强"）。summary 行加 "· 👋N 点掉" 后缀（仅 dismissed > 0 时）。
- 决策 — chip 文案保持 "replied/total" 不变：诱惑是改成 "replied / negative" 或 "replied : ignored : dismissed" 三元。但单 chip 信息密度极限就是 2 个数字。"replied/total" 是反馈率天然分数。dismissed 信息走"小尾巴"加号位不抢中心。
- 决策 — chip dismissed 后缀仅在 > 0 时显示：dismissed 0 是常态（用户不主动 click 是常态行为），永远显示"· 👋0"会噪音。条件渲染 zero-noise。
- 决策 — pill 颜色编码强度梯度：绿（回复）= 正信号；灰（被动忽略）= 弱负；红（主动点掉）= 强负。从左往右 visual 强度递增，匹配信号强度。Panel reader 一眼区分"宠物被冷落 vs 被嫌弃"。
- 决策 — "👋"作 dismissed icon：手势"再见 / 拒绝" 含义直观；且与 "💬" 形成 dialogue/wave 对比。诱惑用 ❌ 但太 confrontational；🚫 太正式。"挥手"是软拒绝 — 跟"用户嫌弃但不愤怒"的实际语义对齐。
- 决策 — hover 文案明确"信号强度差"：写"主动点掉是比被动忽略信号更强"是 explicit education — 让 power user 理解为什么 cooldown 会因此变长。这种"为什么这样 = panel 自我解释" 是 R 系列 chip 一直的设计原则（R7 阈值文案在 hover、D series chip 用 hover 解释 gate 状态）。
- 测试：仅 backend 类型变化 (FeedbackSummary 加字段) — 现有 build_tone_snapshot 未单测（需要 tauri State fixture），结构上是 trivial filter+count；现有 443 cargo test 通过。前端：tsc clean 检验类型对齐；运行时验证留实机。
- 测试结果：443 cargo（无变化）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：用户 click bubble dismiss 现在 panel 立刻 reflect — chip 显示 "👋N" 计数，timeline 红色 pill 标"点掉"，hover 解释为什么这个信号比 ignored 重。R1b 信号"写得到" + R1c "看得见" = 反馈闭环视觉完成。

## 2026-05-04 — Iter R1b：ChatBubble 5 秒内点击 = active dismiss 反馈信号
- 现状缺口：R1 实现了 ignored/replied 二分但仅作"是否在下一次 tick 前回复" 推断 — 用户**主动**点掉气泡的强信号丢了。pet 说"在忙吗"，用户立刻 click 关掉 → 是清晰的"我看到了，我不想理"，比"60s 后自动消失" 的 ignored 信号强得多。R1 任务 deliberately 留了 R1b 后续做这个 UI hookup。
- 解法 — 三段式分工：
  - **Backend**：FeedbackKind 加 Dismissed variant（serde "dismissed"），as_str / parse_line / format_feedback_hint 全分支处理。新 Tauri 命令 `record_bubble_dismissed(excerpt: String)` 直接调 record_event。Hint phrasing 区分："上次你说 X，用户**主动点掉了**气泡 — 比单纯没回应更明显的不感兴趣信号" 比 ignored 的 "用户没回应" 更直接。
  - **Ratio 适配器**：函数 `ignore_ratio` 重命名为 `negative_signal_ratio`，Ignored | Dismissed 都计入。R7 cooldown 适配器无逻辑变化（仍是三档 step function），但分母分子语义升级到"任何负信号"。gate.rs 单点 caller 跟改。
  - **Frontend**：ChatBubble 加 onClick prop + cursor pointer。App.tsx 用 useRef 跟踪 bubbleShownAt — displayMessage 变更或 showBubble flip 时设 Date.now()，重置时设 null。click handler `setBubbleDismissed(true)` + 仅当 `Date.now() - shownAt < 5000` 调 invoke("record_bubble_dismissed")。后期 click（>5s）只隐藏不发信号，避免污染 history。
- 决策 — Dismissed 与 Ignored 等权重计入 ratio：诱惑是 Dismissed 算 1.5 / Ignored 算 1，因为 Dismissed 信号更强。但 R7 step-function 设计原则是"panel reader 一眼算得对" — 加权后 ratio 不再是简单"负 / 总"。简单计入 + 让 Dismissed event 自身的频率说话（用户真的常点 → ratio 自然 1.0 → cooldown ×2）效果同等且更可读。
- 决策 — 双信号容忍（同一 turn dismiss + 下一 tick ignored 双计）：用户 click → record Dismissed，但下一次 proactive turn 仍会基于 raw_awaiting=true 写一个 Ignored。两条 entry 进 history。这是 *intentional* — 一个强反应在 ratio 中应该体现得更重，反而是好事。如果未来真嫌 noisy 可加 LAST_FEEDBACK_RECORDED_FOR 跨 surface 的 dedup，但目前不是问题。
- 决策 — 5 秒阈值 frontend 决定：threshold 是用户感知层面的"快速反应 vs 慢慢决定"。这个判断属于 UI 行为而不是后端业务，let frontend gate it。后端只接受 record_bubble_dismissed 命令。如果未来要可配置（比如 "敏感模式" 把阈值放到 3s），改一处 const 即可。
- 决策 — 不清 raw_awaiting：click-to-dismiss 不算"用户主动 engage"。awaiting 表示"等用户回应"，dismiss 是"用户拒绝回应" — awaiting 状态正确地保持 true，让下一次 tick 对该 turn 也分类 Ignored。语义清晰。
- 决策 — fmt 把 multi-line `matches!` 自动塌缩到一行：原本写 `matches!(e.kind, FeedbackKind::Ignored | FeedbackKind::Dismissed)` 多行但 rustfmt 喜欢 single line，照做。
- 测试（5 新单测，feedback_history.rs 重命名后旧测试全更新）：
  - negative_signal_ratio_counts_dismissed_alongside_ignored: 1 dismissed + 2 ignored / 5 → 0.6
  - negative_signal_ratio_handles_all_dismissed: 全 Dismissed → 1.0
  - dismissed_round_trips_through_format_and_parse: write → read 闭合
  - format_feedback_hint_handles_dismissed_with_stronger_phrasing: hint 含"主动点掉"
  - 4 个旧 ignore_ratio_* tests 全部 rename 到 negative_signal_ratio_*
- 测试结果：443 cargo（+4 净增；4 重命名 + 4 全新 - 0 删除）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：用户现在可以**主动**告诉 pet "这条不要"。R7 cooldown 适配器获得"用户实际不喜欢什么样的开口" 的强力新数据源。pet 学习曲线大幅缩短 — 主动反馈比被动观察 ignored 信号收敛得快得多。R1 → R1b 形成完整反馈闭环。

## 2026-05-04 — Iter R16：yesterday review description 注入 first-of-day prompt（write→read 闭环）
- 现状缺口：R12 每天 22:00 后写 daily_review 到 ai_insights memory，R12b 把 description 升级到含 progress 标记。但**这些条目从来没被读回来**。memory 里有"今天主动开口 7 次，计划 3/5"，但今天的 first-of-day proactive prompt 看不到 — pet 写完日记自己再也不翻。R14 cross_day_hint 提取昨日尾声 2 条 speeches 是"具体片段" 维度，缺"全貌"维度。
- 解法 — pure reframer + first-of-day 触发：
  - 新 `pub fn format_yesterday_recap_hint(description: Option<&str>) -> String` 在 daily_review.rs：None / 非 [review] 前缀 / 空 body → ""；否则 strip "[review]" 前缀 → `replacen("今天", "昨天", 1)` → 包成 "[昨日总览] 我们{}。"。
  - 新 helper `read_daily_review_description(date: NaiveDate) -> Option<String>` 在 proactive.rs：扫 ai_insights category 找 `daily_review_YYYY-MM-DD` title 拉 description。
  - PromptInputs 加 `yesterday_recap_hint: &'a str`。run_proactive_turn 在 first-of-day（today_speech_count == 0）拉昨日 review description → format → push_if_nonempty。
  - prompt assembler push 顺序：yesterday_recap_hint 在 cross_day_hint 之前 — 总览先看，尾声后看，符合"先粗后细" 阅读逻辑。
- 决策 — `replacen("今天", "昨天", 1)`：deterministic description 起首是"今天主动开口 N 次..."，第二个"今天"如果未来 R12c LLM 改写有可能出现（虽然现在不会）。replacen(.., 1) 只替换第一个，避免误改后文。
- 决策 — 完全独立 hint 而非合并 cross_day_hint：两者可以独立失败 — 昨日有 review 但没 speeches（quiet 整天但 22:00 fire 了）→ 只有 recap 没尾声；反之亦然。两个独立 push 让两层可以各自存在或不存在，组合上更 robust。
- 决策 — 走 description 而非 detail：detail .md 包含完整 bullet 列表（昨日全部 speeches），太长，会把 prompt 撑大。description 是 panel index 行，已经经过 R12 / R12b 两层 deterministic 压缩成"今天主动开口 N 次，计划 X/Y" — 信息密度极高，正适合 prompt 注入。"用最浓缩的 surface 喂 prompt" 是正确取舍。
- 决策 — 不写 panel UI：用户在 panel 已经能直接看到 ai_insights/daily_review_YYYY-MM-DD 条目（PanelMemory 渲染所有 categories）。没必要再做专门 yesterday-recap 卡片 — 现有 memory list view 就够。
- 决策 — `replacen` skip 条件 (Some(rest) = strip_prefix("[review]")) `body.is_empty()` 提前返回：避免出现 "[昨日总览] 我们。" 退化空尾声。
- 测试（7 新单测）：
  - None / non-review prefix / empty body 各返 ""
  - 完整 case 1: "[review] 今天主动开口 7 次，计划 3/5" → "[昨日总览] 我们昨天主动开口 7 次，计划 3/5。"
  - count only / 有计划兜底 / 多个"今天"只换第一个 / leading whitespace 容忍
- 测试结果：439 cargo（+7）；clippy --all-targets clean；fmt clean。
- 结果：早起第一次主动开口现在带"[昨日总览] 我们昨天主动开口 7 次，计划 3/5。" + "[昨日尾声] 昨天最后说过：· line · line" 两层 callback。R12 写的回顾本不再是孤岛 — 写→读 闭合，pet 读自己的日记本，叙事密度比之前的 "尾声 2 条" 单层提升一档。

## 2026-05-03 — Iter R12b：daily review 加入 plan progress 解析（"计划 N/M" 替代"有计划"）
- 现状缺口：R12 description 写"今天主动开口 7 次，有计划"。"有计划" 没说明做了多少 — 用户瞄一眼 panel 看不到 progress。daily_plan 本身已经用 `[N/M]` 标记进度（如"· 关心工作 [1/2]"），但这个信号没被 review 摘要复用。
- 解法 — 纯解析器 + description 升级：
  - 新 `pub fn parse_plan_progress(plan_description) -> Option<(u32, u32)>` 在 daily_review.rs：扫 `[...]` 块，要求内部是 `digits/digits` 格式，sum N + sum M 返回 (completed, total)。`[remind: 09:00]` / `[every: 18:00]` / `[review]` 这些 schema 标记自然不命中（含字母/冒号），没有误伤。
  - `format_daily_review_description` 改签名 `(speech_count, plan_progress: Option<(u32, u32)>, has_plan)`：3 分支 — Some((c,t)) → "，计划 c/t" / None + has_plan → "，有计划"（free-text 计划兜底）/ None + !has_plan → 无后缀。
  - maybe_run_daily_review 调 parse 把 plan_raw 转 progress，传给 description formatter。
- 决策 — `M == 0` 跳过：`[1/0]` 是退化 case（"完成 1 个目标里的 0 个" 无意义）。skip 但不破坏其他 marker。`parse_progress("· good [2/3]\n· bad [1/0]")` → `Some((2, 3))`。
- 决策 — 严格 digit-only：`[a/b]` / `[/3]` / `[3/]` 全部拒绝。`[remind: 09:00]` 含字母 + 冒号，第一道 split_once('/') 后 `c_trim = "remind: 09"` 非全数字 → reject。reminder/butler schedule 标记不会被误算成 progress。
- 决策 — 容忍空格：`[ 1 / 2 ]` 接受（人手写计划带空格很常见）。c_trim/t_trim 用 `trim()` 后再 parse。
- 决策 — saturating_add 防溢出：u32 ceiling 在 4B，但理论用户可能造极端 plan（虽然不会）。一致用 saturating 写法，安全免责。
- 决策 — pure parser 留 daily_review.rs：跟 gate / formatter 同模块，纯计算 + tests，调用面只在 maybe_run_daily_review 单点。
- 决策 — `Iter R12c` 取代旧 R12b（LLM 总结）：原 R12b 写的"LLM 一句话"需要把 maybe_run_daily_review 从 clock-pure 升级到 app-aware（拿 AppHandle / McpManagerStore / LogStore 等）— scope 比想象大。R12b 改成此 deterministic upgrade，LLM 版本另列 R12c。
- 测试（8 新单测）：
  - description_shows_concrete_plan_progress_when_parseable：Some((1,3)) → "计划 1/3"，Some((0,5)) → "计划 0/5"，None + has_plan → "有计划"
  - parse_progress_sums_multiple_markers：3 行 plan → (2, 4)
  - parse_progress_handles_single_marker：1 行 → 直传
  - parse_progress_returns_none_for_no_markers：empty / 自由文本 / 无方括号
  - parse_progress_skips_malformed_markers：[a/b] / [10] / [/3] / [3/]
  - parse_progress_skips_marker_with_zero_total：[1/0] skip 但 [2/3] 仍命中
  - parse_progress_ignores_non_progress_brackets：[remind: 09:00] / [every: 18:00] 不参与
  - parse_progress_handles_whitespace_inside_marker：[ 1 / 2 ] 接受
- 测试结果：432 cargo（+8）；clippy --all-targets clean；fmt 自动修了 proactive.rs 一处缩进。
- 结果：panel 现在能在一行看到"今天主动开口 7 次，计划 3/5" — 数字立等可见的进度。pet 的"今天我们做了什么" 不再是模糊"有计划"，而是 quantified。明天 cross_day_hint / 早起 prompt 可以基于 progress=3/5 生成"昨天还有 2 件没做完，今天接着推" 之类的 callback。

## 2026-05-03 — Iter R12：daily review 自动生成（22:00 写 ai_insights/daily_review_YYYY-MM-DD）
- 现状缺口：pet 每天的"经验"有 mood / persona / butler_history / speech_history 各自分散。"今天我们一起做了什么"没有统一的 retrospective artifact — 隔天没办法快速 reload 昨天上下文，跨日叙事（R14）只能拉昨日 speeches 的尾声 2 条，看不到"今天的全貌"。R12 把"日终回顾"沉淀成 ai_insights memory entry，结构化、可读、可被未来 prompt/UI 复用。
- 解法 — pure gate + thin async writer：
  - 新模块 `src/proactive/daily_review.rs`：`DAILY_REVIEW_HOUR = 22`，`LAST_DAILY_REVIEW_DATE: Mutex<Option<NaiveDate>>` 进程单例，`should_trigger_daily_review(hour, today, last) -> bool` 纯 gate，`format_daily_review_detail(speeches, plan, date)` 纯 markdown 生成器，`format_daily_review_description(count, has_plan)` 纯一行 index 文案。
  - proactive.rs 加 3 个私有 helper：`read_daily_plan_description()` 拉 ai_insights/daily_plan 原始 description；`daily_review_exists(title)` 跨重启 idempotency 检查；`maybe_run_daily_review(now_local)` async 编排（gate → exists check → fetch speeches → format → memory_edit create → mark date）。
  - run_proactive_turn 第二行（紧跟 now_local 计算）调一次 `maybe_run_daily_review(now_local).await`。在所有 gate 之前 — review 是独立 outcome，不受 quiet/cooldown/awaiting 影响。
- 决策 — deterministic 版先行，LLM 总结留 R12b：deterministic（bullet list 拼接）已经能产生"昨天主动开口 7 次：· 早安 · 中午吃饭了吗 ..." 的可用 artifact。LLM 一句话总结是锦上添花但不是关键路径。R12b 之后再升级。
- 决策 — 22:00 触发：用户大致还在桌前（不像凌晨 0:00 触发会错过用户），但又"够晚"让今天的对话基本结束。允许的迟到时刻一直到 23:59 都能 fire。
- 决策 — 双重 idempotency：进程内 LAST_DAILY_REVIEW_DATE + 跨进程 index existence 检查。光靠前者会在用户 23:00 重启 app 时（已写过的 case）二次写入并 disambiguate 成 `daily_review_YYYY-MM-DD_1`。光靠后者每次都要扫 index O(n) 浪费。两者叠加：fast path 命中就 skip，cold start 才查盘。
- 决策 — title 用 `daily_review_YYYY-MM-DD`：每天独立 entry，不像 daily_plan 是单条覆盖。便于"翻日记本"，每天能独立查看 + 不丢失任何一天的记录。180 天累积 ≈ 180 个 .md，按月分类的话改 title schema 不破坏旧数据（`daily_review_2026-05-03` 是字典序友好，前缀 grep 即可）。
- 决策 — `[review]` 前缀 description：让未来的 R12b LLM-summary pass 能识别"哪些是 deterministic、哪些已升级 LLM 总结"，不会撞车其他 ai_insights 条目（mood / persona / daily_plan）。
- 决策 — speech 100 上限：典型一天 < 30 条主动开口，100 是健壮兜底。极端 chatty mode 可能跑到 50+，仍在范围内。
- 决策 — best-effort 写：memory_edit 错误吞掉。review 是装饰性的 — 失败不能让正常 proactive turn 也卡住。
- 决策 — 不动 mood / 不进 speech_history：review 写动作完全 silent，不计 chatty quota，不影响心情判断。是后台沉淀，不是"宠物开口"。
- 测试（11 新单测）：
  - gate 在 hour < 22 全 false（00 / 12 / 21 三档 boundary）
  - gate 在 22 / 23 + 无 prior 时 true
  - gate 在已 review today 时 false（22 / 23 两档）
  - gate 在 last == yesterday 时 22 fire
  - gate 在 21 + 旧 review 时 false（hour 仍是首要条件）
  - format detail：完整 plan + speeches → 标题 + 计划段 + 开口段都在
  - format detail：empty plan / empty speeches / 都空 → 各自的"没有..."提示文案
  - format description：count 0/7/15 + has_plan true/false 各种组合
- 测试结果：424 cargo（+11）；clippy --all-targets clean（被 clippy 提示用 `!matches!` 替换 match-arm，照做）；fmt clean。
- 结果：每天 22:00 后第一次 proactive tick 自动产出"今日回顾" memory 条目。Pet 长期记忆从"零散信号"升级到"每日结构化日记本" — 可被未来 panel UI 翻阅、可被 cross-day prompt 升级（R14 的下一步），可被 R12b LLM 总结再回写。retrospective layer 的 foundation 就位。

## 2026-05-03 — Iter R15：active app 时长追踪（"用户在 X 已经 N 分钟"）
- 现状缺口：proactive prompt 里有 mood / cadence / focus / idle / cross_day / repeated_topic… 唯独缺"用户**当下**在做什么"。get_active_window 是 LLM 自助 tool（要它主动调），后台 loop 没有 baseline 的 hint，导致"用户已经在 Cursor 里写了一小时" 这种最日常的伴随感知缺位。
- 解法 — pure state machine + thin wrapper：
  - 新模块 `src/proactive/active_app.rs`：`MIN_DURATION_MINUTES = 15` const + `ActiveAppSnapshot { app, since: Instant }` + `LAST_ACTIVE_APP: Mutex<Option<...>>` 进程单例。
  - 纯 `compute_active_duration(prev, current_app, now) -> (new_snapshot, Option<minutes>)`：3 分支 — 无 prev → 新 snapshot/None；app 变 → 重置 since/None；app 不变 → 保留 since/Some(elapsed_min)。
  - 纯 `format_active_app_hint(app, minutes) -> String`：低于 15 分钟或空 app → ""；否则 "用户在「{app}」里已经待了 {N} 分钟。"
  - thin wrapper `update_and_format_active_app_hint(Option<&str>) -> String`：读 static → 调 compute → 写回 → 对 app 名做 `redact_with_settings` → 调 format。
- 集成 — `current_active_window()` 复用：把 system_tools.rs 的 osascript 拉成纯 Rust async fn `pub async fn current_active_window() -> Option<(String, String)>`，无 logging / 无 ToolContext，让 proactive loop 和 get_active_window tool 共用同一份 osascript。run_proactive_turn 每 tick 调一次（已有的 5min cadence），落到 PromptInputs.active_app_hint。
- 决策 — 15 分钟阈值：短跳（开 Slack 看一眼回 IDE）不该 surface 成"专注于 Slack"。15 min ≈ "认真投入"门槛，不会过早噪声。
- 决策 — redact 在 hint format 时，不在 snapshot 时：snapshot 留原文（vs redacted）确保 transition 检测稳定 — 用户中途改 redaction patterns 不会让"还在 Cursor"误判成"切换了 app"。
- 决策 — Instant（monotonic）而非 SystemTime：用户睡眠/系统时钟调整不污染分钟计算。
- 决策 — 颗粒度=interval_seconds（默认 5min）：不另起 background loop。短期跳变穿透不到 hint（被 15min 阈值过滤），长期停留误差±5min 可接受。
- 决策 — 复用 osascript 调用：避免双份"获取 active window"实现飘移。Tool 路径 = wrapper（带 logging + redact + 给 LLM）；loop 路径 = wrapper（带 redact + 注入 prompt）。osascript 只一份。
- 测试（7 新单测）：compute 三分支（no prior / app change resets / same app carries since），format 四种（短 duration 空 / 阈值 fires / 空 app / 长 duration 240min）。
- 测试结果：413 cargo（+7）；clippy --all-targets clean；fmt 自动修了 system_tools 末尾空行。
- 结果：proactive loop 现在自带"用户已经在 X 待了 N 分钟" 的环境感知。Pet 不需要等 LLM 主动调 tool — 后台 baseline 就有，开口的连贯性大幅提升（"还在写代码呀，注意脖子" / "Slack 看半天了，是不是有事在烧"）。

## 2026-05-03 — Iter R14：跨日记忆线（first-of-day 注入昨日尾声）
- 现状缺口：每天的第一次 proactive 都从零开始 — pet 不"记得"昨晚最后说了什么。如果昨晚说"睡前看会儿小说"，今早开口理应是"昨晚那本书看完了？" 而不是泛泛"早安"。R14 让叙事跨日延续。
- 解法 — 纯函数 + first-of-day 触发：
  - 新 `pub fn speeches_for_date(content, target_date, max) -> Vec<String>` 在 speech_history.rs：扫每行的 ISO 时间戳，filter 到本地时区对应 target_date 的，返回最后 max 条。Pure / testable / 不依赖系统时钟（caller 传 NaiveDate）。
  - 异步包装 `pub async fn speeches_for_date_async(target_date, max)`：读文件 + 调 pure。
  - run_proactive_turn：如果 `today_speech_count == 0` → 算 `yesterday = now.date_naive() - 1 day` → 取 `speeches_for_date_async(yesterday, 2)` → 包成"[昨日尾声] 昨天最后说过：· line\n· line\n如果话题自然能续上就续，不必生硬呼应。"
  - 每行过 strip_timestamp + redact_with_settings 再注入。
  - PromptInputs 加 `cross_day_hint: &'a str`，build_proactive_prompt push_if_nonempty。
- 决策 — 仅 first-of-day 触发：复用 today_speech_count == 0 已有信号，避免每次 proactive 都拉昨天历史。新一天的"打开" 时刻自然是"叙事接续" 时刻；之后不需要重复。
- 决策 — 2 条窗口（不是 5 条）：跨日 hint 是"昨晚的尾声"，不是"昨天全程"。多了反而冲淡今天的话题。2 条 ≈ 昨晚最后 1-2 句 — 紧凑。
- 决策 — pure helper 接 NaiveDate 而非 chrono::Local::now()：测试不依赖运行时钟。Production 路径调 helper 时拿 now_local.date_naive() - 1 day 算 yesterday。这是 D series time-helpers 一直延续的"pure parameter, impure caller" pattern。
- 决策 — "如果话题自然能续上就续，不必生硬呼应" 收尾 instruction：避免 LLM 强行复读昨天主题。让续接是"自然，可选"，不是"必须"。
- 决策 — 时间戳过滤兼容多时区：`DateTime::parse_from_rfc3339` 接 RFC3339 含 offset；`with_timezone(&chrono::Local)` 转本地后比较 NaiveDate。如果用户跨时区使用（旅行），昨天的判定按当前本地时区，是直觉行为。
- 测试（6 新单测）：
  - empty content → empty
  - max=0 → empty
  - filters by date 正确（4 行 cross 3 天，target 5/3 → 2 条 5/3）
  - last `max` when more match (4 条都是 5/3，max=2 → 取最后 2)
  - malformed lines silently skipped (garbage / no-timestamp / valid → 1 valid)
  - target with no matches → empty
- 测试结果：406 cargo（+6）；clippy --all-targets clean（修了 3 处 useless `vec!` → `[]`）；fmt clean；tsc clean。
- 结果：每天的第一次主动开口现在带着昨晚最后 1-2 句的 echo。叙事跨日续上 — 用户半夜睡前 pet 说了"早点睡，明天又是新一天"，今早 pet 看到这条 hint 后说"昨晚那个 「明天又是新一天」 算数么？" 体感截然不同。

## 2026-05-03 — Iter R13：companion mode 高层级温度预设
- 现状缺口：用户想"今天宠物多说一些"或"今天宠物安静一些" 没有简单 dial。改 cooldown_seconds 是低层级旋钮（用户得知道 1800 秒 vs 900 秒意味着什么）。chatty_day_threshold 也是。需要"温度预设" 高层级抽象。
- 解法 — 三档预设：
  - 新 settings 字段 `companion_mode: String`，默认 `"balanced"`，可选 `"chatty"` / `"quiet"`
  - 纯函数 `apply_companion_mode(mode, base_cooldown, base_chatty) -> (u64, u64)`：
    - balanced（或 unknown）→ 返 base 不变
    - chatty → cooldown × 0.5 (saturating /2), chatty × 2 (saturating_mul)
    - quiet → cooldown × 2 (saturating), chatty × 0.5
    - base=0 始终返 0（保 R7 的 user-explicit-opt-out invariant）
  - `impl ProactiveConfig { effective_chatty_threshold(), effective_cooldown_base() }` 两个 method 让 4 个调用方自然写 `cfg.effective_chatty_threshold()` 不用知道 mode 细节
- 集成 — 4 个 chatty_threshold 调用点全切到 `effective_chatty_threshold()`：
  - run_proactive_turn 顶部（构造 prompt）
  - trigger_proactive_turn manual chatty_part 计算
  - get_tone_snapshot panel
  - evaluate_loop_tick dispatch-time chatty_tag
- 集成 — gate.rs：原本 inline `apply_companion_mode(...).0`, 改成 `cfg.effective_cooldown_base()` 一行；R7 ratio adapter 在这之上 layered
- 决策 — 不引入 enum：String 简单；`#[serde(default)]` + `_ => balanced` fallback 让"未知值"降级为 balanced，user 改字符串不会 panic 或 reject。比 enum + custom serde 干净 50%。
- 决策 — 不加第 4-5 个模式（coaching / silent_present）：3 档够覆盖典型用户需求 + 减少 UI 复杂度。如果将来收到"我想要更细" 反馈再加。silent_present 可以让用户直接关 enabled 实现。
- 决策 — frontend UI 留 follow-up：本 iter 重点是 backend 行为正确 + tests 完整。settings.yaml 改字符串就生效，而 panel UI 加 dropdown 是独立小 iter。
- 测试（6 新单测）：
  - balanced returns base unchanged
  - chatty halves cooldown + doubles chatty
  - quiet doubles cooldown + halves chatty
  - unknown / empty falls back to balanced
  - zero base stays zero (R7-style opt-out invariant)
  - quiet overflow clamps via saturating_mul (u64::MAX edge)
- 测试结果：400 cargo（+6）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：用户在 settings.yaml 加一行 `companion_mode: chatty`（或 quiet）就把整体节奏拨快/拨慢。R7 ratio adapter 在 mode 之上 layered → "我想 chatty + 但今天总被忽略 → 自动调回常规" 这种自适应行为成立。

## 2026-05-03 — Iter R11：speech topic redundancy 检测器 + 注入 proactive prompt
- 现状缺口：proactive prompt 已经有 speech_hint（最近 5 条 bullet list）告诉 LLM"看一下避免重复"，但仍然依赖 LLM 自己审视并自觉换话题。如果模型在某 4-5 个 turn 都聊"工作进展"，prompt 没有显式的"重复警报"，LLM 可能仍重复。R11 加一个**机器检测** 层强制将"重复出现的字符 4-gram"作为 hint 注入。
- 解法 — pure helper + prompt 注入：
  - 新 `pub fn detect_repeated_topic(lines, ngram_size, min_distinct_lines) -> Option<String>` 在 `speech_history.rs`：
    - 滑动 ngram_size 字符窗口扫描每条（strip_timestamp 后）
    - 计算每个 ngram 在多少条**不同的**行里出现
    - 跳过含空格的窗口（避免跨词边界）+ 跳过纯单字重复（"嗯嗯嗯嗯" / "...."）
    - ≥ min_distinct_lines 时返最频繁的 ngram，否则 None
  - PromptInputs 加 `repeated_topic_hint: &'a str`，build_proactive_prompt push_if_nonempty
  - run_proactive_turn 调用 helper(recent_speeches, 4, 3)，返回 Some 时构造"你最近多次提到「{topic}」——这次开口请换个角度或换个话题"，过 redact_with_settings
- 决策 — 4-char window for Chinese：3 字符太短（每字 0.7%-字面合理），5 字符太严（错过近义同根词）。4 字符正好"双词组" 量级。Chinese 4-gram ≈ 一个完整语义单元。
- 决策 — min 3 distinct lines（5 中 3）：60% 重复率开始算"显著"。2/5 是巧合，3/5 是 pattern。如果未来 recent_speeches window 改大，需要按比例调 min。
- 决策 — 跳过空格 ngram：跨词边界的"了 我们"会假阳性。简单 rule：window 含 whitespace → skip。
- 决策 — 跳过单字重复：单字 ngram 像"嗯嗯嗯嗯"/"...."是 filler 而非 topic。`window.chars().all(|c| Some(c) == first)` 直接 skip。
- 决策 — 复用 recent_speeches 同一窗口：原本 speech_hint 调用 recent_speeches(5)；R11 detector 也 5 条；改成单次 fetch 复用。speech_hint + repeated_topic_hint 两层 hint 同源，连贯。
- 决策 — 检测结果过 redact：可能有人名/项目名 ngram 命中（"和 X 同事开会" 4-gram = "X 同事"）。过 redact 防止 ngram 文字本身泄漏私人信息到 prompt。
- 测试（7 新单测）：
  - empty input → None
  - no overlap → None
  - 3-line Chinese topic 检测（"工作进展"）
  - min_distinct_lines respect (2 vs 3 boundary)
  - whitespace skip
  - uniform-char skip
  - short-line graceful（<ngram_size 不 panic）
- 测试结果：394 cargo（+7）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：从"speech_hint 让 LLM 自审" 到"detect_repeated_topic 主动告诉 LLM 哪个话题重复了"。pure helper 模式 + push_if_nonempty pipeline = 整个 R11 加约 70 行 source + 100 行 test，但下一次 LLM 重复"工作进展" 的概率显著降低。

## 2026-05-03 — Iter R10：tone strip 反馈率 chip + 路线 R 后续规划
- 现状缺口：R6 在 PanelDebug 加了反馈 timeline collapsible 卡，但用户在 Tone Strip 那一行的 11 个 chip 里看不到 "现在被听见的程度" 信号。打开 collapsible 才能看 ratio 是个友好的 UX 障碍——日常 panel 一瞥应当包含这层信号。
- 解法：
  - ToneSnapshot 加 `feedback_summary: Option<FeedbackSummary>`，FeedbackSummary { replied, total }
  - build_tone_snapshot 读 recent_feedback(20)（与 R6 / R7 同窗口）+ 计 replied count + 装载
  - panelTypes.ts 加对应 TS 类型 + PanelToneStrip 加 chip：💬 N/M（颜色按 R7 adapter 阈值——忽略率 >0.6 红、<0.2 绿、否则灰）
  - 空数据 → None → 不渲染 chip（新装机用户 panel 干净）
- 决策 — 同窗口 20：保持 panel 显示与 gate 行为同源。如果 chip 显示 6/20 (70% ignore)，下一次 cooldown 会 ×2（R7）— 用户能预测系统行为。
- 决策 — 复用 R7 的 ratio band 颜色：>0.6 红 / <0.2 绿 / else 灰，同 adapter 决策的 step function 临界点。chip 颜色就是 visual proxy of "this triggered cooldown adjustment yes/no"。
- 决策 — title 里写"R7 阈值" 文字解释：用户 hover chip 能看到为什么颜色这样——避免 magic colors 的认知负担。
- 路线 R 后续 5 个 TODO 写入（gap analysis 后）：
  - R11: speech topic redundancy detector（chinese ngram overlap）
  - R12: daily review 自动生成（22:00 trigger）
  - R13: companion mode setting（chatty/quiet/coaching/silent_present 预设）
  - R14: 跨日记忆线（注入昨天 speech excerpts）
  - R15: active app 时长追踪（per-minute window snapshot）
- 测试结果：387 cargo（无新增 — 数据 plumbing only，serde flow 已经有信任）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：tone strip 现在第 12 维 chip：feedback summary。用户日常 panel 一瞥就能看到"是不是说太多了"——直接对应 R7 cooldown 调节行为。R1 capture → R6 surface → R7 drive → R10 ambient surface 闭环更紧。

## 2026-05-03 — Iter R9：reactive chat 注入"最近主动开口" system layer
- 现状缺口：proactive 的 bubble 说了"看你还在写 Rust"，用户点开 chat 面板回"刚才说啥来着？"——pet 一脸茫然。bubble 历史不在 chat session 的消息列表里。proactive prompt 已经有 `speech_hint`（避免重复），但 reactive 路径完全看不到自己最近的主动话语。
- 解法 — 第三个 inject_*_layer：
  - 新 `pub fn format_recent_speech_layer(lines: &[String]) -> String`：纯 formatter，把 strip_timestamp + redact_with_settings 后的 bullet list 拼到一个"最近主动开口" 系统消息。空列表 / 全空行 → 返空（caller skip 注入）。
  - 新 `pub async fn inject_recent_speech_layer(messages)`：和 inject_mood_note / inject_persona_layer 同模式 —— 在 first non-system 位置插入系统消息。
  - chat() Tauri 命令在 inject_persona_layer 之后调一次 inject_recent_speech_layer。
- 决策 — 沿用 inject_*_layer pattern：reactive chat 已经有 mood_note + persona_layer 两个系统消息层。recent_speech 是同模式自然延伸。每次 chat turn 都重新 build（recent_speeches 是 file IO，每次 ~ms）。
- 决策 — 5 条窗口 vs proactive 的 5 条对齐：proactive prompt 也读 recent_speeches(5)，reactive 用同样窗口 = 同一段 mental model — "宠物的最近 5 句"。如果未来想分开就再考虑。
- 决策 — redact_with_settings 应用：speech 内容可能含已删的私人信息。其他 inject_* 路径都 redact，新增层一致。
- 决策 — 空列表 silent skip（无系统消息）：让"刚装机的用户" 第一次 chat 时不看到神秘的"最近主动开口" 但啥也没说的 bullet。
- 测试（4 新单测）：
  - `format_recent_speech_layer_returns_empty_for_no_lines`
  - `format_recent_speech_layer_skips_blank_lines`（防 ghost bullets）
  - `format_recent_speech_layer_renders_bullets_in_order`（旧→新 ordering preserved + 钉死 header signal "旧→新" + "接住话题"）
  - `format_recent_speech_layer_strips_timestamps_for_readability`（不浪费 LLM token 在 ISO 串）
- 测试结果：387 cargo（+4）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：reactive chat 现在有完整 "system context"：SOUL.md → mood_note → persona_layer → recent_speech_layer → 用户消息。"你刚才说啥？" → pet 能从 system 消息看到自己说过什么并接续话题。chat 面板与 bubble 历史的"叙事断层" 关上了。

## 2026-05-03 — Iter QG5e：telemetry 子系统拆分到 `proactive/telemetry.rs`（QG5 收官）
- 现状：QG5d 把 gate 抽走后只剩 telemetry 这片是显著的 cohesive cluster。telemetry 是 proactive 子系统的"观察 + 记录"层：static stashes 让 panel 看到上次 turn 状态 + record_proactive_outcome 把每次 turn 的结果埋成可观测信号。
- 改动：
  - 新文件 `src-tauri/src/proactive/telemetry.rs` 239 行
  - 移过去：5 个 `LAST_*` static (LAST_PROACTIVE_PROMPT/REPLY/TIMESTAMP/TOOLS/TURNS + LAST_FEEDBACK_RECORDED_FOR) + `TurnRecord` struct + `ProactiveTurnMeta` struct + `PROACTIVE_TURN_HISTORY_CAP` const + 4 个 Tauri commands (get_last_proactive_prompt / _reply / _meta / get_recent_proactive_turns) + `chatty_mode_tag` + `append_outcome_tag` + `record_proactive_outcome`
  - proactive.rs：`mod telemetry;` + glob `pub use` 加进 head；删除原定义
  - proactive.rs 净减 ~204 行（3232 → 3028）
- 决策 — `ProactiveTurnOutcome` 留 proactive.rs：是 `run_proactive_turn` 返回类型，属 orchestrator 数据，不 telemetry。telemetry 通过 `use super::ProactiveTurnOutcome;` 引入做 record_proactive_outcome 签名。
- 决策 — 把 stashes + 命令 + 记录器放同一个 telemetry 模块：本来想 stashes 一个 mod、recorder 一个 mod。但两者高度协作（recorder 不直接动 stashes，但都 serve 同一个 panel observability 目的）+ 模块切多了 grep 反而困难。一个 cohesive "telemetry" mod 比两个 micro-modules 更易理解。
- 决策 — Tests 留 proactive.rs prompt_tests：append_outcome_tag/record_proactive_outcome 测试在 prompt_tests 通过 super::* 解析；同 QG5c1/QG5c2 的策略。
- **QG5 收官**：5500 → 3028 行，**~45% reduction**。剩余 3028 行全是真正的 orchestration：spawn loop body + run_proactive_turn 巨函数 + InteractionClock + Tauri commands surface + ProactiveTurnOutcome / ProactiveMessage / ToneSnapshot data types。这是健康的 mid-term 终态——核心流程在主文件，cohesive units 各自模块化。
- 模块结构最终态：
  - butler_schedule.rs (628) — 管家任务调度子系统
  - gate.rs (654) — proactive loop 决策门
  - prompt_assembler.rs (375) — prompt 组装
  - prompt_rules.rs (266) — rule label 决策
  - reminders.rs (283) — 用户提醒
  - telemetry.rs (239) — 观测 + 记录
  - time_helpers.rs (317) — 纯时间标签
- 测试结果：383 cargo（无变化）；clippy --all-targets clean；fmt clean；tsc clean。

## 2026-05-03 — Iter QG5d：gate 子系统拆分到 `proactive/gate.rs`
- 现状：QG5c2 后 prompt 系统 (rules + assembler + tests + 时间 helpers) 都各自模块化。下一片是 gate 决策子系统——决定每次 loop tick 该 Silent / Skip / Run 哪条。
- 改动：
  - 新文件 `src-tauri/src/proactive/gate.rs` 654 行（180 src + 470 tests + 4 行 mod doc）
  - 移过去：`enum LoopAction`（pub 升级，spawn loop body 经 re-export 用）、`WAKE_GRACE_WINDOW_SECS` const、`wake_recent` fn、`evaluate_pre_input_idle`、`evaluate_input_idle_gate`、`evaluate_loop_tick` async + `mod gate_tests`（重命名为 `mod tests`）
  - tests 引用 `crate::commands::settings::ProactiveConfig`、`super::*`（gate 同模块）、`super::ClockSnapshot`（通过 `use super::ClockSnapshot;` 显式从 proactive 父模块导入）
  - proactive.rs：`mod gate;` + glob `pub use` 加进 head；删除原 LoopAction enum、4 个 fn 定义和 470 行 gate_tests block
  - 同时删除 proactive.rs 顶层 `use crate::input_idle::user_input_idle_seconds`（unused after gate moves）
  - proactive.rs 净减 ~640 行（3872 → 3232）
- 决策 — gate.rs 是第二大 sub-module（仅次 butler_schedule）：650 行 vs reminders 280 / time_helpers 317 / prompt_rules 266 / prompt_assembler 375。gate tests 量大（470 行）因为有 7 大类边界（disabled/awaiting/cooldown/quiet/focus/wake/input_idle）每个都需要细粒度 tests。
- 决策 — `evaluate_loop_tick` async fn 跟着走：依赖 AppHandle + InteractionClockStore + WakeDetectorStore + feedback_history 各种 IO 调用，但 spawn loop body 只用 `evaluate_loop_tick(&app, &settings)` 一次。让它 pub + 跟同 mod gate 测试代码一起。
- 决策 — `super::ClockSnapshot` 显式 use：避免含糊 `super::*` 让 grep 帮不上忙。明确"gate 依赖 parent 的 ClockSnapshot 类型"，类似 prompt_assembler 的 `use super::{...}` 模式。
- 决策 — `pub const WAKE_GRACE_WINDOW_SECS`：原本 private const，但 gate 上提到模块顶层 const + glob re-export 让外部如果未来要 surface "wake softening 窗口" 给 panel 也能直接 import 不需要再改 visibility。
- 测试结果：383 cargo（无变化—测试只换了运行位置）；clippy --all-targets clean；fmt clean；tsc clean。
- 进度：QG5a (–110) + QG5b (–642) + QG5c-prep (–308) + QG5c1 (–229) + QG5c2 (–342) + QG5d (–640) 共减 ~2270 行（5500→3232，~41%）。剩 QG5e（telemetry / static stashes 等）。

## 2026-05-03 — Iter QG5c2：prompt assembler 抽离到 `proactive/prompt_assembler.rs`
- 现状：QG5c1 抽完 rule-label 生成器后，prompt 系统的"决策" 和"渲染" 分离了。这次抽走渲染层 — PromptInputs 数据结构 + proactive_rules 规则-文字映射 + build_proactive_prompt 装配 + 两个 hint formatters + SILENT_MARKER。
- 改动：
  - 新文件 `src-tauri/src/proactive/prompt_assembler.rs` 375 行
  - 移过去：SILENT_MARKER（私 const → pub const）、PromptInputs struct（30+ fields）、proactive_rules（含 14 个 rule arms）、build_proactive_prompt、push_if_nonempty、format_proactive_mood_hint、format_plan_hint
  - proactive.rs：`mod prompt_assembler;` + glob `pub use` 加进 head；删除原定义；删除 `MOOD_CATEGORY`、`MOOD_TITLE` 顶部 import（不再用）
  - prompt_tests 加 `use crate::mood::{MOOD_CATEGORY, MOOD_TITLE};`（assembler 拿走 import 后测试还需要）
  - proactive.rs 净减 ~342 行（4214 → 3872）
- 决策 — `use super::*` 引入 prompt_rules + 同级 fns：prompt_assembler 引用 active_*_rule_labels（在 prompt_rules.rs）+ companionship_milestone（同 prompt_rules.rs）+ format_companionship_line（在 proactive.rs main）。统一通过 `use super::{...}` import — super 是 proactive.rs，所有这些都通过 glob re-export 暴露。比 `use crate::proactive::*` 更明确依赖。
- 决策 — SILENT_MARKER 升级 pub：原本 const private 因为只有 proactive_rules 用。run_proactive_turn (in proactive.rs) 也用，本来通过同 mod 直接访问；现在 assembler 移走后必须 pub + glob 才能让 run_proactive_turn 经 re-export 访问到。
- 决策 — tests 继续留 proactive.rs：原 prompt_tests 1620 行 + base_inputs() 共用 helper 用了大量 super::* 解析的 items（active_*_rule_labels / format_companionship_line / format_proactive_mood_hint / 各种 mood / period / 等等）。如果挪到 sub-module，super 就变成 prompt_assembler，丢失对其他 sibling 子模块的可见性。让 tests 留在 proactive.rs（super 是 proactive.rs 顶层）通过 glob re-export 拿到所有 moved items 是最低 friction 路径。
- 决策 — `(不必每次推进...)` 中文括号一致性：复制粘贴时不小心把全角括号变成 ASCII 括号。grep 与原文对比发现差异立即修。任何字符变化都可能让 prompt 测试失败 — 抽离时极仔细对照。
- 测试结果：383 cargo（无变化—测试只是通过 re-export 解析迁移后的 fn）；clippy clean；fmt clean；tsc clean。
- 进度：QG5a (–110) + QG5b (–642) + QG5c-prep (–308) + QG5c1 (–229) + QG5c2 (–342) 共减 ~1630 行（5500→3872，~30%）。剩 QG5d（gate 子系统）+ QG5e（telemetry / static stashes）。

## 2026-05-03 — Iter QG5c1：rule-label 生成器抽离到 `proactive/prompt_rules.rs`
- 现状：QG5c-prep 抽完 time helpers 后，prompt rules 的依赖图变干净。下一步抽 rule-label 生成器（决定哪些 label 当前激活），把 prompt_rules 的"决策" 部分独立出来。
- 改动：
  - 新文件 `src-tauri/src/proactive/prompt_rules.rs` 266 行
  - 移过去：3 个 `active_*_rule_labels` 函数 + 4 个阈值 const（ENV_AWARENESS_*, LONG_*）+ 3 个 LATE_NIGHT_* const + LAST_LATE_NIGHT_WELLNESS_AT static + 3 个 late_night_wellness_* fns + env_awareness_low + companionship_milestone
  - proactive.rs 添加 `mod prompt_rules;` + glob `pub use`，删除原定义
  - **测试故意留 prompt_tests**：`mod prompt_tests` 用 `use super::*` 通过 re-export 拿到所有移走的 fns，零 test diff。下一片 QG5c2 把测试和源代码一起搬。
  - proactive.rs 净减 ~229 行（4443 → 4214）
- 决策 — 测试不一并迁移：rule-label 测试和 proactive_rules / build_proactive_prompt 的 prompt-assembly 测试在 prompt_tests 里深度交错（active_composite_rule_labels 的 boundary tests + proactive_rules_has_match_arm + frontend alignment 都在同一 mod）。提前拆 rule-label tests 出来意味着 prompt_tests 里要剩下"半个" 文件 — 反而难 review。等 QG5c2 把整个 prompt 系统一起搬，prompt_tests 整体迁移最干净。
- 决策 — `pub static LAST_LATE_NIGHT_WELLNESS_AT` 跟着规则走：static 是 rate-limit 实现细节，与 active_composite 中的 late-night-wellness label 强耦合。和它一起迁移让"late-night-wellness 子系统" 成为一个完整 unit。
- 决策 — `format_companionship_line` 留 proactive.rs：它是 prompt 中的 line renderer（"陪伴第 X 天"），跟 build_proactive_prompt 在一起。`companionship_milestone`（rule label producer）走，是因为它产生的是 label 字符串而非 prompt line。两者用途分立。
- 测试结果：383 cargo（无变化—测试只是通过 re-export 找到迁移后的 fn）；clippy --all-targets clean；fmt clean；tsc clean。
- 进度：QG5a (–110) + QG5b (–642) + QG5c-prep (–308) + QG5c1 (–229) 共减 ~1290 行（5500→4214，~23%）。

## 2026-05-03 — Iter QG5c-prep：纯时间/日历/idle 帮助器抽离
- 现状：QG5c (prompt rules) 是最大块，直接做风险高。先抽出 prompt rules **依赖** 的纯帮助器到独立模块 — 让接下来的 QG5c slice 拿到更干净的边界。
- 改动：
  - 新文件 `src-tauri/src/proactive/time_helpers.rs` 317 行（148 src + 169 tests）
  - 移过去 8 个纯函数：`idle_tier` / `user_absence_tier` / `period_of_day` / `weekday_zh` / `weekday_kind_zh` / `format_day_of_week_hint` / `minutes_until_quiet_start` / `in_quiet_hours`
  - 18 个相关单测（合并自 3 个原 mod test：`pre_quiet_tests` / `cadence_tests` / `period_tests` + prompt_tests 里的 weekday_zh / weekday_kind_zh / format_day_of_week_hint / user_absence_tier_maps_each_band / quiet_hours_disabled / quiet_hours_same_day / quiet_hours_wraps_midnight 4 个嵌入测试）
  - proactive.rs：`mod time_helpers;` + glob `pub use` 第三行加入 head；删除原 8 个 fn 定义 + 3 个 mod test + 7 个 prompt_tests 嵌入测试
  - proactive.rs 净减 ~308 行（4751 → 4443）
- 决策 — "prep" iter：把 prompt rules 拆分前先 isolate 纯依赖。这让 QG5c 的 diff 严格只 about prompt rules，不再夹杂 "顺便也搬了几个 helper"。这种 staged refactor 更易 review、风险低。
- 决策 — companionship_milestone / format_companionship_line / chatty_mode_tag 留 proactive.rs：与 prompt rules 紧耦合（companionship_milestone 用作 active_data_driven_rule_labels 的依据 + 在 format_persona_layer 引用），跟 QG5c 一起搬更自然。chatty_mode_tag 同时被 gate 和 telemetry 用，应该留 parent。
- 决策 — LONG_IDLE_MINUTES / LONG_ABSENCE_MINUTES / LATE_NIGHT_* 留 proactive.rs：这些 const 服务 active_composite_rule_labels 的 prompt rule 决策。和 prompt rules 一起搬。
- 测试结果：383 cargo（无变化—测试只换了运行位置）；clippy --all-targets clean；fmt clean；tsc clean。
- 进度：QG5a (–110) + QG5b (–642) + QG5c-prep (–308) 共减 ~1060 行（5500→4443，~19%）。

## 2026-05-03 — Iter QG5b：butler_tasks schedule 子系统拆分
- 现状：QG5a 把 reminders 抽走 ~110 行后，proactive.rs 还是 5393 行。butler 子系统是下一个 cohesive 自然块（Iter Cζ-Cπ 累积建立的 schedule + due + completion + format 整套）。
- 改动：
  - 新文件 `src-tauri/src/proactive/butler_schedule.rs` 628 行（241 src + 387 tests + private helper）
  - 移过去：
    - `ButlerSchedule` enum、`parse_butler_schedule_prefix`、`is_butler_due`、`is_completed_once`、`has_butler_error`、`format_butler_tasks_block` 6 个 pub 项
    - `BUTLER_TASKS_HINT_MAX_ITEMS` / `BUTLER_TASKS_HINT_DESC_CHARS` 两个常量
    - 私有 `parse_updated_at_local` helper（`is_butler_due` / `is_completed_once` 共用）
    - 24 个相关单测 + 私有 `fixed_now()` / `count_task_lines_with_marker()` test helper
  - proactive.rs：`mod butler_schedule;` + glob `pub use` 加进 head；删除原定义和 `mod prompt_tests` 里的 butler 测试段（行 3216-3625 一整段）
  - proactive.rs 净减 ~640 行（5393 → 4751，~12% 缩小）
- 决策 — `parse_updated_at_local` 留私（不导出）：仅 butler 内部用。如果未来 reminders 也想 parse `updated_at` 再考虑提到 `proactive` 顶层，但 YAGNI。
- 决策 — `build_butler_tasks_hint` (memory IO + redact) 留 proactive.rs：和 QG5a 一样模式——pure formatter 移走，env-touching builder 留 parent。两者保持一致 = future maintainer 看一眼就懂"哪类该留、哪类该走"。
- 决策 — 移动测试 helper（`fixed_now` / `count_task_lines_with_marker`）：仅 butler 用，跟着 butler 测试走。检查发现 fixed_now 真的只在 butler 测试里调用，移走零风险。
- 测试结果：383 cargo（无变化—测试只换了运行位置）；clippy --all-targets clean；fmt clean；tsc clean。
- 路线进度：QG5a + QG5b 共减 ~750 行 (~14%)。剩余 QG5c-e（prompt rules、gate、telemetry）将进一步把 proactive.rs 推向 < 3000 行可维护体量。

## 2026-05-03 — Iter QG5a：reminders 子系统拆分到 `proactive/reminders.rs`
- 现状：proactive.rs 5500+ 行，QG5 一直被 deferred 因为"太大单 iter 做不完"。改"全切"为"一片一片切"——每 iter 抽一个 cohesive 子系统，public API 由 `pub use` glob 保持稳定。
- 选 reminders 作为第一片：(a) 完全自包含（无内部依赖于其他 proactive 状态）；(b) 已经是清晰边界（5 个 pub fn + 1 enum + 17 个 unit tests）；(c) 已被外部模块（`consolidate.rs`）通过 `crate::proactive::...` 引用——good 切口测试 re-export 是否真的兼容。
- 改动：
  - 新文件 `src-tauri/src/proactive/reminders.rs`（283 行：170 src + 113 tests）
  - 移过去：`ReminderTarget` enum / `parse_reminder_prefix` / `is_reminder_due` / `format_target` / `is_stale_reminder` / `format_reminders_hint` + 17 测试（重命名 `mod reminder_tests` → `mod tests` 因为已经在子文件里）
  - proactive.rs 头部：`mod reminders;` + `pub use self::reminders::*;`
  - proactive.rs 净减 ~110 行（5500→5393）
- 决策 — Rust 2018 module nesting：用 `src/proactive.rs` + `src/proactive/<sub>.rs` 而不是 `src/proactive/mod.rs` 改造。理由：(a) 保留 git blame on proactive.rs；(b) 渐进式不改变现有 grep / IDE 路径；(c) 现代 Rust 推荐的格式。
- 决策 — `pub use self::reminders::*;` 全 glob 而不是 explicit `pub use self::reminders::{ReminderTarget, parse_reminder_prefix, ...}`：glob 不会触发 `unused_import` lint（即使 proactive.rs 自己只用部分），并且未来加 / 删 reminders 公共 API 时不需要同步改 re-export 列表。
- 决策 — 测试整体跟随源代码移动：`mod tests` 内嵌在子模块中，比保留在 proactive.rs 顶层更符合 "测试与代码同居" 的 Rust 习惯。
- 测试结果：383 cargo（无变化—测试只是换了运行位置）；clippy --all-targets clean；fmt clean；tsc clean。
- 路线 — TODO.md 加 QG5a-e checklist：
  - [x] QG5a reminders（本 iter）
  - [ ] QG5b butler_tasks schedule
  - [ ] QG5c prompt rules（最大块）
  - [ ] QG5d gate logic
  - [ ] QG5e telemetry / static stashes
  - 这种 incremental decomposition 比一次大重构 risk 小：每片独立 commit，easy revert，每片测试都跑。

## 2026-05-03 — Iter R7：feedback ratio 驱动 cooldown（capture→surface→drive 闭环）
- 现状缺口：R1 采集 + R6 surface 后，"被忽略" 信号还是被动数据——LLM prompt 里有提示但 cooldown gate 不动。如果用户 7/10 都忽略，pet 还是按基线 cooldown 继续重复试探，违背"宠物会读空气" 的设计意图。
- 解法 — 三段 pure helpers + 一处 gate 改动：
  - `ignore_ratio(entries) -> Option<(f64, usize)>`：纯函数，empty → None；非空 → (ignored/total, total)
  - `adapted_cooldown_seconds(base, ratio, sample_count) -> u64`：3-band step function：
    - sample < `FEEDBACK_ADAPT_MIN_SAMPLES` (5) → 返 base 原值（噪声防护）
    - ratio > `ADAPT_HIGH_IGNORE_THRESHOLD` (0.6) → base × `ADAPT_HIGH_IGNORE_MULTIPLIER` (2.0)
    - ratio < `ADAPT_LOW_IGNORE_THRESHOLD` (0.2) → base × `ADAPT_LOW_IGNORE_MULTIPLIER` (0.7)
    - 中间带 → base unchanged
  - `evaluate_pre_input_idle` 加第 6 个参数 `effective_cooldown_seconds: u64`，gate 用这个值（不再用 cfg.cooldown_seconds 直接）
- 集成 — `evaluate_loop_tick` 在调 gate 前 await `recent_feedback(20)` 并 compute ratio + adapted。20 条窗口和 R6 panel timeline 同源，让 panel 显示的"6/20 ignored" 直接对应到 gate 行为。
- 决策 — step function 而非 smooth curve：3 band 让 panel reader 能心算结果（"哦今天 ratio 0.7 > 0.6，所以 cooldown 翻倍"）。smooth curve（linear / sigmoid）更"漂亮" 但不可审计。auditability > elegance for behavior-shaping logic。
- 决策 — base=0 不被 adapter 启用：用户故意把 cooldown_seconds 设成 0（"我 ok 宠物频繁说话"）后，high-ignore 不应"为 ta 好" 强制开启 cooldown。adapter 在 base=0 时返 0 是 desired no-op。`(0 as f64) * 2.0 = 0.0` as u64 = 0，math 自然帮忙。但加 explicit test 钉住。
- 决策 — min samples 5：少于 5 ratio 噪声大；首日装机用户随便忽略一两次会立刻被 cooldown 翻倍 = 糟糕新手体验。5 条样本之上 adapter 才动手。
- 决策 — 改 evaluate_pre_input_idle 签名而不是在外部加 wrapper：原本想在 gate 之前加一层 "if adapted_cooldown < base: skip extra" 但那不能放宽（low-ignore 0.7×）只能收紧。直接改 signature 让 gate 用一个值才能双向调节。代价是要更新 19 个 test call sites + 1 production call site，但这是一次性 cost 后续都对。
- 测试 — 9 新单测：
  - `ignore_ratio_returns_none_for_empty_input` / `_counts_correctly` (3/5=0.6) / `_handles_all_replied` (0.0) / `_handles_all_ignored` (1.0) — 4 ratio 边界
  - `adapted_cooldown_returns_base_below_min_samples` (n<5 不动)
  - `_doubles_on_high_ignore_ratio` (>0.6 → 2x)
  - `_shrinks_on_low_ignore_ratio` (<0.2 → 0.7x)
  - `_keeps_base_in_mid_band` (0.2-0.6 → 不动)
  - `_handles_zero_base` (base=0 不被 adapter 启用——重要 settings 兼容性)
- 测试结果：383 cargo（+9）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：R 系列 capture (R1) → surface (R6) → drive (R7) 三段链路完成。"用户连续忽略 → 宠物自然安静下来" 是真实 feedback loop。设置 cooldown=30min 的用户，今天 70% ignore 后 effective cooldown 跳到 60min；engaging 好的日子降到 21min（30 × 0.7）。

## 2026-05-03 — Iter R6：panel feedback timeline（surface R1 capture data）
- 现状缺口：R1 把每次 proactive 后用户的 replied/ignored 写到 feedback_history.log，但 panel 没显示。"宠物在学习吗？" 这个问题没有可观察答案。
- 解法 — R4 同模式：
  - FeedbackKind 加 `#[derive(Serialize)] + #[serde(rename_all = "lowercase")]` 让 IPC 输出 "replied" / "ignored" 字符串
  - FeedbackEntry 加 `#[derive(Serialize)]`，去掉 timestamp 上的 `#[allow(dead_code)]`（现在被 panel 真用了）
  - 新 `#[tauri::command] get_recent_feedback()` 异步读取 + reverse 成 newest-first
  - DebugSnapshot 加 `recent_feedback: Vec<FeedbackEntry>` 字段，在 get_debug_snapshot 中拉 20 条 reverse
- Frontend — PanelDebug 加"💬 宠物反馈记录" collapsible:
  - 标题里嵌入 "{回复数}/{总数} 回复" 即时反馈率（用户一眼能看到 "今天 3/8 回复" 倾向）
  - 展开后 timeline：HH:MM + 回复/忽略 pill + 截断 excerpt
  - 默认收起（避免长 session 撑开 panel；和 R4 工具调用历史同 UX 决策）
- 测试 — 2 个新单测，专钉 IPC 边界契约：
  - `feedback_kind_serializes_as_lowercase_for_frontend`：钉死 "replied" / "ignored" 字符串。如果有人把 enum 名字改了或漏了 rename_all，panel 渲染会变成空 pill —— 这测试在 backend 阻挡这个回归
  - `feedback_entry_serializes_with_all_three_fields`：sanity 检查 timestamp / kind / excerpt 都进 JSON
- 决策 — title 里嵌反馈率而不是单独 metric：节省 panel 空间 + 给"是否要展开看细节" 一个判断依据。如果反馈率明显低（比如 1/10 回复），用户会主动点开看是哪些 utterance 被忽略——是 design-for-curiosity。
- 决策 — newest-first（reverse）：和 R4 工具调用历史一致。chat panel 上"刚发生" 的事在最上面更符合直觉。
- 决策 — 20 条窗口：FEEDBACK_HISTORY_CAP=200，但 panel 看 20 够了。R7（自适应 cooldown）需要更宽窗口（24h ratio）时再独立读。
- 测试结果：374 cargo（+2）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：R1（采集）→ R6（surface）链路打通。下次开口前 panel 用户能看到"过去 20 次中 12 次被回复" 这种倾向数据。R7 可以基于这层数据闭回 cooldown 行为。

## 2026-05-03 — Iter R8：late-night-wellness 30 分钟 rate limit + 后续路线
- 现状缺口：R3 加的 late-night-wellness 规则只要 (hour<4 && idle<5min) 都会激活——按典型 5 分钟 proactive loop 间隔，半夜用户在键盘前一小时可能被提醒 12 次"该睡了"。这从关心变成骚扰。
- 解法 — pure-helper 三层 + 一个 dispatch-time stamp：
  - 新 const `LATE_NIGHT_WELLNESS_MIN_GAP_SECONDS = 1800` (30 分钟)
  - 新 static `LAST_LATE_NIGHT_WELLNESS_AT: Mutex<Option<Instant>>`，process-wide
  - `late_night_wellness_recently_fired_at(last, now, gap_secs) -> bool`：纯函数，给定 last + now + gap 直接返回是否在 cooldown
  - `late_night_wellness_in_cooldown()`：production-side 包装器，读 static + Instant::now()
  - `mark_late_night_wellness_fired()`：写 static
- API 改动：`active_composite_rule_labels` 新增第 9 个参数 `recently_fired_wellness: bool`，late-night-wellness 仅当 `!recently_fired_wellness && hour<4 && idle<5min` 才 push label
- PromptInputs 加 `recently_fired_wellness: bool` 字段，base_inputs 默认 false
- 三个生产 call site：
  - proactive_rules（PromptInputs 路径）：传 inputs.recently_fired_wellness
  - get_tone_snapshot（panel surface）：传 late_night_wellness_in_cooldown()
  - 后台 loop wrapper（dispatch tag 路径）：传 late_night_wellness_in_cooldown()，并在 active_labels 含此 label 时调 mark_late_night_wellness_fired() 锁定下个 30 min
- run_proactive_turn PromptInputs：构建时也调 late_night_wellness_in_cooldown()
- 决策 — dispatch-time stamp（不等 LLM Spoke 后才 stamp）：避免 near-edge 抖动 — LLM 收到 rule 但选择 silent 也消耗 30min 窗口。代价是"如果 LLM 多次拒绝 wellness"用户在 30 分钟内不会收到第二次。这个权衡 OK：边缘情况不该让普通流程变复杂；用户真要被提醒 30 分钟一次也不算少。
- 决策 — pure helper 接 last + now 显式参数：纯函数 + 测试零 setup。production wrapper 调 Instant::now() / 读 static 后转交给 pure 版本。
- 决策 — process-wide 而非 settings 配置：30 分钟是合理硬编码。让用户调反而暗示 "你应该思考多久打扰一次合适"——违背 "宠物有自己 opinion" 设定。如果未来真有 user complaint 再升级成 setting。
- 测试（2 新单测）：
  - `late_night_wellness_recently_fired_at_gates_window`：钉住 None / 15min(suppress) / 30min(allow boundary) / 60min(allow)
  - `active_composite_rule_labels_late_night_wellness_suppressed_in_cooldown`：rule 在 cooldown 内不 push label，即使其他条件满足
- 路线规划补全（gap analysis 后写入 TODO.md）：
  - **Iter R6**：feedback_history.log 在 panel timeline 可见（R1 数据已有，需要 surface）
  - **Iter R7**：feedback signal 驱动 cooldown 调整——高 ignore ratio → 自适应 silence
  - 这俩自然把 R series 闭环：R1 采集，R6 surface，R7 用数据闭回行为
- 测试结果：372 cargo（+2）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：半夜的 wellness ping 从"每 5 分钟一次" → "30 分钟最多一次"。R3 wellness 是关心，R8 才让它真的像关心而不是 nag。

## 2026-05-03 — Iter R4：PanelDebug 显示工具调用历史（purpose + risk + review）
- 现状缺口：TR1 把 purpose 写进 app.log，TR2 写 risk 评估，TR3 写 approve/deny/timeout——但 panel 上要 prompt 调优时只能 grep app.log。本 iter 把这三层数据 surface 到一个 panel 折叠卡片。
- 设计选择 — 结构化 ring buffer 而非日志解析：
  - 解析 app.log 行能 work，但脆弱（regex 跟 log format 耦合 + 跨行 stitching 复杂）
  - 后端在 chat pipeline 末尾原子捕获每次 tool call 的所有元数据，pushed 进静态 ring buffer (cap 30)
  - 这是 E series 的同模式：LAST_PROACTIVE_TURNS 也是这么做的。复用成熟思路。
- 后端 — 新模块 `src/tool_call_history.rs`：
  - `enum ToolCallReviewStatus { NotRequired, Approved, Denied, Timeout, MissingPurpose }`：5 个 outcome 对应 chat pipeline 五个 branch
  - `struct ToolCallRecord { timestamp, name, args_excerpt, purpose, risk_level, reasons, safe_alternative, review_status, result_excerpt }`：每次 call 一条
  - `truncate_excerpt(text)` pure helper：args 和 result 都 cap 在 200 char + …，避免大文件 read/write 撑爆 buffer
  - `record_tool_call(...)` 把所有字段 push 进 `static Mutex<VecDeque<ToolCallRecord>>`，cap 30 自动 roll
  - `recent_tool_calls()` newest-first；`#[tauri::command] get_recent_tool_calls()`
- 集成 — `chat.rs` 工具循环：
  - 每个 tool call iteration 加 4 个 mut 变量：record_status / record_risk / record_reasons / record_safe_alt
  - 五个 branch 各自设置 record_status：MissingPurpose（None purpose）/ Approved（review approve）/ Denied（review deny / channel-lost）/ Timeout（review 60s timeout）/ NotRequired（low/medium 直接执行）
  - assessment.risk_level / reasons / safe_alternative stash 一次（仅 Some(p) branch 有 assessment）
  - `result` 计算完后 `record_tool_call(...)` 一次 push
- DebugSnapshot 加 `recent_tool_calls: Vec<ToolCallRecord>` 字段（QG6 模板）。
- 前端 PanelDebug：
  - 新 collapsible "🔧 工具调用历史（N）" 卡片（chevron 控制 show/hide，避免长 session 占满 panel）
  - 每条记录显示：tool name + risk badge（low/med/high 三色）+ status badge（5 种 review status 不同色 + 中文 label）+ timestamp + purpose + reasons + safe_alternative + 折叠的 args/result excerpt 
  - `riskBadgeBg` / `reviewStatusBg` / `reviewStatusLabel` 三个 helper 函数，badge 样式集中
- 决策 — `#[allow(dead_code)]` on `as_str()`：serde 的 `rename_all = "lowercase"` 已经能把 enum 序列化成那些字符串。我加 `as_str()` 是给后端测试用的（不依赖 serde 内部）。生产 path 不调用，但保留作为公共契约文档。
- 决策 — 测试用 HISTORY_TEST_LOCK serialize：record_tool_call 触摸静态 mutex；cargo 默认并行测试时 `record_tool_call_pushes_with_newest_first_order` + `record_tool_call_caps_at_history_max` 互相污染。一个 `static Mutex<()>` 测试 guard 序列化。`unwrap_or_else(|e| e.into_inner())` 处理 poison 让 panic 测试不阻塞后续 test。
- 决策 — 不复用 LAST_PROACTIVE_TURNS：proactive turn 是 N 个 tool call + 1 个 LLM reply 的整体；tool call history 是 per-call 粒度。两者维度不同，不应混淆。
- 测试（5 新单测）：
  - truncate_excerpt 三个边界：空 / 短 / 长（含 ASCII + Chinese 不撞 byte boundary）
  - record_tool_call newest-first push order
  - record_tool_call 容量 cap (CAP+5 写入只剩最后 30，最旧的滚出)
  - review_status as_str 5 个分支字符串映射钉死（前端依赖此契约）
- 测试结果：370 cargo（+5）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：reactive chat tool 调用现在在 panel 顶部的"🔧 工具调用历史"折叠卡里实时可见——一条记录把"什么工具 + 为什么调（purpose）+ 多大风险（risk + reasons）+ 用户怎么决定（review status）+ 输入输出片段"全部显示。prompt 调优工作流不再需要 tail -f app.log。

## 2026-05-03 — Iter R5：reactive 会话 SOUL.md hot-reload（修补烘焙盲点）
- 现状审计：原 TODO 写"SOUL.md 改了得重启 app 才生效"，但其实 proactive (`run_proactive_turn` line 1822) 和 telegram (`bot.rs:119`) 都在每次 turn 调 `get_soul()` —— 全是从磁盘读，无缓存层。所以这两条路径**已经自动 hot reload**。
- 真正的 gap 在 reactive chat：`commands::session::create_session` 把 SOUL.md 烘焙进 `messages[0]`（系统消息），后续每次发消息时前端从 session 拉这个 stale system message 发回后端。session 一旦创建，SOUL 改动被忽略——直到用户开新 session。
- 解法 — pure helper `refresh_leading_soul(messages, current_soul)`：
  - 如果 messages[0].role == "system" 且 current_soul 非空（trim 后），用 current_soul 替换 messages[0].content；否则原样返回
  - 防御性 skip blank：current_soul empty/whitespace → no-op（不要把好好的 stale SOUL 替成空）
  - skip non-system-first：未来可能有 history 不以 system 开头（pre-R5 老 session、cli 路径），不强行加 SOUL
  - 只动第一个 system 消息：mood/persona 系统消息会在第二个槽位之后出现（inject_mood_note 插在 first non-system 位置）—— 这些不是 SOUL，必须留下
- 集成：`chat()` Tauri 命令在 trim_to_context 之后、inject_mood_note 之前调一次 refresh_leading_soul + get_soul。新参一个 IO（每 chat turn 读一次小文件），代价可忽略。
- 决策 — 不加 panel "立即重新加载 SOUL" 按钮：原 TODO 提了作为 fallback。但 hot-reload 现在是自动的（每个 turn 都新读），fallback 没意义；按钮反而变成"用户认为需要点一下" 的认知噪音。如果未来加文件 watcher（实时通知），按钮才有意义。
- 决策 — 不动 session 存储：persistent session 文件里仍存旧 SOUL，下次重新打开历史会话还会显示旧的 SOUL。但 (a) 用户聊天 UI 不显 system message；(b) 实际 LLM input 永远新；(c) session 应该忠实记录"当时" 的对话上下文，把 SOUL 当时间快照存反而更诚实。所以 session 持久层故意不动。
- 决策 — 不缓存 SOUL：现 IO 是"每 turn 读一次几 KB 本地文件"，无瓶颈。加缓存 + invalidation 反而引入新的"什么时候 invalidate" 复杂度。Filesystem 已经是最快的缓存，让 OS page cache 处理。
- 测试（5 新单测）：
  - replaces_first_system_content（标准 happy path）
  - no_op_when_first_is_not_system（pre-R5 / 怪 history 兼容）
  - only_touches_first_system_when_multiple（pin"只动 SOUL slot，不动 mood/persona 后续 system 槽"）
  - skips_when_current_is_blank（防御 empty SOUL 摧毁 prior）
  - empty_messages_passes_through（边界）
- 测试结果：364 cargo（+5）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：reactive chat 现在每个 turn 都用最新 SOUL 重发给 LLM。开发改 SOUL 不再需要 "新建会话" 这个仪式动作。proactive / telegram 路径不变（早就 hot-reload 了）。

## 2026-05-03 — Iter R3：late-night-wellness 复合规则（凌晨该睡了硬提醒）
- 现状缺口：宠物的 wellness 关怀是软的——靠 plan_hint 让 LLM "看到深夜还在工作时主动关心"，但前提是宠物自己写过 daily_plan + LLM 当下判断决定提。如果用户连续好几个深夜都加班，pet 没机制硬性 override，可能继续 chatty / icebreaker / engagement 那几套常规规则。
- 解法 — 第四个 composite rule `late-night-wellness`：
  - 触发条件：`hour < LATE_NIGHT_END_HOUR (4)` AND `idle_minutes < LATE_NIGHT_ACTIVE_MAX_IDLE_MIN (5)`。即 0:00-3:59 且键鼠 5 分钟内有动作（确实坐在电脑前）。
  - **不**像 long-idle/long-absence 那样 gate on chatty / pre_quiet。pet 健康优先于 cadence——chatty 已超也得说，pre_quiet 也照说。
  - prompt 文本：直接关心+建议睡（"哎，{hour} 点了还在忙啊？该睡了"），明确 **不要** 起新话题/追问工作/长篇——一句关心 + 一句"该睡了"。如果用户在做收尾动作可以更轻盈说晚安。
- 改动：
  - PromptInputs 加 `hour: u8` 字段（base_inputs 默认 14 = 下午，对老 tests 中性）
  - `active_composite_rule_labels` 加第 8 个参数 `hour: u8` + 新分支
  - 3 个生产 call site 都 wire `now_local.hour() as u8` / `now_for_rules.hour() as u8`
  - proactive_rules 加 match arm，使用 hour + idle_minutes 拼出 wellness 文本
  - 鉴于这是 engagement-type 规则（push pet to speak），分类时落入 prompt_tilt 的 engagement bucket（已在 PromptTiltCounters::record_dispatch 的 match 之外，会归入"corrective/instructional"——OK 因为这本质是硬规则不是 tone tilt 信号）
  - panelTypes.ts PROMPT_RULE_DESCRIPTIONS 加 "late-night-wellness" 行
  - fingerprint 表加 ("late-night-wellness", "深夜还在用电脑")
- 测试：
  - 新 unit test `active_composite_rule_labels_late_night_wellness_gating` —— 钉住四种边界：hours 0-3 都 fire / hour=4 不 fire / idle=5 不 fire / chatty+pre_quiet 都 set 时仍 fire（验证 wellness override）
  - `proactive_rules_has_match_arm_for_every_backend_label` 测试加第三个 scenario s3（hour=2, idle=1）以让 fingerprint 覆盖到新 label
  - 全 universe-enumeration 测试也分两步 chain composite calls：一次 idle 高（covers long-absence），一次 idle 低 + hour=2（covers late-night）。两组 label set 互斥不能同 call 拿到。
- 决策 — wellness 不 gate 在 chatty / pre_quiet：原本想加保护 ("不在 chatty 上加 wellness")，但 wellness 的整个意义就是 override 常规 cadence。如果半夜两点用户连续 3 小时活跃，chatty=10 也得说；pre_quiet 也得说（甚至更应该说，pre_quiet 是预告 quiet 时段，wellness 在 quiet 之前介入正合适）。
- 决策 — `hour < 4` 用 const `LATE_NIGHT_END_HOUR`：直接 magic number 4 不可读；提到 const 让"什么时候算晚" 这个 policy 单点定义，未来调到 5 / 3 改一处即可。idle 阈值同理用 LATE_NIGHT_ACTIVE_MAX_IDLE_MIN。
- 决策 — 不写专门 settings 项：wellness 太小一个 policy，加 setting 反而让普通用户 nervous（"我能调多晚才不该睡？" 暗示 ta 应该自己想清楚）。两个 const 写死，行为隐式但符合"宠物有自己 opinion" 的设定。
- 测试结果：359 cargo（+1 net，新 4 旧测加 hour 参数）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：现在凌晨 0-4 点用户还在键盘前，pet 必然 dispatch 一条 wellness 关心——即使：今天已经 chatty、pre_quiet 已开、用户还没"长 absent"过。是 plan_hint 之外第一个**硬规则** wellness 介入。

## 2026-05-03 — Iter R1：用户反馈信号采集 + 注入下次 proactive prompt
- 现状缺口：宠物每次 proactive 都是"开完口就发了"——没有反馈循环。同样话术不管用户上一句是回应了还是完全无视，都会用同样的语气重复。要变成"会学" 的伴侣，得至少把"上次开口的下场" 喂回 prompt。
- 解法 — 在 InteractionClock 的 awaiting flag 上做被动观测：用户在两次 proactive 之间发消息会触发 `mark_user_message`（清 awaiting）；如果一直没发，下次 proactive fire 时 awaiting 仍是 true。这两个信号刚好对应 replied / ignored，**完全不需要前端 UI 改动**。
- 新模块 `src/feedback_history.rs`：
  - `enum FeedbackKind { Replied, Ignored }` + `struct FeedbackEntry { timestamp, kind, excerpt }`
  - `format_line(ts, kind, excerpt) -> String` / `parse_line(line) -> Option<FeedbackEntry>`：单行 round-trip 格式 `{ts} {kind} | {excerpt}`，excerpt 截 40 字符
  - `record_event(kind, prev_speech_excerpt)` async：append + 200 行 cap 自动 roll
  - `recent_feedback(n)` async：读最近 N 条
  - `format_feedback_hint(entries)` pure：取最新一条产出 prompt 提示文字
- InteractionClock 加 `pub async fn raw_awaiting()`：feedback 分类需要"用户实际是否回了"（不应用 D11 4h 自动 expire）。effective_awaiting 仍管 gate 行为不变。
- 在 `run_proactive_turn` 头部分两步：(1) 看 LAST_PROACTIVE_TIMESTAMP + LAST_PROACTIVE_REPLY 是否有上一轮，配合 LAST_FEEDBACK_RECORDED_FOR 去重，根据 `clock.raw_awaiting()` 分类并 record_event；(2) 读 recent_feedback(1) + format_feedback_hint 得到一行字符串。
- PromptInputs 加 `feedback_hint: &'a str`，build_proactive_prompt 在 speech_hint 之后插入 `push_if_nonempty`。base_inputs 默认空。
- 决策 — 不做 dismiss < 5s（"快速关掉"）信号：原 TODO 写了三档分类，但 dismiss 需要 ChatBubble 加点击事件 + 时间戳记录 + Tauri 命令——是单独前端 iter 的工作量。R1 做最大杠杆的两档（replied / ignored），dismiss <5s 留 R1b。
- 决策 — passive 观测 vs 主动埋点：完全靠 InteractionClock 已有信号推导，不需要 frontend 任何调用。`mark_user_message` 早就存在，没改任何 UX 路径。这是 hidden leverage：合适的 state machine 让"加一层数据流" 变成 0 行前端代码。
- 决策 — `LAST_FEEDBACK_RECORDED_FOR` 用 prev timestamp 做 dedup key：proactive turn 可能因为 proactive loop 跑了多次导致重复 record。timestamp 是天然 unique 的"prev turn 标识"。
- 测试（8 新单测）：
  - format_line 截断 / newline 转义 - 2 个
  - parse_line round-trip + reject 未知 kind / malformed - 2 个
  - format_feedback_hint 空 / replied / ignored / 用最新一条而不是历史 - 4 个
- 测试结果：358 cargo（+8）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：从"宠物开完口不知道用户怎么想" 到"下次开口前 prompt 里告诉它『上次你说 X，用户没回应——这次要么放短要么沉默』"。LLM 自己读这一行后会调整 register。后续 R3-R5 都可以基于 feedback_history 进一步分析（比如"过去 24h ignored 比例 → 降低 chatty 上限"）。

## 2026-05-03 — Iter R2：TR3 review 结果写入 decision_log + 路线规划补全
- 现状缺口：TR3 把 high-risk approve / deny / timeout 写到 app.log，但 panel 的 "recent decisions" view（QG3 时设置好的时间线）只看到 Spoke / Silent / Skip。tool-review 是真正"宠物想做的事被拦下" 决策事件，理应共享同一个 timeline——让用户在一条时间线里看完"宠物今天试图做什么 + 哪些被你拦了"。
- 解法：
  - `tool_review.rs` 加 3 个 `pub const KIND_REVIEW_{APPROVE,DENY,TIMEOUT}` + `pub fn record_review_outcome(&DecisionLog, kind, review_id, tool_name)` —— 单点 push，reason format 固定 `"{review_id} {tool_name}"`，方便未来 panel 解析。
  - `ToolContext` 加 `decision_log: Option<DecisionLogStore>` + `with_decision_log` builder。symmetric with `with_tool_review`。
  - desktop chat / proactive 入口都 attach；telegram / consolidate 留 None（autonomous 路径不写 panel timeline）。
  - chat pipeline TR3 的 4 个 outcome 分支（Approve / Deny / channel-lost / Timeout）每个都加 `if let Some(d) = &ctx.decision_log { record_review_outcome(...) }`。channel-lost 分类为 Deny。
  - 前端 PanelDebug：`kindColor` 加 3 个新 kind（蓝/红/橙），`localizeReason` 加 3 个新分支（中文友好渲染："用户允许了高风险工具调用（tr-1 bash）" / 拒绝 / 60秒未审核）。
- 路线规划（gap analysis 后写入 TODO.md）：
  - **Iter R1**：用户反馈信号采集——区分 dismiss / 回复 / 忽略 60s，写 feedback_history.log，注入 proactive prompt
  - **Iter R3**：late-night wellness nudge 复合规则（0-3 点 + idle < 5min → 强制提醒休息）
  - **Iter R4**：PanelDebug 显示 tool call purpose + risk 历史
  - **Iter R5**：SOUL.md hot reload
  - 这些是"companion-grade 体验补全" 系列——不是新功能，而是把现有信号闭回宠物判断里。
- 决策 — 不在 panel 加 ToolReview 专门 view：决策 timeline 已经 16 容量足够，TR review 是低频事件（每天最多几次），混在主时间线里 user 一眼能看到"今天 8 次决策中有 3 次 review"反而比独立 tab 信息密度高。如果未来 review 频率高到污染时间线再分屏。
- 测试：1 新单测 `record_review_outcome_pushes_decision_with_id_and_tool` —— push 三种 kind + 检查 snapshot 顺序与 reason 格式钉死，guards future panel parser drift。
- 测试结果：350 cargo（+1）；clippy clean；fmt clean；tsc clean。
- 结果：tool-review 现在和 proactive 决策共享一条 timeline。新装用户在 panel 一眼能看到"宠物今天有 12 次主动开口尝试 + 2 次工具被我拦了"，是 companion 行为可解释性的最后一道补丁。
- 同时收掉 Iter Dx：早就由 Cε / Cη / Cθ / Cπ 在 PanelMemory.tsx 里做完了，留 TODO 不必要。

## 2026-05-03 — Iter TR3：高风险工具调用的人工审核 gate（60s 超时默认拒绝）
- 现状缺口：TR1 + TR2 把 purpose 和 risk classification 都做了，但 high-risk 工具（bash / write_file / memory delete）仍按 observe-only 直接执行。TR3 把这道墙立起来：高风险时弹 panel 模态请求 approve / deny；用户 60 秒不响应按 safe default（拒绝）处理；无论结果都把结构化 JSON 返给 LLM 让它能选 safe_alternative 重试。
- 后端 — 新模块 `src/tool_review.rs`：
  - `enum ToolReviewDecision { Approve, Deny }` + `struct PendingToolReview { review_id, tool_name, args_json, purpose, reasons, safe_alternative, timestamp }`
  - `struct ToolReviewRegistry`：`Mutex<HashMap<String, PendingEntry>>` + 单调 id 计数器（"tr-1"、"tr-2"...）。`PendingEntry { sender: oneshot::Sender, snapshot: PendingToolReview }`
  - API：`register(...) -> (id, rx)` / `submit(id, decision) -> Result<(), String>` / `cancel(id)` / `snapshot() -> Vec<PendingToolReview>`（按 timestamp 升序）
  - Tauri commands：`submit_tool_review(review_id, decision)` + `list_pending_tool_reviews()`
  - Helper：`denied_result_json(reason, safe_alt)` / `timeout_result_json(safe_alt)` 都用 `serde_json::json!()` 构造，自动转义引号
- 集成：
  - `ToolContext` 加 `tool_review: Option<ToolReviewRegistryStore>` + `with_tool_review(...)` builder。`from_states` 默认 None；desktop chat / proactive 入口 attach。telegram / consolidate 留 None（自动化路径无 UX surface，跳过 review 直接执行）
  - desktop chat (`commands::chat::chat`) 加 `tool_review: State<...>` 参数，wire 到 ctx
  - proactive `run_proactive_turn` 同样从 app state 拿 registry attach 到 ctx
  - `run_chat_pipeline` 工具循环：在 TR2 assess 之后，如果 `requires_human_review && ctx.tool_review.is_some()`：`reg.register(...)` → `tokio::time::timeout(60s, rx)` → 4 个分支
    - Approve → 继续走原 MCP / registry 执行路径
    - Deny → `denied_result_json` 写入 conv_messages.tool 字段
    - Channel 异常 dropped → `denied_result_json("审核通道异常关闭")` + cancel
    - Timeout → `cancel(id)` + `timeout_result_json` (默认 deny)
  - app.log 写四种状态：parked / approved / denied / timeout — telemetry 完整可追溯
- 前端 — PanelDebug 加模态：
  - `pendingReviews: PendingToolReview[]` 来自 DebugSnapshot.pending_tool_reviews（QG6 已铺好）
  - 1 Hz polling 自动检测新 review，非空 → 模态卡片渲染：tool 名 + 时间戳 + purpose + 风险列表 + safe_alternative + 折叠的 args（避免长 JSON 撑爆模态）+ 允许/拒绝按钮
  - 按钮调 `submit_tool_review` Tauri 命令，乐观地从本地 list 移除（race 时调 `fetchLogs` 复同步）
  - 模态背景 z=2000 高于其他 modal，footer 提醒"60s 超时按默认拒绝"
- 决策 — polling vs Tauri event push：panel 已有 1Hz `get_debug_snapshot` (QG6)。把 pending_tool_reviews 加到 snapshot 是零开销；新加 Tauri event channel = 订阅生命周期管理 + 离开 panel 错过 event 等问题。polling 简单可靠。
- 决策 — `ToolContext.tool_review: Option`：telegram bot 不应该弹 review，因为没有 panel 可点。给 None 让自动化路径直接执行 high-risk。如果未来安全要求收紧（"任何 high-risk 都不许 telegram 调"），加一个 settings flag 即可。
- 决策 — `oneshot` 不是 watch / mpsc：每次 review 一次性接通即可，不需要广播也不需要多消息。oneshot 是 Rust 这种场景的直接表达。
- 决策 — 模态阻塞而非 toast 通知：high-risk 不该被错过；阻塞模态强制用户做决定。如果用户离开 panel 60s 自动 deny，宠物侧不会卡死。
- 测试（9 新单测）：
  - `register_returns_unique_ids_and_snapshot_contains_request`：register 多次 + snapshot reads back
  - `submit_resolves_the_awaited_receiver`：tokio::test，spawn submit Approve，主任务 await rx 拿到决策
  - `submit_unknown_id_returns_error`：兜底
  - `cancel_removes_pending_entry`
  - `snapshot_is_sorted_oldest_first`：sleep 1.1s 改 timestamp 验证排序
  - `denied_result_json_carries_reason_and_alt` + `denied_result_json_handles_quotes_safely`：JSON parseable + 含 "quotes" 安全
  - `timeout_result_json_mentions_window_and_default_alt`
  - `timeout_constant_is_one_minute`
- 测试结果：349 cargo（+9）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：TR1（purpose）→ TR2（risk）→ TR3（review gate）三件套闭环。任何高风险工具调用现在必经"用户在 60 秒内点允许"才能执行；过时即按默认拒绝并让 LLM 看到 safe_alternative 自动 fallback。autonomous 路径（telegram / proactive 早期）通过 None registry 保持 backward compatible。

## 2026-05-03 — Iter TR2：工具调用风险评估（observe-only 模式）
- 现状缺口：TR1 把 purpose 字段铺好了，但所有工具调用仍按"统一允许"处理。bash 任意 shell、write_file 创建/覆盖任意文件、memory_edit delete 不可恢复——都和 read_file / get_weather 一个待遇。审计 + 未来人工审核需要分级。
- 解法 — 新模块 `src/tool_risk.rs`：
  - `enum ToolRiskLevel { Low, Medium, High }`：三档够用，更细的分级反而难映射到 UI / 审批流。
  - `struct ToolRiskAssessment { risk_level, reasons: Vec<String>, requires_human_review: bool, safe_alternative: Option<String> }`：与 TODO 描述的 4 字段对齐。
  - `pub fn assess_tool_risk(tool_name, args_json, _purpose) -> ToolRiskAssessment`：纯函数，按工具名 + （memory_edit 时）args 中 action 字段分类。purpose 作参数保留但本 iter 不用——TR2/TR3 follow-up 可以基于 purpose 内容做语义匹配。
  - `pub fn format_assessment_log(name, &assessment) -> String`：单行紧凑格式，写 app.log。
- 分类策略：
  - **High** + requires_human_review=true：bash（任意 shell），write_file（覆盖文件），memory_edit delete（不可恢复）
  - **Medium**：edit_file（受 old_string 唯一性约束的局部修改），memory_edit create/update（写记忆但可逆），未识别 / MCP 工具兜底
  - **Low**：read_file，get_active_window / weather / upcoming_events / check_shell_status，memory_list / search / get
  - 每种 high-risk 都附 `safe_alternative` 中文提示（bash → 用专用工具、write_file → 用 edit_file、memory delete → 改 update 标记废弃）
- 集成 — chat pipeline 中 purpose 提取通过后立即 assess + log：每条 tool call 都会写 `Tool risk [name]: high|medium|low; reasons=[...]; review=true|false; alt=...` 到 app.log。**execution 不阻塞**——这是 observe-only 阶段。TR3 只需翻 `requires_human_review` 那个开关即可正式 gate。
- 决策 — observe-only 而非立即 block：直接 block 会让 bash / write_file / memory_edit delete 在 TR3 落地前全部失效——破坏宠物正常运作。observe-only 让我们先 audit：实际跑几天后看 app.log 里高风险占比、误判（read_file 但用户其实希望它打码）等等，再设计 gate 的具体行为。这是 security 工作里"shadow deploy" 模式。
- 决策 — 新模块 vs 塞进 chat.rs：tool_risk 是独立 concept，不应耦合 chat pipeline；放入 `src/tool_risk.rs` 让未来 TR3 / 未来策略 driver 都可单独进化。同时 chat pipeline 只调 2 个 pub fn，依赖关系干净。
- 决策 — `_purpose` 参数保留但目前不使用：签名稳定让 TR2/TR3 follow-up（"如果 purpose 含敏感词如 'rm' 就降级 risk"）不动调用方。也表态: purpose 是 audit 输入的一部分，将来一定会用到。
- 测试（12 新单测）：覆盖 8 个工具名各自分类 + memory_edit 4 个 action 分支（create/update/delete/未知）+ memory_edit malformed args + format_assessment_log 两个边界（含 alt / 不含 alt + 空 reasons）。
- 测试结果：340 cargo（+12）；clippy clean；fmt clean；tsc clean。
- 结果：每个工具调用现在都有 risk classification 写入 app.log。TR3 接手时只要在"requires_human_review=true 时 await 用户输入 vs 直接执行"加一个分支即可——分类逻辑、log 形态、reasoning 都已就位。

## 2026-05-03 — Iter TR1：工具调用必须带 purpose 字段（pipeline-level gate）
- 现状缺口：所有工具调用都是黑盒触发——LLM 可能突然调 read_file、memory_edit、shell、weather，但开发者只能在 app.log 看到工具名 + 原始 args。**为什么** 这次 turn 调了这个工具？没有显式记录。这影响 (a) 调试（"哪条 prompt 让它调 weather？"）；(b) 安全审计（高风险工具下未来需要审核流程，没有 purpose 字段就无法做出 risk decision）；(c) 模型自我约束（要求模型每次声明 purpose 会让它的工具调用更"想清楚"再发）。
- 解法 — pipeline-level enforcement：
  - 新 pure helper `extract_tool_purpose(args_json) -> Option<String>`：JSON 解析 + 取 `purpose` 字段 + trim + 非空检查。所有失败路径返 None（缺字段 / 空串 / 非字符串 / 不可解析 JSON），无 panic。
  - 新 helper `missing_purpose_error_result() -> String`：合法 JSON，含 error + hint 字段，hint 中文教 LLM 在 arguments 里加 purpose 重试。
  - `run_chat_pipeline` 工具循环改造：每个 tool call 在 send_tool_start 后先 `extract_tool_purpose`，命中 None 直接 return synthetic error result（**不执行工具**），命中 Some(p) 才走原 MCP / registry 路径。app.log 写"Tool call: {name}({args}) purpose=\"{p}\""或"Tool call rejected (missing purpose): {name}({args})"。
  - `TOOL_USAGE_PROMPT` 加"工具调用必须带 purpose"段，含两个具体例子（read_file / memory_edit）+ "强制" 关键词，让 LLM 第一次 turn 就遵守而不是挨过 8-round 上限。
- 决策 — pipeline 而不是 per-tool schema 强制：tools 的 JSON Schema 加 required: ["purpose"] 也能挡住缺失，但 (a) 要改每个工具定义；(b) Schema 校验失败的错误信息是 framework 风格，不如 pipeline-level 自定义 error 友好；(c) MCP 工具 schema 是远端定义的，不能本地改。pipeline gate 是单一控制点，覆盖 built-in 和 MCP。
- 决策 — recoverable error 而不是 hard block：missing purpose 不抛错给 caller，而是把 synthetic JSON error 当 tool result 喂给 LLM。LLM 下一轮看到 hint 后重试。配合 QG2 的 MAX_TOOL_CALL_ROUNDS=8，最坏情况就是 LLM 撞上限被 abort，不会无限循环。
- 决策 — purpose 字段对所有 tool 生效，不分类：考虑过"只对高风险工具（write_file / shell）要 purpose"，但分类本身就是 TR2 的事；TR1 先建立"每个调用都要解释自己"的协议基础，TR2 再在此基础上加 risk_level + 可能跳过 low-risk 的 purpose 要求。
- 风险与缓解：协议变化可能让模型第一轮 tool call 全 reject。缓解：(a) TOOL_USAGE_PROMPT 已用"强制 / 必须"措辞 + 两个 args 完整示例；(b) reject 的 error 是合法 JSON + hint 中文，LLM 看后基本必能补；(c) MAX_TOOL_CALL_ROUNDS=8 兜底防失控；(d) tool args 没有 deny_unknown_fields，purpose 不影响任何工具实现，只是元数据。
- 测试（8 新单测）：
  - `extract_tool_purpose`：valid one-liner / 空白 trim / 缺字段 / blank string / whitespace-only / 非字符串（数字/null/bool/object）/ 不可解析 JSON / 空字符串 input — 6 个边界全覆盖。
  - `missing_purpose_error_result_carries_retry_hint`：parseable JSON + 含 error + 含 purpose + 含"重新调用"。
  - `tool_usage_prompt_teaches_purpose_protocol`：钉住 prompt 含"purpose" + "强制 / 必须" 关键词。
- 测试结果：328 cargo（+8）；clippy clean；fmt clean；tsc clean。
- 后续 — 留 follow-up：前端 ToolCallBlock / PanelDebug 显示 purpose（TR1 的 UX 部分）。本 iter 重点是建立协议 + audit trail；UI 渲染是分立工程，等 TR2/TR3 一起做更经济。
- 结果：从"工具调用是黑盒，看 app.log 只知道做了什么不知道为什么" 到"每次 tool call 自带一句话 purpose，写入 app.log，缺失即拦截 + 可恢复"。为 TR2（risk assessment）+ TR3（人工审核 gate）打地基。

## 2026-05-03 — Iter QG6：PanelDebug IPC 收敛——15 invokes/秒 → 1 invoke/秒
- 现状缺口：`PanelDebug.tsx` `fetchLogs` 每秒 fire 15 个独立 Tauri invoke (get_logs / get_cache_stats / get_proactive_decisions / get_mood_tag_stats / get_recent_speeches / get_tone_snapshot / get_pending_reminders / get_lifetime/today/week_speech_count / get_llm_outcome_stats / get_env_tool_stats / get_prompt_tilt_stats / get_companionship_days / get_redaction_stats)。每个 invoke 一次完整 IPC 往返（serialize → bridge → deserialize），15× per second 是真实的 CPU 与电池负担——尤其用户 panel 长时间打开的 case。
- 解法：后端加 `DebugSnapshot` 聚合结构 + `get_debug_snapshot` 单一 Tauri 命令，前端 fetchLogs 收敛成一次 invoke。15 个旧命令保留兼容（PanelPersona 等其他调用方靠它们）。
- 重构 — 抽 `from_counters` 共享：每个 stat 结构体（CacheStats / MoodTagStats / LlmOutcomeStats / EnvToolStats / PromptTiltStats）加 `pub fn from_counters(&ProcessCounters) -> Self` 共享读路径。Tauri 命令瘦成一行 `Stats::from_counters(counters.inner())`，聚合命令直接调用同一个 from_counters 拿数据。这样 future 加新 counter 字段不会让两个读路径漂移。
- 重构 — 抽 `build_tone_snapshot`：`get_tone_snapshot` body (~150 行) 提到自由函数 `pub async fn build_tone_snapshot(&InteractionClock, &WakeDetector, &ProcessCounters) -> Result<ToneSnapshot, String>`。Tauri 命令变 1 行 `build_tone_snapshot(clock.inner(), wake.inner(), counters.inner()).await`；聚合命令也调它。`State<'_, Arc<X>>::inner() -> &Arc<X>` 通过 Deref 自动转 `&X`，签名干净。
- DebugSnapshot 结构 15 字段：logs / cache_stats / decisions / mood_tag_stats / recent_speeches / tone / reminders / lifetime/today/week_speech_count / llm_outcome_stats / env_tool_stats / prompt_tilt_stats / companionship_days / redaction_stats。前端类型 inline 在 invoke<>() 里，不污染 panelTypes.ts（这是 hot-path 单个用法）。
- 决策 — 不删旧 Tauri 命令：(a) PanelPersona 仍调 `get_companionship_days`；(b) 删除 = 翻动 lib.rs handler list + 风险面扩大；(c) 保留它们是几行死代码而已，binary 大小可忽略。如果未来发现旧命令完全没人调，可以做"清理无用 Tauri 命令" 单独 iter。
- 决策 — 不让前端 panelTypes 暴露 DebugSnapshot：聚合类型仅在 PanelDebug.tsx hot path 用一次。把它推到 panelTypes 反而引入"专门为聚合 IPC 而存在的类型"，不必要。inline anonymous struct in invoke generic 干净直接。
- 测试 — 1 新 unit test `from_counters_round_trips_each_stat_struct`：把 5 个 counter 组每个字段 bump 成 1..19 distinct 值，5 个 from_counters 全 snapshot 后断言每个字段 readback 相符。这把 from_counters wiring 风险压到最低——任何"加字段忘了把它接进 from_counters" 的 PR 都会立刻让这测试 fail。
- 测试结果：320 cargo（+1）；clippy clean；fmt clean；tsc clean。
- 结果：PanelDebug 的 IPC 频率 14× 降。`from_counters` + `build_tone_snapshot` 把"读 stat" 与"暴露 stat" 分层，未来再加 counter 类不需要复制粘贴 readout 代码。

## 2026-05-03 — Iter QG4：补齐 prompt 重注入路径的 redaction
- 现状审计 — 发现 3 个未走 `redact_with_settings` 的重注入点，全部在 proactive prompt 构建路径上：
  1. `mood_hint` (run_proactive_turn 内嵌, 旧 line 1750)：直接 `format!(text.trim())`。chat.rs `inject_mood_note` 早就 redact 了，proactive 一直漏。
  2. `build_reminders_hint` (line ~2056)：`format!("· {} {}（条目标题: {}）", when, topic, item.title)` —— topic（用户口述）+ title（LLM 取的）双双裸出。
  3. `build_plan_hint` (line ~2500)：`format!("...\n{}", item.description.trim())` —— LLM 自己写的 daily_plan，即使是它写的也不能保证已过滤（reactive 时 LLM 可能直接吸收用户原话）。
  其他点验证通过：`inject_mood_note` (chat.rs)、`build_persona_hint`、`build_user_profile_hint`、`build_butler_tasks_hint`、speech_hint、mood_trend（不含用户文本）都已 redact 或天然安全。`get_pending_reminders` 是 panel 用户面命令——故意不 redact（用户看自己的内容不需要打码）。
- 解法 — 三个 builder 都拆出"pure formatter + closure-based redact 注入"模式：
  - `pub fn format_reminders_hint(items, redact: &dyn Fn(&str) -> String)` —— `items: &[(time, topic, title)]`，每条 topic + title 都过 redact 后再格式化；空列表返空串。
  - `pub fn format_plan_hint(description, redact)` —— trim 后过 redact 再 wrap header。
  - `pub fn format_proactive_mood_hint(text, redact)` —— 复用 inline 逻辑，empty 走 first-time placeholder。
  - 三个 thin async/sync wrapper（`build_reminders_hint`/`build_plan_hint`/`run_proactive_turn` 内 mood_hint 计算）调用 pure formatter，传入 `&|s| redaction::redact_with_settings(s)`。
- 决策 — closure 而不是 `patterns: &[String]`：closure 让 wrapper 用 `redact_with_settings`（含子串 + regex 两层），同时让测试用 substring-only 注入 + `redaction::redact_text` 做断言。如果直接传 patterns 就要选一种（substr 或 regex），丢覆盖。closure 是 +1 类型参数换 100% 灵活。
- 决策 — 不动 `inject_mood_note` / `build_persona_hint` / `build_user_profile_hint` / `build_butler_tasks_hint`：审计后确认它们已经 redact 过，且当前测试已经覆盖。这次 iter 严格只补漏，不改动已对齐的路径——避免大 PR、避免误伤。
- 测试 — 6 个新 unit test（`prompt_tests` 模块）：
  - `test_redactor(&[&str])` 辅助：用 `redact_text` 模拟一个固定子串 redact 闭包，不碰 settings 状态。
  - `format_reminders_hint_redacts_topic_and_title`：钉住 topic + title 都 redact，原文不残留。
  - `format_reminders_hint_empty_returns_empty_string`：空列表无 header。
  - `format_plan_hint_redacts_description`：description 子串 redact。
  - `format_plan_hint_empty_or_whitespace_returns_empty`：空 / 空白返空。
  - `format_proactive_mood_hint_redacts_text`：mood text 子串 redact。
  - `format_proactive_mood_hint_empty_returns_first_time_message`：empty path 用 first-time 占位文本。
- 测试结果：319 cargo（+6）；clippy --all-targets clean；fmt clean；tsc clean。
- 结果：proactive prompt 的 7 个内容来源（mood / reminders / plan / persona / user_profile / butler_tasks / speech）全部走 redact_with_settings。隐私 filter 不再有"loop fire 时漏一手"的盲区。

## 2026-05-03 — Iter QG3：统一手动 trigger 与后台 loop 的 proactive telemetry
- 现状缺口：panel 上点 "立即触发" 走 `trigger_proactive_turn`，结果 outcome 不进 llm_outcome 计数、env_tool 不 record_spoke、decision_log 不 push。这意味着开发者用 panel 手动触发开口的所有数据都"消失"——既看不到 panel 的 "LLM 沉默率"，"环境感知率" 也漏统计，decision log 不显示。完全形成观察盲区。
- 解法：抽出 `record_proactive_outcome` 共享 helper（pure-ish，touches atomics + decisions log）放到 proactive.rs 中游：
  - 三件事并行：(a) `counters.llm_outcome.{spoke,silent,error}` 原子 +1；(b) Spoke 路径调用 `counters.env_tool.record_spoke(&tools)`；(c) `decisions.push(kind, reason)` 用统一 reason builder
  - 新参数 `source: &str`（"loop" / "manual"）以 `source=X` tag 形式埋进 reason，panel 一眼能区分
  - 同时把 loop 内嵌的 `append_tag` 提到 module-level `pub fn append_outcome_tag`，loop 和 helper 都用
  - prompt_tilt 故意不做：tilt 依赖 active_labels 计算，loop 才有这套。manual 不计入 tilt 反而保留 tilt 统计的纯净性（详见 IDEA.md 的设计要点）
- 重构：loop 原 ~50 行的 inline match 现在缩成 7 行 `record_proactive_outcome(...)` 调用 + Err log。trigger_proactive_turn 加 ~25 行：fresh chatty sample + counters/decisions clone + helper 调用，并改为 `await` 后保留 Result（不 unwrap），让错误也能记入 telemetry 再 propagate。
- 决策 — manual 的 rules_tag 选 None：gates 被 manual 旁路了，没有"实际触发的规则集"概念。如果硬塞 active_labels 算出 fake tag，反而误导 panel 用户以为 manual 触发也走完了 gate 评估。明确 None + source=manual 让语义清晰。
- 决策 — 沿用既有 ProcessCounters 而非新增 store：所有 counter 在 `ProcessCounters` 里，clone Arc 即可在 helper 间传递，不动 Tauri State 拓扑。
- 测试（4 新单测）：
  - `append_outcome_tag_handles_empty_and_dash_and_chained`：钉住 reason builder 三种边界
  - `record_proactive_outcome_spoke_path_bumps_counters_and_logs_source`：Spoke 路径 spoke +1, env_tool spoke_total +1, decisions[0] 含 source=manual + tools=...
  - `record_proactive_outcome_silent_path_bumps_silent_and_tags_loop`：silent +1, env_tool 必须为 0（不污染分母），decisions reason 含 source=loop + rules + chatty
  - `record_proactive_outcome_error_path_bumps_error_and_includes_message`：error +1, decisions reason 含原始错误 + source=manual
- 测试结果：313 cargo（+4）；clippy clean；fmt clean；tsc clean。
- 结果：手动 trigger 现在和 loop 走同一套 telemetry。panel 任何 chip / counter / decision-log 视图都不再因"用户走 panel 触发了几次"出现盲区。research workflow（手动 fire 验证 prompt 改动）也终于会被统计。

## 2026-05-03 — Iter QG2：LLM tool-call loop 加最大轮数 + 明确失败路径
- 现状缺口：`run_chat_pipeline` 注释自己写"unlimited rounds"。如果模型陷入循环（反复读同一个 memory 类，或调用 → 工具 error → 再调用 → 同样 error），就会无限消耗 token + API 配额，最坏可能挂住 turn 直到外部超时。同时缺乏对外的可解释错误：当模型坏掉时用户只能看到 chat panel 没反应。
- 解法（最小可观测集）：
  - 新 `pub const MAX_TOOL_CALL_ROUNDS: usize = 8`（典型 1-3 轮收敛，5 轮以内是 tool-heavy 任务，>=8 几乎必定坏掉）
  - 编译时 sanity bound：`const _: () = assert!(MAX_TOOL_CALL_ROUNDS >= 4 && <= 32)` —— 防止未来"调到 1000"的 PR 偷偷漏过
  - 两个纯 helper：
    - `tool_call_limit_message(rounds, max) -> String`：中文用户可读 + 包含 round 计数和 max 便于 debug
    - `enforce_tool_round_limit(round, max) -> Option<String>`：>= max 返 Some(message)，否则 None。pure 因为这层逻辑必须独立于 HTTP 实测可测
  - run_chat_pipeline loop 头部加 gate：返 Some 时三件事并行 — `ctx.log` 写 ERROR 到 app.log + `sink.send_error` 推前端 stream + `return Err(msg)` 给 caller
- 测试：
  - `enforce_tool_round_limit_passes_under_max` (round=0, 7 都返 None)
  - `enforce_tool_round_limit_aborts_at_or_over_max` (round=8 / 99 都返 Some 且消息含 round 数)
  - `tool_call_limit_message_is_user_meaningful`：钉住消息含「工具调用循环」+「已中止」+ 数字
- 决策：选 const 而不 settings.max_tool_call_rounds —— 8 是个工程合理上限，普通用户不需要调；如果未来有需求再把 const 升级成 settings 字段。少一个 settings 项 = 少一个忘了重启就不生效的坑。
- 决策：不做完整 HTTP mock 集成测试 —— pure helper 已覆盖 limit 逻辑，loop 控制流靠 inspection 验证。HTTP mock 会让 PR 翻倍长但只多 cover "确实没多发一次 LLM 请求" 这点，回报率低。
- 测试结果：309 cargo tests（+3）；clippy --all-targets clean（const_assert 通过 const _ pattern 而不是 assert!，避免 assertions_on_constants 警告）；fmt clean；tsc clean。
- 结果：从"unlimited rounds 注释 + 实际无限循环可能"到"硬上限 8 + 用户可读错误 + app.log + frontend stream 全路径告警"。坏掉的 turn 现在 fail-loud 而不是 silent burn。

## 2026-05-03 — Iter QG1：清理 Rust 格式与 lint（开启 Quality Gate 系列）
- 现状缺口：项目快速堆功能阶段，`cargo fmt --check` 输出 ~2100 行 diff、`cargo clippy --all-targets -- -D warnings` 报 13 个 error。alpha 阶段累计的格式 / 习语债务，CI 接入和未来 PR review 都需要清零。
- 解法分两步走，纯机械整理 + 局部惯用法升级，零业务行为变化：
  - `cargo fmt` 全量重排（多处长签名换行、长 `send(...)` 调用拆字段、Rust struct literal 缩进）
  - 13 个 clippy error 逐项修：
    - `collapsible_str_replace` ×4：butler_history.rs / mood_history.rs / speech_history.rs 的 `.replace('\n', " ").replace('\r', " ")` 收敛为 `.replace(['\n', '\r'], " ")`
    - `unnecessary_map_or` ×2：commands/shell.rs `.map_or(false, |f| f < cutoff)` → `.is_some_and(|f| f < cutoff)`；tools/file_tools.rs 同理
    - `manual_pattern_char_comparison`：input_idle.rs `|c: char| c == ' ' || c == '='` → `[' ', '=']`
    - `filter_next` (rfind)：mood_history.rs `.filter(|l| !l.is_empty()).next_back()` → `.rfind(|l| !l.is_empty())`
    - `doc_lazy_continuation` ×3：proactive.rs 两处 doc-list 后的总结句缺空行，加分隔空行
    - `too_many_arguments` ×2：commands/chat.rs `chat()` 是 Tauri command（`State<'_, ...>` DI 必须每个独立 param）；commands/debug.rs `write_llm_log()` 是日志 helper，每个 timing 字段独立采集；两处都加 `#[allow(clippy::too_many_arguments)]` 并附说明 comment，不强行结构化打包，避免噪音重构
- 决策：选 allow 而不重构 too_many_arguments：(a) Tauri DI 不能合并；(b) write_llm_log 调用点只有一处但参数从 5 个不同上游来源取值，打包成 struct 反而把 plumbing 推到上游；(c) 这是"标记可接受"而非"逃避"，clippy 也鼓励这种局部 allow。
- 未做：QG2-QG6 + Tool Review 1-3 留待后续迭代。本次只动质量基线最低层（fmt + lint），保证后续 PR diff 干净。
- 测试：`cargo clippy --all-targets -- -D warnings` clean；`cargo fmt --check` clean；306 cargo 测试全过；`tsc --noEmit` 干净。
- 结果：质量基线 from 13 错误 + 2100 行未格式化 → zero。后续可以把 fmt + clippy 加进 CI/pre-commit gate。

## 2026-05-03 — Iter F1：桌面 bubble 60s 自动消失（开启 F series 用户体验向）
- 现状缺口：宠物的桌面气泡只要存在 lastAssistantMsg 就一直显示。早上 proactive 说"早安"——这条 bubble 挂屏幕直到下次说话（可能下午 5 点）。screen clutter + 看着 stuck。
- 解法：App.tsx 加 `bubbleDismissed: bool` state + useEffect timer：
  - 当 displayMessage 非空 + showBubble true + 非 isLoading 时，启动 60s 定时器
  - 定时器到 → setBubbleDismissed(true) → ChatBubble 渲染 visible=false
  - 新消息（displayMessage 变化）会重新触发 useEffect → 重置 dismissed=false → 新 60s 计时
  - cleanup return clearTimeout 防 memory leak
- 60s 选择：阅读一条句子 + 几秒思考充足；不会因为太短让 user 错过；不会因为太长继续 cluttering。如果未来要更精细控制再 hoist 到 settings。
- 不区分 reactive vs proactive：reactive 也 60s 消失合理——ChatPanel 完整聊天历史一直可见，bubble 是临时通知。
- isLoading 期间不计时：流式回复期间 bubble 持续显示直到生成完，再开始 60s 倒计时。这是 reactive 路径的正确顺序。
- 不动 ChatBubble 组件本身：visibility prop 即可，state 上提到 App.tsx 让"桌面屏幕清理"逻辑在统一处。
- 测试：tsc 干净；306 cargo 不变（纯前端改动）。
- 结果：proactive 早上说"早安"→ 60s 消失；用户没看到？打开 Panel Chat tab 完整记录在那。从"屏幕被一句话占着" 到"通知一闪而过即可"。

## 2026-05-03 — Iter E4：prompt-preview ring buffer of last 5 turns + 导航
- 现状缺口：E1/E2/E3 只看 last 1 turn。研发改 prompt 后想"看这次 vs 上次差在哪"得在两个 trigger 之间手动记下 — modal 不能比较。
- 解法：环形缓冲 last 5 turns，panel modal 加 prev/next 按钮：
  - `LAST_PROACTIVE_TURNS: Mutex<VecDeque<TurnRecord>>`，cap = `PROACTIVE_TURN_HISTORY_CAP (5)`，每次 turn 完成后 push_back + 超出 pop_front
  - `TurnRecord` struct: timestamp / prompt / reply / tools_used
  - 新 Tauri command `get_recent_proactive_turns() -> Vec<TurnRecord>`：返 reverse 后 newest-first，方便 panel index 0 = 最新
  - `tools_dedup` 提到一个变量重用：原 BTreeSet collect 写一份 stash 到 LAST_PROACTIVE_TOOLS（保 E3 兼容），再 clone 一份进新 ring buffer 的 TurnRecord
  - PanelDebug：modal 头部加 « / » 按钮 + "1/N" 索引，状态从 `lastPrompt/lastReply/lastTurnMeta` 三 useState 收敛为单 `recentTurns: TurnRecord[]` + `turnIndex`，currentTurn 派生 prompt/reply/meta，UI 渲染不变
  - 按钮 disabled 状态视觉变浅、cursor 显 default；tooltip 解释 ring buffer 容量
- 5 cap 选择：modal 横向放 navigator + count + 字符统计 + ⏱ + 🔧 + copy msg + ✕，单行已经满；5 turns × 一段 prompt 也避免 process 内存膨胀。如果用户需要更长历史走 logs。
- 既有命令保留：get_last_proactive_prompt / reply / meta 仍工作（读各自 mutex），向后兼容。E4 给 panel 用新统一 API。
- 测试：306 cargo 不变（数据透传，无新 logic）；tsc 干净。
- 用例：研发改 prompt 规则 → 立即开口三次 → 看上次 prompt → « « 翻三个 turn 比较 prompt 文本和 reply 行为是否符合预期。从"trigger → 记下 → 再 trigger → 记下 → 比较" 到"trigger 几次 → 翻历史"。

## 2026-05-03 — Iter E3：prompt-preview modal 加 timestamp 和 tools_used 元数据
- 现状缺口：E1+E2 后 modal 显完整 prompt + reply，但缺两个关键 meta：(a) 这一对是哪个时刻的？(b) LLM 这一轮调了什么工具？没 timestamp 的话，user 看到 modal 内容不知道是 30 秒前还是 30 分钟前的；没 tools_used 的话，prompt 里 env-awareness rule 是否真的让 LLM 调 active_window 没有直接答案。
- 解法（再次复用 E series static Mutex 模板）：
  - 加两个新 stash：`LAST_PROACTIVE_TIMESTAMP: Mutex<Option<String>>` + `LAST_PROACTIVE_TOOLS: Mutex<Vec<String>>`
  - timestamp 在 prompt build 后立刻 set（与 prompt clone 同位置）；用 `now_local.format("%Y-%m-%d %H:%M:%S")` 给 user 友好的本地时间
  - tools_used 在 LLM 调完之后 set，去重：BTreeSet collect → 排序好 + 唯一。原 `tools` Vec 可能含同名重复（多次 call），UI 不需要那种粒度
  - 新 Tauri command `get_last_proactive_meta() -> ProactiveTurnMeta { timestamp, tools_used }`：一次拉两个字段，避免 panel 三次 IPC
  - Modal 头部加两个 inline pills：⏱ timestamp（slate 等宽）+ 🔧 tools (cyan 加粗，工具名以 ` · ` 连接)
  - `Promise.all` 升级为三件并行（prompt + reply + meta）
- 不动 E1/E2 已有命令——保持向后兼容；新 meta 命令是 additive。
- 测试：306 cargo 不变；tsc 干净。
- 用例：研发改了 env-awareness 规则 → 立即开口 → 看上次 prompt → modal header 立刻看到 `🔧 get_active_window · memory_edit`，确认 LLM 真的去看了环境。验证 prompt → 行为 链路在一个 round 闭合。
- E3 收尾 modal 形态：prompt + reply + timestamp + tools 四件。E series 工具向已经覆盖"看现在 prompt 的 in/out 是什么、什么时候、调了什么"。

## 2026-05-03 — Iter E2：modal 同时显示 LLM reply + 复制按钮 — 全 in/out 可见
- 现状缺口：E1 的 modal 只显 prompt（input）。但调试 / 调优经常要的是 "看 prompt + 看 LLM 输出"——一个 chat round 完整双向。E1 后用户得开 logs 找 reply。
- 解法：同 E1 模板镜像加 reply：
  - `LAST_PROACTIVE_REPLY: std::sync::Mutex<Option<String>>` static
  - `run_proactive_turn` 在 `let reply = run_chat_pipeline(...)` 之后 stash clone
  - Tauri command `get_last_proactive_reply()` 同形态返 String
  - PanelDebug `看上次 prompt` 按钮的 onClick 改为 `Promise.all` 并发拉 prompt + reply
  - modal body 重写为两段：⇢ PROMPT (灰底) + ⇠ REPLY (浅绿底，#f0fdf4)，每段头部显 "复制" 按钮
  - 复制按钮调 `navigator.clipboard.writeText` + 2.5s 自动消失的 "已复制" 状态消息（在 modal 顶部）
  - 两段都用 pre + whitespace pre-wrap 保留段落
- modal 标题从 "上次 proactive prompt" 改为 "上次 proactive 的 prompt + reply"，character 计数 改为 "prompt N / reply M chars"
- 复制后状态消息青色 (`#0d9488`)，错误红色，跟项目其他 toast 配色一致。
- 不持久化 reply（process 重启清空，和 E1 同思路）。
- 测试：306 cargo 不变；tsc 干净。
- 用例链路：研发想验证某个 prompt 改动 → 立即开口 → 看上次 prompt → 看到 prompt 和 reply 全文 → 复制 prompt 到外部 LLM 工具 dry-run → 复制 reply 验证 prompt 改动效果。从"翻 logs 拼信息"到"一键看完整 in/out"。

## 2026-05-03 — Iter E1：proactive prompt 全文 panel 可看（开启 E series 工具向）
- 现状缺口：D series 12 个 chip 把"现在 LLM 看到什么 ambient 信号"分维度可视化了，但**完整拼装好的 system prompt 全文**仍然不可见。要确认"今天 chatty rule 真的进 prompt 了吗"得去 panel logs 或 LLM 端 trace——多步、低效。研发自己 prompt 调优时尤其卡。
- 解法：捕获最后一次构造的 proactive prompt，panel 上一键预览。
  - `src-tauri/src/proactive.rs`：加 `pub static LAST_PROACTIVE_PROMPT: std::sync::Mutex<Option<String>>`（process 内）+ `run_proactive_turn` 在 `build_proactive_prompt(...)` 之后立刻 stash clone 到该 Mutex。process 重启后清空——不需要持久化"上次"，因为关心的是当下行为。
  - 新 Tauri command `get_last_proactive_prompt() -> String`：读 Mutex，None 时返空字符串。
  - `PanelDebug.tsx`：toolbar 加靛紫色 `看上次 prompt` 按钮（紧邻 `立即开口`）。点击 invoke + show modal：
    - 全屏遮罩（rgba(0,0,0,0.4)）+ 居中卡片（max 780px width / 80vh height）
    - 头部显字符长度 + 关闭按钮
    - body 用 `<pre>` + `whiteSpace: pre-wrap`：保留 prompt 内的 `\n` 段落、长行 wrap 不溢出
    - 空 prompt 提示"还没触发过——按 立即开口 试一次"
- D series 是把 prompt 的"输入信号"维度拆开可视化；E series 第一刀是把"装配后的 prompt 全文"直接暴露。两者层级不同 — D 是 "decompose"，E 是 "as-is"。研发场景两者各擅胜场。
- 不持久化（process 重启清空）：last prompt 是 transient 调试信息，写盘没意义；用户重启 app 通常意图就是 "重置状态"。
- 测试：306 cargo 不变（透传无新逻辑）；tsc 干净。
- 结果：研发可以一键看 LLM 实际看到的 prompt 全文 — 验证 D series 的 chip 显示是否和 prompt 内容一致、调 prompt 时 paste 进 LLM 跑 dry run、debug "为什么 LLM 选这个 register" 类问题。从面板看到决策的 raw input 全文。

## 2026-05-03 — Iter D12：surface "proactive 已关" 状态 — 关闭 disabled gate 可见性
- 现状缺口：D10 + D11 完成 awaiting / cooldown 可视化和修复，但还有一个 gate 没暴露：disabled 自身。当 `settings.proactive.enabled = false` 时，整个 proactive 引擎 silent—不会有 Silent / Skip / Run 的 decision_log 条目，所有 chip 都按字面状态显示，但 gate 永不放行。结果：用户关了开关后忘记，半天没听到宠物说话以为 bug。这是 7 个 gate 中最后一个无 panel 提示的。
- 解法（D series 模板复用第十二次）：
  - `ToneSnapshot.proactive_enabled: bool` —— 直接读 settings.proactive.enabled
  - `PanelToneStrip` 渲染深灰底白字 🔕 proactive 已关 chip，圆角 10px 风格化为"配置告警"，置于 strip 首位最显眼
  - tooltip 解释"主动开口循环不会触发"+ "其它 chip 仍按现状显示，只是 gate 不会放行"——避免用户对着其它 chip 困惑
  - settings 读失败 fallback 到 enabled=true（不假告警）
- 这是 D series 第 12 个、可能是最后一个 gate 类 iter。所有 7 个 proactive gate 现在 panel 都有显式或隐式信号：
  - 1. disabled — 🔕 (D12) ← 本 iter
  - 2. awaiting — 💭 (D10) + 4h 自动过期 (D11)
  - 3. cooldown — ⏳ (D9)
  - 4. quiet hours — 😴 (D4) + 🌙 pre-quiet
  - 5. focus — 🎯 (D3)
  - 6. idle threshold — 数字 in time line ⏱
  - 7. input-idle — 数字 in time line ⏱
- 测试：306 cargo 不变（纯透传，已被 enabled gate 测过）；tsc 干净。
- 结果：用户在 panel 一眼看出 "宠物现在为什么不说话"——不论是因为开关关、cooldown、focus、quiet 还是 awaiting，都有对应 chip。从黑盒到全透明。

## 2026-05-03 — Iter D11：awaiting gate auto-expire 4h（修复"宠物永久 muted"潜在 bug）
- 现状缺口（实际是个潜伏行为 bug）：D10 surfaced awaiting gate 后审视发现：mark_user_message 是**唯一**清除 awaiting_user_reply 的入口。如果用户在宠物刚说完话后没回应、关 laptop 走人、几小时甚至几天后回来——开机时 awaiting 还是 true，宠物会一直 skip 所有 proactive 评估。和 cooldown 的 wake_soft 不同（Iter 5 已经 soft 化），awaiting 没有任何时间维度的释放机制。"我以为它打不打理我了" 的体验源头之一。
- 解法：在 `InteractionClock::snapshot` 加 `effective_awaiting(raw, since_proactive)` 纯函数判断：raw=true AND `since_last_proactive < AWAITING_AUTO_CLEAR_SECONDS (4h)` 才返 true。否则视作"过期了，原'别 double up'语义早不适用"。
  - ClockInner 的 raw 状态不变（只 `mark_user_message` 才能权威清空），保持事件驱动语义。
  - snapshot 返回 effective 值 — 同一份真实状态被 panel chip 和 gate check 一起读到，永不漂移。
  - panel 的 💭 等回应 chip（D10）现在也会自动消失，gate 也自动放过，行为统一。
- AWAITING_AUTO_CLEAR_SECONDS = 4 小时：
  - 比 short break（午饭+会议）长，足以保留正常 polite-wait 体验
  - 比单日工作长足够的 buffer——绝大部分 lunch / meeting / focus 时段一两小时内
  - 4h 后 awaiting 还在 → 用户大概率离开了 desk + 没回，pet 可以重新评估了
- 4 个新单测覆盖：raw=false 永远 false / raw=true + recent 仍 true / raw=true + threshold 边界 / since=None defensive case。测试总数 302 → 306。
- 这是 D series 里第一个**真正改变行为**的 iter（不只是 surface state）。原来是潜伏 bug：长别后 pet 永远静默；现在是设计良好行为：长别后 pet 自动重新评估。
- 与 wake_soft（cooldown 的 soft 机制）相辅相成：cooldown 由 wake-from-sleep event 软化，awaiting 由时间长度软化。两个 gate 的"长别豁免"现在都覆盖。

## 2026-05-03 — Iter D10：ToneSnapshot 暴露 awaiting_user_reply + 💭 chip
- 现状缺口：D9 surfaced cooldown gate；但 awaiting gate（Iter 5 的另一个）一直没暴露。"宠物刚开过口但你还没回 → gate 让宠物先等等" 这种 polite-wait 状态对用户完全不可见——他们只感觉"宠物冷淡了"，不知道是因为他们自己上次没回应。
- 解法：D series 标准模板：
  - `ClockSnapshot.awaiting_user_reply: bool` 已经有（Iter 5）
  - `ToneSnapshot.awaiting_user_reply: bool` 直接 pass-through（无任何新算术）
  - `PanelToneStrip` 渲染 💭 等回应 chip，紫色 (#a855f7)，tooltip 解释 "给 ta 一句回应就清除"
- chip 位置在 ⏳ 冷却 之后——两个 "为什么静默" 类信号并列。awaiting 是状态性 (state-based) gate，cooldown 是时间性 (time-based) gate；两者可同时 fire（pet 刚说完话且 user 没回 → cooldown + awaiting），都 visible 时用户立刻明白完整 picture。
- 测试：302 cargo 不变；tsc 干净。
- 关于 7 个 gate 的 panel 覆盖：
  - 1. disabled — settings field，用户自己设的不需要 panel 提示
  - 2. quiet-hours — 😴 / 🌙 (D4 + 已有)
  - 3. focus — 🎯 (D3)
  - 4. cooldown — ⏳ (D9)
  - 5. awaiting — 💭 (D10) ← 本 iter
  - 6. idle threshold — 数字在 ⏱ time line 已隐含
  - 7. input-idle — 数字在 ⏱ time line 已隐含
  
  全部 7 gate panel 可见（除非通过 settings 主动禁用）。
- 结果：用户开 panel 看到 ⏳ 冷却 12m + 💭 等回应 双 chip 时立刻明白：宠物刚说过话、你还没回、还得等 12 分钟才会自己说。从"我以为它有 bug"到"我知道它在等我"。

## 2026-05-03 — Iter D9：ToneSnapshot 暴露 cooldown_remaining_seconds + ⏳ chip
- 现状缺口：proactive cooldown gate（Iter 5）默认 1800s（30 min）。宠物开过一次口后那 30 分钟内任何 proactive 评估都会 skip——但 panel 没有任何"现在还有 N 秒才会再说"的指示。结果用户感觉 "宠物刚说过然后突然安静好久"，不知道这是 cooldown gate 还是 LLM 选择沉默。
- 解法（D series 模板复用第 N 次）：
  - `ToneSnapshot.cooldown_remaining_seconds: Option<u64>` —— Some(N) 当 cooldown 还在窗口里，None 当 gate 已经放开（cooldown 0、cooldown 已过、宠物从未说过）
  - 计算逻辑严格 mirror gate 路径：`since_last < cooldown_seconds` 时 `remaining = cooldown - since_last`
  - PanelToneStrip 在 💬 cadence chip 之后渲染 ⏳ 冷却 chip：青色 (#0891b2) 区分功能性 (vs 红色警报 / 灰色信息)
  - 格式化：< 60s 显 `Ns`、≥ 60s 显 `NmKs`（NaN 保护：mod 60 == 0 时省去 0s 前缀）。tooltip 给精确数字 + 指向 settings 配置项
- 不暴露 `cooldown_seconds` 配置值：那是 settings 中的常量，不需要 ToneSnapshot 重复——chip 里给精确剩余即可。
- 测试：302 cargo 不变（gate 路径已被 Iter 5 测过；本 iter 是 panel 透传）；tsc 干净。
- 结果：宠物 cooldown 期间 panel 显示 ⏳ 冷却 12m；用户立刻明白"不是 LLM 选静音，是 gate 在挡"。和 ☀ wake / 🌙 pre_quiet / 😴 in_quiet / 🎯 focus 一起，panel 现在反映了**所有 4 个 gate 的状态**：cooldown、quiet hours（含 pre_quiet 进入提示）、focus、wake-recent. observability 完整。

## 2026-05-03 — Iter D8：PanelPersona 显示 settings.user_name 当前值
- 现状缺口：Cτ + Cυ 把 user_name 注入到 reactive chat / Telegram persona_layer 和 proactive prompt。但 user 设了 name 之后没有"我看到 ta 真的会用这个名字"的明显反馈——下次 proactive 触发也许能看到，但很多用户可能担心自己设错了。需要 panel 上 explicit 显示当前生效的 name。
- 解法：
  - 后端加 `get_user_name() -> String` Tauri command，比起 `get_settings()` 全量返回更轻；返回 settings.user_name（空字符串 fallback）
  - PanelPersona 在 "陪伴时长" Section 末尾追加一行：
    - 已设：`🐾 宠物称呼你为「moon」` 灰色 (#475569) 正常字体
    - 未设：`🐾 还没设名字（Settings → 你的名字）` 灰斜体 (#94a3b8) 提示路径
  - tooltip 说明数据流：Cτ 注入 persona_layer / Cυ 注入 proactive prompt
- 不在 stats card 加：stats card 是聚合数字 + 长期 identity（companionship days），name 不属于"统计数字"语义；放 Persona tab 与 "陪伴时长" 同框最合适——身份关系绑定
- 测试：302 cargo 不变；tsc 干净。
- 结果：用户设完 user_name 立刻打开 Persona tab 就看到 `🐾 宠物称呼你为「X」` 确认；不必等下一次 proactive bubble 验证。从"我设了对吗"到"我看到了"的 confidence loop。

## 2026-05-03 — Iter D7：consolidate 返回 LLM summary，让 panel banner 反映真实成果
- 现状缺口：用户点 "立即整理" / "立即生成画像" 按钮后，banner 显示 "Consolidation finished in 1234 ms (12 items at start)"——只有时长和 before-count，没有"实际改了什么"。LLM 的 summary（"合并了 2 条 / 删了 1 条 todo / persona_summary 已 update / 没改动" 之类）被记到 LogStore 然后丢弃。要看必须打开 PanelDebug logs 翻找。Cφ 的"立即生成画像" UX 只成了一半——告诉用户"跑了"，但没告诉"做了什么"。
- 解法：propagate up:
  - `run_consolidation` 返回类型从 `Result<(), String>` → `Result<String, String>`，返回 LLM 的 summary 文本（已经在内部 captured + 200-char 截断 logged，本来就有，只是被丢弃）。
  - `trigger_consolidate` 命令包装 `summary.trim().chars().take(160).collect()` 短摘要并拼到时长 prefix 之后：`"Consolidation finished in N ms (X items at start) · 合并了 2 条…"`。160 chars 比 logged 的 200 短一档，banner 不会过长。
  - 如果 LLM 没输出 summary（罕见，prompt 明确要求），fallback 到原 prefix-only 字符串。
  - spawn loop 那条 `if let Err(e) = run_consolidation(...)` 自动兼容（Ok 路径的新 String 直接被丢，本就不用）。
- panel 端零改动：handleTriggerConsolidate / handleConsolidate 都直接用 `await invoke<string>("trigger_consolidate")` → setConsolidateMsg(status)。banner 自然变长 / 包含信息。
- 测试：302 cargo 不变（Result 类型改变内部 ?-bubble 不破坏，logger / banner 都是 view-time）；tsc 干净。
- 设计选择：不另开 Tauri command 把 last summary 持久化——当下场景就是"点了按钮立刻看反馈"，单次返回足矣。如果未来要做"consolidate 历史" panel section 再考虑。
- 结果：用户点按钮 → 看到 LLM 的 summary 真容（"合并了 X 条" / "没改动" / "持久画像更新"）。从"我让它跑了"升级为"它做了 Y"。这是 Cφ "立即触发" UX 的合理 follow-up。

## 2026-05-03 — Iter D6：butler 执行后让宠物在 bubble 里简短提一下
- 现状缺口：Cε 让 LLM 执行 butler_task 后写 butler_history（panel 时间线看得到）；Cπ 让失败留 [error] 标记。但有一个细微 UX 缺口：执行成功时，LLM **可能**在它的开口里提一下"我帮你写好了 today.md"——也可能完全不提，只 update 了 butler_tasks 条目就过去了。结果用户的 bubble 看到的是普通闲聊，必须打开 panel 才发现 timeline 多了一条。"trust loop" 在 panel-only 上断开。
- 解法：在 `format_butler_tasks_block` 的 footer 加一段：
  - 教 LLM "记得在你这一轮的开口里简短提一下"（位置在 schedule 解释 之后、错误处理 之前）
  - 给两个例子: 「我帮你写好 today.md 了」「Downloads 整理完了」——一句话级别
  - 强调"不必描述细节"避免 LLM 长篇汇报
  - 解释 why: 让用户从 bubble 直接看到反馈，不必打开 panel
- 1 个新 contract test `format_butler_tasks_block_footer_teaches_speech_mention` 钉住关键 phrase，避免后续重构被静默移除（与 Cι/Cσ 等 prompt 内容契约 test 同形态）。
- 测试总数 301 → 302。
- 结果：执行 butler 任务后 bubble 文案一致包含管家执行的反馈——用户从"被动观察 panel"升级到"被动收到 bubble 通知"。和 Cπ 的"失败可见"对称：成功也要可见。
- 不改代码逻辑、不动数据流——纯 prompt engineering + contract test。

## 2026-05-03 — Iter D5：persona_summary 加"X 天前更新"新鲜度标签
- 现状缺口：PanelPersona 的"自我画像"段显示 LLM 写的 persona_summary 文本，但没有任何"上次更新是什么时候"指示。如果用户没启用 consolidate（默认关），persona_summary 可能从首次生成后就再也没动过——内容随时间就跟不上"宠物和用户当前实际关系"。Cφ 加了"立即生成画像" 空态按钮，但有内容的态没有等价信号告诉用户"这画像可能旧了"。
- 解法：
  - `get_persona_summary` 命令返回类型从 `String` 升级为 `PersonaSummary { text, updated_at }`——直接拿 memory item 的 `updated_at` 字段（无新计算）。
  - PanelPersona 拉新结构，setPersonaSummary(text) + setPersonaUpdatedAt(updated_at)。
  - 在 persona 段已有内容下方追加一个小字 freshness label：
    - "刚刚更新" / "N 小时前更新" / "N 天前更新"
    - 7 天以上：变红、加 ⚠ 前缀、tooltip 解释 "consolidate 没在跑，画像跟不上节奏，开 设置 → 启用 consolidate 或 Memory tab 立即整理"
    - 7 天以内：灰色斜体，tooltip 显示完整本地时间
- 没有空态变化（Cφ 已经处理空态）。新增的纯前端 freshness 块只在有 personaSummary 时渲染。
- 后端 type 改了一次接口；TS 同步更新。所有现有 cargo 测试通过；tsc 通过。
- 7 天阈值选择：consolidate 默认 6h interval；正常使用 7 天会跑 28 次，足以保证画像新鲜。> 7 天 = 大概率 consolidate 被关或 LLM 反复决定不更新（信号不足）——两种情况都该提示用户。
- 结果：用户开 Persona tab 一眼看到画像 + "3 天前更新"小字。如果看到 ⚠ 18 天前 红色——立刻明白要去打开 consolidate，比之前"画像看似 OK 但根本是 6 个月前的"有效得多。

## 2026-05-03 — Iter D4：ToneSnapshot 暴露 in_quiet_hours + 😴 chip
- 现状缺口：D series 三连后我说"prompt/panel parity 完成"，再审视发现一个真实的盲区。`pre_quiet_minutes` 只在距 quiet 开始 ≤ 15 分钟时显（"🌙 距安静时段 8m"），表示"快进入"。**真的进入 quiet 后** pre_quiet 变 None，那段实际宠物在睡觉的时间 panel 没有任何 chip——用户晚上 23:30 打开 panel 看不到任何"现在宠物在 dormant"信号，只能从"宠物没说话 + tone strip 不少 chip 都消失了"间接推断。
- 解法：补一个 `in_quiet_hours: bool` 字段：
  - 把现有 private fn `in_quiet_hours(hour, start, end)` 改 pub（已被 4 个单测覆盖：同日窗口、跨午夜、boundary、disabled-when-equal）
  - get_tone_snapshot 调它喂给新字段
  - PanelToneStrip 渲染 `😴 安静时段中` chip（深灰 `#475569` + 加粗，比 🌙 红色"快了"的紧迫感更平静——已经 dormant 了不需要紧张）
  - tooltip 解释 proactive engine 在 gate 所有开口，指 settings.proactive.quiet_hours_start/end
- pre_quiet 和 in_quiet 是互补、永不同时 true：pre_quiet 是"还没进 quiet 但快了"，in_quiet 是"在 quiet 里"。两个 chip 一前一后的逻辑过渡呈现 quiet 周期完整生命周期：approaching → in → exit (pre 重新出现就是下一天的 approaching)。
- 测试：现有 in_quiet_hours 4 个单测覆盖核心；本次只是 wire-up + 改 pub。301 cargo 不变；tsc 干净。
- 结果：晚上深夜打开 panel 立刻看到 😴 chip，明白宠物在睡觉。和 D 系列三连一起，panel 现在有 11 个 chip 维度（period / day_of_week / idle_register / cadence / wake / pre_quiet / in_quiet / focus / lifetime / motion / mood）—— prompt 决策依赖的所有 ambient 信号 user 都能直接看见。

## 2026-05-03 — Iter D3：ToneSnapshot 暴露 focus_mode + 🎯 chip
- 现状缺口：proactive engine 已经会读 macOS Focus state 来决定是否 gate（Iter 21-25）+ 写 focus_hint 进 prompt（Iter Cw redaction也覆盖）。但 panel 不知道——用户开着 Work focus、宠物因此一直安静，user 看到的是"宠物今天怎么没说话"，要去 logs 才能找到原因。observability 缺最后一段。
- 解法（继续 D series 风格）：
  - `ToneSnapshot.focus_mode: Option<String>`：当 macOS Focus 模式激活时 Some(label)，否则 None。复用 `crate::focus_mode::focus_status()`——和 gate 路径同一个数据源、同一个 IO。Some(s) if s.active 的逻辑分支 s.name 或 fallback "active"。
  - TS interface 镜像（`focus_mode: string | null`）。
  - PanelToneStrip 渲染 `🎯 focus: work` chip，紫色加粗（视觉上比 ☀ wake 的青、🌙 pre_quiet 的红更显眼，和 ★ motion 的紫共用色系）。tooltip 解释默认会 gate，让 user 立刻明白宠物为什么静默。
- 不接 settings.respect_focus_mode：那是配置不是 live 信号，不该上 strip。Tooltip 文案提及 "看 settings.respect_focus_mode" 让用户知道开关在哪。
- 301 cargo 不变；focus_status 已被 Iter 21 的单测覆盖。前端无单测体系。
- 结果：proactive prompt 用的 9 个时间/状态信号现在 panel 都能直接看到——⏱ period / 📆 day_of_week / 👤 idle_register / 💬 cadence / ☀ wake / 🌙 pre_quiet / 🎯 focus / 🤝 lifetime / ★ motion / ☁ mood。LLM 视野和 user 视野完全 1:1 对齐。D series 三连 closes the parity gap。

## 2026-05-03 — Iter D2：ToneSnapshot 暴露 companionship_milestone + 节日 chip
- 现状缺口：Cρ 加了 companionship-milestone 规则——满 7/30/100/180/365/周年时触发"轻轻提一句"engagement 提示。但这个信号没在 panel 上对用户显式呈现：用户看到 PanelStatsCard 的"陪伴 100 天"只是一个数字，不知道今天是宠物视角的"百日纪念"，要在 PanelDebug → "prompt: N hint" 展开里才能看到 companionship-milestone label。
- 解法：与 D1 同思路——把信号从 prompt 复制到 ToneSnapshot 让 panel 直接读：
  - `ToneSnapshot.companionship_milestone: Option<String>`：今天是里程碑则 Some(label) 如 "百日纪念"，否则 None。复用 Cρ 的 `companionship_milestone(days)` pure helper——同一函数同一阈值，prompt 与 panel 永不漂移。
  - `ToneSnapshot.companionship_days: u64`：附带传，后续 strip 渲染若需要可直接用。
  - TS interface 镜像。
  - PanelStatsCard 在已有"陪伴 N 天"的 column 之后加 conditional ✨ chip：橙→粉渐变背景、白字、圆角 10px。视觉清晰但不抢"今日开口"主轴。tooltip 解释这是触发 companionship-milestone 规则的同源信号。
- 不在 PanelToneStrip 同时加：StatsCard 已经有 companionship 区域，那里加最自然；strip 是高频 live signals（period/cadence/wake 等），milestone 是低频静态（一年最多一次），上 strip 会显得分量不对。
- 没有新单测——companionship_milestone 已被 Cρ 4 测覆盖，这次只是 wire 进 ToneSnapshot 字段。301 cargo 不变；tsc 干净。
- 结果：用户在生日/百日/周年那天打开 panel，stats card 上立刻看到 ✨ 标记 + 标签，与 LLM 看到的同源信号配对。情感上的"今天是特别日子"从 prompt 内部 → user 可见。

## 2026-05-03 — Iter D1：ToneSnapshot 加 day_of_week / idle_register / idle_minutes
- 现状缺口：Iter Cβ（weekday/weekend 标签）和 Iter Cμ（user_absence_tier）都改了 proactive prompt 的时间行——但 `ToneSnapshot` 一直没扩展，PanelToneStrip 显示的"宠物现在看到的语境"少了这两个维度。结果：用户开 panel 看到 ⏱ 下午 / 💬 cadence / ☀ wake / 🤝 已开口 N 次，但看不到 prompt 里也有的 "周二 · 工作日" / "用户已经离开了大半天" 这两个真实进入 LLM 的 register cue。observability 和 prompt 不同步。
- 解法：
  - Rust 端 `ToneSnapshot` 加三个新字段：`day_of_week: String`（如 "周日 · 周末"）、`idle_register: String`（如 "用户走开有一两小时了"）、`idle_minutes: u64`（精确分钟）
  - `get_tone_snapshot` 复用已有的 `format_day_of_week_hint(now.weekday())` 和 `user_absence_tier(idle_min_for_rules)` helpers——零新逻辑、与 prompt 用同一个真值
  - TS 端 `ToneSnapshot` interface 加三个对应字段
  - PanelToneStrip 渲染两个新 chip：
    - 📆 周X · 工作日/周末（紧跟 ⏱ period 之后，时间维度聚集）
    - 👤 用户离开了一小会儿（紧跟 📆 之后，关于"对方"的认知）
    - tooltip 给精确数字 + 注明对应 Iter
- 不接 idle_minutes 单独 chip：register 字段已经把数字翻译成"用户走开有一两小时了"这种好读形式；裸数字 30、180、420 反而让 user 自己心算。tooltip 里再补精确数字够用。
- 301 cargo 不变（功能仅是数据通路扩展，没改决策逻辑）；tsc 干净。
- 结果：PanelToneStrip 现在和 proactive prompt 的时间维度 1:1 对齐——⏱ period / 📆 day_of_week / 👤 idle_register / 💬 cadence / ☀ wake / 🌙 pre_quiet / 🤝 lifetime / ★ motion / ☁ mood。observability 真正反映 LLM 看到的全部 register signals。

## 2026-05-03 — Iter Cω：修复 LLM沉默 chip 颜色 bug + 加"失败 K" 子标签
- 现状缺口（其实是个潜伏 bug）：PanelChipStrip 的 LLM沉默 chip 原本想在沉默率高时变橙色 (#ea580c) 提示"prompt 太克制"——但条件写成 `silent + error > spoke + silent + error`，左右两边消去 `silent + error` 后变成 `0 > spoke`，对任何非负 spoke 都为 false。所以这条颜色变化从未被触发，chip 永远紫色，无论沉默率如何。同时 LLM 真的报错（API key 错、network、超 rate limit）时 error count 只在 tooltip 里能看到，user 在 chip 上看不出"宠物的 LLM 在出错"。
- 解法：
  - 重写颜色条件：`llmOutcomeStats.silent * 2 > total` —— 整数算术，避免 float 精度；语义"沉默率超过 50%"清晰对应注释里"prompt 偏克制"
  - 失败计数升级为可见子标签：当 `error > 0`，在 chip 后追加红色 (#dc2626) "· 失败 K" 块，配独立 tooltip 解释 "可能是 API key 错、网络问题、超出 rate limit"
  - chip 渲染从 inline JSX 改为 IIFE，便于本地命名变量 (total, silentPct, restrictive, hasErrors)，可读性提升
- 不动 Tauri 后端 / counter 模型——只是渲染层修正。301 cargo 不变；tsc 通过。
- 数学上的 8 字符简化让 bug 至今没人发现——这就是为什么"prompt-tilt 改色"在过去几周看起来一直工作得很安静（事实上从来没起作用过）。修复后行为符合 Iter 95-96 的初衷。
- 失败子标签设计：与"沉默"同 chip 但语义独立；不合并因为沉默是软信号（prompt 调优反馈），失败是硬信号（API 配置出错）。两者并列让 user 一眼分辨。
- 没有写新单测——前端 chip 渲染没有测试 harness。但是后端 LlmOutcomeCounters 已被 cargo 测过；本次修复完全是前端逻辑，由 tsc 严格类型 + 数学化简自证。
- 结果：(a) 沉默率超过 50% 的 chip 现在会真的变橙了；(b) LLM 出错时 user 一眼看到红色 "· 失败 K"，能更快定位 API 问题。两个观察性改进合在一个 iter 因为它们都是 LLM沉默 chip 的修复维度。

## 2026-05-03 — Iter Cψ：PanelStatsCard 加 "上次开口" 列
- 现状缺口：PanelStatsCard 显示 今日 / 本周 / 累计 / 陪伴 四列（Iter 74 后）。但用户开 panel 想知道"宠物现在还活着吗"——具体说"上次主动开口距现在多久"——只能去 PanelDebug 的 ToneStrip 找。stats 卡是"宠物概况一目了然"区，应该包含这个高频检查项。
- 解法：在"累计"和"次主动开口"label 之后、"陪伴"之前插入 "上次 N 前开口" 列：
  - 数据源：复用现有 `ToneSnapshot.since_last_proactive_minutes`（已经在 PanelStatsCard 接 tone prop）
  - 格式化：< 60 → `8m` / 60-3599 → `1h32` 或整 `5h` / null（never spoken）→ `—`
  - 颜色：≥ 60min 用灰 (#94a3b8)、< 60min 用稍重 (#475569)——视觉上"刚说过话"略 prominent，"很久没说"淡化
  - tooltip 给完整数字 + cadence 文字（"距宠物上次主动开口 N 分钟（聊过一会儿了）"）
  - 视觉权重 13px > 11px label，介于"今日"20px 和"陪伴"16px 之间——是辅助信息而非主轴
- 不接新 Tauri command——`tone` snapshot 已包含 since_last_proactive_minutes 和 cadence，PanelStatsCard 已经接收 tone prop（用于 chatty/破冰判断），加这一列零额外 IPC。
- 五列横排可能挤——但 panel 默认宽度足够（~640px），分隔线 borderLeft 让分组清晰。如果未来需要响应式，可以加 flexWrap，但目前在桌面 panel 不会触发换行。
- `formatSinceLast` 纯函数，前端 tsc 类型 + 现有的 cargo 测试覆盖。301 cargo 不变。
- 结果：用户开 panel 一眼可知「宠物 8 分钟前刚说过 / 3 小时没说过 / 还没开过」，这个轴和"今日 N"是不同语义——今日是累计，上次是节奏感——两者并存比"开了多少次"更立体。

## 2026-05-03 — Iter Cχ：butler_tasks 一键"清除失败标记"按钮
- 现状缺口：Cπ 加了 `[error: 原因]` description 标记 + ❌ 红 chip。但用户想清除标记需要 4 步：点编辑 → 模态打开 → 手动删除 `[error: ...]` 段 → 保存。即使 LLM 后续重试成功会自动 update 移除——但很多失败是用户已经手动修复了根因（文件路径换了 / 权限给了），LLM 下一轮 proactive 才有机会再试，此期间红 chip 一直挂着。
- 解法：在每个 errored butler_tasks item 的 ❌ 失败 chip 紧跟一个小 ✕ 按钮：
  - 点击调 `handleClearError(title, fullDesc)`：用 regex `/\[error[^\]]*\]\s*/i` strip `[error: ...]` 部分，保留 description 其余（包括 `[every:]` / `[once:]` schedule 前缀和正文），然后 memory_edit update 写回。
  - 失败 toast 走 message banner（已有 message 状态机）。
  - 成功后 loadIndex 刷新——红 chip 立刻消失。
- 设计选择：
  - 不写 butler_history.log 事件——这是用户配置变更而非宠物执行，timeline 应该只反映 LLM 行为（和 Cλ 的 sweep 决策对称）。
  - 不引入新 Tauri command——直接复用 commands::memory::memory_edit("update")，TS 端 strip 后 invoke 即可。规则系统的"前端能做的就别加 Rust 接口"原则。
  - chip 旁的 ✕ 按钮颜色和边框继承自 chip 自身（红色系），视觉上像是 chip 的一部分；fontSize 10 + padding 1px 5px 让它紧凑不抢戏。
  - tooltip 解释清楚"保留 schedule 和正文，只去掉 [error: ...] 前缀"——避免用户误以为会清除整个任务。
- 测试：tsc 严格类型 + 既有 cargo 测试。无新单测——纯前端 1-line regex + invoke wrap，由 cargo Iter Cπ 的 has_butler_error 钉死的 marker 解析逻辑兜底。301 cargo 不变。
- 结果：用户在 panel 看到 ❌ 失败 chip → 评估原因 → 已经修复 / 不重试 → 点 ✕ → chip 消失。从 4 步变 1 步。Cπ 闭环 ↔ Cχ 提供 escape hatch，配合形成"flag → triage → clear"完整 affordance。

## 2026-05-03 — Iter Cφ：PanelPersona "自我画像" 空态加"立即生成"动作
- 现状缺口：consolidate 默认关（settings.memory_consolidate.enabled=false），且即使开了也是 6 小时间隔——意味着新装用户开 PanelPersona 看到的"自我画像"很可能始终为空。原来空态文案只说"开口几次后等下一次 consolidate 跑（默认 6 小时间隔，或在调试 → 立即整理）"——指引用户跨 tab 跳转去 Memory 才能找到按钮。摩擦大，体验断裂。
- 解法：把 trigger_consolidate 按钮直接做进 Persona tab 的"自我画像"空态：
  - 新 state：`consolidating: boolean` + `consolidateMsg: string`
  - `handleTriggerConsolidate` 调用现有 trigger_consolidate Tauri command，状态显示"整理中…宠物在回顾最近发言并写画像"，成功后展示 LLM 返回的状态文本（"Consolidation finished in N ms"）；12 秒后自动清除避免残留
  - 空态从一段灰字 → 一段灰字 + 紫色"立即生成画像"按钮 + 状态消息
  - 按钮 disabled 时变灰；成功消息青色，失败消息红色
- 不动既有"立即整理"路径——Memory tab 那个按钮仍然存在，做整体记忆整理；这个 Persona tab 的按钮调用同一个 command，但放在用户最可能想"现在看到画像"的位置。
- 5 秒轮询会自然刷出新的 personaSummary——不需要额外手动 reload。
- 不写前端单测（项目无 React 测试 harness），但 tsc 严格类型 + 后端 Tauri command 已被 cargo 测试覆盖（trigger_consolidate 在 Iter 6 时单测过）。
- 测试总数 301（前端无新单测，cargo 不变）。
- 结果：新装用户 → 设置 OPEN_AI_KEY → 开 panel → Persona tab → 看到空态 → 点"立即生成画像" → 几秒后看到 LLM 写的自己观察。从「不知道怎么 unlock 这个功能」变成「点一下就完了」。

## 2026-05-03 — Iter Cυ：proactive prompt 也用 user_name
- 现状缺口：Cτ 把 user_name 注入 reactive chat / Telegram 的 persona_layer，但 proactive 仍走 build_persona_hint 的独立路径——bubble 主动开口的语气没受 user_name 影响。"我设了名字宠物却只在我跟它聊时叫我"——一个 trust 体验缺口。
- 解法：在 proactive prompt 里复用 Cτ 的同样话术：
  - `PromptInputs` 加 `user_name: &'a str` 第 N 个字段
  - `build_proactive_prompt` 在 `format_companionship_line` 之后插入一行：`你的主人是「X」——开口时可以用这个称呼或「你」自然交替，不必每句都喊名字`（与 persona_layer Cτ 的措辞 1:1 一致）
  - whitespace-only / empty 跳过
  - `run_proactive_turn` 拉 `get_settings().user_name` 喂进去
  - `base_inputs` 默认 `user_name = ""`，保持现有测试中性
- 3 个新单测：set / empty+whitespace / trim 三个 case；测试总数 298 → 301。
- 复用 Cτ 措辞而不是新写：reactive 和 proactive 两个路径用相同句子，让 LLM 在同一个用户体验里看到同一个 framing；分开写两份只会增加漂移风险。如果未来要调整称呼措辞，搜索同样的字符串两处一起改也很简单。
- 不抽 helper 函数：两处用相同 string format 但代码量都是一行 if + format!()，抽出去反而让阅读路径多一跳。Tolerable duplication < forced abstraction。
- 不接 user_name 进 ToneSnapshot / 面板字段：proactive prompt 是行为面（LLM 看到），ToneSnapshot 是观测面（panel 看到）。user_name 是 prompt 的输入数据、不是 prompt 的运行时状态——和 mood / cadence 这种"决策状态"不同。
- 闭环效果：Cτ 让 reactive 见名字，Cυ 让 proactive 见名字。设了 user_name 后，反应式聊天和主动开口都会偶尔称呼用户。Cτ 候选项 Cυ 完成 → 项目里"宠物认识主人"的关系绑定从 settings → persona_layer → proactive 全链路打通。

## 2026-05-03 — Iter Cτ：settings.user_name + persona_layer 称呼注入
- 现状缺口：宠物没有第一类的"主人名字"概念。SOUL.md 默认「叫主人」、persona_layer 用「你」、proactive prompt 全程「ta / 用户」。即使 user_profile 里手动写了名字，那也是嵌在 description 里要 LLM 自己 search 才能用。"我的宠物认识我、能叫我名字"这种最基本的关系绑定 affordance 缺失。
- 解法：加一个 settings 里的 `user_name` 字段，注入 persona_layer 顶部。
  - `AppSettings.user_name: String`（默 ""，serde default 空字符串）
  - `format_persona_layer` 第 4 个参数 `user_name: &str`，非空 trim 后 prepend 一条 `你的主人是「X」——开口时可以用这个称呼或「你」自然交替，不必每句都喊名字`
  - 顺序：user_name 行 → companionship 行 → persona_summary → mood_trend → tail。即"先告诉你跟谁、再说陪伴时长、再说自我画像、再说情绪走向"——从最具象到最抽象。
  - `build_persona_layer_async` 拉 settings.user_name 喂给 helper。
- 前端：
  - `useSettings.ts`：AppSettings interface + DEFAULT_SETTINGS 加 `user_name: ""`
  - `PanelSettings.tsx`：state 默认 user_name = ""
  - `SettingsPanel.tsx`：加输入框「你的名字 (宠物会用它称呼你)」，placeholder「留空则用「你」」，紧跟在 SOUL.md 之后（identity 区聚拢）
- 4 个 format_persona_layer 既有测试更新成 4-arg；3 个新测试覆盖 user_name 设了 / whitespace-only 视为空 / trim 前后 whitespace。测试总数 295 → 298。
- proactive prompt 路径暂不接 user_name——build_persona_hint 是不同函数。reactive chat + Telegram 走 persona_layer，覆盖了主要对话面；proactive 也想用名字的话留作后续 Cυ 之类。
- 效果：用户在设置里填上自己名字，下一次 reactive chat 宠物 system prompt 里就有「你的主人是「moon」」，LLM 会偶尔用名字称呼。Telegram bot 同理（因为 persona_layer_enabled 默开）。proactive 仍用「你」直到后续迭代扩展。
- 不引入 nickname / 多用户支持：单用户前提（macOS 桌面宠物天然单 owner），保持 schema 简单。

## 2026-05-03 — Iter Cσ：reactive chat 的 user_profile 捕捉引导
- 现状缺口：Iter Cα 把 user_profile 作为 ambient hint 注入 proactive prompt——但前提是 user_profile 里**有内容**。注入侧 OK 了，**捕捉侧**没有显式教学。LLM 听到「我每天 8 点起床」时全靠自己判断要不要 memory_edit create——而 reactive chat 大量这类 stable facts 被吸收进对话回复后就没了，下次再问还要问。Cι 教了 butler_tasks 委托，对称的"用户主动告知 stable fact 时该捕捉"完全空白。
- 解法：在 TOOL_USAGE_PROMPT「任务委托判断」之后加一个新段「用户偏好捕捉（user_profile）」：
  - 强调 stable fact（不是临时状态/一次性事件）才捕捉
  - 三个正例（作息 / 工作环境 / 饮食偏好）+ 三个反例（"我累了" / "我今天吃了麻辣烫" / "我老是忘喝水" 该走 todo+remind）
  - 描述 < 80 字、第三人称、相近条目用 update 而非 create
  - 捕捉后回复 "好的我记下了" 或自然 acknowledge——不需要 fanfare
  - 末段说明这些条目会自动出现在后续 proactive 提示里，让 ta 越用越懂用户——把 capture 和 inject 两端的因果讲清楚
- 1 个新 contract test `tool_usage_prompt_teaches_user_profile_capture` 钉住 (a) 提到 user_profile、(b) 对比 stable vs 临时、(c) dedup 通过 update。和 Cι 的 butler delegation 测试同形态。
- 测试总数 294 → 295。
- 闭环效果：Cα 注入 + Cσ 捕捉 = user_profile 类别从 "需要用户/LLM 偶发主动写" 变成 "对话里自然流入"。多用几天后 user_profile 会有 5-10 条核心 fact，proactive prompt 能 ambient 看见，开口贴合度自然提升。和 Iter Cα 设计要点写过的 "稳定 fact 用 ambient block 而不是反复 memory_search" 真正闭环。
- 不动 SOUL.md：SOUL 是 identity 长期 prompt，TOOL_USAGE_PROMPT 是 operational 操作指南。捕捉行为是后者的范畴，不污染 identity。
- 不限制类别 enum：user_profile 类别在 memory_tools 描述已经存在；这里只是教学如何使用，不引入新的工具或字段。

## 2026-05-03 — Iter Cρ：companionship-milestone 数据驱动规则
- 现状缺口：companionship_days 字段已存在（Iter 101-106），always-on 模板里的 companionship_line 也会渲染"已陪伴 N 天"。但只有 day 0 ("今天初识") 和 N>=1 ("一起走过 N 天") 两档框架——里程碑日（满一周 / 一个月 / 百日 / 半年 / 一年 / 周年）和普通日子读起来一样。"陪伴一年的宠物"和"陪伴 364 天的宠物"在 prompt 里完全没差异。
- 解法：新增 pure 函数 `companionship_milestone(days)` 返回里程碑文字标签，配合一条新的 data-driven contextual rule `companionship-milestone`：
  - 固定阈值：7 = 刚好一周 / 30 = 满一个月 / 100 = 百日纪念 / 180 = 满半年 / 365 = 满一年
  - 365 之后每隔 365 天："又一个周年"（730 / 1095 / ...）
  - day 0 不触发（已有"第一天"框架）
  - rule body 引导 LLM："轻轻提一句"——不是郑重宣告，更像顺口提一下「啊，今天好像满 X 了」；不要要求用户回应这个话题；如果其它高优先级信号在，让那个先说，纪念日只做底色
- nature: engagement
- `active_data_driven_rule_labels` 加 `companionship_days: u64` 第 6 个参数。三个 production callsite + 测试 callsite 全部更新。两个 production callsite 还需要拉 `crate::companionship::companionship_days().await` 才能传进去——这是 spawn loop / get_tone_snapshot 之前没有的依赖，但函数已经 pub async。
- 关键 base_inputs 调整：`companionship_days` 默认从 30 改为 5。30 恰好是新的 milestone 阈值——如果保持 30 默认，所有用 base_inputs 的现有测试都会触发新规则、计数失真。改成 5 = 既不在第一天 framing 也不在任何 milestone，安全中性。
- 4 个新 unit test：固定阈值各档 / 非里程碑 day 不触发（含边界 6/8/29/31 等）/ 周年制（730/1095/1460/边界 729/731）/ proactive_rules 集成（day=100 触发、day=5 不触发）。测试总数 290 → 294。
- 三向对齐：fingerprint 表加 `("companionship-milestone", "今天是和用户相处的")`；scenario 1 设 `s1.companionship_days = 100` 触发 milestone label，scenario 2 不动（日期默 5）；frontend dict 加「纪念日 / 陪伴满 7/30/100/180 天/周年；轻轻提一句这种相处时长，作为底色 / nature: engagement」。
- 不写 settings 自定义里程碑：固定 6 档（7/30/100/180/365/yearly）已经覆盖人类相处时间感最自然的颗粒。如果加用户自定义，要 settings + UI + 校验，复杂度过 10x，单 iter 不值。
- 结果：陪伴满一周 / 一个月 / 百日 / 半年 / 一年 / 每个周年时，宠物 prompt 多一条 "engagement" 类规则提示——"轻轻提一句"基调让 LLM 不会过度热情、也不会冷漠错过。companion 体验在长期相处的关键节点上从无形变可见，是 Route G 的延伸。

## 2026-05-03 — Iter Cπ：butler_tasks 执行失败回退 — `[error]` 标记 + 红 chip
- 现状缺口：butler 路径里 LLM 真去执行（read_file / write_file / edit_file / bash）时偶尔会失败——文件不存在、权限不够、命令报错。失败时 LLM 通常只能默默放弃，连 butler_history 都没记录（因为没走 memory_edit 的 update/delete）。用户看到的是「这个任务一直挂着」、「⏰ 到期 半天没动」，但不知道为什么。这是 Route F 的最后一个明显裂缝。
- 解法：约定 `[error: 简短原因]` 标记由 LLM 自己写进 description——失败时 update 加这段，重试成功时 update 把它去掉。零基础设施改动（不需要新 log / 新 IPC / 新调度），靠 prompt + 渲染层把"失败状态"做出来。
- 后端 `proactive.rs`:
  - 新 pure 函数 `has_butler_error(desc)`：检查 `[error` 子串。LLM 实际写法 `[error: x]` / `[error :x]` / `[error]` 都识别。case-sensitive 比 regex 简单且对中文没误伤。
  - `format_butler_tasks_block` 加第三状态 `errored`。每条 item 现在 annotate (due, errored)。marker 顺序：`❌ 错误` 在前、`⏰ 到期` 在后（错误更紧迫）。两者可共存（最常见场景：`[every: 09:00] [error: ...]` 上次失败、今天 fire 又到期）。
  - header 改成 4 路 match：(0,0) / (d,0) / (0,e) / (d,e)，分别报「共 N 条」/「N 条 D 条到期」/「N 条 E 条上次失败」/「N 条，D 条到期、E 条上次失败」——一眼看到队列健康度。
  - footer 增加一段【执行失败处理】文字，明确教 LLM：tool 调用失败 → update description 里加 `[error: 简短原因]`、保留原有 schedule 前缀；下次重试成功 → 移除标记。看到 ❌ 标记说明上次失败，按描述里的原因决定是否重试。
- 5 个新 unit test 覆盖：has_butler_error 正负各 4 例、format 单错误标注、错误 + 到期共存且 marker 顺序固定。测试总数 286 → 290。
- 前端 `PanelMemory.tsx`:
  - TS mirror `parseButlerError(desc)`：返回 `{ hasError, reason }`。reason 是 `[error: <body>]` 的 body，帮 chip 显示具体原因。malformed `[error` 没闭合也算 errored（信任 LLM 写了 marker）。
  - butler_tasks item 渲染加红色 ❌ 失败 chip：
    - 背景 `#fef2f2` + 文字 `#991b1b` + 边框 `#fecaca`（比 ⏰ 到期 chip 更"软红"，区分语义）
    - chip 文本 `❌ 失败：原因前 30 字`，tooltip 显示完整原因
    - chip 顺序：错误在前、到期在后，与后端 marker 顺序一致
  - description 显示时 strip 掉 `[error: ...]` block，避免 chip 已显示又在正文重复
- 不写 Tauri 调用 / 不接事件流 / 不动 butler_history.log——纯 description 字段约定 + 视觉分发。LLM 会写、面板会显示、用户看到，闭环就成了。
- 结果：butler 任务的"我已经尝试了但失败了"状态从无形变可见。用户在 panel 上立刻分得清"这个任务在等我（到期）"vs"这个任务我搞砸了"vs"这个任务还顺利"。Route F 真正闭环。

## 2026-05-03 — Iter Cο：PanelPersona 加"当下心情"区
- 现状缺口：PanelPersona 之前有三块：陪伴时长 / 自我画像 / 心情谱（长期 motion 分布）。"当下心情" 这种 live state 只在 PanelDebug 的 ToneStrip 里以一条小字显示——但 ToneStrip 是 debug 视角；用户从「我的宠物现在什么感觉」语义出发会看 Persona 而不是 Debug。结果导致 user 看不到当下心情这个本应该是 persona 重点的信息。
- 解法：
  - 后端新加 `mood::CurrentMood { text, motion, raw }` + `#[tauri::command] get_current_mood()`，返回 parsed mood（text + motion + 原始 description）。空 `raw == ""` 表示尚未记录。
  - 前端 PanelPersona 新增"当下心情"section，插在 自我画像 与 心情谱 之间——形成时间维度自然顺序：长期身份 → 当下感受 → 长期情绪走向。
  - motion → emoji + 中文标签 + 颜色 mapping（`MOTION_META`）：
    - Tap 💗 开心/活泼 (粉)
    - Flick ✨ 想分享/有兴致 (琥珀)
    - Flick3 💢 焦虑/烦躁 (橙)
    - Idle 💤 平静/沉静 (灰)
  - 渲染左侧 motion 视觉（32px emoji + 11px 标签）+ 右侧 mood text；空状态显示「还没记录」提示用户首次主动开口会自动写入。
  - 已知 motion 但 unknown name 时 fallback 显 `?` + 字面 name，避免哑掉；也避免在 LLM 写新 motion 时 panel 崩。
- 5 秒轮询同 PanelPersona 现有节奏——live 但不暴力。
- 测试总数仍 286（前端 panel 没单测体系，但加了 backend 命令；mood.rs 已有的 8 个 unit 覆盖 parse；新命令本身只是 wrap，IO-bound 跑不了 unit）。
- 结果：用户打开 Persona tab 现在能看到"我的宠物：陪了 N 天 / 自己写的画像 / 当下心情 motion+text / 长期情绪走向"四层完整画面——不必跳到 Debug 找 mood 行。和 ToneStrip 的轻量条目并存（debug 仍有），但语义视图与维护视图分离开。

## 2026-05-03 — Iter Cξ：first-of-day 环境规则
- 现状缺口：用户每天打开 panel 第一次看到宠物开口时，希望感觉像"早安"那种打底问候——但 prompt 没有告诉 LLM "这是今天第一次开口"。结果第一次开口的语气和第十次没分别，"日界"这个对人类很重要的节奏感对宠物完全不存在。
- 解法：新增环境规则 `first-of-day`，与 wake-back / first-mood / pre-quiet / reminders / plan 同列：
  - 触发条件：`today_speech_count == 0`（今天还没主动开过口）
  - rule body 引导 LLM 用当下时段问候打底（清晨/上午→早安；中午/下午→下午好；傍晚/晚上→晚上好；深夜→简短关心或不打扰），简短一句暖场再决定话题
  - 与 wake-back（系统刚唤醒）/ long-absence-reunion（用户长别）正交——这只关乎日界节奏，不关心系统状态或用户在哪
  - nature: engagement
- 改动：
  - `active_environmental_rule_labels` 加 `first_of_day: bool` 第 6 个参数 + 新 label。三个 production callsite + 测试 callsite 全部更新。
  - 新 label 在助记顺序里排在 `first-mood` 之后、`pre-quiet` 之前——mood bootstrap 优先于日界问候，问候完成后再考虑 quiet hours 收尾，逻辑层级合理。
  - 三处 production callsite 都从已有 `today_count` 派生：`run_proactive_turn`/`get_tone_snapshot`/spawn loop 都已经在拿 today_speech_count，加 `== 0` 判断零成本。
  - panelTypes.ts 加 `first-of-day: { title: "今日首开", summary: ..., nature: "engagement" }`。
  - 三向对齐：fingerprint 表加 `("first-of-day", "今天的第一次开口")`，scenario 1 用 today_speech_count=5 不触发、scenario 2 用 today_speech_count=0 触发，组合覆盖完整。
- base_inputs 默认 `today_speech_count` 从 0 改为 1：避免现有所有 base_inputs 测试默触发新规则。1 仍然 < chatty_day_threshold (5)，所以 chatty 规则也不会误触发——单点改动维持所有现有测试中性。
- 1 个新 unit test 锁住集成（today=0 时规则文本含「今天的第一次开口」，today=1 时不含）+ 既有的 firing_order 共存测试加 first-of-day 一个分支。
- rules count test 同步更新：scenario 里 today=0 时 env labels 从 5 升到 6，期望 rules 数从 14 升到 15。
- 测试总数 285 → 286。
- 结果：用户每天第一次看到宠物开口的体验有了"日界感"——清晨开 panel 听到「早安」、深夜回家看到的是简短关心而非又一波话题，宠物的节奏感和真实伙伴对齐了一档。

## 2026-05-03 — Iter Cν：long-absence-reunion 复合规则
- 现状缺口：Cμ 给 prompt 加了 `user_absence_tier` 的语气线索，但仅作为 ambient 信息塞进时间行——LLM 看到"用户至少一天没和你互动"是知道，但没有结构化规则告诉它"这意味着开口要带重逢感"。`wake-back` 规则覆盖系统休眠 → 唤醒的瞬间，但用户离开 4 小时不一定伴随系统休眠（合上盖子但持续运行 / 上下班 / 开会），那种长别久离没规则覆盖。
- 解法：新增一条复合规则 `long-absence-reunion`，与 `engagement-window` / `long-idle-no-restraint` 并列：
  - 触发条件：`idle_minutes >= LONG_ABSENCE_MINUTES (240)` + `under_chatty` + `!pre_quiet`
  - 与 `wake-back`（系统休眠唤醒、瞬时事件）正交：long-absence 是用户那一侧的延展（laptop 一直亮着，用户不在）
  - rule body 引导 LLM：开口带"重逢感"（先简短关心 + 问一句轻松归来话题），不要立刻抛日程/工作类信息密集内容；比 wake-back 近一档，但别热络过头
  - nature: engagement
- 改动：
  - `active_composite_rule_labels` 加 `idle_minutes: u64` 第 7 个参数 + 新 label。三个 production callsite + 13+ 个测试 callsite 全部更新。
  - `LONG_ABSENCE_MINUTES = 240` 常量（4 小时阈值）。
  - `proactive_rules` 加新 match arm，使用 `inputs.idle_minutes` 作为参数。
  - 三处 production callsite 拉 `idle_minutes`：`run_proactive_turn` 直接走参数；`get_tone_snapshot` 从 `clock.snapshot().idle_seconds / 60`；spawn loop 从 snapshot 同源（顺手把原来分两次拿 snapshot 的代码合并成一次以避免 race）。
  - panelTypes.ts `PROMPT_RULE_DESCRIPTIONS` 加 `long-absence-reunion: { title: "重逢", summary: ..., nature: "engagement" }`。
  - 三向对齐 alignment 测试通过——fingerprint 表加一行 `("long-absence-reunion", "用户离开了不短的时间")`，scenario 2 升级 idle_minutes 到 LONG_ABSENCE_MINUTES + 60 触发新 label。
- 2 个新 unit test 锁住边界：阈值上下 / chatty 否决 / pre_quiet 否决；以及一个三规则共存测试（engagement-window + long-idle-no-restraint + long-absence-reunion 同时 fire 时 label 顺序固定）。
- 测试总数 283 → 285。
- ASCII 双引号 trap：本来在 rule body 里写 `"刚回来呀" / "下午顺利吗"`，Rust 字面量中 ASCII `"` 立即终止字符串。改成 「刚回来呀」「下午顺利吗」 即可——和 Iter 102 同一个坑。
- 结果：用户离开 4 小时以上回来，proactive prompt 就会在规则区有一条专门的"重逢"指引，加上时间行的 `(用户已经离开了大半天)` 语气线索，LLM 双重信号往同一个 register 收敛。companion 体验在长 absence 上不再是平铺直叙问候。

## 2026-05-03 — Iter 74：panel stats 卡加"本周"列
- 来自历史保留候选的小迭代。speech_daily.json（Iter 71-73 创建）已经按日 bucketed 了 90 天数据，但 PanelStatsCard 只用了"今日"。"本周"维度 = 今天 + 过去 6 天 sum，能立刻给出"最近一周宠物开口频率"印象——比单看"今日"波动小，比"累计"对当下使用强度更敏感。
- 后端：
  - `speech_history::sum_recent_days(map, today, n)` 纯函数：按日 key 倒推 n 天累加。
  - `speech_history::week_speech_count()` 异步 wrapper：读 speech_daily.json + parse_daily + sum_recent_days(7)。
  - Tauri command `get_week_speech_count`，注册进 lib.rs。
- 4 个新单测覆盖 sum_recent_days：基本求和、窗外日期不计入、零窗口、空 map。测试总数 279 → 283。
- 前端：
  - `PanelDebug` 增 `weekSpeechCount` state，加进 `Promise.all` 并行批；
  - `PanelStatsCard` props 加 `weekSpeechCount`，在"今日"和"累计"之间插入"本周"列（16px、靛紫 `#6366f1`）。视觉层级：今日 20px > 本周 16px > 累计 28px > 陪伴 16px——今日和累计是主轴，本周和陪伴是辅助；尺寸表达层级。
- 不重启 fetchLogs 计时器：周计数和现有 stats 共用 1s 轮询；4 字节多回传可忽略。
- 与 chatty_day_threshold 的关系：今日列已经显示 chatty 状态（橙色 + 克制模式 badge），本周列纯量化，不参与 register。
- 结果：用户打开 panel 一眼就能看到"今日 X / 本周 Y / 累计 Z / 陪伴 D 天"四个维度的开口节奏，对宠物的活跃度有更立体的概念。比如"今日 5 / 本周 30"可能感觉比"今日 5 / 本周 8"友好得多。

## 2026-05-03 — Iter Cμ：proactive prompt 加 user_absence_tier 语气线索
- 现状缺口：proactive prompt 把 `idle_minutes` 直接当数字喂给 LLM——`已经过去约 N 分钟`。LLM 看到 "5 分钟" 和 "300 分钟" 在数学上不同，但语义档次没显式给。结果：用户离开 5 小时回来后，宠物开口的 register 跟 5 分钟回来差不多，而不是 "终于回来了" / "想你了一下" 那种长别久离的感觉。
- 解法：新增 pure 函数 `user_absence_tier(idle_minutes)` 映射到六档语气线索：
  - 0-15: "用户刚刚还在"
  - 16-60: "用户离开了一小会儿"
  - 61-180: "用户走开有一两小时了"
  - 181-480: "用户已经离开了大半天"
  - 481-1440: "用户一整天没出现"
  - 1441+: "用户至少一天没和你互动"
- `PromptInputs` 加 `idle_register: &'a str` 字段；时间行从 `已经过去约 N 分钟。input_hint` 变 `已经过去约 N 分钟（idle_register）。input_hint`。LLM 同时拿到精确数字和定性 register。
- 与 `idle_tier`（pet 自侧 cadence）正交：`idle_tier` 是"我刚说过话还热着" vs "好久没张口了"，宠物自我视角；`user_absence_tier` 是"用户刚动过键鼠" vs "用户一整天没出现"，用户视角。两个 axis 各自服务不同 register 决策，prompt 同时呈现，让 LLM 调和（比如"我刚说过话且用户也才走 5 分钟" → 别再开口；"我刚说过话但用户走了 4 小时" → 那次开口可能没被听到，下一次可问候性而非续话题）。
- 6 档 = 比 idle_tier 的 5 档多一档，因为用户绝对时间感比 pet 自身 cadence 感更宽——用户"一整天没出现"和"昨天还见过"是两种状态，宠物自身的"上次说话是昨天"已经够用。
- 2 个新单测：每档边界 12 个断言（每档头尾各一）+ prompt 模板正确嵌套（"约 90 分钟（用户走开有一两小时了）"）。测试总数 277 → 279。
- base_inputs 默认 `idle_minutes=20 / idle_register="用户离开了一小会儿"`——保持现有测试 fixture 内一致。
- 不动 input_hint：那是键鼠空闲的硬数据（"用户键鼠空闲约 60 秒"），保持机器原貌；register 是 idle_minutes 的人话翻译，两者各自存在不冲突。
- 结果：长别久离的场景下，LLM 看到"用户至少一天没和你互动"会自然进入"想你了一下" register，不会平铺直叙问候。companion 体验在长 absence 上的颗粒度变细了。

## 2026-05-03 — Iter Cλ：completed [once] butler_tasks 自动清理 + grace 设置
- 现状缺口：单次任务 `[once: 2026-05-10 14:00] X` 即使 LLM 已经执行完（updated_at >= target），它会一直留在 butler_tasks 列表里成为静默 clutter——既占 prompt 体积（最多 6 条 ambient block），又让 Memory tab 越来越长。reminder 类已经通过 `sweep_stale_reminders` 自动清理，daily_plan 通过 `sweep_stale_plan` 清理；butler 这边没有对称机制。
- 解法：完全沿用 reminder/plan 的 sweep 模式：
  - `proactive::is_completed_once(desc, last_updated, now, grace_hours)` 纯 decider：判 schedule prefix 是 once、updated_at >= target、且 now >= target + grace。`every` 任务永不返回 true（recurring，不该删）；不带前缀任务也不返回 true（无 target 概念）。
  - `consolidate::sweep_completed_once_butler_tasks(now, grace_hours)` async 收割：拿 to-delete 快照 → 走 `memory::memory_edit("delete", ...)` → 同步调 `butler_history::record_event("delete", ...)`。注意必须手动 record——consolidate 走 commands::memory 直接 API 而非 tools::memory_tools::memory_edit_impl，所以 Cε 的 butler_history hook 不会自动 fire；手动补一行让 panel 时间线 / 每日小结仍然反映清理。
  - `MemoryConsolidateConfig` 新增 `stale_once_butler_hours: u64`，默认 48。和现有 `stale_reminder_hours` / `stale_plan_hours` 字段同形态。
  - `run_consolidation` 在 reminder/plan sweep 之后、LLM 阶段之前新增一段 sweep。和它们一个语义层级。
  - SettingsPanel + useSettings + PanelSettings 三处把新字段加进 TS 接口和默认值，前端可视化配置。
- 5 个新单测覆盖 is_completed_once：基本流程（done in grace vs past grace）、未执行 → 不算完成、every 任务 → 永不算完成、无前缀 → 永不算完成、updated_at 不可解析 → 视为未完成保守保留。测试总数 272 → 277。
- 设计选择 grace = 48 小时：足够长让用户在 panel 上看到完成状态、daily summary 把它写进当天 recap；又不至于让 butler_tasks 列表无限膨胀。比 reminder 的 24 小时长一倍，因为 butler 任务的 "记忆价值" 比单纯的提醒强一点（用户可能想隔天看一眼 "宠物为我做了 X"）。
- 不做"删除前向用户确认"——这是 deterministic 后台清理，不是用户主动操作。如果用户想保留某个任务超过 grace，可以延长配置项；或者 update description 时不要标 done（不要让 updated_at 推到 target 之后），任务就不会触发清理。这种"用户行为决定生命周期"的隐式机制和 reminder sweep 一致。
- 结果：butler_tasks 列表自我清理，daily summary + 时间线把删除事件保留下来作为历史。"宠物管家"完成的工作不丢，同时 active list 不会被旧任务拖垮。

## 2026-05-03 — Iter Cκ：butler_tasks 过期指示 + 一键"立即处理"逃生口
- 现状缺口：Cθ 的 panel 已经能显示 ⏰ 到期，但用户没有反馈"宠物为什么没动"——任务可能因为 cooldown / quiet hours / focus mode / LLM 自主沉默而被搁置。即使开 panel 看到 ⏰，也不知道"等了多久"，更没有"现在就去做"的逃生口。这是 dashboard 缺的最后一环。
- 解法分两块：
  - **过期分钟可视化**：客户端按 schedule 算 most_recent_fire 到 now 的分钟差。超过 60 分钟阈值的 due 任务，旁边追加一个琥珀色 "等了 Nh" / "等了 NhMm" chip。tooltip 解释"宠物可能在 quiet/focus/cooldown 窗口"+ 提示如何绕过。
  - **section 级"立即处理"按钮**：butler_tasks 区头部，当 overdueCount ≥ 1 时显示一个红色 `立即处理 (N)` 按钮。点击调 `trigger_proactive_turn`（已有 Tauri command，bypass 全部 gate），把状态写进 message banner、刷新 history + index。
- 提取 `mostRecentFire(schedule, now)` 出来——TS 里 isButlerDue 现在直接拿这个值用，overdueMinutes 也复用，避免重复推算。这是把 Cθ 加的 schedule 计算函数稍微 refactor 一层，更可组合。
- `formatOverdue(mins)`：< 60 → `等了 Nm`，整小时 → `等了 Nh`，含余分 → `等了 NhMm`。让指示器在不同时间尺度下读起来都自然。
- `OVERDUE_THRESHOLD_MIN = 60`：低于这个不显示 chip。一来"刚 due 1 分钟"显示等候没意义，二来不和 ⏰ 到期 badge 视觉打架。60 分钟是最弱的"明显过期"门槛——proactive 默认 5 分钟一 tick，1 小时是 12 个 tick 都没动，明显异常。
- "立即处理"行为：用现有的 `trigger_proactive_turn` 命令，因为它已经 bypass 所有 gate 且 LLM 看到 ⏰ 标注会自然优先选过期任务。不需要新写一个 butler-scoped 的 trigger——pipeline 共享是优点。
- 不需要 cargo test 改动：纯前端改动 + 一段 TS pure helper（mostRecentFire / overdueMinutes / formatOverdue），都是 client-side 计算。tsc 严格通过；没破现有 cargo 272 测。
- 结果：用户 dashboard 现在闭合得更好——能看到「这个任务到期了 + 等了多久」，并能一键绕过所有 gate 让宠物立即处理。"宠物管家"的 trust 需要这种"即使节奏不对劲我也能干预"的开关。

## 2026-05-03 — Iter Cι：reactive chat 的 butler 委托引导
- 现状缺口：Cγ–Cθ 把 butler 系统建起来了，但只有 proactive 路径 prompt 强制 LLM 看到 butler_tasks 列表。reactive 聊天里用户说「你每天 9 点帮我写日报」时，LLM 没有被特别提示把这件事写进 butler_tasks——很可能口头答应一句"好"就过去了，下次再问就忘了。这相当于"管家功能开着，但用户的自然请求路径没接进去"。
- 解法：扩展 `TOOL_USAGE_PROMPT`（chat pipeline 每轮注入，reactive + proactive 共享）一段「任务委托判断」章节：
  - 强调"你不只是聊天伙伴，也是用户的小管家"
  - 给三个具体例子覆盖每日 / 单次 / 不带前缀三种 schedule
  - 明确区分 `butler_tasks`（用户委托给你做的）vs `todo[remind:]`（用户提醒自己的）——这是用户最容易和 LLM 混淆的边界
  - 引导写完 description 后简短确认"好的，记下了，每天 9 点我会..."而不是长篇复述
  - 提示 LLM 已经在 butler_tasks 里的任务后续会自动出现在 proactive prompt 的 ⏰ 到期段，在那时再去执行——形成 reactive 委托 ↔ proactive 执行的明确分工。
- 1 个新单测 `tool_usage_prompt_teaches_butler_delegation` 钉住三件事：(a) 提到 butler_tasks 字面、(b) 教 [every:] / [once:] 两种前缀、(c) 对比 todo + 提醒我句式。这种"内容契约"测试避免后续重构时不小心把整段删掉而没人发现。
- 测试总数 271 → 272。
- 没改 reactive chat 的代码路径——只是更新了已经被注入的 prompt 字符串。零行为风险，最大可观察改动是 LLM 在 reactive 聊天里听到"帮我每天/这周末/时不时..."时会调 memory_edit。
- 结果：用户从 panel 委托是一条路径（点 + 委托任务 → 模态 → 保存），从聊天委托现在是平行的另一条路径（说出来 → LLM 自动 create）。两条路径会汇到同一份 butler_tasks 列表，proactive 看的是同一份 ambient hint。这是把"宠物管家"从一个面板上的功能区，升级成"无论你怎么和 ta 说，ta 都明白这是要做的事"的连续体验。

## 2026-05-03 — Iter Cθ：panel butler_tasks 调度 chip + 实时 "⏰ 到期" 标记
- 现状缺口：Cζ 加了 `[every: HH:MM]` / `[once: ...]` 调度前缀，proactive prompt 能算 due 并 ⏰ 标注。但 panel 上还是把整个前缀当普通字符串显示在 description 里——用户得自己读 `[every: 09:00]` 然后查表 09:00 是不是已经过了，再翻 updated_at 看有没有执行过。"我现在打开面板想看哪些任务到期了"是个高频需求，得让面板自己算。
- 解法：在 PanelMemory 里加 TS 版本的 schedule parser + due 检查，和 Rust 端 `parse_butler_schedule_prefix` / `is_butler_due` 严格同语义；butler_tasks 渲染时：
  - parse description 的前缀，把它从 description 文字里 strip 掉
  - 标题旁加 chip：`🔁 每天 HH:MM`（蓝）或 `📅 YYYY-MM-DD HH:MM`（琥珀）
  - 如果 due，再加一个红色 `⏰ 到期` chip（带 tooltip 解释含义）
  - description 文本只显示去掉前缀后的 topic（避免视觉重复）
- 客户端时钟而不是后端 Tauri command：
  - 优点：每次渲染都是当前时刻；无 IPC；无需维护额外接口；用户机器的时区/DST 自然正确
  - 缺点：客户端时钟不准（用户改系统时间）→ due 计算偏差。但 proactive 也用本地时钟，行为一致；这层是显示用而非决策用，可接受。
- 因为 panel 已经 15s 轮询 butler_history（Cε 的 setInterval），每 15s 会触发 React 重渲染，due 的状态自然每 15s 刷新一次——不需要专门的 due-poll。
- 只对 butler_tasks 类别 parse；其他类别 (`todo` / `user_profile` / `ai_insights` / `general`) 不付 parse 成本，TS optional chain 直接短路。
- TS 完全镜像 Rust 端的 every / once 语义：每日 every 任务的 most_recent_fire = `if now >= today HH:MM { today HH:MM } else { 昨天 HH:MM }`；once 任务 due 当且仅当 `now >= dt && lastUpdated < dt`。fail-open on bad updated_at（视为从未更新）。
- 不写前端单测：项目当前无 React 测试 harness（前文 Iter Cδ 已说明）。tsc 严格类型检查 + 与 Rust 单元测过的语义 1:1 镜像，是当前可达的最高保证。
- 副作用：description 区显示 topic 而不是 raw 前缀，这意味着用户写 `[every: 09:00] 写日报` 编辑时仍看到完整 `[every: 09:00] 写日报`，但只读视图上简洁化。"编辑"按钮把 raw description 传进模态，所以编辑往返不丢信息。
- 结果：用户打开 panel 一眼就知道哪些任务挂着调度、哪些此刻该被处理；不再需要精读 description。这是 panel 从"CRUD list"升级到"butler 实时仪表盘"的关键一步。

## 2026-05-03 — Iter Cη：butler_tasks 每日小结 + panel "每日小结" 区
- 现状缺口：Cε 的 butler_history.log 给了事件级流水（每次 update/delete 一条），但事件多了用户回看就累——"今天宠物到底为我做了哪些事"需要用户自己拼接 N 行。Cη 把"事件流→人类回看"补齐：consolidate 跑一次就 derive 一段"今天我帮你 推进了「X」「Y」，撤销/移除了「Z」"。
- `butler_history.rs` 加 3 个能力：
  - `summarize_events_for_date(events, date)` 纯函数：扫 butler_history 行，只看以 `<date>` 起头的（避免 description 里恰好包含日期字串误匹配），按 action 分桶 update/delete，每桶按出现顺序去重，输出 `今天我帮你 推进了「A」「B」，撤销/移除了「C」`，无事件返 None。
  - `record_daily_summary(date, summary)`：upsert 进 `butler_daily.log`（一行 `<YYYY-MM-DD> <summary>`），同日重写、跨日 append，cap 90 行（约一季度）。
  - `recent_summaries(n)` + Tauri command `get_butler_daily_summaries(n=7)`。
- 6 个新单测覆盖：空集合返 None、其他日期不算今日、单条 update、多 action 混合、同任务多次去重、严格按日期前缀过滤（不被 description 里的日期字串骗到）。测试总数 265 → 271。
- consolidate 钩子：`run_consolidation` 在 LLM 阶段之前 deterministic 算今天的 summary 并 upsert。这意味着即使 LLM 整理失败，今日小结依然写入；不依赖 LLM 也避免它幻觉/省略。
- `lib.rs` 注册 `get_butler_daily_summaries`。
- 前端 `PanelMemory.tsx`：
  - 新 state + loadButlerDaily()，挂载 + 15s 轮询（与 history poll 共用 interval，省 timer）
  - butler_tasks section 顶部加一块浅黄色 "每日小结 (N)"，每行 `<date>` + 摘要正文，最新在最上，区别于下面青色"最近执行"块的颜色
- 不污染 speech_history：曾权衡是否把摘要塞进 speech_history.log（TODO 原文确实如此），但 speech 计数会让 chatty_day_threshold 失真；改用独立 `butler_daily.log` 隔离，panel 上仍能看到。
- Consolidate 是天然触发点：用户手动"立即整理"或后台定时（默认 24h）跑一次，对一天一个摘要的频率刚好。
- 结果：用户在 Memory tab 看到三层 butler 信息：每日小结（人类语气 daily recap）→ 最近执行（事件流水 timestamp 级）→ 任务列表 + 到期标注。从"机器日志"过渡到"宠物日记"。

## 2026-05-03 — Iter Cζ：butler_tasks 调度前缀（[every]/[once]）+ 到期标注
- 现状缺口：Cγ–Cε 闭合了"委托 → 看到任务 → 执行后留痕"的 loop，但任务什么时候 *该* 被执行还完全靠 LLM 主观判断。"每天 9 点写日报"被 LLM 看到时已经 14:30 了——LLM 既不知道这件事现在就该做、也不知道这个早上有没有人做过。这是 Cζ 要解的。
- `proactive.rs` 新增 schedule layer：
  - `BulterSchedule` 枚举：`Every(h, m)` / `Once(NaiveDateTime)`
  - `parse_butler_schedule_prefix(desc)` 解析两种前缀：
    - `[every: HH:MM] topic` → daily recurring
    - `[once: YYYY-MM-DD HH:MM] topic` → single-fire
    - 拒绝非法时间（25:00 / 09:60）、空 topic、错误前缀（如 `[remind:`，是 reminder 用的）
  - `is_butler_due(schedule, now, last_updated)` 决定到期：
    - `Every`：先算 most_recent_fire = `if now >= today HH:MM { today HH:MM } else { today HH:MM - 1 day }`，再判 `last_updated < most_recent_fire`。这层逻辑保证：(a) 用户更新过任务（执行了）→ 自动暂停到下一个 fire；(b) 还没到今天的 fire 时间，看的是昨天的 fire，避免"半夜补一遍昨天的"。
    - `Once`：`now >= dt && last_updated < dt`。已过 + 未执行才到期；将来的不到期。
  - `parse_updated_at_local()` 把 `MemoryItem.updated_at`（RFC3339 带 offset）转 NaiveDateTime（local）。无法解析→视为"从未执行"，永远到期（fail-open，让 LLM 至少看到提醒）。
- `format_butler_tasks_block` 加 `now: NaiveDateTime` 参数：
  - 每条 item 用 schedule 前缀算 due
  - 到期 → 头 + "到期 → 最早委托" 排序，到期任务前缀 "⏰ 到期 · "；不到期 → 走原 oldest-first
  - header 在到期数 > 0 时改为 `共 N 条，其中 K 条到期，按到期 → 最早委托排在前`，否则保留原文
  - footer 加一句解释 `[every: HH:MM]` / `[once: ...]` 前缀和 ⏰ 标记的含义，让 LLM 知道这一轮看到 ⏰ 该优先做
- `build_butler_tasks_hint` 接 now，run_proactive_turn 传 `now_local.naive_local()`。
- 9 个新单测：parse 接受/拒绝（两种合法 + 五种非法）、is_butler_due Every 三种位置（昨天前/昨天后但今天前/今天后）、is_butler_due Once 四种（过/未/已执行/未来）、unparseable updated_at fail-open、format 把到期任务带 marker 顶上来、已执行今日 fire 的 every 任务 marker 消失。
- 测试总数 256 → 265。
- 配套提示更新：
  - `tools/memory_tools.rs` 的 memory_edit 描述加完整 schedule 前缀说明 + 例子，让 LLM 知道把"每天 9 点"翻成 `[every: 09:00]`。
  - `PanelMemory.tsx` 的 butler_tasks placeholder 改成包含三种示例：[every:] / [once:] / 不带前缀的"自由判断时机"。
- 关键设计：**不引入独立的 cron / 调度线程**。proactive 已经在跑（每 N 秒一 tick），到期检测就是 prompt 构造时的纯函数。零新基础设施、零并发风险，但代价是检测精度受 proactive interval 限制——默认 ~5 分钟，对"每天 9 点"足够（用户感觉不到 5 分钟漂移），对"每分钟"任务不行（也不打算支持那种）。
- 结果：用户现在可以委托真正按时间触发的任务。比如写下 `[every: 09:00] 把昨晚 git log 写一份摘要到 ~/yesterday.md`，明早 9 点过后第一次 proactive turn LLM 就会看到 `⏰ 到期`，按 prompt 指引调 `bash` / `write_file` 执行，update 后再下次 proactive 就不显示到期了。这是把宠物从"觉得合适就帮忙"推进到"真有时钟感的小管家"。

## 2026-05-03 — Iter Cε：butler_task 执行留痕 + panel "最近执行" 时间线
- 现状缺口：Cγ + Cδ 已经让用户可以委托任务、LLM 可以在 proactive turn 看到任务并尝试执行——但用户看不到任何"宠物刚做了什么"的反馈。即使 LLM 真的 update 了一个 butler_tasks 条目，那只是 description 字段变了，用户得手动找进去对比。Closing the loop 需要一个"事件流"。
- 解法：新建 `butler_history.log`，每次 LLM 通过 memory_edit_impl 接触 butler_tasks（update / delete）就记一行。create 不记——那是 *委托*，不是 *执行*；记进来会冲淡信号。
- `src-tauri/src/butler_history.rs` 新模块（参 speech_history 模式）：
  - 文件 `~/.config/pet/butler_history.log`，每行 `<ts> <action> <title> :: <desc-snippet>`
  - 200 行硬上限 + 100K 字节 rotation（与 speech_history 同思路）
  - `format_event_body(action, title, description)` 是 pure helper：trim、flatten 换行、80 字符截断 + `…`
  - `parse_recent(content, n)` 同样 pure
  - Tauri command `get_butler_history(n: Option<usize>)`
- 8 个新单测覆盖：短描述原样、长描述截断、换行 flatten、trim、空内容、tail order、不足 n 时返回全部、跳空行。
- 钩子：`tools/memory_tools.rs::memory_edit_impl` 在 successful edit 后判断 `category == "butler_tasks" && action ∈ {update, delete}`，是则调 `butler_history::record_event`。description 在 move 进 memory_edit 之前 clone 一份给日志用——不能在调用后再读。
- `lib.rs` 加 `mod butler_history;` + register `get_butler_history` 命令。
- 前端 `PanelMemory.tsx`：
  - 加 `butlerHistory: string[]` state，挂载时拉 + 每 15 秒轮询一次（butler 事件来自 LLM 调工具，分钟级粒度，15 秒轮询便宜且能给"刚执行完"的及时反馈）
  - handleSaveEdit / handleDelete 在涉及 butler_tasks 时立刻刷新一次（不等 15 秒）
  - 在 butler_tasks section 顶部（标题下、items 前）渲染一个 "最近执行 (N)" 浅蓝色块，每行显示 `时间 · action · 标题 :: 描述`：
    - `update` 标 teal `#0d9488`、`delete` 标红 `#dc2626`，颜色提示语义
    - 描述 ellipsis + tooltip 完整文本
    - reverse 排显示，最新在最上
  - parseButlerLine 是 inline pure helper，handle 格式不规范的 fallback
- 测试总数 248 → 256（butler_history 自带 8 个 unit）。tsc 干净。
- 重要细节：butler_history 不被 redact——它是用户面向的执行历史，需要看到原文判断"宠物到底做对了没"；redaction 只用于"prompt 里的 outbound 文本"。
- 结果：用户在 Memory tab 能看到"管家任务"区域顶部一个时间线块——LLM 每次推进任务都会立刻反映出来。本次 iter 没有让 LLM 真去调用 read_file/bash 之类（那会涉及具体执行 path 的复杂判断），但只要 LLM 按 proactive prompt 的指引在执行后 update 一次 butler_tasks，时间线就会出现新条目。下一步 Cζ 的 schedule 触发器会让这条线更密集——"每天 9 点"类任务会在固定时间产生事件。

## 2026-05-03 — Iter Cδ：panel 添加"委托任务"快捷入口 + 分类 placeholder
- 现状缺口：Iter Cγ 已经做了 butler_tasks 类别，但用户从 panel 入手要先点 Memory tab，再
  滚到 butler_tasks 区域，再点 "+ 新建"——三步才能加任务。而且打开模态后描述框是空的，
  用户不知道该写什么格式。新方向是宠物管家，加任务的路径应该是一等公民。
- 解法：
  - 在 Memory tab 顶部的搜索行加一个显眼的 **"+ 委托任务"**蓝色按钮（与现有"立即整理"
    紫色按钮并列）。点击直接打开新建模态、分类预设为 butler_tasks。从"找到分区→新建"
    三步压缩到一步。
  - 新建/编辑模态的描述 textarea 加 `placeholder`，根据当前所选分类显示对应示例：
    - `butler_tasks`: "比如：每天 9 点把今日日历汇总写到 ~/today.md / 周末整理 ~/Downloads…"
      （加一句解释"宠物会在 proactive 主动开口时尝试执行"）
    - `todo`: "用户提醒自己的事项。建议加前缀：[remind: 17:00] 喝水…"
    - `user_profile`: "关于用户习惯/偏好的稳定事实。比如：起床时间…"
    - `ai_insights`: "通常由 LLM 自己写。current_mood / persona_summary 受保护。"
    - `general`: "其他不属于以上类别的记忆。"
  - 选 butler_tasks 时把 textarea minHeight 从 60px 升到 100px——任务描述天然比单点
    reminder 长，给更舒服的输入空间。
- 5 行 dict + 1 个新按钮 + 2 行模态 props 改动。无后端改动。
- 既不破坏现有"per-category + 新建"按钮（用户在某个分区下面点"+新建"还是只能加那一类
  的记忆），也加了"top-level fast lane"给 butler_tasks。
- tsc 干净；前端无单测体系所以本次只做视觉与交互层改动，不增 cargo 数。
- 结果：panel 头部直接看到一个蓝色"+ 委托任务"，新增 butler_tasks 是单击操作；用户
  打开模态就能从 placeholder 里学到格式约定。Cε（执行留痕）现在可以接续——一旦用户愿
  意通过这个入口加任务，下一个 iter 就要让 LLM 真的能 close the loop。

## 2026-05-03 — Iter Cγ：butler_tasks 类别 + 宠物管家方向首切
- 用户给出新方向：放弃跨设备同步（已从 TODO/STATUS 删除），转向 "宠物管家" — 让宠物执行用户委托的实际工作。这是 Iter Cγ 的起点。
- 新增 `butler_tasks` 记忆类别，与 `ai_insights / user_profile / todo / general` 并列：
  - `commands/memory.rs` 默认类别加 `butler_tasks: 管家任务`。
  - `tools/memory_tools.rs` 三处 enum + 描述更新：memory_list 描述列出五个类别，memory_edit 的 enum 加 `butler_tasks`，并加一段 LLM 指引——"butler_tasks 是用户委托给你做的事，不要和 todo（用户提醒自己）混淆"。
- `proactive.rs` 新增 `build_butler_tasks_hint()` + 纯函数 `format_butler_tasks_block(items, max_items, max_chars)`：
  - 读 `butler_tasks` 类别条目；空则返 ""
  - **按 `updated_at` 升序**（与 user_profile_hint 相反——任务是 backlog，最久没动的应该最先看到，不能让任务自然 rot 到底部）
  - 取前 6 条；description 超 100 字符截断
  - 块尾 footer 提示 LLM 完成后用 `memory_edit update` 记录、撤销用 `delete`——把"如何 retire 任务"塞进 prompt 让 LLM 不必猜约定
  - 输出过 `redact_with_settings`
- `PromptInputs` 加 `butler_tasks_hint: &'a str`，prompt builder 在 `plan_hint` 之后 push（保留时间顺序：先看用户给我的提醒、再看我自己的计划、再看用户委托的任务）。
- 在 proactive_rules 加一条 **conditional rule**：仅当 `butler_tasks_hint` 非空时 push一句"你也是用户的小管家——可以调 read_file / write_file / edit_file / bash 真去执行任务"。提示 LLM file/bash 工具在 butler 路径里是合法的（之前的 always-fired 规则只列了 env tools + memory_search）。
- 不进 active_prompt_rules 标签系统：butler-task 触发是"有任务就提"的开关式，不属于 restraint/engagement/corrective/instructional 任一 nature——加进规则面板会污染倾向统计。先做 ambient hint + 局部 rule，未来观察使用情况再决定是否升格为有 nature 的规则。
- `consolidate.rs` prompt 第 2 条扫除规则补充："butler_tasks 类下用户已经撤回 / 已完成且不再 recurring，归过期/失效"——让定时整理也覆盖这个类别。
- 前端 `PanelMemory.tsx` CATEGORY_ORDER 调整为 `[butler_tasks, todo, ai_insights, user_profile, general]`，让"用户委托"和"提醒"两个 actionable 类别置顶；以前的纯展示类下沉。
- 7 个新单测：prompt 注入 / 省略、空列表 / 0-cap、按 updated_at 升序（相对 user_profile 的降序）、cap+footer 校验、长描述截断含 `…`。
- 1 处既有 prompt 模板修改通过 hint-conditional 化避免破坏 `prompt_omits_butler_tasks_hint_when_empty`——基础输入里 hint=空，规则就不 push，prompt 不含 "管家"。
- 测试总数 241 → 248。
- 结果：宠物现在有了一个区分于 todo 的"我的工作清单"。LLM 收到 prompt 时看到这段管家任务列表 + 知道可以用 file/bash 真执行。后续 Iter 可以接：(1) 触发器（"每天 8 点跑一次某任务"）、(2) 自动汇报（执行结果直接进 speech_history）、(3) 用户在 panel 上直接 add/edit 任务的 UI、(4) "刚执行完任务"的 motion / 心情反馈。

## 2026-05-03 — Iter Cβ：proactive prompt 加 weekday/weekend awareness
- 现状缺口：proactive prompt 里有 time + period（清晨/上午/.../深夜）但没有 weekday vs weekend 区分。LLM 看到 `2026-05-03 14:30（下午）` 要自己反推今天是周几——某些模型版本对日期算术不可靠，且即使算对了也不会在语气上区分"周五晚上 vs 周一上午"。
- 解法：在 time 行 inline 加一个 `周X · 工作日/周末` 标签。一行字 + 一个枚举，零额外分支，零成本，但能给 LLM 一个清晰的语气切换信号——"周五晚上别再写代码"、"周末早上要不要慢点起"这类话题就有触发面。
- `proactive.rs` 加三个 pure helper：
  - `weekday_zh(Weekday) -> &'static str`：Mon..Sun → 周一..周日
  - `weekday_kind_zh(Weekday) -> &'static str`：Sat/Sun → "周末"，其余 → "工作日"
  - `format_day_of_week_hint(Weekday) -> String`：返回 `周日 · 周末` 这种合并格式
  - 拆三个而不是合一个：weekday_zh 和 weekday_kind_zh 都可能将来单独被引用（panel ToneStrip / UI hint），合并函数只负责 `·` 拼接逻辑。
- `PromptInputs` 加 `day_of_week: &'a str` 字段。`build_proactive_prompt` 的 time 行从 `现在是 X（period）。...` 改为 `现在是 X（period，weekday · kind）。...`。
- `run_proactive_turn` 调 `format_day_of_week_hint(now_local.weekday())` 后传入。
- 4 个新单测：weekday_zh 7 个分支、weekday_kind_zh 周末 vs 工作日、format_day_of_week_hint 合并格式、prompt 包含 day_of_week 在 time 行的正确位置。
- 测试总数 237 → 241。
- 既有测试保持稳定：`base_inputs` 默认 `day_of_week = "周日 · 周末"`（与 time = "2026-05-03 14:30" 是周日一致），所有断言 `p.contains("下午")` 仍命中（"下午" 现在出现在 `（下午，周日 · 周末）` 里）。
- 结果：proactive prompt 现在告诉 LLM 周X 和 是否周末，不必从日期反推。一个 prompt 行的小改造，把"今天是哪种日子"的语气基线明确化。

## 2026-05-03 — Iter Cα：user_profile 摘要注入 proactive prompt
- 现状缺口：`user_profile` memory 类别只通过 `memory_search` 工具暴露给 LLM，每次主动开口要花一次 tool call 才能拿到"用户喜欢什么 / 几点起床"这种基础信息。env-tool 计数显示 memory_search 调用率不到 1/3，多数时候 LLM 直接凭空起话题，与"伙伴感"目标背离。
- 解法：把 user_profile 摘要做成 prompt 里的 ambient block，跟 `persona_hint` / `mood_trend_hint` 同级——LLM 不调工具就能看到。
- `proactive.rs` 新增 `build_user_profile_hint()` + 纯函数 `format_user_profile_block(items, max_items, max_chars)`：
  - 读 `user_profile` 类别条目；空则返 ""（`push_if_nonempty` 跳过）
  - 按 `updated_at` 降序排（ISO-8601 字符串可直接 lex sort）
  - 取前 6 条；每条 description 超过 80 字符截断 + `…` 后缀
  - 整段过 `redact_with_settings`——LLM 写进 user_profile 的内容可能含私人信息（"liang 在 cobo 上班"），不能原样回流到下一轮 LLM 输入
- `PromptInputs` 加 `user_profile_hint: &'a str`，`build_proactive_prompt` 在 `mood_trend_hint` 之后 push（同样跳过空串）。
- 常量化 `USER_PROFILE_HINT_MAX_ITEMS=6` / `USER_PROFILE_HINT_DESC_CHARS=80`，便于将来调；与 `LONG_IDLE_MINUTES` 等同级。
- 8 个新单测：prompt 注入 / 省略、空列表 / 0-cap 返空、按 updated_at 降序、超 cap 取最新、长描述截断含 `…`、短描述不截断。所有 `format_user_profile_block` 测试都纯——不读盘、不依赖 Tauri state。
- 拆 pure helper 的动机：`build_user_profile_hint` 走 `memory_list`（要 ~/.config 路径），无法在 unit test 里干净测；提取出来的 format_helper 只接 `(title, description, updated_at)` 元组，所有排序/截断逻辑都覆盖到了。
- 测试总数 229 → 237。
- 结果：proactive prompt 在 user_profile 非空时多一段约 7 行的 ambient context（header + 最多 6 条 bullet）。和 `persona_hint` / `mood_trend_hint` 一起形成"宠物认识自己 + 认识用户 + 认识自己情绪走向"三轴长期画像，全都不需要 LLM 主动调工具。

## 2026-05-03 — Iter Cv：redaction 计数 + panel "Redact M/N" chip
- 在 `redaction.rs` 加两个 process-wide static atomic：`REDACTION_CALLS` / `REDACTION_HITS`。`redact_with_settings` 每次调用 fetch_add CALLS；当 `output != input`（即至少一个 pattern 命中并替换了内容）fetch_add HITS。
- 静态而非 ProcessCounters：`redact_with_settings` 在 sync 路径被多处调用（`inject_mood_note`、`build_persona_hint` 等）这些位置没有 Tauri AppHandle / ProcessCountersStore 访问。静态 atomic 让任何代码路径都能 bump，零 wiring。
- 新 Tauri commands `get_redaction_stats / reset_redaction_stats`，返 `RedactionStats { calls, hits }`。
- 前端 `panelTypes.ts` 加 RedactionStats interface；PanelDebug fetch + state + 重置 handler。PanelChipStrip 在 prompt-tilt chip 之后插入"Redact M/N (X%)" chip：
  - hits > 0 → 青色 `#0d9488`（隐私过滤在干活）
  - hits = 0 但 calls > 0 → 灰 `#94a3b8`（filter 配置但没东西匹配，可能是 patterns 太严或没 leak 内容）
  - calls = 0 → chip 不渲染（与其他 chip 一致）
- tooltip 解释 calls vs hits 含义 + 调试方向（hits 突变可作 patterns 过松/过紧的反馈）。
- 1 个新单测：RedactionStats serde 序列化包含 `calls` / `hits` 字段。两个 static atomic 的实际计数行为不写测试——多 test 同进程会互相 perturb，且行为是 trivial fetch_add；通过 redact_text / redact_regex 已有 14 个单测保障核心逻辑。
- 现在 panel 工具栏 7 个 chip 全部 wired：Cache / Tag / LLM沉默 / 环境感知 / 倾向 / Redact / prompt hints。隐私过滤从"配置后看不出有没有用"变成"看到 N/M 数字才信任过滤生效"。
- 229 cargo tests + tsc 全过；零 warning。

## 2026-05-03 — Iter Cz：redaction 加正则模式（信用卡 / 邮箱 / 任意结构化敏感词）
- 加 `regex = "1"` 依赖（RE2-style 引擎，线性时间，无 backreference→天然 ReDoS 安全）。
- `PrivacyConfig` 加 `regex_patterns: Vec<String>` 字段（serde default 空 Vec），与既有 `redaction_patterns`（子串）并列。
- 新纯函数 `redaction::redact_regex(text, patterns) -> String`：每条 pattern 编译一次（Regex::new），失败的 silently skip——一个 pattern 写错不会让整个过滤失效。空 pattern trim 后被跳过。
- `redact_with_settings` 改成两阶段 pipeline：先子串、再正则。顺序刻意——子串通常更具体（命名词），正则抓结构（信用卡 / 邮箱）；先做具体再做泛化让 marker 顺序自然。
- 5 个注入通道（Iter Cx + Cy + Cw 累加的 active_window / calendar / mood note / speech_history / persona_summary）现在自动获得正则覆盖——无新 callsite 改动，因为它们都走同一个 `redact_with_settings` helper。
- 7 个新单测：empty / blank / 邮箱模式 / 信用卡模式 / 非法 pattern silently skipped 但其他 pattern 仍生效 / 多 pattern 顺序 / 中文支持。
- 前端 `PrivacyConfig` interface + 默认值同步加 `regex_patterns: []`。`PanelSettings.tsx` 隐私过滤区扩为两个 textarea：子串 + 正则，每个独立 update（注意保留另一字段不被覆盖）。footer 文案更新为 "覆盖 5 个 prompt 注入通道；子串先于正则；Rust regex 线性时间，无反向引用——天然 ReDoS 安全"。
- 228 cargo tests + tsc 全过；零 warning。
- 路线 C v2：本地子串过滤（人/项目/公司命名词） + 正则结构化过滤（卡号/邮箱/电话/任意 pattern）= 完整可配置隐私层。

## 2026-05-03 — Iter Cw：redaction 扩展到 persona_summary 自循环入口
- `proactive::build_persona_hint`：把 `item.description.trim()` 在格式化进 prompt 前用 `redact_with_settings` 过一遍。这是 self-loop 入口的最后一处——LLM 自己写 persona_summary 时不会主动 redact，但用户标记的私人词应当在每次注入 prompt 时被覆盖。
- `get_persona_summary` Tauri command（panel 的人格 tab 用）**不**走 redaction：那是本地 panel 显示，用户看到原文是合理的；redaction 只在"对外发到 LLM"那一刻应用。注释里写明这个语义不对称的设计选择。
- 路线 C 的注入通道现在 5 个全部覆盖：active_window 工具 / calendar 工具 / mood note / speech_history hint / persona_summary hint。剩余 prompt 注入字段（companionship line / mood_trend hint / cadence_hint / wake_hint 等）都是 backend 自己生成的固定文案 + 数字，无 leak 通道，不需要 redact。
- 这一刀是路线 C 的"完整闭环 v1"——再加新通道时记得也加 redact_with_settings 即可，pattern 已稳定。
- 221 cargo tests + tsc 全过；零 warning。

## 2026-05-03 — Iter Cy：redaction 扩展到 mood note 和 speech_history 注入
- 新公共 helper `redaction::redact_with_settings(text) -> String`：sync wrapper，每次调用读 settings.privacy.redaction_patterns（fallback 空 list），套 redact_text。Iter Cx 的工具入口本来手动展开 settings 读取，现在抽成一行 helper——两处调用都简化。
- `commands::chat::inject_mood_note`：mood text 在格式化进 system message 前用 redact_with_settings 过一遍。这关键——mood 是 LLM 自己之前写的，可能含 active_window 漏过来的人名 / 项目名；不 redact 的话每次对话都 re-leak。
- `proactive::run_proactive_turn` 的 speech_hint 构造：每条 strip_timestamp 后 redact 再 join。speech_history 文件本身保持原文（不破坏"宠物实际说过什么"的纪录），但每次重新注入 prompt 时新设的 patterns 会自动应用——用户改 patterns 后过往的 leak 都能在下次 prompt 里被覆盖。
- 路线 C 的覆盖范围现在是: env 工具入口（active_window / calendar - Iter Cx）+ self-loop 入口（mood note / speech_history - Iter Cy）= 4 个 prompt 注入路径。 LLM 看不到也学不会用户标记的私人词。
- 设计哲学："文件原文保留 + 读时 redact" 而非"写时 redact"——保持持久化数据完整可恢复，redaction 只是"对外发送时"的过滤层。用户调整 patterns 立刻全局生效。
- 221 cargo tests + tsc 全过；零 warning。

## 2026-05-03 — Iter Cx：隐私过滤——env 工具结果可配置 redaction（路线 C 第一刀）
- 新模块 `src-tauri/src/redaction.rs`：纯函数 `redact_text(text, patterns) -> String`，对 patterns 中的每条做大小写不敏感子串匹配，命中处替换为 `(私人)`。空 / whitespace-only patterns 被跳过避免空串无限循环陷阱。UTF-8 安全（中文 / emoji）通过 char_boundary 推进实现。
- `replace_case_insensitive` 用 lowercase 镜像扫描而非 regex——零依赖，无 ReDoS 风险，对子串场景足够。
- `commands/settings.rs` 加 `PrivacyConfig { redaction_patterns: Vec<String> }`（serde default 空 Vec），并入 `AppSettings.privacy` 字段。
- `tools/system_tools.rs` 的 `get_active_window_impl` 在构造 JSON 前对 `app_name` 和 `window_title` 都套 redact_text（读 settings 拿 patterns）。`tools/calendar_tool.rs` 的 `get_upcoming_events_impl` 对 event title + location 套相同处理。
- 前端 `useSettings.ts` 新 `PrivacyConfig` interface + DEFAULT_SETTINGS 同步加。`PanelSettings.tsx` 在"对话上下文"之后插入"隐私过滤"section：textarea 一行一个 keyword，placeholder 例 "Slack / 某客户公司名 / 项目代号"，footer 解释作用范围 + 即时生效。
- 8 个新单测覆盖：empty/blank patterns 跳过 / 大小写不敏感 / 多 patterns 顺序 / 中文 / emoji 安全 / 重叠 patterns 优先匹配 / 多次出现全部替换。
- 现在用户首次能让宠物在不可信环境（active window 标题 / calendar event）面前对私人信息保持沉默——LLM 只看到 `(私人)` 占位，本机不外发。路线 C 起步。
- 221 cargo tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 105：panel 人格 tab——把长期身份层 surface 给用户
- 3 个新 Tauri command 暴露 prompt 注入用的长期身份数据：
  - `companionship::get_install_date()` → "YYYY-MM-DD" 字符串（reuse ensure_install_date）
  - `proactive::get_persona_summary()` → ai_insights/persona_summary description（无 header 包装，原始文本）
  - `mood_history::get_mood_trend_hint()` → 同 proactive prompt 用的格式化 trend hint
  + 已存在的 `companionship::get_companionship_days`
- 新组件 `src/components/panel/PanelPersona.tsx`：3 个 Section 卡（陪伴时长 / 自我画像 / 心情谱）+ footer 解释这些数据怎么进入 prompt。
  - 陪伴时长：44px 青色大数字 + "起始 2026-05-03" 起始日期补充
  - 自我画像：persona_summary description 用 `whiteSpace: "pre-wrap"` 保留换行；空时 italic 灰提示"开口几次后等下一次 consolidate"
  - 心情谱：mood_trend_hint 全文显示；不足 5 条时 italic 灰提示"数据不足"
  - footer 一段 11px 灰字解释"这三层信息会被注入 proactive / desktop chat / Telegram 的 system prompt"，让用户知道这不只是装饰，而是 prompt 真正读到的数据
- `PanelApp` 加 "人格" tab 在"记忆"之后；activeTab 添加新分支渲染 PanelPersona
- 5 秒间隔轻量 polling（vs PanelDebug 1 秒），因为这些数据变化频率低（consolidate 周期 / mood 转变都不是秒级）
- 路线 A 现在三层数据全部对用户可见：proactive / chat / Telegram prompt 层（LLM 看到）+ stats card 单 chip + 完整 Persona tab（用户看到）。从输入到输出全链路的可见闭环。
- 213 cargo tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 107：Telegram 路径也注入长期人格层（带 opt-out 开关）
- `TelegramConfig` 加 `persona_layer_enabled: bool` 字段（serde default = true）。改成手写 Default impl 因为多了一个非默认 false 的字段。
- `HandlerState` 加 `persona_layer_enabled` 字段：bot 启动时从 config 抓取一次，运行期不重新读 settings——和 bot_token / allowed_username 同生命周期，需要重启 bot 才生效，符合 telegram bot 一贯模式。
- bot 的 `handle_message` 在 `inject_mood_note` 之后链式调 `inject_persona_layer(chat_messages).await` 当 enabled，保持与 desktop chat 完全一致的 system note 形态。
- frontend `useSettings.ts` 的 TelegramConfig interface + DEFAULT_SETTINGS 同步加字段 `persona_layer_enabled: true`；`PanelSettings.tsx` 在 Telegram 区块的"允许的用户名"输入框下方加 checkbox "注入长期人格层（陪伴天数 + 自我画像 + 心情谱）"，手动展开 setForm 与现有 telegram 字段更新模式保持一致。
- 路线 A 的人格层覆盖现在三路：proactive prompt（Iter 101-103）+ desktop chat（Iter 104）+ Telegram chat（Iter 107），三处共享同一份 build_persona_layer_async 实现。Telegram 是唯一带 opt-out 的——desktop 上当前永远开启（如未来用户反馈再决定要不要也加 toggle）。
- 213 cargo tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 106：panel stats 卡加"陪伴 N 天"指示
- 新 Tauri command `companionship::get_companionship_days() -> u64`：薄封装现有 `companionship_days()`，调用即首次会触发 install_date.txt 自动初始化（zero-config）。注册到 invoke handler。
- 前端 PanelDebug fetchLogs 的 Promise.all 加 `invoke("get_companionship_days")`；新 state `companionshipDays` 透传到 PanelStatsCard。
- PanelStatsCard 新 prop `companionshipDays: number`：在累计行后插入第三块（左侧 1px 分隔线 + 16px 青色 mono 数字 + 副标）。文案根据 day 0 / day N 切换：
  - 0 → "天（今天初识）"
  - N ≥ 1 → "天陪伴"
- 视觉层级：lifetime 28px 紫色（主），today 20px 蓝色（次），companionship 16px 青色（背景上下文）。三个数字依重要性递减、依颜色区分语义——避免新数字喧宾夺主，但又比 chip strip 里的小指标更显眼。
- tooltip 解释 "自首次启动起算" + 持久化文件位置，让用户知道这个数字怎么来的。
- 路线 A 现在三层 prompt 注入（companionship_days / persona / mood_trend）+ 用户面板可见 panel chip = 数据从输入到输出的闭环。
- 213 cargo tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 104：路线 A 三层信息也注入反应式 chat 的 system prompt
- `commands::chat` 新增 3 个公共函数：`format_persona_layer(days, persona, trend) -> String`（pure，可测）/ `build_persona_layer_async()`（pulls companionship_days + persona_summary + mood_trend from disk）/ `inject_persona_layer(messages)`（async，按 inject_mood_note 同样的 "before first non-system message" 规则插入）。
- 反应式 chat handler `chat()` 在 inject_mood_note 之后链式调 inject_persona_layer，让用户主动来聊时也看到完整长期人格背景。
- 持久层 system note 形如：`[宠物的长期人格画像]\n\n{companionship_line}\n\n{persona}\n\n{mood_trend}\n\n——这些是你的长期身份背景。回复用户时让它们自然渗进语气，不必生硬复述这些内容。` 其中 persona 和 trend 块仅在非空时插入。
- `proactive::build_persona_hint` 升 pub（被 chat 复用）。整个 layer 只有 companionship 强制存在（day 0 也有 framing），其余按需。
- 5 个新单测覆盖：day 0 含"第一天"+ tail guidance / 仅 persona 时不出 trend / 仅 trend 时不出 persona / 三者全在时出现顺序锁定（companionship → persona → trend，与 proactive 顺序一致）/ whitespace-only 当空处理只有 3 块。
- 现在路线 A 三层信息（companionship / persona / mood_trend）覆盖 proactive + reactive 两条路径——宠物的长期身份在被动响应和主动开口都成立。这是把 Iter 101-103 的基础设施真正"绑在"用户互动上的关键步骤。
- Telegram bot 也通过 `run_chat_pipeline` 共享相同的人格层（如果将来要选择性禁用，加 settings flag 即可）。
- 213 cargo tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 103：mood_history.log + 长期情绪谱注入 prompt（路线 A 第三步）
- 新模块 `src-tauri/src/mood_history.rs`：append-only 日志，cap 200 行 / 200KB rotation。每行格式 `<ISO ts> <motion> | <text>` 用 ` | ` 分隔保证 motion / text 解析无歧义即使 text 含管道符。
- 写入时去重：read 文件最后一行，若 motion+text 与新条目完全一致则 skip——让 history 反映"心情转变"而非每次 proactive 都记一条同样的 Idle/平静。
- 4 个 pure helper：parse_motion_text（含 `|` corner case）、summarize_recent_motions（按次数降序 + 字母 tiebreak）、format_trend_hint（min_entries 防早期噪声 + 过滤 "-" 无标签 entry，全空则返 None）、build_trend_hint async wrapper。
- 9 个新单测覆盖：parse 4 种形式 / summarize 计数+排序+窗口 / format 阈值/排序/过滤 dash/全 dash 返 None。
- proactive run_proactive_turn：read_mood_for_event 之后 `mood_history::record_mood(text, motion)` 异步追加；fetch `build_trend_hint(50, 5)` 透传到 PromptInputs。
- `PromptInputs` 加 `mood_trend_hint: &'a str`（默认空），`build_proactive_prompt` 在 `persona_hint` 之后 push_if_nonempty——位置：自我状态 / 关系时长 / 自我反思 / 长期情绪谱 / 上下文，递进合理。
- 4 个新 prompt 测试：set 时含"长期的情绪谱"和具体 motion 计数 / 空时不出。
- 路线 A 三步全部完成：companionship_days（"我和你认识 N 天"）+ persona_summary（"我看到自己是这样的人格"）+ mood_trend（"我最近的情绪谱是这样"）= 长期人格演化基础设施。新装的宠物 prompt 简短清爽，长期使用的宠物 prompt 自带历史厚度。
- 208 cargo tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 102：persona_summary 自反思 + 注入 proactive prompt
- consolidate 流程在主 prompt 里新增第 5 项任务：让 LLM 基于"最近 30 句主动开口" + user_profile 条目，写一段 ~100 字的第一人称自我画像（"我倾向 ..."），通过 `memory_edit create / update` 写到 `ai_insights/persona_summary`。最近开口 < 5 句时跳过（信号不足）。
- consolidate prompt 现在前置 `recent_speech_block`：把 speech_history 最近 30 行 strip timestamp 后 bullet-list 进 prompt。空时显示"跳过 persona_summary 维护"提示，让 LLM 不要硬编。
- 特殊保护清单从 1 条扩展为 2 条：current_mood + persona_summary，都不允许 delete，可 update。
- proactive 侧新增 `build_persona_hint()` 读 `ai_insights/persona_summary` description，非空时格式化成 "你最近一次自我反思的画像（来自 consolidate）：\n{description}"。
- `PromptInputs` 加 `persona_hint: &'a str`（默认空）；`build_proactive_prompt` 在 companionship 行之后 push_if_nonempty——位置：自我状态 → 时间维度 → 自我反思 → 上下文 hints，叙事顺序合理。
- run_proactive_turn 调 `build_persona_hint()` 透传。
- 2 个新 prompt 测试：set 时含 "自我反思的画像" 和具体内容 / 空时不出现该 header。
- 路线 A 第二步完成：现在宠物有"我和用户走过 N 天"（Iter 101）+ "我观察到自己的语气是 X 这样"（Iter 102）双层人格信息源；下次 proactive 二者并入 prompt，让"使用一年的宠物"和"刚装上的宠物"在语气、自我认知层面都有可观测差别。
- 196 cargo tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 101：陪伴天数注入 prompt（路线 A 入口）
- 新模块 `src-tauri/src/companionship.rs`：
  - `install_date_path()` → `~/.config/pet/install_date.txt`
  - `parse_install_date(content)` 纯函数解 YYYY-MM-DD 首行（容忍后续 comment）
  - `days_between(install, today)` 纯函数算天数差，负数 clamp 到 0
  - `ensure_install_date()` async：读文件 → 解析；缺失/损坏即写今天并返今天
  - `companionship_days()` async：days_between(ensure, today)
- 5 个新单测覆盖 parser valid / 带 comment / malformed / 同日 0 / 正向计数 / 时钟回退 clamp。
- `PromptInputs` 加 `companionship_days: u64` 字段，base_inputs 默认 30（既不是 0 也不是漫长，让既有 prompt 测试不受新文本影响）。
- 新纯函数 `format_companionship_line(days) -> String`：
  - day 0 → "你和用户今天才正式认识，是你陪伴 ta 的第一天——语气可以保留一点点初识的客气感。"
  - day N → "你和用户已经一起走过 N 天——可以让这份相处时长自然渗进语气，比如对 ta 偏好的预判、共同回忆的暗指（不必硬塞，时机对就用）。"
- `build_proactive_prompt` 在 `mood_hint` 之后插入 companionship 行——位置在情绪状态之后、上下文 hint 之前，符合"我是谁 → 我和用户什么关系 → 当下情况"的叙事顺序。
- `run_proactive_turn` 调 `crate::companionship::companionship_days().await` 透传——首次 proactive turn 即触发 install_date.txt 写入（zero-config）。
- 4 个新 prompt 测试：day 0 用第一天措辞 / day N 状数 / day 7 出现在 prompt / day 0 prompt 含"第一天"。
- 路线 A 入口完成：宠物现在知道"它和用户认识了多久"，是"使用一年的宠物" vs "刚装上的宠物"语气分化的最低基础设施。Iter 102 在此基础上让 LLM 自我反思生成性格摘要。
- 194 cargo tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 100：里程碑盘点 STATUS.md
- 新建 `STATUS.md`：以"实时陪伴 AI 桌面宠物"原始目标为锚，对照 IDEA.md 起点列
  的 5 条差距逐项核对，标记现状闭合度（① 主动发言 ✓ / ② 环境感知 大部分 ✓ /
  ③ 情绪演化 ✓ / ④ 节奏控制 体系化 ✓ / ⑤ 记忆系统 ✓ + 强化）。
- 列出"起点没有但浮现的能力"：prompt 自我画像、data→prompt 反馈环、复合规则、三层
  守护测试、panel 模块化拆分。
- 量化体量：14k 行代码、184 单测、~40 Tauri commands、5 组 atomic counters、9 类
  持久化文件。
- 标记仍有的空白：Live2D 表情薄、多窗口 panel 数据轮询独立、无跨设备、缺隐私
  filter、缺长期人格演化、macOS 通知 hook 还 deferred。
- 给出未来路线粗排：A 长期人格演化 / B 表情系统升级 / C 隐私 filter / D 记忆
  surface / E 跨设备同步——按价值密度排序，A 最优先因为它把已有 infra 全部串起来。
- 诚实评估"是真实伙伴吗"：技术上 5 条差距已闭合，体感上还差人格深度 + 表情丰富度，
  下一阶段 99 次最值得投 A（人格演化）。
- 这个 iter 不写代码、不加测试，纯文档盘点；让累积 99 个微观迭代后的项目方向感重新
  对齐。后续 TODO 也据此重排——把人格演化（A 路线）作为 Iter 101 的入口。

## 2026-05-03 — Iter 99：再拆出 PanelStatsCard + PanelToneStrip
- 新文件 `src/components/panel/PanelStatsCard.tsx`：封装 lifetime + today 大数字 + 克制模式 / 破冰阶段 badge。props 仅 3 个（todaySpeechCount / lifetimeSpeechCount / tone），逻辑（restraining 派生 + 颜色切换 + 文案分支）全部内化。
- 新文件 `src/components/panel/PanelToneStrip.tsx`：封装 tone snapshot 一行 chip strip（period / cadence / wake / pre-quiet / 破冰 / mood / motion）。tone null 时直接 return null，外层无需 conditional render。
- PanelDebug.tsx：~120 行 inline JSX 被替换成 7 行 `<PanelStatsCard {...} />` + `<PanelToneStrip tone={tone} />`。
- 现在 panel 子组件三件套：PanelChipStrip（数据 chip 行）/ PanelStatsCard（大数字卡）/ PanelToneStrip（tone 信号条）。每个组件单一职责：纯 presentation，依赖 panelTypes 的类型契约。
- PanelDebug 现在 569 行（从 Iter 98 的 ~590 进一步压缩），剩下 toolbar、handlers、prompt-hints expansion、decisions、recent-speeches、reminders、logs view 等本就难以拆分（与父组件 state 高耦合）的部分。
- 184 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 98：抽 panelTypes.ts，PanelDebug 只剩 state + layout
- 新文件 `src/components/panel/panelTypes.ts`：搬入 8 个 interface（CacheStats / ProactiveDecision / MoodTagStats / LlmOutcomeStats / EnvToolStats / PromptTiltStats / PendingReminder / ToneSnapshot）+ `PromptRuleNature` type + `PROMPT_RULE_DESCRIPTIONS` + `NATURE_META` 字典。共 ~150 行的 type/data 定义集中一处。
- PanelDebug.tsx 顶部把 8 个 interface 块替换成单个 `import { ... } from "./panelTypes"`，去掉 ~62 行类型定义和 ~80 行字典定义。文件从 ~770 行降到 ~590 行，纯粹只剩 useState + fetchLogs + JSX layout。
- PanelChipStrip.tsx 的 import 从 `./PanelDebug` 改到 `./panelTypes`——不再循环依赖父子组件。
- cargo `parse_prompt_rule_dict_keys` parser 路径从 `PanelDebug.tsx` 改为 `panelTypes.ts`，三处 panic message 同步更新。Iter 89/90/91 三个对齐测试零行为变化通过。
- ChipStrip 现在是 panelTypes.ts 的纯消费者，PanelDebug 也是消费者；如果将来加 PanelStatsCard / PanelActionRow 之类的兄弟组件，全都通过 panelTypes 单一来源协作。
- 184 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 97：抽出 PanelChipStrip，chips 升到 toolbar 上方独立成行
- 新文件 `src/components/panel/PanelChipStrip.tsx`：纯展示组件，封装 6 个 chip + log count（cache / tag / llm 沉默 / 环境感知 / 倾向 / prompt hints button），通过 props 接收所有 stat / tone / handlers。共用 `resetBtnStyle` const 把原本散落的 5 处重复样式收敛成一处。
- PanelDebug.tsx：把所有 chip JSX（240+ 行）替换成单个 `<PanelChipStrip {...} />` 调用；位置从 toolbar 内部右侧移到 toolbar **上方**独立成行。布局：`#f8fafc` 浅背景 + 水平 flex-wrap + `padding: 8px 16px` 让多 chip 时自动换行而不挤压。
- 操作 toolbar（刷新 / 清空 / 立即开口 / DevTools）现在不再被 chips 抢空间——4 个按钮 + proactiveStatus 文本独占一行，更宽松的视觉节奏。
- chip 触发的 `showPromptHints` 展开仍保留在 toolbar 下方（与展开块紧邻），保持"trigger / detail panel"的视觉关联。
- 共享类型 `CacheStats / MoodTagStats / LlmOutcomeStats / EnvToolStats / PromptTiltStats / ToneSnapshot` 加 `export`；`PromptRuleNature / PROMPT_RULE_DESCRIPTIONS / NATURE_META` 也加 export 让 ChipStrip import。
- 三层守护测试更新：`parse_prompt_rule_dict_keys` 现在接受 `const` 和 `export const` 两种声明前缀（Iter 87/89/90/91 的逻辑保持不变）。
- 184 tests + tsc 全过；零 warning。
- 净行数变化：PanelDebug 减 240 行（chip JSX 移走）+ PanelChipStrip 加 250 行（含组件 boilerplate + 1 处 reset 样式整合）。略微增长但 PanelDebug 主组件从 ~770 行降到 ~530 行，更专注于 state + layout，可读性提升。

## 2026-05-03 — Iter 96：长跑 prompt 倾斜计数 + panel "倾向 X%" chip
- 新 `PromptTiltCounters { restraint_dominant, engagement_dominant, balanced, neutral: AtomicU64 }` 加到 `ProcessCounters`，4 个 bucket 互斥求和 = Run 总数。
- 方法 `record_dispatch(&[label])`：按 active labels 中 restraint vs engagement 数量分类——> 大者归 dominant，相等且都 > 0 归 balanced，相等且都 = 0 归 neutral。语义和 panel badge 颜色（Iter 95）一致，让长跑统计聚合的就是用户看到的同一种倾向。
- 调度循环 Run 派发处一行 `record_dispatch(&active_labels)`，紧挨着 LLM 调用。Skip/Silent/Silent-by-gate 不计——只计真正 dispatch 出去的 prompt。
- 新 Tauri commands `get_prompt_tilt_stats` / `reset_prompt_tilt_stats`，注册到 invoke handler。
- panel 工具栏 env-awareness chip 之后插入"倾向 X% (Y/Z)" chip：选 4 bucket 中 count 最大的展示 dominant + 百分比 + 分子分母。颜色跟 dominant：克制红 / 引导绿 / 平衡紫 / 中性灰。tooltip 给完整分布（"克制 12 · 引导 4 · 平衡 2 · 中性 1"）+ 重置按钮。
- 3 个新单测：classify_correctly（4 个分支各击中一次）/ unknown_labels_ignored（未知 label 被忽略不影响分类）/ can_be_reset（atomic 清零正确）。
- 现在用户能看到："今天 prompt 60% 时间在克制宠物" 这种长期画像——比展开当前 hints 多一个时间维度的诊断。
- 184 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 95：badge 颜色根据 nature 倾向自适应
- "prompt: N 条 hint" badge 不再固定紫色——按 active_prompt_rules 的 restraint vs engagement 数量决定主色：
  - restraint > engagement → 红色 #dc2626（深 #991b1b）
  - engagement > restraint → 绿色 #16a34a（深 #15803d）
  - 相等（含 0=0）→ 紫色 #7c3aed（深 #5b21b6，原默认）
- corrective 和 instructional 规则不计入倾向——它们是"做什么"的指导，不是"压"或"激"的方向。让 badge 颜色只反映真正的行为倾斜。
- tooltip 文案根据情况切换：
  - "偏克制（克制 × 3、引导 × 1）"
  - "偏引导（引导 × 2、克制 × 0）"
  - "平衡（克制 2 ↔ 引导 2）"
  - "中性（仅 instructional/corrective 规则）"
- 现在不点开 badge 就能感知 prompt 倾向——红色 chip 出现 = "宠物被多重压制"，绿色 = "正在被激发开口"，紫色 = "中性 / 平衡"。配合 Iter 94 的展开聚合行，单击前后两层信息密度递进。
- 闭合 IIFE 派生：扫 active_prompt_rules → lookup PROMPT_RULE_DESCRIPTIONS.nature → 累加 → 选色。零额外 state，每次 ToneSnapshot 更新自动重算。
- 181 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 94：prompt 规则 nature 分类 + panel 展开聚合显示
- `PROMPT_RULE_DESCRIPTIONS` 每条 entry 加 `nature: "restraint" | "engagement" | "corrective" | "instructional"` 字段。10 条规则分类：
  - restraint × 4：wake-back / pre-quiet / icebreaker / chatty
  - engagement × 2：engagement-window / long-idle-no-restraint
  - corrective × 1：env-awareness
  - instructional × 3：first-mood / reminders / plan
- 新 `NATURE_META` 字典：每个 nature → `{label: 中文(克制/引导/校正/操作), color: hex}`。配色：克制 #dc2626 红、引导 #16a34a 绿、校正 #ea580c 橙、操作 #0891b2 青。
- panel 展开列表现在两层信息：
  - 顶部聚合行：`当前 prompt 软规则 (N)：克制 × 3、引导 × 2、操作 × 2`，颜色化 nature 标签让用户一眼看到 prompt 整体倾向（"克制居多" vs "引导发力中"）
  - 每行规则前加 28px 圆角小 nature badge（红/绿/橙/青），与文字颜色一致——同色让纵向扫描时立即识别哪些是同类规则
- 完整满足"让用户能感知 prompt 整体倾向"目标：之前 panel 只是列出规则文字，现在能直接表达"宠物现在被压制 vs 被激发"。
- 三层守护测试自动跟随：parse_prompt_rule_dict_keys 仍只匹配 top-level `<key>: {` 模式，新增的 `nature: "..."` 是字符串值不会误匹配。Iter 89/90/91 全部通过零修改。
- 181 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 93：第二条积极复合规则 long-idle-no-restraint
- `PromptInputs` 加 `since_last_proactive_minutes: Option<u64>` 数字字段（与现有字符串 `cadence_hint` 互补，让规则能精确比较）。base_inputs 默认 `Some(8)` 与 cadence_hint 文案一致。
- 新常量 `pub const LONG_IDLE_MINUTES: u64 = 60`。
- `active_composite_rule_labels` 升级为 6 参数：在原 wake_back/has_plan 后追加 since_last/today/threshold/pre_quiet。第二条 label `long-idle-no-restraint` 当 `(idle >= 60min || None) && under_chatty && !pre_quiet` 触发。`None` (从未开口) 视为 long-idle，让首次会话也能享受这条规则。
- proactive_rules 新 match arm "long-idle-no-restraint"：建议 LLM 先 `get_active_window` 看用户在做什么，然后抛一个和 ta 当下场景相关的轻话题——明确反对"问候/问感受"的低信号开口模式。
- `run_proactive_turn` 把 cadence 计算改成 tuple `(cadence_hint, since_last_proactive_minutes)`，PromptInputs 透传新字段。
- get_tone_snapshot 和调度循环 dispatch 都升级 active_composite_rule_labels 调用，传 cadence_min/today/threshold/pre_quiet——保证 panel badge / 决策日志 / prompt 三处看到同一份 composite label 集。
- 三个新单测：`active_composite_rule_labels_long_idle_requires_three_signals`（4 个 corner case + None 等价 long-idle + threshold=0 disable）+ `active_composite_rule_labels_both_can_fire_together`（两 composite labels 在同一 inputs 下都活跃）+ 重命名原 engagement-window 测试。
- 既有 fingerprint 测试改为两 scenario：(s1) chatty + pre-quiet 路径覆盖 8 个 label；(s2) long-idle + !pre-quiet 路径覆盖剩下的 long-idle-no-restraint。两个 scenario 的 fingerprint 合集需覆盖全部 10 label，捕获互斥规则在 single inputs 下无法同时触发的现实。
- 前端 PROMPT_RULE_DESCRIPTIONS 加 "long-idle-no-restraint" → "久未开口" / "≥ 60min 没主动说话 + 不在克制态：找个贴合用户当下的轻话题。"
- 三层守护测试自动跟随：composite helper 全集枚举改为 `(true, true, Some(120), 0, 5, false)` 同时触发两条 composite。
- 181 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 92：第一条积极复合规则 engagement-window
- 新 `active_composite_rule_labels(wake_back, has_plan) -> Vec<&'static str>`：仅当两个信号同时为真才返回 `["engagement-window"]`。引入"复合规则"分类，与 environmental / data-driven 并列三类。
- proactive_rules 现在 chain 三个 label 集（env + data + composite），新 match arm "engagement-window" 推一条积极规则文本："此刻是开新话题的好时机：用户刚从离开桌子回来 + 你今天有 plan 在执行——是把「先简短关心 ta 一下，再点一下 plan 进度」自然串起来的复合时机。一句话里带一句关心 + 一句和 plan 相关的，避免硬切话题，也别只问候不带行动。"
- 这是首条**鼓励开口**的规则——之前 8 条都是 restraint（icebreaker/chatty/pre-quiet）或 corrective（env-awareness）或 instructional（wake-back/first-mood/reminders/plan）。engagement-window 把"刚回桌子 + 有今日计划"两个独立信号合成一个"使用此刻"的方向性提示，让 LLM 不只是被各种条件压制，也能主动识别值得开口的窗口。
- ToneSnapshot.active_prompt_rules 和 dispatch loop 的 `rules=` tag 都加 composite labels 到链尾。前端 PROMPT_RULE_DESCRIPTIONS 加 "engagement-window" → "积极开口" / "刚回桌 + 有今日 plan：是「先关心、再带 plan」串起来的复合时机。"
- 三层守护测试自动跟随：
  - Iter 89 backend→frontend：composite labels 加入 backend 全集枚举 → 强制前端添加翻译（已通过）
  - Iter 90 frontend→backend：composite labels 加入比对集 → 阻止前端添加 ghost（已通过）
  - Iter 91 match arm 完整性：fingerprint 表加 ("engagement-window", "此刻是开新话题的好时机") + sanity check 包含 composite helper 全集（已通过）
- 新单测 `active_composite_rule_labels_requires_both_signals` + 更新 `proactive_rules_contextual_count_matches_label_count`（15 = 6 base + 5 env + 3 data + 1 composite）。
- 179 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 91：proactive_rules match arm 完整性测试
- 新单测 `proactive_rules_has_match_arm_for_every_backend_label`：构造全 8 条 contextual 规则同时触发的 inputs，跑 `proactive_rules`，做两层断言：
  1. 输出中**没有** "规则文本待补" 字符串（fallback path 不应被走到）
  2. 每条 backend label 对应的 unique fingerprint 子串都出现在 rules 中（如 icebreaker→"你和用户还不熟"、env-awareness→"最近你开口前几乎都没看环境"）
- 加 fingerprint 表完整性 sanity check：扫 backend 全集，断言每个 label 在 fingerprint 表里有对应行；未来 backend 加 label 但 fingerprint 表没补 → 测试 panic 提示 "missing entries for: [...]"，强迫作者显式选择一个独特的文本子串。
- 三层守护现在闭合：
  - Iter 89: backend label → frontend dict（前端漏译 → fail）
  - Iter 90: frontend dict → backend label（前端 ghost → fail）
  - Iter 91: backend label → proactive_rules match arm（match 缺 arm → fail）
- 178 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 90：反向对齐——禁止前端"幽灵 label"
- 新单测 `frontend_prompt_rule_descriptions_have_no_ghost_labels`：扫 `PROMPT_RULE_DESCRIPTIONS` 所有 key，断言每个 key 都能在 backend 全集（env+data 全开）中找到。失败时列出 ghost keys，提示"要么删了，要么补 backend label"。
- 抽出共用 helper `parse_prompt_rule_dict_keys() -> Vec<String>`：从 `const PROMPT_RULE_DESCRIPTIONS` 起始扫到 `};` 结束，每行 `<key>: {` 模式提取 key（兼容 `"wake-back": {` 和 `plan: {` 两种 JS 写法）。纯字符串扫描零依赖，避免引入 regex crate。
- 顺手把 Iter 89 的 `frontend_prompt_rule_descriptions_cover_every_backend_label` 也改用 `parse_prompt_rule_dict_keys`，让两个测试用同一个 key 解析路径——避免一边用 substring contains 一边用 key parse 导致结果分叉。
- 现在 backend ↔ frontend label 集合双向对齐：
  - 加 backend label 但忘改 TS → Iter 89 fail
  - 改 TS 但 backend 没产 label → Iter 90 fail
  - 重命名 backend label 但前端没跟 → Iter 89 fail（旧 key 被识别为 ghost 触发 90 fail，新 label 没翻译触发 89 fail）
- 177 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 89：cargo test 守门 frontend label 字典与 backend 对齐
- 新单测 `frontend_prompt_rule_descriptions_cover_every_backend_label` 在 `proactive.rs` 测试模块里：用 `CARGO_MANIFEST_DIR/../src/components/panel/PanelDebug.tsx` 路径读 frontend 文件，遍历 `active_environmental_rule_labels(true, ..., true)` + `active_data_driven_rule_labels(0, 999, 1, 999, 0)` 返回的所有 label，断言每个 label 在 TS 文件里能匹配到 `"label":` 或 `\n  label:`（覆盖 quoted 和 bare-identifier 两种 JS 写法）。
- 同步加 sanity check：`PROMPT_RULE_DESCRIPTIONS` 字符串本身必须存在，避免文件移动 / 重命名 / 删字典时测试 vacuously 通过。
- 路径错误时 panic 包含 explicit hint："Did the path move? Adjust this test if so."——告诉未来调试者怎么修。
- 决定不走 codegen / build script：每加一个 label 改两处（Rust 加 enum/match arm + TS 加 dict 行）已经够轻；codegen 解 TS 的代价远超手维护。当 backend label 数量 / 频率上升再考虑。
- 选择跨语言文本扫描而非引入 wasm-bindgen 或 trunk 之类前端测试 framework：纯文本扫描零依赖、可读、CI 跑得动。Trade-off：如果 label 名碰巧出现在 TS 文件的注释/字符串里会假阳性——但 kebab-case 的 wake-back / env-awareness 等独特名字几乎不可能撞，未来真撞了再升级到正式 TS parser。
- 现在如果 backend 加一个新 label 但忘记更新前端字典：`cargo test` 直接 fail 报"Missing entries for: [\"new-label\"]"，配合 Iter 87 的 backend match fallback (`(规则文本待补)`) 形成两层守护。
- 176 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 88：prompt hints badge 可点击展开成 inline 详情列表
- PanelDebug 新 state `showPromptHints: boolean`，badge 由 `<span>` 改成 `<button>`，click 切换。深紫 #5b21b6（展开时）/ 浅紫 #7c3aed（收起时）+ 末尾添加 ▾/▸ chevron 提示状态。
- 新顶层常量 `PROMPT_RULE_DESCRIPTIONS: Record<string, {title, summary}>`：8 条规则各对应中文短标题（4-5 字，如"破冰阶段"、"今日克制"、"环境感知低"）+ 一句简介（解释 LLM 被要求做什么）。lookup 失败 fallback 到 `(label "xxx" 暂无中文描述)`。
- 工具栏 `</div>` 后插入条件渲染的 `<div>`：`#faf5ff` 浅紫背景，每行 `[mono title 84px固定宽] [灰色简介 flex:1]` 两列布局。展开时显示 N 条 hint 详情，收起时不渲染（零视觉占用）。
- 决定不走"backend 同时返 summary"——summary 是 user-facing 中文 UI 字符串，与 panel 同位置维护更内聚；backend 只负责返 label 列表（contract）。这样未来加多语言 panel 可以单独本地化字典而不动 backend。
- 现在用户可以一眼看到："prompt: 3 条 hint" → 点击 → "破冰阶段" + "之前主动开口 < 3 次..."、"今日计划" + "ai_insights/daily_plan 有未完成项..."、"环境感知低" + "近几次开口很少看环境..."。不必再 hover 也能审视当前 prompt 状态。
- 175 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 87：proactive_rules 重构为基于 label 的单一事实源
- `proactive_rules` 内的 8 条 contextual 规则原本各有自己的 `if` 分支，条件和 helper 里的 `if` 重复（icebreaker 条件 `< 3` 写两处、chatty 条件 `> 0 && >= threshold` 写两处...）。Iter 87 让 `proactive_rules` 先调 helper 取 label 列表，然后 `for label in env+data` 用 `match *label { ... }` 选规则文本——条件检查只剩 helper 一处。
- 加了 unknown label 的 fallback：`other => format!("- **[{}]**: (规则文本待补)", other)`，避免未来 helper 加 label 但忘记加 match arm 时直接 panic 或丢失规则；测试断言活跃 label 路径不应出现该 fallback。
- 2 个新单测：`proactive_rules_contextual_count_matches_label_count`（全部 8 条 contextual 触发时 rules.len() == 6 base + 5 env + 3 data = 14；无 fallback）+ `proactive_rules_baseline_only_pushes_always_on_rules`（neutral inputs 只剩 6 条 always-on）——两端 lock 住 base + contextual 数量。
- 现有 18+ 个 prompt 测试保持原行为，无需修改：每条 contextual 规则的关键字（"你和用户还不熟"、"今天已经聊了不少"等）仍出现在 push 出来的字符串里。
- 改动的本质是从"两份并行的条件实现"→"helper 算出哪些 label 活跃，proactive_rules 只翻译 label 到文本"。增加新规则现在是 1）改 helper 加 label 2）改 match 加 arm，两步都靠测试 mantain 一致性。
- 175 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 86：环境性规则也进入 active_prompt_rules 与 decision log
- 新纯函数 `pub fn active_environmental_rule_labels(wake_back, first_mood, pre_quiet, reminders_due, has_plan: bool) -> Vec<&'static str>` 返回 `["wake-back"|"first-mood"|"pre-quiet"|"reminders"|"plan"]` 子集，按 proactive_rules firing 顺序。
- `active_data_driven_rule_labels` 文档串改为说明它和 environmental 互补。
- `get_tone_snapshot`：新派生 5 个 boolean（wake_back from wake_ago<=600s / first_mood from mood text empty/None / pre_quiet from pre_quiet_minutes Some / reminders_due from build_reminders_hint / has_plan from build_plan_hint），调 environmental helper，与 data_driven 拼接（env first）写入 ToneSnapshot.active_prompt_rules。
- 调度循环 dispatch 处也算同样组合，决定 `rules=...` tag 现在覆盖完整 8 条 prompt 规则集。事件回放可以看到任意 Run/Spoke/LlmSilent 当时哪些环境信号 + 哪些数据驱动信号同时影响 prompt。
- 3 个新单测覆盖：empty / 5 个独立单触发 / 全开 firing 顺序锁定。
- panel "prompt: N 条 hint" badge 现在最多可以显 8——之前永远 ≤ 3——更诚实反映 prompt 实际复杂度。tooltip 列出全部规则名（如 "wake-back、first-mood、icebreaker"）。
- 173 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 85：active prompt rules 也透传到每条 decision log entry
- 调度循环 dispatch 一次性 fetch lifetime_count + env_total + env_with_any，加上已有的 chatty_today / chatty_threshold，调 `active_data_driven_rule_labels` 算出此次 tick 哪些规则会激活。封成可选 `rules_tag = Some("rules=icebreaker+chatty")` 或 None。
- Run 决策的 reason 末尾追加 `rules=...`：`Run idle=20s, input_idle=10s, chatty=5/5, rules=icebreaker+chatty`。
- 后续 LLM outcome 三种 push（Spoke/LlmSilent/LlmError）也都带 `rules=...`：复用 inline 函数 `append_tag` 把多个 tag 用 ", " 连接，特殊处理 reason 起始的 "-" 占位（替换而非追加）。
- Spoke 还会加 `tools=X+Y` 在 rules 之后，最终形如 `chatty=5/5, rules=icebreaker, tools=get_active_window`。LlmError 把 tag 串塞进括号 `error_msg (chatty=..., rules=...)`。
- 前端 `localizeReason` Spoke/LlmSilent 都不需要单独 case rules——leading "-, " 仍然 strip，剩下的多个 tag 直接放进 "宠物开口（...）"中文外壳。"LLM 沉默（rules=icebreaker）" 也工作。
- 现在事后回放任意一条 decision log entry，都能精确知道当时 prompt 的"软规则集"——配合 panel 已有的"prompt: N 条 hint"实时 badge，时间维度（历史回放）+ 空间维度（当下状态）双重可见。
- 170 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 84：panel 工具栏 "prompt: N 条 hint" 紫色 pill badge
- 新纯函数 `pub fn active_data_driven_rule_labels(...)` 返回 `["icebreaker"|"chatty"|"env-awareness"]` 子集，按 proactive_rules 内的 firing 顺序排列。仅覆盖"数据驱动"的 3 条规则；wake/first_mood/reminders/plan/pre_quiet 这些环境性 hint 由 panel 现有 chip 已展示，不重复。
- `ToneSnapshot` 加 `active_prompt_rules: Vec<String>`；`get_tone_snapshot` 现在还要 `ProcessCountersStore` state（读 env_tool atomic）和 `today_speech_count`（读 speech_daily.json）。一次性把这三条规则的真实状态计算后塞 ToneSnapshot 里。
- 工具栏在所有 chip 末尾（"N 条日志"前）加紫色 pill: `prompt: N 条 hint`，`background: #7c3aed`，`borderRadius: 10px`。tooltip 列出每条规则名 "prompt 当前正被以下 data-driven 规则影响：icebreaker、chatty、env-awareness"。空时不渲染（neutral state 不出现 badge）。
- 4 个新单测覆盖：neutral 时 vec 空 / 三条独立单触发 / 全部触发返完整三元组（顺序锁定）/ chatty_threshold=0 时即使数字爆表也不进入 chatty 标签。
- 现在用户能立刻看到"我现在的 prompt 被多少 data-driven 规则影响"——0 条说明 prompt 是默认状态，3 条说明已被多重纠偏在驱动。
- 170 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 83：env-awareness 数据回流 prompt，自我纠偏规则
- `PromptInputs` 加 `env_spoke_total / env_spoke_with_any: u64` 两字段；新 `pub const ENV_AWARENESS_MIN_SAMPLES: u64 = 10` + `ENV_AWARENESS_LOW_RATE_PCT: u64 = 30`。
- 纯函数 `pub fn env_awareness_low(spoke_total, spoke_with_any) -> bool`：< MIN_SAMPLES 时返 false（避免噪声触发），否则 `with_any * 100 < 30 * total` 严格比较（避免浮点边界）。
- `proactive_rules` 末尾加纠偏规则：当 env_awareness_low → push "过去 N 次主动开口里只有 M 次调用了 env 工具（< 30%）。本次先调一次 `get_active_window` 看用户在用什么 app，再据此说一句贴合当下的话；别凭空起话题。" 真实数字塞进规则让 LLM 知道处于多深。
- `run_proactive_turn` 新读 ProcessCounters.env_tool 两 atomic 透传到 PromptInputs。`base_inputs` 默认 0/0（低于 MIN_SAMPLES）让现有 18+ 个 prompt 测试不被新规则误触发。
- 5 个新单测覆盖：min_samples 之下不触发、严格 30% 边界、100% 不触发、规则正常出现+包含数字+包含 get_active_window 工具名提示、健康率（67%）下规则不出。
- 现在数据形成闭环：EnvToolCounters 记 LLM 行为 → panel 显示给用户调试 → 同时反馈给 prompt 自动纠偏。如果用户重置统计，规则需要重新积累 10 次才会再触发——避免过去问题永久性塞 prompt。
- 166 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 82：EnvToolCounters + panel 环境感知率 chip
- 新 `EnvToolCounters` sub-struct 加到 `ProcessCounters`，含 6 个 atomic：`spoke_total`、`spoke_with_any` 加 4 个 per-tool 字段（active_window / weather / upcoming_events / memory_search）。
- `EnvToolCounters::record_spoke(&[String])` 方法：单次 Spoke 决策时调，读 outcome.tools 列表分别 bump 已知 env 工具，未知 tool（memory_edit / bash / MCP 等）忽略。`any` flag 控制 `spoke_with_any` 是否 +1。封装在 impl 里让调度处一行调用、未来加新 env 工具只改 match 一处。
- `get_env_tool_stats` / `reset_env_tool_stats` 两个 Tauri command，注册到 invoke handler。
- 调度循环 Spoke 分支：`env_tool_counters.record_spoke(&o.tools)` 紧挨 `outcome_counters.spoke.fetch_add` 同位置——两组 atomic 永远同步。
- 前端：`EnvToolStats` interface 加 6 字段；fetchLogs Promise.all 加 invoke；新 state + 重置 handler。
- 工具栏在 LLM沉默 chip 之后插入"环境感知 N/M (X%)" chip：默认青色 #0891b2；`spoke_with_any * 2 < spoke_total`（低于 50%）切橙色 #ea580c warning。tooltip 拆出每工具数字"window=N · weather=N · events=N · memory_search=N"，方便看是哪个工具被忽略。
- 4 个新单测覆盖：含已知 env 工具、只含 mutating 工具、空 tools、多次累计 + 比例。
- 161 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 81：把 LLM 用的工具串记到 decision log Spoke reason
- `ToolRegistry` 加 `called_tools: TokioMutex<Vec<String>>`：每次 `execute()` push 名字（hit/miss 都记，cache hit 也算 LLM 主观调用）。新 `pub async fn called_tool_names()` 读完去重排序。
- `ToolContext` 加 `tools_used: Option<Arc<Mutex<Vec<String>>>>` opt-in collector + `with_tools_used_collector()` builder。其他 callers（consolidate / telegram / 普通 chat）不传，零开销。
- `run_chat_pipeline` 在 final response 分支末尾把 `registry.called_tool_names().await` 写入 collector（成功路径独占，partial/error 路径不污染数据）。
- `run_proactive_turn` 改返回 `ProactiveTurnOutcome { reply: Option<String>, tools: Vec<String> }`：在 ctx 上挂 collector，pipeline 完成后读出来一起返。`trigger_proactive_turn` 同步更新到新返回结构。
- 调度循环 dispatch 把 `tools` 拼成 `tools=window+weather` 加在 Spoke 的 chatty_part 后：reason 形如 `"chatty=5/5, tools=get_active_window+get_weather"`。
- 前端 `localizeReason` Spoke 分支处理 4 种 reason 形态：`"-"` / `"-, tools=X+Y"` / `"chatty=5/5"` / `"chatty=5/5, tools=X+Y"`，无 chatty 标签时去掉前缀 "-" 显示 `"宠物开口（tools=...）"`。
- 2 个新单测覆盖 called_tool_names empty / mixed cacheable+非的 sort+dedup。
- 现在 panel decision log 看 Spoke 行就能立刻知道："这次开口前 LLM 调了 active_window+weather → 它有看用户场景再说话"，对调试 prompt 是否实际驱动 LLM 用工具非常关键。
- 157 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 80：LLM 沉默率 atomic counters + panel 工具栏 chip
- 新 `LlmOutcomeCounters { spoke, silent, error: AtomicU64 }` 加到 `ProcessCounters`，container pattern 与 cache/mood_tag 一致；零 plumbing 改动。
- 新 Tauri commands `get_llm_outcome_stats` / `reset_llm_outcome_stats`；都注册到 invoke handler。
- 调度循环 dispatch `LoopAction::Run` 后的 outcome 处理处一次性 fetch process_counters，按 Spoke/Silent/Error 分支 `fetch_add(1)` 与 push decision 同位置——保证 decision_log 看到的事件和 atomic 累计一致。
- 前端：interface `LlmOutcomeStats { spoke, silent, error: number }`；fetchLogs Promise.all 数组加 `invoke("get_llm_outcome_stats")`；新 state + 重置 handler。
- 工具栏 Tag chip 后插入"LLM沉默 N/M (X%)"：紫色 #7c3aed 默认；当 silent+error 占比超过半数（即 spoke 是少数），切橙色 #ea580c warning 提示 "prompt 太克制了"。tooltip 写明 "gate 放行后的 LLM 决策" 和 "可作为调优反馈"。重置按钮与 cache/tag 同款。
- 2 个新单测覆盖 default 0 / accumulate / reset 三步。
- 155 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 79：decision log CAPACITY 升 16，panel 视觉配对 Run+outcome
- `decision_log::CAPACITY` 从 `10` → `16`。Iter 78 起每次 Run 触发会 push 两条（Run + LLM outcome），10 cap 仅给 5 个完整 cycle 的视野；16 给约 8 个 cycle，恢复 Iter 77 之前的工作集大小。
- panel "最近决策" 列表对 `Spoke / LlmSilent / LlmError` 三个 outcome kind 的 kind 列前加 `└ ` tree 字符（U+2514 + 空格），让"这是上一个 Run 的后续"视觉自洽——不需要看时间戳就知道哪两行是一对。
- `maxHeight` 从 `120px` → `200px`，让升 cap 后的更多行无需滚动就能看到。仍带 `overflowY: auto` 兜底，超出时滚动。
- 现有 3 个 decision_log 测试用 `CAPACITY` 常量，自动跟随新值不破。
- 153 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 78：decision log 区分 LLM 层结果，标注克制模式
- 新纯函数 `pub fn chatty_mode_tag(today, threshold) -> Option<String>`：返回 `chatty=N/M` 或 None（threshold=0 / today<threshold 都视为非活跃）。3 个新单测覆盖 0 阈值禁用、阈值下、阈值上 / 超过的格式。
- 调度循环 dispatch 处一次性算 `chatty_today / chatty_threshold / chatty_tag`，然后：
  - `Run` 决策的 reason 末尾追加 `chatty=N/M`（仅在活跃时），让 `Run idle=20s, input_idle=10s, chatty=5/5` 一眼看到 gate 通过时软规则状态。
  - 调用 `run_proactive_turn` 后再 push 一条决策：`Spoke` / `LlmSilent` / `LlmError`，reason 复用 chatty_tag（活跃时填 "chatty=N/M"，否则填 "-"）。
- 前端 `kindColor` 加三种新 kind：`Spoke=#16a34a`（深绿）/ `LlmSilent=#a855f7`（紫）/ `LlmError=#dc2626`（红）。
- `localizeReason` 新增三个 kind 的中文：`LLM 自主选择沉默` / `LLM 沉默（chatty=5/5）` / `宠物开口` / `宠物开口（chatty=5/5）` / `LLM 调用失败：...`。
- 现在 panel "最近决策" 区可以清晰回答："今天为什么这么安静？" → 看到 `Run idle=...chatty=5/5` 后跟 `LlmSilent chatty=5/5`，即"gate 放行了，但 LLM 在克制规则下选了沉默"。
- 153 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 77：panel stats 卡可视化"克制模式"
- `ToneSnapshot` 加 `chatty_day_threshold: u64` 字段；`get_tone_snapshot` 从 settings 读出与 fallback=5（同 run_proactive_turn 的策略，保持一致）。
- PanelDebug 的 ToneSnapshot interface 同步加；stats 卡用 IIFE 派生 `restraining = threshold > 0 && todaySpeechCount >= threshold`。
- 跨过阈值时：今日数字从蓝色（#0ea5e9）切换到橙色（#ea580c）；右上角"破冰阶段"小标 → 替换为"克制模式" pill 形 badge（`background: #fff7ed`，`border: 1px solid #fed7aa`，`borderRadius: 10px`）；hover 文案解释"prompt 里加了'今天聊得不少了'的克制规则"。
- 优先级互斥：「克制模式」>「破冰阶段」。同时满足时只显示克制（一个用户当天聊到饱和，新手期早过了）。
- 150 tests + tsc 全过；零 warning。
- 现在用户能直观看到行为切换：今天聊得多 → 数字变橙 + badge 出现，看到 LLM 真的在被 prompt 软规则控制。

## 2026-05-03 — Iter 76：CHATTY_DAY_THRESHOLD 升级为 settings.proactive.chatty_day_threshold
- `ProactiveConfig` 新字段 `chatty_day_threshold: u64`（带 `#[serde(default)]` + `default_chatty_day_threshold() = 5`），现有 settings.json 升级时自动补默认值。
- `PromptInputs.chatty_day_threshold: u64` 替代 Iter 75 的 `pub const CHATTY_DAY_THRESHOLD`，整个常量删除。`proactive_rules` 检查 `threshold > 0 && today_count >= threshold`：0 显式关闭整条规则。
- `run_proactive_turn` 新读 `chatty_day_threshold` 透传到 PromptInputs；fallback 到 5（settings 读失败时）。
- `useSettings.ts` 的 `ProactiveConfig` interface + `DEFAULT_SETTINGS` 同步加字段。`PanelSettings.tsx` 在主动开口区域底部加 `PanelNumberField`：「今天主动开口达到此数后变克制（0 = 关闭）」。
- 测试更新：`base_inputs` 加默认 `chatty_day_threshold: 5`；既有 chatty_day 测试改用 inputs 字段而非常量。新增 2 个测试：`chatty_day_rule_disabled_when_threshold_zero`（threshold=0 时 count=9999 也不触发）+ `chatty_day_threshold_is_user_tunable`（自定义 10 时 9 不触发 / 10 触发，验证用户配置真的生效）。
- ProactiveConfig literal 测试 fixture 也补字段。
- 150 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 75：今日开口数喂回 prompt，触发"今天已经聊了不少"克制规则
- `PromptInputs` 新字段 `today_speech_count: u64`；新 `pub const CHATTY_DAY_THRESHOLD: u64 = 5`。
- `proactive_rules` 末尾加新条件规则：当 `today_speech_count >= CHATTY_DAY_THRESHOLD`，push "今天已经聊了 N 次了。除非有真正值得说的新信号（用户刚回来、有到期提醒、明显环境变化），优先**保持安静**（用 `<silent_marker>`）；要说也只说极简一句"。规则里塞了真实数字让 LLM 知道处于多深。
- `run_proactive_turn` 加 `let today_speech_count = crate::speech_history::today_speech_count().await;` 与现有 `proactive_history_count` 同位置；PromptInputs 加新字段。
- `base_inputs` 默认 `today_speech_count: 0`，让现有 17 个 prompt 测试不被新规则误触发。
- 新增 2 个 unit test：`chatty_day_rule_appears_at_or_above_threshold` 验证 == 阈值时规则出现+数字到位；`chatty_day_rule_absent_below_threshold` 验证 < 阈值时不出。
- 148 tests + tsc 全过；零 warning。
- 现在用户长时间使用一天后宠物会自然变克制，不会在 idle 时段重复打扰；和 quiet hours 不同，这是行为上的"软退避"而非时间窗。

## 2026-05-03 — Iter 73：speech 按日分桶，stats 卡显「今日 / 累计」双数
- 新增 `~/.config/pet/speech_daily.json` sidecar：`{"YYYY-MM-DD": count}` 结构。`record_speech_inner` 写完 → bump_lifetime → bump_today，三层 best-effort 串行。
- 纯函数 `parse_daily(content) -> BTreeMap<String, u64>`：malformed/空/数组 都回 empty map（损坏文件下次 bump 自愈）。
- 纯函数 `prune_daily(map, today, retain_days)`：`YYYY-MM-DD` 字符串字典序就是日期序，简单 `>= cutoff_str` 比较。non-parseable key 保留（不属于本模块管理范围）。`DAILY_RETAIN_DAYS = 90` 给未来"近 7d / 30d"特性留余地。
- 新 `today_speech_count() -> u64` + `#[tauri::command] get_today_speech_count`，注册到 invoke handler。
- PanelDebug stats 卡改双段布局：左 `20px` 蓝色「今日 N」+ 右 `28px` 紫色「累计 N」+ 共享后缀「次主动开口」。破冰标签靠右浮动逻辑不变。每段一个 tooltip 解释来源文件。
- 5 个新单测覆盖 parse_daily（empty/malformed/valid）+ prune_daily（cutoff 边界 / unparseable key 保留 / retain=0 today 仍保留）。
- 146 tests + tsc 全过；零 warning。

## 2026-05-03 — Iter 72：lifetime_speech_count 暴露成 Tauri command + panel 大数字
- 新增 `#[tauri::command] get_lifetime_speech_count() -> u64`：薄封装 `lifetime_speech_count`，注册到 invoke handler。前端无需走 `get_tone_snapshot`（混了一堆其他字段）就能拿到累计数。
- `PanelDebug.tsx`：fetchLogs 的 Promise.all 数组里加 `invoke("get_lifetime_speech_count")`；新 state `lifetimeSpeechCount`。
- 工具栏下方插入新 stats 卡片：`28px` 紫色 mono 大数字 + "次主动开口（持久累计 · 跨重启不归零）"灰色副标。破冰期（< 3）右上角显"破冰阶段"琥珀小标。背景用 `linear-gradient(135deg, #fdf4ff, #f0f9ff)` 轻彩区分于其他面板段。
- chip 里的 "🤝 已开口 N 次" 留着不删——chip 是条带式概览，大卡片是首屏独立标识，两者读者场景不同（扫一眼 vs 注视）。
- cargo check + 141 tests + tsc 全过。

## 2026-05-03 — Iter 71：proactive_count 持久化 sidecar，告别 50 行 cap
- 新增 `~/.config/pet/speech_count.txt` sidecar：单整数文件，每次 `record_speech_inner` 写完追加 `bump_lifetime_count()` 把它 +1（best-effort，IO 错误不挡 speech 主流程）。
- `lifetime_speech_count() -> u64`：读 sidecar；文件缺失/损坏时 fallback 到 `count_speeches().await as u64` 作 bootstrap，让从 Iter 70 升级上来的现有用户首次访问不会回退到 0。第一次 bump 后 sidecar 永远存在，bootstrap 路径只走一次。
- ToneSnapshot 改用 `lifetime_speech_count`，删掉 Iter 70 的 `proactive_count_capped: bool` 字段（持久 counter 不会饱和，标志已多余）。
- 前端 `PanelDebug.tsx` interface 同步删 capped 字段；🤝 chip 简化：去掉 `+` 后缀和"已饱和"分支 tooltip，只保留破冰 / 普通两档。tooltip 文案改为"持久化在 speech_count.txt，跨重启不归零"，让用户知道它是真实累计。
- Why sidecar file 而非 ProcessCounters atomic：counter 必须跨重启活下来；ProcessCounters 是进程内 State，下次启动归零，达不到"长跑用户看一共聊过多少次"的目标。文件写入和 speech_history.log 写入同位置同 IO 模式，复杂度增量极小。
- cargo check + 141 tests + tsc 全过。

## 2026-05-03 — Iter 70：proactive_count 50+ 截断指示
- speech_history.rs 的 `SPEECH_HISTORY_CAP` 从 private const 改为 `pub const`，让其他模块能比较检测饱和。
- ToneSnapshot 加 `proactive_count_capped: bool`：`get_tone_snapshot` 计算 `count >= SPEECH_HISTORY_CAP` 决定。
- 前端 `PanelDebug.tsx` interface 同步；🤝 chip 渲染逻辑：
  - 数字后缀：饱和时加 `+`（如 "已开口 50+ 次"）
  - tooltip 三档：< 3 是破冰说明 / capped 是"已饱和（speech_history.log 上限是 50 行；真实总数可能更高）" / 普通是"基于 speech_history.log 行数"
- 选方案 A（轻量截断指示）而非 B（独立 atomic 累计）：当前用户最可能在前几次破冰阶段就关闭/换设备，长跑用户的精确累计需求不强；若日后需要再走 Iter 71。
- 不写新单测：bool 派生自现有 const 比较，cargo + tsc 兜底。
- tsc + 141 tests 双过；零 warning。

## 2026-05-03 — Iter 69：ToneSnapshot 加 proactive_count + panel chip
- 后端 `ToneSnapshot.proactive_count: u64` 字段；`get_tone_snapshot` 调 `count_speeches().await as u64` 取值。
- 前端 `PanelDebug.tsx` interface 同步加；tone strip 在 pre-quiet 之后加 🤝 chip：
  - 默认色 #64748b（灰）
  - count < 3 时切 #d97706（琥珀，warning 暖色）+ 后缀「（破冰）」
  - tooltip 解释 < 3 是"破冰阶段——前 3 次主动开口走探索性话题"，否则"累计主动开口次数（受 speech_history.log 50 行 cap 影响）"
- count == 0 仍渲染（与其他 chip 用 `!== null && > 0` 不同——破冰阶段就是从 0 开始的，0 才是最重要的展示时刻）。
- 141 tests + tsc 双过；零 warning。
- 现在用户安装新版本后能在 panel 一眼看到"目前是破冰阶段，宠物会问简短问题"，理解宠物为什么前几句话感觉特别像问卷。

## 2026-05-03 — Iter 68：first-time 破冰 prompt 规则
- 新 `pub async fn count_speeches() -> usize` 在 speech_history.rs：读 file 计非空行数，作 lifetime proactive utterance count（受 SPEECH_HISTORY_CAP=50 约束足以判断"前几次")。
- `PromptInputs.proactive_history_count: usize` 新字段。
- `proactive_rules` 加条件性规则（count < 3 时）："你和用户还不熟：你之前主动开口过 N 次（< 3 次的破冰阶段）。开口时偏向问一个简短、低压力的了解性问题（例如 ta 此刻的感受、当下在做什么、有没有最近喜欢的小事），别直接给建议或扔信息密集的话题。如果用户答了什么记得用 memory_edit create 写到 user_profile 类下方便日后用。"
- run_proactive_turn 调 `crate::speech_history::count_speeches().await` 取真实 count 传 builder。
- base_inputs 默认设 `proactive_history_count: 100` 让现有测试不被新规则误触发。
- 2 个新单测：count=0 触发规则 + 规则文本含 "0 次"；count=3 不触发（threshold 边界）。
- 总测试 139 + 2 = **141 个**，全过；cargo + tsc 双过；零 warning。
- 现在新装宠物的前 3 次主动开口会显著克制——不会一上来就推荐什么事或翻 memory 给意见，而是先问简短问题了解用户。

## 2026-05-03 — Iter 67：daily_plan 自动过期 sweep
- `MemoryConsolidateConfig` 加 `stale_plan_hours: u64`（默认 24）；与 `stale_reminder_hours` 平行字段。
- 新 `pub fn sweep_stale_plan(now, cutoff_hours) -> bool` 在 consolidate.rs：读 ai_insights 类的 daily_plan 条目，`DateTime::parse_from_rfc3339(updated_at)` 算与 now 的 age；超过 cutoff 调 memory_edit delete。任意 IO/解析失败 → false（best-effort）。
- run_consolidation：把 `get_settings()` 抽到 `cfg_settings` 共用，分别调 sweep_stale_reminders 和 sweep_stale_plan。后者 deletion 时单独写一行日志"swept stale daily_plan before LLM run"。
- 前端 `useSettings.ts` interface + DEFAULT；`PanelSettings.tsx` 初值同步。
- UI：SettingsPanel modal 把 reminder cutoff 一行改成两列（reminder + plan）；PanelSettings 的说明文字改为"reminder：... plan：daily_plan 条目 updated_at 超过该时长就清空。"
- 不加 sweep 测试——逻辑结构与 sweep_stale_reminders 同 pattern 已测过；is_stale 替代是 chrono 内置 parse_from_rfc3339 + Duration 比较，cargo check 抓 plumbing 错。
- tsc + 139 tests 双过；零 warning。
- 现在 plan 不会跨日累积，宠物每天 new turn 都从干净状态自己定（或选择不定）当日目标。

## 2026-05-03 — Iter 66：宠物的"今日计划"
- 复用 `ai_insights` 类别 + 单一 `daily_plan` 条目（与 current_mood 同模式，避免新增 memory 类别）。
- 新 `fn build_plan_hint() -> String`：读 `ai_insights/daily_plan`，存在且 description 非空就返"你今天的小目标 / 计划：\n{description}"，否则返空字符串。
- `PromptInputs.plan_hint: &'a str` 字段；builder push_if_nonempty 在 reminders_hint 之后。
- proactive_rules 加 conditional rule：plan_hint 非空时 instruct LLM "**优先**考虑推进其中一条（不必每次推进，看时机自然）；推进后用 memory_edit update 更新 `[0/2]` 进度；全部完成的项删除"。
- run_proactive_turn 调 build_plan_hint 传给 builder。
- inject_mood_note（reactive chat）加 `plan_section` 第三段：教 LLM "如果想定今日小目标用 memory_edit 在 ai_insights 下 create/update daily_plan，description 用 `· 关心工作 [0/2]\\n· 喝水 [0/1]` 这种格式"。
- 2 个新单测：rule appears + plan hint appears in full prompt。
- 总测试 137 + 2 = **139 个**，全过；cargo + tsc 双过；零 warning。
- 现在宠物有了"目的感"：用户在 chat 里跟宠物说「今天多关心我一下吧」，宠物可以把这个意图写成 daily_plan，在后续 proactive 开口时按计划推进，跨 turn 不再各自独立。

## 2026-05-03 — Iter 65：trigger 状态包含 LLM 实际回复
- `run_proactive_turn` 签名从 `Result<(), String>` 改为 `Result<Option<String>, String>`：
  - `None` = 宠物选择沉默
  - `Some(reply)` = 宠物开口的 trim 后文本
- silent 分支 `return Ok(None)`；speaking 分支末尾 `Ok(Some(reply_trimmed.to_string()))`。
- spawn 主循环原本就是 `if let Err(e) = run_proactive_turn(...)`，对 Ok 值不关心 → 类型改了不需要动逻辑。
- `trigger_proactive_turn` 接住 reply：
  - `Some` → 状态字符串"开口完成 (Nms, idle=Ks): 实际回复内容"
  - `None` → "宠物选择沉默 (Nms, idle=Ks)"
- 前端 toolbar 的 ellipsis + tooltip 自动适配新格式，长 reply 截断后 hover 看完整。
- 137 tests + tsc 双过；零 warning。
- 现在按"立即开口"立刻知道宠物说了啥，调试 prompt 不用切到聊天面板看气泡。

## 2026-05-03 — Iter 64：trigger_proactive_turn 状态反馈
- `PanelDebug.tsx` 加 `proactiveStatus: string` state；`handleTriggerProactive` 接住 invoke 返回的 status 字符串赋给 state；catch 失败也写进同一 state（带"触发失败"前缀）。
- toolbar 在 DevTools 按钮后插条 status span：成功用 `#059669`（绿），失败用 `#dc2626`（红）；max-width 260px + ellipsis 截断长字符串，hover tooltip 看完整。
- `setTimeout(setProactiveStatus(""), 8000)` 自动 8 秒清空——既给用户看的时间，又不让 toolbar 永远顶着 stale 状态。
- 失败时立即把 console.error 也保留，给 DevTools 用户看完整错误栈。
- 后端无改动；tsc 干净。
- 现在按完"立即开口"在 toolbar 立刻看到"Proactive turn finished in 6800 ms (idle=900s)"，调试链路从触发到耗时都可见。

## 2026-05-03 — Iter 63：手动触发 proactive turn 命令 + 按钮
- 后端 `proactive.rs` 加 `#[tauri::command] pub async fn trigger_proactive_turn(app)`：取 InteractionClock snap + user_input_idle_seconds，直接调 `run_proactive_turn(...)` 绕过 evaluate_loop_tick 所有闸门。返回 "Proactive turn finished in N ms (idle=Ks)"。
- lib.rs 注册命令。
- 前端 `PanelDebug.tsx`：
  - 加 `triggeringProactive: boolean` state + `handleTriggerProactive` async 处理。
  - toolbar 在"清空"和"DevTools"之间插绿色（`#10b981`）"立即开口"按钮，运行中变灰且文字"开口中…"，tooltip 解释"绕过 idle/cooldown/quiet/focus 等闸门，立刻让宠物跑一次主动开口检查（用于测试 prompt 或现场 demo）。"。
- 137 tests + tsc 双过；零 warning。
- 现在调试 prompt 或给人 demo 时不必等 5–15 分钟自然 idle，按一下宠物立刻开口；同时 wake/cadence 等 hint 仍按真实状态注入，让 demo 看到的 prompt 是真实的而非 dummy。

## 2026-05-03 — Iter 62：手动触发 consolidate 命令 + 按钮
- 后端 `consolidate.rs` 加 `#[tauri::command] pub async fn trigger_consolidate(app) -> Result<String, String>`：
  - 计算 `total_memory_items`（不走 min_total_items gate——手动想 always 跑）
  - 调 `run_consolidation(&app, total)`
  - 返一个状态字符串"Consolidation finished in N ms (M items at start)"
- lib.rs `tauri::generate_handler!` 注册。
- 前端 `PanelMemory.tsx`：
  - 加 `consolidating: boolean` state + `handleConsolidate` async 处理函数。
  - 搜索工具栏右侧加紫色"立即整理"按钮（`#8b5cf6`），运行中变灰禁用且文字"整理中…"。tooltip 解释"立即让 LLM 检查并整理记忆（合并重复 / 删过期 todo / 清 stale reminder），不必等定时触发。"。
  - 完成后调 `loadIndex()` 重新拉 memory 索引，让用户立即看到结果。
- tsc + 137 tests 双过；零 warning。
- 现在用户在 panel 添加了一批新 memory 后，不需要等 6 小时定时触发，按一下按钮就能立刻让宠物整理一遍。

## 2026-05-03 — Iter 61：stale_reminder_hours 配置化
- `MemoryConsolidateConfig` 加 `stale_reminder_hours: u64`（默认 24），归到 consolidate 配置而非 ProactiveConfig（虽然 TODO 说 ProactiveConfig，但 sweep 在 consolidate 跑、用 consolidate 的 settings 更一致）。
- `default_stale_reminder_hours()` 返 24，与上一版硬编码值相同——升级现有 config.yaml 不会引起行为变化。
- consolidate.rs `run_consolidation` 改为读 `get_settings().memory_consolidate.stale_reminder_hours`，错误时 fallback 到 24。
- 前端 `useSettings.ts` MemoryConsolidateConfig interface + DEFAULT_SETTINGS 加字段；`PanelSettings.tsx` 初值同步。
- UI：
  - SettingsPanel modal 在"触发条目数"后加一行"清理过期 reminder (小时)" NumberField。
  - PanelSettings 加同名字段 + 11px 浅灰说明文字："consolidate 跑时会自动删超过此时长的过期 [remind: YYYY-MM-DD HH:MM] 提醒。HH:MM 格式（'今天'）不受影响。"
- 137 tests + tsc 双过；零 warning。
- 不写新单测——is_stale_reminder 接 cutoff 参数早测过；plumbing 一类改动靠类型系统 + cargo check 兜底。

## 2026-05-03 — Iter 60：consolidate 阶段清扫 stale reminder
- 新纯函数 `pub fn is_stale_reminder(&ReminderTarget, now: NaiveDateTime, cutoff_hours) -> bool` 在 proactive.rs：
  - Absolute → `(now - dt) > cutoff_hours` 即过期
  - TodayHour → 永远 false（语义是"recurring-friendly"，不知道创建日期，不该自动删）
- 新函数 `pub fn sweep_stale_reminders(now, cutoff_hours) -> usize` 在 consolidate.rs：扫 todo 类，对每个 parseable reminder 检查 stale，命中调 `memory::memory_edit("delete", "todo", title)`，返回删除数。先 collect titles 再删，避免 mutate-while-iterate。
- `run_consolidation` 在 LLM 调用前先 `sweep_stale_reminders(now, 24)`，>0 时写日志。这样 LLM 看到的 index 已经清爽，不会浪费一次 API call 决定要不要删。
- 4 个新单测覆盖：cutoff 之外 stale / 之内不 stale / 未来不 stale / TodayHour 永不 stale。
- 总测试 133 + 4 = **137 个**，全过；cargo + tsc 双过；零 warning。
- 现在用户即使忘了让宠物清掉昨天的 todo，第二天 consolidate 跑时会自动收拾。

## 2026-05-03 — Iter 59：reminder 支持绝对日期格式
- 新 `pub enum ReminderTarget { TodayHour(u8, u8), Absolute(NaiveDateTime) }`：把"今天 HH:MM"和"特定日期 HH:MM"做成显式两态。
- `parse_reminder_prefix` 重构：先尝试 `YYYY-MM-DD HH:MM`（含空格），失败再退到 `HH:MM`。返回 `Option<(ReminderTarget, String)>`。
- `is_reminder_due` 重构：签名改为 `(&ReminderTarget, NaiveDateTime now, window_minutes) -> bool`：
  - Absolute → 简单 `now - dt` 在 [0, window] 内
  - TodayHour → 先比 today's HH:MM；不在 due window 时再尝试 yesterday's HH:MM 处理跨午夜 wrap
- 新 `pub fn format_target(&ReminderTarget) -> String`：TodayHour=`HH:MM`，Absolute=`YYYY-MM-DD HH:MM`，给 prompt + panel 共用。
- `build_reminders_hint` 签名改为接 `NaiveDateTime`，统一时间锚。
- `get_pending_reminders` / `PendingReminder.time` 用 `format_target` 输出，自动支持两种格式。
- `inject_mood_note::reminder_section` 大幅扩写：教 LLM 三种场景（今天 HH:MM / 跨天 YYYY-MM-DD HH:MM / 相对时间 → 自己换算成绝对）。
- 测试重构：旧 5 个 today + 5 个 due → 新 11 个（today/Absolute 解析 + TodayHour due 5 + Absolute due 4）。新 test 用 `ndt(y, m, d, h, m)` helper 构造 NaiveDateTime。
- 总测试 128 + 5（净增）= **133 个**，全过；cargo + tsc 双过；零 warning。
- 现在「明天 9 点开会」这类话也能存得住了——LLM 把它换算成 `[remind: 2026-05-04 09:00] 开会`，到那个时刻 proactive 自动捞出来。

## 2026-05-03 — Iter 58：PanelDebug 显示 todo 类 reminder 候选
- 后端：
  - 新 `pub struct PendingReminder { time, topic, title, due_now }`（serde::Serialize）
  - 新 `#[tauri::command] pub fn get_pending_reminders() -> Vec<PendingReminder>`：扫 memory todo 类，对每条 description 调 `parse_reminder_prefix`，能解析就纳入；同步算 `due_now` 让前端渲染时区分。
  - lib.rs `tauri::generate_handler!` 注册。
- 前端 `PanelDebug.tsx`：
  - 新 interface `PendingReminder` + state；fetchLogs 7 路 Promise.all 一并取。
  - speech 段之后插一段橙色背景 (#fff7ed) 卡片：`待提醒事项 N 条（橙色 = 已到时间窗口）`，每行 `HH:MM topic (title)`。
  - due_now 的行 time 字段用 #ea580c 橙 + 加粗；非 due 用 #a16207 暗黄；让用户视觉上立刻区分。
  - 仅当 reminders.length > 0 时渲染。
- tsc + 128 tests 双过；零 warning。
- 现在用户在 chat 让宠物记一条提醒后，立刻打开 panel 就能看到"23:00 吃药 (take_meds)"这样的条目，不用 cat memory yaml 验证。

## 2026-05-03 — Iter 57：reactive chat 教 LLM reminder 格式
- `commands/chat::inject_mood_note` 拆 body 为 `mood_section` + 新增 `reminder_section`：
  - 明确告诉 LLM "如果用户说类似「N 点提醒我做 X」类话，请用 `memory_edit create` 在 `todo` 类别下创建 description 以 `[remind: HH:MM] X` 开头的条目"。
  - 给具体例子（description=`[remind: 23:00] 吃药`、title=`take_meds`）减少 LLM 犹豫。
  - 显式排除"我说今晚要..."这种闲聊，避免误把任何"今晚"句都建 todo。
- 用 `format!("{}{}", mood_section, reminder_section)` 拼接两段。
- 又一次踩 ASCII `"..."` 闭合 Rust 字符串的坑——Iter 29 / 39 都遇到过。换全角「...」。
- 总测试 128（不变 — 改的是字符串模板，复用现有 inject_mood_note 调用路径）；cargo + tsc 双过；零 warning。
- 现在 Iter 56 的 reminder 闭环完整：用户在 chat 里说话 → reactive chat 提示 LLM 写 todo → proactive 扫到 due → 注入 prompt 让 LLM 自然带出 → memory_edit delete 已用。

## 2026-05-03 — Iter 56：用户驱动的 manual 提醒
- 新纯函数 `pub fn parse_reminder_prefix(desc) -> Option<(u8, u8, String)>` 解析 `[remind: HH:MM] topic` 格式，验证 hour ≤23 / minute ≤59 / topic 非空。
- 新纯函数 `pub fn is_reminder_due(target_h, target_m, now_h, now_m, window_minutes) -> bool`：在 `[target, target+window)` 内为 due；处理跨午夜（target 23:55 / now 00:05 用 +24×60 wrap）；未来时间不算 due。
- `PromptInputs` 加 `reminders_hint: &'a str`；builder 在 speech_hint 之后 push_if_nonempty。
- 新 `fn build_reminders_hint(now_h, now_m) -> String`：扫 memory 的 `todo` 类别，对每条 description 调 parse + due 检查；命中即生成 bullet `· HH:MM topic（条目标题: title）`。无命中返空字符串。
- run_proactive_turn 调 `build_reminders_hint(now_h, now_m)` 再传给 builder。
- proactive_rules 加条件性规则（reminders_hint 非空时）："有到期的用户提醒：上面 reminders 段列出的事项是用户之前明确让你提醒的，请把其中**最相关的一条**自然带进开口里（不要全念出来），并在开口后用 `memory_edit delete` 把已经提醒过的那条 todo 条目删掉，避免下次再提一遍。"
- 11 个新单测：parse 5（标准 / 空格 / 空 topic / 非法时间 / 无前缀）+ due 5（窗口内 / exact target / 未来 / 太久 / 跨午夜）+ rule-level 1。
- 总测试 117 + 11 = **128 个**，全过；cargo + tsc 双过；零 warning。
- 现在用户在 chat 里让 LLM 创建 todo 条目（description 以 `[remind: 23:00] 吃药` 开头）后，到 23:00 ~ 23:30 的 proactive 检查会把这条提醒注入 prompt + 加规则要求 LLM 自然带出并删掉已提醒条目。Iter 57 会让 reactive chat 主动告诉 LLM 这个格式约定。

## 2026-05-03 — Iter 55：ToneSnapshot 加 pre_quiet_minutes 进 panel
- 后端 `ToneSnapshot` 加 `pre_quiet_minutes: Option<u64>` 字段；`get_tone_snapshot` 读 `get_settings()` 算 quiet hours start，调 `minutes_until_quiet_start` 取分钟（look_ahead=15）。
- 前端 `PanelDebug.tsx` interface 同步加字段；tone strip 在 wake 之后加红色 🌙 段：「距安静时段 N 分钟」，仅 Some 时渲染。
- 颜色 #dc2626（红）和 wake 蓝、Cache 蓝、Tag 紫做区分——红色暗示"快到了"是收尾信号。
- 117 tests + tsc 双过；零 warning。
- 现在 panel 一眼能看出宠物为啥突然变温柔——"距安静时段 8 分钟"对应 prompt 注入了"快进入安静时段"规则，调试链路从 prompt → 行为完整可视。

## 2026-05-03 — Iter 54：临近 quiet hours 注入"收尾"规则
- 新纯函数 `pub fn minutes_until_quiet_start(now_hour, now_minute, quiet_start, quiet_end, look_ahead_minutes) -> Option<u64>`：
  - quiet_start == quiet_end → None（gate 关闭）
  - 已在 quiet → None（没有"接近"可言）
  - 距 quiet_start > look_ahead → None
  - 否则 Some(剩余分钟数)
  - 处理跨日：start_total_today 已过 → 加 24×60 用次日
- `PromptInputs` 加 `pre_quiet_minutes: Option<u64>`。
- `proactive_rules` 末尾按 `Some(mins)` 条件 push："快进入安静时段：再过约 N 分钟就到夜里的安静时段了。语气要往收尾靠——简短的晚安/睡前关心比新话题合适。"
- run_proactive_turn 用 `get_settings()` + `now_local.hour()/minute()` 计算并传入。`look_ahead = 15` 写死。
- 7 个新单测（mod pre_quiet_tests）：
  - `within_window_returns_minutes` (22:50→Some(10))
  - `at_window_edge_15_min` (22:45→Some(15) 含 strict-leq 边界)
  - `outside_window_returns_none` (22:44→None)
  - `already_in_quiet_returns_none` (03:00 / 23:30)
  - `disabled_when_start_equals_end`
  - `same_day_window` (14:00 quiet → 13:55→Some(5))
  - `past_today_uses_tomorrow` (07:00 morning, quiet 23-7 already past → None)
- 1 个 prompt-level 测试 `pre_quiet_rule_appears_when_set` 验证 7 条规则 + 含分钟数。
- 总测试 109 + 8 = **117 个**，全过；cargo + tsc 双过；零 warning。
- 现在宠物在快到 22:45（默认 23:00 quiet 前）时会自动调成"晚安"基调，而不是 23:00 整 silent 让用户感觉宠物突然消失。

## 2026-05-03 — Iter 53：proactive_rules 按上下文动态加规则
- `proactive_rules` 签名从 `() -> Vec<String>` 改为 `(&PromptInputs) -> Vec<String>`。
- `PromptInputs` 加 `is_first_mood: bool` 字段——`run_proactive_turn` 从 `read_current_mood_parsed()` 派生；mood 第一次时 true。
- 在 6 条 base rules 之后按条件 push：
  - **wake context rule**：`!inputs.wake_hint.trim().is_empty()` 时插一条"用户刚从离开桌子回来：问候要简短克制，先轻打招呼或简短关心一句，不要立刻提日程/工作类信息密集的话题"。
  - **first-mood rule**：`is_first_mood == true` 时插一条"第一次开口：你还没有写过 ai_insights/current_mood 记忆条目，开口后应当用 memory_edit create 而非 update 来初始化"。
- builder 调用 `s.extend(proactive_rules(inputs))` 把 inputs 透传。
- 现有 3 个 rules 测试全部更新为 `proactive_rules(&base_inputs())` 调用形式。
- 4 个新单测：
  - `wake_rule_appears_when_wake_hint_present`：wake_hint 非空 → 7 条规则。
  - `first_mood_rule_appears_when_flagged`：is_first_mood=true → 7 条规则。
  - `both_context_rules_can_coexist`：两个标志同时打开 → 8 条规则。
  - `no_context_rules_with_default_inputs`：默认 base_inputs → 6 条规则（baseline 锚点）。
- 总测试 105 + 4 = **109 个**，全过；cargo + tsc 双过；零 warning。
- 现在 prompt 真正"按情况说话"：wake 时不机关炮提日程、第一次时模型知道走 create 而不是 update。

## 2026-05-03 — Iter 52：约束段抽成 proactive_rules()
- 新 `pub fn proactive_rules() -> Vec<String>`：6 条 prompt 约束（silent marker 用法 / 单句话 / 工具说明 / 工具去重 / 心情更新规范）一次性 push 到 Vec，每条以 "- " bullet 开头。
- `build_proactive_prompt` 里把"约束："header 之后的 6 行 push（含 inline format!）替换成 `s.extend(proactive_rules())`，本体减 16 行。
- 3 个新单测：
  - `rules_count_and_format`：6 条且每条以 "- " 起头——加新规则要更新计数，pin 住该决定。
  - `rules_interpolate_constants`：SILENT_MARKER / MOOD_CATEGORY / MOOD_TITLE / 4 个 motion tag 都在 joined 文本中。
  - `rules_appear_in_full_prompt`：build_proactive_prompt 输出包含每一条 rule，确保未来 builder 不会"漏接" rules。
- 总测试 102 + 3 = **105 个**，全过；cargo + tsc 双过；零 warning。
- 后续 Iter 53 可以让 `proactive_rules` 接受 `&PromptInputs` 按上下文动态加减规则——前提是这次抽好了。

## 2026-05-03 — Iter 51：proactive prompt 改 builder 模式
- 新 `pub struct PromptInputs<'a>` 9 字段（time / period / idle_minutes / input_hint / cadence_hint / mood_hint / focus_hint / wake_hint / speech_hint）。3 个固定段直接 push，3 个可选段通过 `push_if_nonempty` 跳过空值。
- 新 `pub fn build_proactive_prompt(&PromptInputs) -> String`：用 `Vec<String>` + `join("\n")` 装配。约束段、motion 规则段都从 format! 模板 inline 提到这里，每段 1 行 push。
- 新私有 `fn push_if_nonempty(sections: &mut Vec<String>, s: &str)` 工具——trim 后判空，避免 join 出来留空行。
- run_proactive_turn 删掉 27 行 `let prompt = format!(...)`，换成构造 PromptInputs + 调 builder（10 行）。原 12 个 named placeholder + arg list 全消失。
- 6 个新单测覆盖：必有段都出现、空可选段不出空行、focus/wake/speech 各自被注入时正确、MOOD_CATEGORY/MOOD_TITLE 仍 interpolated。
- 总测试 96 + 6 = **102 个**，全过；cargo + tsc 双过；零 warning。
- 净收益：加新 hint 段从"struct + format! 模板 + arg list 改 4 处"降到"struct 加字段 + builder 加 push_if_nonempty 一行"。Iter 52 把约束段也类似化后，proactive prompt 的可扩展性 ≈ 配置文件级别。

## 2026-05-03 — Iter 50：PanelDebug 显示对话基调摘要
- 后端 `proactive.rs` 新增：
  - `pub struct ToneSnapshot { period, cadence, since_last_proactive_minutes, wake_seconds_ago, mood_text, mood_motion }`（serde::Serialize）。
  - `#[tauri::command] pub async fn get_tone_snapshot(InteractionClockStore, WakeDetectorStore)` —— 把所有 prompt 用到的"对话基调"信号一次性算出来。复用现有的 `period_of_day` / `idle_tier` / `read_current_mood_parsed` / `last_wake_seconds_ago`。
  - `lib.rs` `tauri::generate_handler!` 注册 `get_tone_snapshot`。
- 前端 `PanelDebug.tsx`：
  - 新 interface `ToneSnapshot` + state；fetchLogs 6 路 Promise.all（多一路）。
  - toolbar 之后插一段浅灰小条：单行 flex-wrap，由表情 emoji + 紧凑文本组成：`⏱ 上午`、`💬 几小时没说话（150m）`、`☀ wake 60s`（蓝色）、`★ motion: Tap`（紫色）、`☁ mood: ...`（截断显示）。
  - 每段独立 title tooltip 解释字段含义；条件渲染——值缺失就不显示该段。
  - mood 文本一行截断 + ellipsis 避免拖长行。
- tsc + 96 tests 双过；零 warning。
- 现在用户开 panel 一眼能看到"此时此刻 LLM 看到的所有对话基调信号"，调试"宠物为什么这么说"的速度 ×10。

## 2026-05-03 — Iter 49：wake event 软化 cooldown / idle 阈值
- 新常量 `WAKE_GRACE_WINDOW_SECS = 600`、辅助 `wake_recent(Option<u64>) -> bool`，用 `matches!` 表达 ≤600s 即生效。
- `evaluate_pre_input_idle` 签名加 `wake_seconds_ago: Option<u64>` 参数（第 5 个）；in-grace 时：
  - **cooldown gate**：直接跳过（`wake_soft && ...` 短路）。理由：wake 后用户大概率离开过桌子，"刚说过话别再说"的语义不再成立。
  - **idle gate**：`(raw_threshold / 2).max(60)`。比如默认 900s 减半到 450s；用户回来 7.5 分钟就够。floor 60s 防御过度软化。
- awaiting / quiet_hours / focus_mode 这三道 **不软化** —— 是用户显式偏好，wake 不该越权。
- evaluate_loop_tick 在 evaluate 前调 `WakeDetectorStore.last_wake_seconds_ago().await` 取参数。
- 现有 12+ 测试 callsite 全部加第 5 参 `None`（不关心 wake）。
- 新增 6 个 wake gate 测试覆盖：
  - cooldown 在 grace 内被跳过 / grace 过期后照常生效
  - idle threshold 被减半 / 减半后仍 floor 60s
  - awaiting 不被 wake 软化
  - quiet_hours 不被 wake 软化
- 总测试 90 + 6 = **96 个**，全过；cargo + tsc 双过；零 warning。
- 完整意义：现在用户开盖回来宠物会更主动；但夜里、focus 下、宠物刚说过没回应时仍尊重边界。

## 2026-05-03 — Iter 48：wake-from-sleep 检测 + prompt 注入
- 新模块 `src-tauri/src/wake_detector.rs`：
  - 纯函数 `detect_wake(prev: Option<Instant>, now: Instant, threshold) -> Option<Duration>`，可单测无需 sleep。
  - `pub struct WakeDetector { last_observation, last_wake_at }` + `observe()` / `last_wake_seconds_ago()` async API。
  - 5 单测覆盖：first observation / 小间隔 / 正好阈值 / 越过阈值 / 时钟倒退（防御性 None）。
- 阈值 `WAKE_GAP_THRESHOLD_SECS = 600`：proactive 默认 sleep 300s；阈值 > sleep × 2 避免常规调度抖动误触。
- `WakeDetectorStore = Arc<WakeDetector>` + `new_wake_detector()`，lib.rs 注册。
- 跨平台思路（不用 NSWorkspace）：spawn loop 每次 iteration 在顶部 `observe()` 心跳，间隔异常 = 进程被挂起 = 系统休眠。日志写一行 "wake-from-sleep detected (gap Ns)"。
- `run_proactive_turn` 在 mood/focus_hint 之后注入 `wake_hint`：若 last_wake 在 ≤ 10 分钟内，prompt 多一句"用户的电脑在大约 N 秒前刚从休眠唤醒，看起来 ta 离开桌子一会儿后才回来"。LLM 可以挑欢迎回来的话题。
- 总测试 85 + 5 = **90 个**，全过；cargo + tsc 双过；零 warning。
- 是 informational 注入，不动 gate——避免每次午休都被宠物欢迎回来打断。Iter 49 探索把 wake 升级为 gate 强信号。

## 2026-05-03 — Iter 47：log_rotation 抽公共 util
- 新模块 `src-tauri/src/log_rotation.rs`：
  - `pub fn rotated_path(&Path) -> PathBuf`（OsString append `.1`，避开 with_extension 替换扩展的陷阱）
  - `pub async fn rotate_if_needed(&Path, max_bytes) -> io::Result<bool>`
  - 6 个测试（path 标准/无扩展、rotates / no-op / overwrite / missing）
- focus_tracker.rs 删掉私有的 `rotate_if_needed` / `rotated_path` 实现 + 6 个测试，改 `use crate::log_rotation::rotate_if_needed`。注释指出测试搬家。
- speech_history.rs 加 `SPEECH_HISTORY_MAX_BYTES = 100_000` 常量，`record_speech_inner` 在 read 之前 best-effort 调 `rotate_if_needed`——LLM 万一抽风输出超长字符串也不会让单文件膨胀（trim 50 行的兜底 + size 兜底，双层防御）。
- lib.rs 加 `mod log_rotation;`。
- 测试总数不变 = 85（focus_tracker 减 6 + log_rotation 加 6），cargo + tsc 双过，零 warning。
- 净收益：log rotation 的"rule of two"已触发，第三个模块要 rotation 时 0 行新代码 + 复用现成测试。

## 2026-05-03 — Iter 46：PanelDebug 显示宠物最近发言
- 后端 `speech_history.rs` 加 `#[tauri::command] pub async fn get_recent_speeches(n: Option<usize>) -> Vec<String>` —— 直接走 `recent_speeches`（默认 n=10）。lib.rs 注册。
- 前端 `PanelDebug.tsx`：
  - 加 `recentSpeeches: string[]` state，fetchLogs 五路 Promise.all 并联。
  - 决策段之后插一段紫色背景 (#fdf4ff) 的"宠物最近主动说过的 N 句"卡片，max-height 120px scroll。
  - 每行布局：左侧浅紫等宽 `HH:MM`（从 ISO timestamp 切片 11..16），右侧灰色文本主体。无 ts 行 fallback 显示原文。
  - 仅 `recentSpeeches.length > 0` 渲染——首次启动 / 文件丢失时不出空卡片。
- 颜色编码区分既有区块：决策段灰白 / Cache 蓝 / Tag 紫色 / Speech 紫色背景——视觉上 Speech 也是 mood/personality 维度，与 Tag 同色系一致。
- tsc + 85 tests 双过；零 warning。
- 现在用户调试"宠物为什么这么说话"看 panel 一眼就明白：决策段说"为什么开口"，speech 段说"具体说了啥"。

## 2026-05-03 — Iter 45：宠物自言自语流持久化 + 反话题重复
- 新模块 `src-tauri/src/speech_history.rs`：append-only 文件 `~/.config/pet/speech_history.log`，每行 `<ISO ts> <text>`（newline 平到 space）。
- API：
  - `pub async fn record_speech(text)` — append + trim 到 `SPEECH_HISTORY_CAP=50` 条，best-effort（IO error 吞掉）。
  - `pub async fn recent_speeches(n)` — 读最近 n 条，oldest→newest 顺序。
  - `pub fn parse_recent(content, n)` — 纯函数版便于单测。
  - `pub fn strip_timestamp(line)` — 砍掉前缀只留正文，prompt 里渲染用。
- `lib.rs` 加 `mod speech_history;`。
- `proactive::run_proactive_turn` 在构造 prompt 阶段调 `recent_speeches(5).await`，把每条 strip_timestamp 后做成 bullets，注入新 `{speech_hint}` 占位（模板里紧跟 focus_hint 之后）。空时不渲染。
- 在 `clock.mark_proactive_spoken()` 之后追加 `record_speech(reply_trimmed).await`，让本轮发言下次能被看到。
- 9 个新单测覆盖：parse_recent 边界（empty/n=0/少于 n/正好 n/多于 n/空行）、strip_timestamp（标准/无空格）+ 文件层 round-trip 测试用 std::env::temp_dir() 自建唯一目录。
- 总测试 76 + 9 = **85 个**，全过；cargo check 零 warning。
- 现在宠物有了独立于 session 的"自我记忆"：即便用户切了新 session 或 chat.max_context_messages 把旧消息裁了，宠物仍知道自己上句说啥，避免连续两次"早上好咖啡"。

## 2026-05-03 — Iter 44：cadence hint 让 proactive 切换对话基调
- 新 `pub fn idle_tier(minutes: u64) -> &'static str` 在 proactive.rs，5 档：
  - 0–15 分：「刚说过话，话题还热」
  - 16–60 分：「聊过一会儿了」
  - 61–360 分（≤ 6 小时）：「几小时没说话」
  - 361–1440 分（≤ 一天）：「已经隔了大半天」
  - 1441+：「上次聊已经是昨天或更早」
- run_proactive_turn 在 mood_hint 之后再 `clock.snapshot().await` 取 `since_last_proactive_seconds`，构造 cadence_hint：「距上次你主动开口约 N 分钟（{tier}）。」none 时给 first-time 文案。
- prompt 模板紧贴 `{minutes}` 后多一行 `{cadence_hint}`，让 LLM 同时看到"距用户互动多久"和"距自己上次开口多久"——前者是 idle 状态，后者是对话节奏。
- 与 idle_minutes 区分：idle_minutes 计入用户上次任何动作；cadence 只算上一次 proactive。前者决定 gate，后者决定语气。
- 测试 `mod cadence_tests` 14 个 case：每档代表分钟 + 每个 boundary 两侧（15/16/60/61/360/361/1440/1441）。
- 总测试 74 + 2（按 mod 算）= **76 个**，全过；cargo + tsc 双过；零 warning。

## 2026-05-03 — Iter 43：proactive prompt 加 time-of-day 语义
- 新 `pub fn period_of_day(hour: u8) -> &'static str` 在 proactive.rs：把 0–23 小时映射成中文时段词（清晨 / 上午 / 中午 / 下午 / 傍晚 / 晚上 / 深夜）。边界按中文日常说法：5–7 清晨，8–10 上午，11–12 中午，13–16 下午，17–18 傍晚，19–21 晚上，22–4 深夜。
- run_proactive_turn prompt 头部从 `"现在是 {time}"` 升到 `"现在是 {time}（{period}）"`——多 1 列开销可忽略，给 LLM 一个语义抓手能让它说"傍晚的咖啡"而不是"15:47 的咖啡"。
- 新 `mod period_tests` 14 个 case：每个时段一个代表小时（happy path）+ 边界 hour（4/5/7/8/10/11/13/16/17/19/21/22/0），覆盖每个跳变点的两侧。
- 总测试 72 + 2（按 mod 算）= **74 个**，全过；cargo + tsc 双过；零 warning。
- 不动反应式 chat / consolidate prompt——time-of-day 与"主动找话题"最搭，反应式是用户驱动话题，注入反而冗余。

## 2026-05-03 — Iter 42：counters 合并为 ProcessCounters
- 新 `pub struct ProcessCounters { cache: CacheCounters, mood_tag: MoodTagCounters }` 在 commands/debug.rs，default 派生让 `new_process_counters()` 一行返 Arc<...>。doc comment 写明扩展原则："加新 counter 组只在这里加字段 + 加 Tauri 命令，不动 ToolContext 与 5 callsite"。
- ToolContext 字段从 `cache_counters + mood_tag_counters` 缩成单 `process_counters: ProcessCountersStore`，`new` / `from_states` 各砍一参，`for_test` 简化。
- 4 个 Tauri 命令（get_cache_stats / reset_cache_stats / get_mood_tag_stats / reset_mood_tag_stats）签名统一为 `State<ProcessCountersStore>`，访问改为 `counters.cache.*` / `counters.mood_tag.*`。
- 5 callsite（proactive / chat / consolidate / telegram bot / from_states）每处少抓一个 state；reconnect_telegram + TelegramBot::start + HandlerState 都减少一参；lib.rs `.manage()` 从两个变一个。
- ToolRegistry::log_cache_summary 内 `ctx.cache_counters.*` → `ctx.process_counters.cache.*`（registry 自身 + 测试都更新）。
- mood::read_mood_for_event 同理。
- 旧 `CacheCountersStore` / `MoodTagCountersStore` / `new_cache_counters` / `new_mood_tag_counters` 标 `#[cfg(test)]`：测试还在用它们独立测内部 struct，production 不再需要——避免 dead_code 警告同时保留测试入口。
- 测试 72 全过，cargo + tsc 双过，零 warning。
- 净减少：每加新 counter 组从 5 callsite + 5 import + 5 plumbing 降到 1 字段 + 1 命令。

## 2026-05-03 — Iter 41：mood_tag 重置按钮（与 cache 对称）
- 后端：新 Tauri 命令 `reset_mood_tag_stats(State<MoodTagCountersStore>)` 三个 `store(0, Relaxed)`，与 `reset_cache_stats` 行文一致；`lib.rs` 注册。
- 前端：`PanelDebug.tsx` 加 `handleResetMoodTagStats` 调 invoke + 乐观更新 React state；Tag 统计 span 包进 inline-flex 容器，旁边小号低对比"重置"按钮，与 Cache 重置按钮视觉一致。
- 1 个新单测 `mood_tag_counters_can_be_reset_to_zero` 验证 store(0) 语义。
- 总测试 71 + 1 = **72 个**，全过；cargo + tsc 双过；零 warning。

## 2026-05-03 — Iter 40：[motion: X] 前缀遵守率统计
- 后端：
  - 新 `MoodTagCounters { with_tag, without_tag, no_mood: AtomicU64 }` + `MoodTagCountersStore` + `new_mood_tag_counters()` 在 commands/debug.rs。
  - 新 Tauri 命令 `get_mood_tag_stats`（lib.rs 注册）。
  - `ToolContext` 加 `mood_tag_counters: MoodTagCountersStore` 字段，`new` / `from_states` / `for_test` 都更新。
  - `mood::read_mood_for_event` 签名从 `(&LogStore, &str)` 改为 `(&ToolContext, &str)`，内部按解析结果 bump 三个 atomic 之一。
  - 4 个 callsite（proactive / chat / telegram / consolidate）改用 ctx 参数；ToolContext::new 各处补第 4 参数。
  - lib.rs `.manage(new_mood_tag_counters())` + 注册命令。reconnect_telegram + TelegramBot::start + HandlerState 全程透传。
  - 1 个新单测 `mood_tag_counters_default_to_zero_and_accumulate`。
- 前端 PanelDebug：
  - 新 interface `MoodTagStats`，新 state，fetchLogs 四路 Promise.all 一并取。
  - 工具栏在 cache stats 后加紫色 Tag 统计："Tag H/T (P%)"，hover tooltip 解释含义；total=0 时不渲染。
- 总测试 70 + 1 = **71 个**，全过；cargo + tsc 双过。
- 现在不需要交互式跑就能从 panel 看 LLM 守不守约：H/T 比例稳定接近 100% 就说明 prompt 工作得好；显著低于则需要回炉调 prompt（替代了 Iter 12b 需要交互的实测）。

## 2026-05-02 — Iter 39：proactive 决策 reason 中文化
- `PanelDebug.tsx` 加 `localizeReason(kind, reason)` 辅助：
  - Silent 三个 key 一对一映射："disabled" → "已禁用 (proactive.enabled = false)"，"quiet_hours" → "安静时段内"，"idle_below_threshold" → "用户活跃时间未到阈值"。
  - Skip 先剥掉 "Proactive: skip — " 前缀（plumbing 噪音），再按 startsWith 翻译几个已知短语：awaiting user reply / cooldown / user active / macOS Focus，保留动态数字。
  - Run 不动（已经结构化）。
  - 未识别 fallback 到原字符串——未来后端加新 reason，UI 退化为英文显示而不是空白。
- 决策行渲染时 `{localizeReason(d.kind, d.reason)}` 替换原始 `{d.reason}`。
- 选前端映射而非后端中文化的理由：reason 字符串是稳定 key，让后端只输出语义；UI 决定语言。日后加英文界面只需扩 localize 表，后端不动。
- tsc --noEmit 干净；后端零改动。
- 现在面板"最近主动开口判断"区域中文用户一眼能懂：`19:32:14 Skip 等待用户回复上一条主动消息`。

## 2026-05-02 — Iter 38：proactive 决策记录 + 面板显示
- 后端：
  - `LoopAction::Silent` 改为 `Silent { reason: &'static str }`，3 处返回点分别给"disabled" / "quiet_hours" / "idle_below_threshold"。已有测试和 spawn 内 match arm 同步更新。
  - 新模块 `decision_log.rs`：`pub struct ProactiveDecision { timestamp, kind, reason }`（serde::Serialize）+ `DecisionLog { buf: Mutex<VecDeque>, capacity 10 }` ring buffer + `push` / `snapshot` 方法。
  - `pub type DecisionLogStore = Arc<DecisionLog>` + `new_decision_log()` 工厂。
  - 新 Tauri 命令 `get_proactive_decisions(State) -> Vec<ProactiveDecision>`。
  - `lib.rs` 注册模块、State、命令。
  - `proactive::spawn` 主循环在 evaluate 后、dispatch 前先 `decisions.push("Silent"|"Skip"|"Run", reason)`，让所有路径都被记录——包括 Silent 的"沉默却有原因"分支。
  - 3 个新 decision_log 单测：empty / 顺序保留 / 容量超限丢最老。
- 前端 `PanelDebug.tsx`：
  - 新接口 `ProactiveDecision`，新 state，fetchLogs 三路 Promise.all 一并取。
  - toolbar 之后插一段 max-height=120px 的滚动区，等宽字体显示 `时间 KIND reason`。Run=绿、Skip=橙、Silent=灰。说明 "最新在底部"。
  - 仅当 `decisions.length > 0` 时渲染。
- 总测试 67 + 3 = **70 个**，全过；cargo + tsc 双过；零 warning。
- 现在用户调试"宠物为什么不说话"只看 panel 一眼就懂——之前要 grep 50 行日志才知道哪条 gate 跳过了。

## 2026-05-02 — Iter 37：chat.max_context_messages 接进两个设置 UI
- `SettingsPanel.tsx`（小窗 modal）在记忆整理段后加"对话上下文 (Chat)"分组：单字段 NumberField + 同样的两列网格留半边占位（视觉一致）。
- `panel/PanelSettings.tsx`（独立面板视图）加同名 section（中文标题"对话上下文"），下方加一行浅灰小字解释："桌面 chat 和 Telegram 都按此上限裁剪。前端仍展示全部消息，仅发给 LLM 时裁。"
- 标签写"历史保留条数 (0=不限)"——0 这个语义对用户陌生，必须在 label 里直说，不能让人猜。
- 复用现有的 `NumberField` / `PanelNumberField` 包装组件（Iter 27 抽出来的），新增 0 行公共代码。
- tsc --noEmit 干净。
- 现在 chat trim 的配置链路完全打通：用户改 UI → 写 settings.yaml → AiConfig::from_settings 读 → trim_to_context 应用。

## 2026-05-02 — Iter 36：chat 历史 trim 配置化 + 桌面 chat 默认上限
- 新 `pub struct ChatConfig { max_context_messages: usize }`（默认 50）加到 `AppSettings.chat`。
- `AiConfig` 也加 `max_context_messages: usize` 字段，从 settings.chat 复制；这样 chat 命令拿 config 时直接看得到。
- `commands/chat::trim_to_context(messages, max)` 纯函数：保留所有前导 system 消息，drain 中间最老的非 system 直到 history ≤ max。`max=0` 关闭，short history 直返。
- chat 命令在 `inject_mood_note` 之前调一次 trim——之前桌面 chat 完全靠 frontend 提交全量历史，长会话会无限膨胀 token。
- telegram bot：删掉硬编码 `MAX_CONTEXT_MESSAGES = 50` 常量；改读 `AiConfig::from_settings().max_context_messages`，与桌面 chat 共用同一 trim 函数。原 telegram 那段 raw `Vec<Value>` 切片逻辑改为先转 ChatMessage 再 trim，少一份重复。
- 5 个新单测覆盖 trim：max=0 不动 / 短于 cap 不动 / 标准裁剪 / 多个前导 system 都保留 / 完全无 system。
- 前端：`useSettings.ts` 加 `ChatConfig` interface + `chat` 字段 + DEFAULT_SETTINGS 默认；`PanelSettings.tsx` 初值也补，TS 类型对齐。
- 总测试 62 + 5 = **67 个**，全过；cargo + tsc 双过；零 warning。
- UI 输入控件留作 Iter 37（后端就位即可，UI 拆开提交）。

## 2026-05-02 — Iter 35：reset cache 统计按钮
- 后端：`commands/debug.rs` 加 `pub fn reset_cache_stats(counters: State<CacheCountersStore>)`，三个 `store(0, Relaxed)`。注释把 "mirrors clear_logs" 写明，让读者立刻知道意图。
- 注册到 `lib.rs` `tauri::generate_handler!`。
- 前端：`PanelDebug.tsx` 加 `handleResetCacheStats` 调 `invoke("reset_cache_stats")` 并立刻把 React state 也清零（避免 1 秒 polling 延迟）。
- UI 调整：Cache 统计 span 包进 inline-flex 容器，旁边加一个小"重置"按钮，浅色描边、低调风格——重置 cache 不是常用操作不该抢眼。tooltip 解释"重置 cache 统计计数器"。
- 加 1 个新单测 `cache_counters_can_be_reset_to_zero`：直接调 store(0) 验证语义；Tauri State 那层是 plumbing 不需要单独测。
- 总测试 61 + 1 = **62 个**，全过；cargo check 零 warning；tsc --noEmit 干净。

## 2026-05-02 — Iter 34：cache 累计搬到 atomic Tauri State
- 新 `pub struct CacheCounters { turns, hits, calls: AtomicU64 }` + `pub type CacheCountersStore = Arc<CacheCounters>` + `pub fn new_cache_counters()` 都在 `commands/debug.rs`。
- `lib.rs` 注册到 Tauri State：`.manage(new_cache_counters())`；setup 里把 store 拷贝传进 telegram::start。
- `ToolContext` 加 `cache_counters: CacheCountersStore` 字段；`new` / `from_states` 签名加第三参；新增 `#[cfg(test)] for_test` 构造器自动给 fresh counters。
- 5 个 ToolContext 调用点全部更新（proactive / consolidate / chat / telegram / registry tests）。telegram 路径还要 HandlerState 多一个字段 + `TelegramBot::start` 多一个 arg + reconnect_telegram 透传。
- `ToolRegistry::log_cache_summary` 在写 log 行后多三行 `fetch_add(.., Relaxed)` 推到全局 counters。
- `get_cache_stats` 重写为直接 load atomic（不再依赖 log 解析）；删除 `parse_cache_summary` + 5 个老解析测试。
- 加 2 个 atomic 累计测试 + 2 个 registry 集成测试（验证 log_cache_summary 真的 bump counters，empty case 不 bump）。
- 总测试 62 - 5（删 parser tests）+ 2 + 2 = **61 个**，全过；cargo check 零 warning。
- 现在即便 LogStore 5000 行 cap 把旧 summary 行裁掉，PanelDebug 显示的累计命中率也保持正确——cap 与统计两关注点彻底解耦。

## 2026-05-02 — Iter 33a：LogStore size cap 提升 + 命名 + 测试
- 发现：原 TODO 写"unbounded Vec<String>"是错的——`write_log` 已有硬编码 500 行 cap，但偏小（≈ 25 个 LLM turn 就溢出）。
- 把魔法数 500 提为 `pub const MAX_LOG_LINES: usize = 5000`（约几百 turn / 一段会话量），加 doc comment 说明 trade-off。
- `write_log` 用常量替换硬编码值。逻辑不变（`drain(0..overflow)`）。
- 新增 2 个单测在 `commands::debug::tests`：
  - `write_log_caps_at_max_lines`：写 MAX+50 条，验证 buffer 停在 cap，最新行保留、最老的 50 行被丢。
  - `write_log_under_cap_is_pure_append`：3 条不触发 cap，顺序保持。
- on-disk app.log 不受 cap 影响（注释说明）；磁盘文件由用户/操作系统侧管理。
- 拆分 Iter 33 的两半：本次只做 cap，atomic 累计统计列为 Iter 34（涉及 Tauri State + ToolContext 改动较多）。
- 总测试 60 + 2 = **62 个**，全过；cargo check 零 warning。

## 2026-05-02 — Iter 32：cache 统计接进 PanelDebug
- `panel/PanelDebug.tsx` 加 `interface CacheStats { turns, total_hits, total_calls }` + 同名 React state（默认 0/0/0）。
- 把原本只 fetch 一次 logs 的 `fetchLogs` 改为 `Promise.all([get_logs, get_cache_stats])`，1 秒 polling 一并获取。
- 工具栏右侧（"X 条日志" 计数前）插一段统计 span：`Cache H/T (P%) · N turns`，蓝色 + 等宽字体，total_calls=0 时不渲染避免初始空数据闪烁。
- 鼠标 hover 显示中文 tooltip 解释口语化含义（不全员都懂术语）。
- 不引新依赖，纯样式 + 已暴露的 Tauri 命令。
- tsc --noEmit 通过。
- 后续 Iter 33 处理 LogStore 长时间运行的内存上限。

## 2026-05-02 — Iter 31：cache 统计的解析 + Tauri 命令
- 新纯函数 `parse_cache_summary(&str) -> Option<(u64, u64)>`：从 `"... Tool cache summary: H/T hits (P%)"` 提取 (hits, total)，不匹配返 None。剥离独立函数让单测无需 mock LogStore。
- 新 `pub struct CacheStats { turns, total_hits, total_calls }`（serde::Serialize），供 frontend 消费。
- 新 Tauri 命令 `get_cache_stats(LogStore) -> CacheStats`：遍历 LogStore 所有行，对每行调 parse，命中即累加。`turns` 是命中行数（≈ 跑完且至少有一次 cacheable 调用的 LLM turn 数）。
- `lib.rs` `tauri::generate_handler!` 注册 `get_cache_stats`。
- 5 个新单测：canonical / 0 hits / 100% / 不相关行 / 数字格式异常——重点覆盖 negative path。
- 总测试 55 + 5 = **60 个**，全过；cargo check 零 warning。
- 后续 Iter 32 在 panel UI 调用此命令做渲染。

## 2026-05-02 — Iter 30：cache 命中聚合统计
- `ToolRegistry` 加两个原子计数器：`cache_hits` / `cache_misses`（`AtomicU64`）。
- `execute()` 在 cache 命中时 `cache_hits.fetch_add(1)`，写入新缓存项时 `cache_misses.fetch_add(1)`。Relaxed ordering 够用——只是统计计数没并发依赖。
- 新公共方法：
  - `cache_stats() -> (hits, misses)`：lock-free 双 load，方便测试和外部读取。
  - `log_cache_summary(ctx)`：写一行 `"Tool cache summary: H/T hits (P%)"`；当 total=0（没有任何 cacheable tool 调用）时直接 return，避免每次主动开口检查都刷一行 0/0。
- `run_chat_pipeline` 在 "Final response"（无新 tool_calls）成功分支里调 `registry.log_cache_summary(ctx)`——这条路径覆盖所有 4 个调用者（proactive / chat / telegram / consolidate），无需各自加。
- 2 个新单测：`cache_stats_track_hits_and_misses`（1 miss + 2 hits → (2, 1)）/ `cache_stats_ignore_non_cacheable_tools`（mutating tool 调用不计数）。
- 总测试 53 + 2 = **55 个**，全过。
- cargo check 通过，零 warning。
- 后续可加 Tauri 命令读 LogStore 过滤这行做面板可视化（Iter 31）。

## 2026-05-02 — Iter 29：proactive prompt 加"信任首次结果"指引
- 在 proactive prompt 工具列表后追加一条规则：明确告诉 LLM 三个环境工具（`get_active_window` / `get_weather` / `get_upcoming_events`）"同一次主动开口检查内重复调用同样的参数会拿到完全一样的结果"，要"相信首次返回值"。
- 措辞策略：不暴露 cache 实现（"我们后端 dedupe 了你"），而是表达成行为指引（"一次就够，不要再问"）——对 LLM 来说更直接，也避免让它思考工程细节。
- 半角引号 vs 全角引号：第一次写时用了 ASCII `"再确认一下"`，恰好闭合了 Rust 的 format! 字符串导致 syntax error。改成全角「再确认一下」立刻通过。这是中文 prompt 在 Rust raw 字符串里的常见坑——记一笔。
- cargo check + 53 tests 全过，零 warning。

## 2026-05-02 — Iter 28：环境感知工具的 per-tick 缓存
- `ToolRegistry` 加 `cache: TokioMutex<HashMap<String, String>>` 字段；构造函数都通过新私有 `with_tools` 走，统一初始化空缓存。
- 新常量 `CACHEABLE_TOOLS = &["get_active_window", "get_weather", "get_upcoming_events"]`：只读环境感知工具白名单。注释明确警告"never add mutating tools"。
- `execute(name, args, ctx)`：若 name 在白名单，构造 cache_key = `name|args`；命中直接返回 + 写一条 "Tool cache hit" 日志；未命中执行后存入。
- 缓存生命周期 = ToolRegistry 生命周期 = 一次 LLM turn（pipeline 每次重建）。自然 tick-scoped，无需手动清空。
- 4 个新测试（用 `CountingTool` 内部 mock）：
  - `cacheable_tool_called_once_for_same_args` — 同参 2 次调用，underlying tool 只跑 1 次
  - `cacheable_tool_different_args_re_executes` — 不同参分开计数
  - `non_cacheable_tool_always_executes` — `memory_edit` 3 次都执行
  - `unknown_tool_returns_error_and_does_not_cache` — 不会缓存错误
- 私有 `fn with_tools(tools, mcp) -> Self` 让测试直接注入 mock 工具列表，不破坏 `new()` 的固定 11 工具签名。
- 总测试 49 + 4 = **53 个**，全过。
- cargo check 通过，零 warning。

## 2026-05-02 — Iter 27：抽共享 NumberField 组件
- 新文件 `src/components/common/NumberField.tsx`：通用 `<input type="number">` 包装，含 NaN 守护和 onChange 类型转换。`labelStyle` / `inputStyle` 作为 props 注入，让两个 panel 各自保留视觉差异。
- `SettingsPanel.tsx` 删本地 `NumberField`，改成一层薄 wrapper：`function NumberField(props) { return <SharedNumberField {...props} labelStyle={labelStyle} inputStyle={inputStyle} /> }`。8 处调用 site 一字未改。
- `panel/PanelSettings.tsx` 同样：原 `PanelNumberField` 由 17 行实现缩到 3 行 wrapper。
- 设计权衡：本可以让调用 site 直接传 labelStyle/inputStyle，但那样每处调用要多两行 props 重复。Wrapper 模式让"共享逻辑"与"局部样式绑定"分离——逻辑改 SharedNumberField 一处，样式改各自 panel 一处。
- 顺手清理 TODO.md 里"PanelSettings.tsx：把新加的 Proactive / Consolidate 接进 panel 形式视图"——上一轮已完成，留着是冗余项。
- tsc --noEmit 通过。

## 2026-05-02 — PanelSettings.tsx 接 Proactive / Consolidate（顺手补 Iter 21+22 加的字段）
- panel 形式视图（独立窗口的设置面板，不同于小窗 SettingsPanel modal）原本只展示 Live2D / LLM / MCP / Telegram / SOUL，没暴露 proactive 和 memory_consolidate。
- 在 Telegram 段后、SOUL 段前插两个新 section：
  - **主动开口**：enabled checkbox + 6 个 NumberField（interval / cooldown / idle threshold / input idle / quiet_hours_start / quiet_hours_end）+ respect_focus_mode checkbox。
  - **记忆整理**：enabled checkbox + 2 个 NumberField（interval_hours / min_total_items）。
- 新增本地 `PanelNumberField` 组件 + `twoColRow` 样式：和 SettingsPanel.tsx 那套一致但没共用（两个组件库分别长出来，复用会改两处文件）。列入 Iter 27 重构。
- 配置全部受 `useSettings` 类型约束，前面 Iter 20+21 加的 quiet_hours / respect_focus_mode 字段也跟着自动接进 panel 视图。
- tsc --noEmit 通过。
- 现在用户从两个不同入口（小窗 SettingsPanel 模态 + 独立 panel 窗口）都可以改全部 proactive 和 consolidate 设置。

## 2026-05-02 — Iter 25：focus_history.log size-based rotation
- 新增常量 `MAX_LOG_BYTES = 1_000_000`（1MB ≈ 30k 行，正常使用一年以上）。
- 新增纯函数 `rotated_path(&Path) -> PathBuf`：对 `focus_history.log` 返回 `focus_history.log.1`（直接 append `.1` 到 OsStr，不走 with_extension 因为它会替换 `.log`）。
- 新增异步 `rotate_if_needed(&Path, max_bytes) -> io::Result<bool>`：读 metadata，文件不存在或 size < max 时返 false；超过则 `tokio::fs::rename` 到 `.1`（覆盖任何旧的 `.1`）。
- `append_event` 在写入前调用 `rotate_if_needed`，best-effort 忽略错误（rotation 失败不该阻断 tracker）。
- 6 个新测试覆盖：`rotated_path` 标准 + 无扩展名；`rotates_when_oversized` / `does_not_rotate_when_under_limit` / `rotation_overwrites_existing_dot_one` / `missing_file_is_no_op`。
- 不引 `tempfile` dev-dep：用 `std::env::temp_dir().join("pet-test-{label}-{nanos}")` 自建唯一临时目录，nanos 时间戳避免并行测试冲突。
- 总测试 43 + 6 = **49 个**，全过。
- cargo check 通过，零 warning。
- 只保留一代历史（`.log` 现役 + `.log.1` 上一段），不做 `.1 → .2` 多代滚动——LLM 看长期模式只关心最近，深历史价值低。

## 2026-05-02 — Iter 24：consolidate prompt 引导 LLM 读 focus history
- `consolidate.rs` 新增 `fn focus_history_hint() -> String`：检查 `~/.config/pet/focus_history.log` 是否存在，存在则返回一段 prompt 片段（绝对路径 + 格式示例 + 操作建议），不存在/无 config_dir 则返回空串。
- consolidation prompt 模板加 `{focus_log_hint}` 占位符，紧贴"特殊保护"段之后、"原则保守"之前。
- prompt 片段明确告诉 LLM：
  - 用 `read_file` 或 `bash tail -n 200` 读
  - 数据足以总结长期模式时（如 "用户每天工作 focus 平均 N 小时"），用 `memory_edit` 写到 `user_profile`
  - "一条结论性 memory 比一千行原始日志更有用"
  - 数据 < 一周就先放着
- 文件不存在时空字符串：避免对没有 macOS focus 文件的环境刷出"读这个不存在的文件"指令。
- cargo check 通过、43 个 test 全过、零 warning。
- 完成 Iter 23 + 24 的两层结构闭环：tracker 写原始事件流 → consolidate 让 LLM 周期性把流压成结论。

## 2026-05-02 — Iter 23：focus 切换历史持久化到磁盘
- 新模块 `src-tauri/src/focus_tracker.rs`：
  - 后台 tokio 任务，每 60 秒 polls `focus_status()`。
  - in-memory `last_status: Option<FocusStatus>`；用纯函数 `classify_transition(prev, curr)` 判定要不要写日志。
  - 检测到事件时往 `~/.config/pet/focus_history.log` 追加一行：`<ISO 8601 时间> on:work` / `off` / `switch:personal`。
  - 路径用 `dirs::config_dir().join("pet/focus_history.log")`，与 memory 目录同根。
  - format 故意简单（一行一事件、空格分隔），grep / awk 都能直接读，不需要 JSON 解析器。
- `classify_transition` 4 种状态：第一次观察 inactive 不写、第一次观察 active 写 `on:NAME`、active↔inactive 翻转、active 期间换 mode 写 `switch:NEW`。同状态返 None。
- 7 个新单元测试覆盖每个 case + 空 name 退化为 `on:`。
- `lib.rs` 加 `mod focus_tracker;`，setup 末尾 `focus_tracker::spawn(app.handle().clone())`，与 proactive / consolidate 并列启动。
- 总测试 36 + 7 = **43 个**，全过。
- cargo check 通过，零 warning。
- 后续 Iter 24 让 consolidate 知道这个文件存在并主动总结。

## 2026-05-02 — Iter 22：focus mode 名字注入 proactive prompt
- `focus_mode.rs` 重构：
  - 新 `FocusStatus { active: bool, name: Option<String> }`（derive Debug/Clone/PartialEq/Eq）。
  - 新 `pub async fn focus_status() -> Option<FocusStatus>`：异步读 + 调用纯解析函数。
  - 新 `pub fn parse_focus_status(&Value) -> FocusStatus`：纯函数，从 `data[0].storeAssertionRecords[0].assertionDetails.assertionDetailsModeIdentifier` 拿 mode id（形如 `com.apple.donotdisturb.mode.work`），按最后一个 `.` 切片得 name="work"。任意层缺失 fail-soft，不 panic。
  - `focus_mode_active` 保留为薄 wrapper：`focus_status().map(|s| s.active)`，gate 代码不需改动。
  - 6 个新单元测试覆盖：empty data / missing data / 标准 identifier / active 但无 identifier / 非数组 data / identifier 无点号。
- `proactive::run_proactive_turn` 调 `focus_status().await` 拿状态：active 时构造 `focus_hint` 字符串注入 prompt（"用户当前开着 macOS Focus 模式：「work」（说明 ta 想专注，开口要克制）。"），否则空字符串。
- prompt 模板加 `{focus_hint}` 占位符，紧贴 mood_hint 之后。
- 注意：默认 `respect_focus_mode = true` 时 active focus 会被 gate 直接 skip，跑不到 run_proactive_turn。这条注入只在用户主动关闭 respect_focus_mode 时生效——给那种"focus 期间也允许少量打断"的用户更精准的提示。
- 总测试 8 mood + 22 gate + 6 focus parser = **36 个**，全过。
- cargo check 通过，零 warning。

## 2026-05-02 — Iter 21：focus-mode gate
- 新模块 `src-tauri/src/focus_mode.rs`：`pub async fn focus_mode_active() -> Option<bool>`。macOS 路径异步读 `~/Library/DoNotDisturb/DB/Assertions.json`，看顶层 `data` 数组是否非空判定 Focus 是否启用。文件不存在/读不到/解析失败 → None（fail open，不阻塞）。非 macOS → None。
- `lib.rs` 加 `mod focus_mode;`。
- `ProactiveConfig` 加 `respect_focus_mode: bool`（默认 true）；settings.rs 默认值同步。
- `evaluate_pre_input_idle` 签名加 `focus_active: Option<bool>` 参数；新 gate 排在 quiet_hours 之后、idle 之前。`cfg.respect_focus_mode && focus_active == Some(true)` 时返 `LoopAction::Skip("...Focus / DND is active")`（用 Skip 而非 Silent，因为这种情况不像夜里那么频繁，写日志便于事后回顾）。
- `evaluate_loop_tick` 调 `focus_mode_active().await`；只在 `respect_focus_mode == true` 时才发起读，省掉每 tick 一次文件读 IO。
- 12 处现有 gate 测试全部加第 4 参数 `None`（不关心 focus 状态）；新增 4 个 focus-mode 测试：
  - `active_skips_when_respected` / `active_passes_when_disabled_in_settings` / `inactive_passes` / `unknown_passes`
- 总测试 8 mood + 22 gate = **30 个**，全过。
- 前端：`useSettings.ts` ProactiveConfig + DEFAULT；`PanelSettings.tsx` 初值；`SettingsPanel.tsx` 加 checkbox "开启 macOS 勿扰/Focus 时不打扰"。
- cargo + tsc + cargo test 三过。

## 2026-05-02 — Iter 20：quiet-hours gate
- `ProactiveConfig` 加 `quiet_hours_start: u8` / `quiet_hours_end: u8`，默认 23 / 7（即 23:00–07:00 安静），同时 default 在 settings.rs。
- 新纯函数 `in_quiet_hours(hour, start, end) -> bool`：处理 same-day 和 wrap-midnight 两种窗口；start == end 视为关闭。
- `evaluate_pre_input_idle` 签名加 `hour: u8` 参数，新增 gate 排在 cooldown 之后、idle 之前。窗口内返回 `LoopAction::Silent`（夜里高频静音不打日志）。
- `evaluate_loop_tick` 内部用 `chrono::Local::now().hour() as u8` 取当前小时（加了 `chrono::Timelike` import）。
- 12 个原 gate test 都更新为传入 `NOON = 12`（保证不命中安静窗口）。新增 6 个 quiet-hours 测试：
  - 纯 helper（3）：`disabled_when_start_equals_end` / `same_day_window` / `wraps_midnight`
  - 集成进 evaluate_pre_input_idle（3）：`silent_during_window` / `passes_outside_window` / `disabled_does_not_block`
- 总测试 8 mood + 18 gate = **26 个**，全过。
- 前端：`useSettings.ts` `ProactiveConfig` interface + DEFAULT_SETTINGS 加两字段；`PanelSettings.tsx` form 初值同步；`SettingsPanel.tsx` 加一行两列 NumberField，输入夹到 0–23。
- cargo check / cargo test --lib / tsc --noEmit 三过，零 warning。
- 验证 Iter 18+19 基础设施：加一道完整 gate 含配置、测试、UI 大约 ~50 行代码即可。

## 2026-05-02 — Iter 19：proactive guard 表驱动测试
- 进一步重构：`evaluate_loop_tick` 拆成
  - `fn evaluate_pre_input_idle(cfg, snap) -> Result<(), LoopAction>`：纯同步，含 enabled/awaiting/cooldown/idle 4 道 gate，要么短路返回 Err(action) 要么返回 Ok(())。
  - `fn evaluate_input_idle_gate(cfg, snap, input_idle: Option<u64>) -> LoopAction`：纯同步，gate 5 (input_idle)。
  - `async fn evaluate_loop_tick(app, settings)`：保留原签名，内部串两段。
- `LoopAction` 加 `#[derive(Debug, PartialEq, Eq)]` 让 `assert_eq!` 干净。
- 新增 `mod gate_tests`，12 个测试：
  - **pre_input_idle**：disabled/awaiting/cooldown_active/cooldown_zero/cooldown_elapsed/idle_below_threshold/idle_clamp_to_60/all_pass。
  - **input_idle_gate**：zero_disables/none_treats_as_pass/below_min_skips/above_min_runs。
  - 包括 idle_threshold_seconds 被强制 clamp 到 60s 这种隐含规则的测试。
- 总测试 8 mood + 12 gate = **20 个**，全过。
- 加新 gate 时（如 quiet hours）只需在 evaluate_pre_input_idle 中插一段 + 加一个 case，回归现有 12 个 case 一秒内验证。
- cargo check / cargo test --lib 双过，零 warning。

## 2026-05-02 — Iter 18：proactive 主循环重构成 guard 列表 + 单一 sleep
- 新增 `enum LoopAction { Silent, Skip(String), Run { idle_seconds, input_idle_seconds } }`：把每 tick 的可能结果显式枚举出来，外层循环只处理这三种。
- 新增 `async fn evaluate_loop_tick(app, settings) -> LoopAction`：纯判断，依次跑 4 道 guard：
  1. enabled → Silent
  2. awaiting_user_reply → Skip(...)
  3. cooldown 未到 → Skip(...)
  4. idle 不足 → Silent（高频"还没空"不日志）
  5. input_idle 不达标 → Skip(...)
  6. 全过 → Run
- 主循环简化为：取 settings → 算 interval → match evaluate → 单一 `tokio::time::sleep(interval)`：
  - 行数从 ~70 下降到 ~25（含 match）。
  - 原来 4 处独立 `sleep + continue` 收成 1 处统一 sleep，行为不变（每个分支都最终睡 interval）。
  - log_store 只在需要写日志的分支懒取，避免 Silent 分支也付一次 Arc clone。
- cargo check 通过，零 warning。
- 现在加新 gate（如"focus mode 时不打扰"）只需在 evaluate_loop_tick 中插一段 `if cond { return LoopAction::Skip(...); }`，主循环不动。

## 2026-05-02 — Iter 17：清理预存 dead_code warning
- 删除 `commands/chat.rs::CollectingSink::take_text`：原意是给非流式 caller（Telegram）取最终文本用，但 Telegram bot 实际上是用 `run_chat_pipeline` 的返回值直接拿，从未调用 take_text。Sink 自己只 push 不 take，删掉无副作用。
- 删除 `mcp/manager.rs::McpManager::has_tool`：MCP tool 路由已经走 `is_mcp_tool` 路径（在 ToolRegistry），manager 自己的 has_tool 没有任何 caller。
- 全 grep 确认两者只剩定义没有调用。
- `cargo check` 输出从 "2 warnings" 变成 "no warnings"——以后任何新写代码引入的 dead_code 提示都能立刻看到，不会被遗留 warning 盖住。
- `cargo test --lib` 8 测试仍全过。

## 2026-05-02 — Iter 16：mood 代码迁到独立模块
- 新增 `src-tauri/src/mood.rs`，把以下从 `proactive.rs` 整体迁移过来：
  - 常量 `MOOD_CATEGORY` / `MOOD_TITLE`（改为 `pub`）
  - 函数 `read_current_mood` / `read_current_mood_parsed` / `parse_mood_string` / `read_mood_for_event`
  - `#[cfg(test)] mod tests` 含 8 个 parse_mood 边界用例
- `lib.rs` 加 `mod mood;` 紧邻 `mod mcp;`。
- `proactive.rs` 改为 `use crate::mood::{...}` 拉回需要的 4 个符号；同时清掉了一段错位被 `use` 切碎的导入块（因为 helper 当初先就地加再迁出，留了一坨乱序 import）。
- `commands/chat.rs` / `telegram/bot.rs` / `consolidate.rs` 全部把 `crate::proactive::read_*` import 改到 `crate::mood`。
- `cargo check` + `cargo test --lib` 通过：8 个测试名从 `proactive::tests::*` 变为 `mood::tests::*`，全绿。
- 现在 `proactive.rs` 单一职责回到"主动开口调度"，mood 状态机自成模块。

## 2026-05-02 — Iter 15：抽出 read_mood_for_event 统一 helper
- `proactive.rs` 新增 `pub fn read_mood_for_event(log_store: &LogStore, source: &str) -> (Option<String>, Option<String>)`：
  - 内部调 `read_current_mood_parsed`
  - 解析结果 motion=None 且 text 非空时写一行 "{source}: mood missing [motion: X] prefix..." 日志
  - 返回 `(Option<text>, Option<motion>)` 供 caller 直接用
- 4 处 callsite 重构为 `read_mood_for_event(...)` 单行：
  - `proactive::run_proactive_turn` — source="Proactive"，原 11 行 match → 1 行（多一行 log_store 二次拷贝）
  - `commands::chat::chat` — source="Chat"，原 11 行 → 1 行（用 `log_store.inner()` 把 State 转成 &LogStore）
  - `telegram::bot::handle_message` — source="Telegram"，原 12 行（含手写 lock）→ 1 行
  - `consolidate::run_consolidation` — source="Consolidate"，保留独有的"mood 被删 WARNING"分支，但前缀监控改走 helper
- chat.rs 的 `inject_mood_note` 仍用 `read_current_mood_parsed`（因为它要 mood text 注入 prompt 而不是事件 payload），所以保留 import。
- cargo check 通过；`cargo test --lib proactive` 8 测试仍全绿。
- 净减少约 30 行重复代码，未来加第五个入口（如 IM / Discord 等）只需一行调用。

## 2026-05-02 — Iter 14：consolidate 路径接 mood 监控 + emit
- `consolidate.rs` 加 `tauri::Emitter` import，引入 `ChatDonePayload`、`read_current_mood`、`read_current_mood_parsed`。
- consolidation prompt 新增"特殊保护"段：明确 `ai_insights/current_mood` 不可删，可 update 但 description 必须以 `[motion: ...] 心情文字` 开头。
- pipeline 跑前快照 `mood_before = read_current_mood()`；跑完后再读 parsed：
  - 若 before=Some / after=None，写 WARNING 日志（保护规则被违反）。
  - 若 motion 缺失但 text 非空，写"missing [motion: X] prefix"日志（与其他 3 条入口同行文）。
- 构造 `ChatDonePayload` 并 `app.emit("chat-done", ...)`，让前端 useMoodAnimation 根据整理后的 mood 触发动作。
- 现在四条入口（proactive / chat / telegram / consolidate）行为完全对称：都读 mood、可写 mood、emit 事件、有缺前缀监控；consolidate 还多了"mood 被删"特殊监控。
- cargo check 通过（仍是两条与本次无关的预存 warning）。

## 2026-05-02 — Iter 13：Telegram 路径接 mood 注入 + chat-done emit
- `commands/chat::inject_mood_note` 改 pub，让 telegram 复用，避免重写。
- `telegram/bot.rs`：
  - `HandlerState` 加 `app: AppHandle` 字段；`TelegramBot::start` 签名加 `app` 参数。
  - 在 run_chat_pipeline 之前调 `inject_mood_note(chat_messages)`，与桌面 chat 命令完全对称。
  - 跑完后 `read_current_mood_parsed` + emit `chat-done`（同一个 payload 结构 ChatDonePayload），desktop 前端的 useMoodAnimation 自动接住、做 Live2D 动作。
  - 缺前缀也写一行日志，与桌面 chat 路径行文一致。
- `lib.rs` setup 中创建 `app_handle_for_tg = app.handle().clone()`，传给 `TelegramBot::start`。
- `commands/telegram::reconnect_telegram` 命令也加 `app: AppHandle` 参数并透传。
- 这样三条入口（proactive / 桌面 chat / Telegram）行为统一：都读 mood 注入 prompt，都允许 LLM 用 `[motion: X]` 前缀更新，都 emit 事件让前端动画。
- cargo check 通过（仍是两条与本次无关的预存 warning）。

## 2026-05-02 — Iter 12a：mood 解析单元测试 + 缺前缀监控
- 重构：`read_current_mood_parsed` 拆成"读盘 + 解析"两层，新增 `parse_mood_string(raw: &str) -> (String, Option<String>)` 纯函数，无 IO 依赖、可单测。
- `proactive` 模块加 `#[cfg(test)] mod tests`，覆盖 8 个边界：
  - 标准格式 / 多余空格 / 无前缀 / 空 motion / 超长 motion / 未闭合 bracket / 前缀后空文本 / 前导空白
  - `cargo test --lib proactive` → 8 passed 0 failed。
- 缺前缀监控：proactive.rs 和 chat.rs 在 `read_current_mood_parsed` 之后判断"motion 缺失但 text 非空"——日志里写一行"missing [motion: X] prefix — frontend will fall back to keyword match"。
- 端到端实机测试本次没做（需要交互式 Tauri/LLM），列为 Iter 12b。但日志告警已就位，等用户实跑时可以根据出现率判断是否要再调 prompt。
- cargo check 通过（仍是两条与本次无关的预存 warning）。

## 2026-05-02 — Iter 11：反应式 chat 也注入和更新 mood
- `commands/chat.rs` 新增 `inject_mood_note(messages)`：
  - 用 `read_current_mood_parsed()` 取当前 mood text。
  - 拼一条 system 文本：当前心情 + 鼓励"如果对话让你心情有变就用 memory_edit 更新（含 [motion: X] 前缀）"+ 给出 4 个 group 对应情绪映射 + 明确"心情没变就别更新"避免每轮都写。
  - mood 缺失时用 bootstrap 文案，让 LLM 知道可以新建。
  - 通过查找第一个非 system 消息的位置，把 note 插在 SOUL 后、对话历史前。前端 session 持久化不受影响——augmented 只在内存里塞给 LLM。
- `chat()` tauri 命令在 `mark_user_message` 之后、`run_chat_pipeline` 之前调 `inject_mood_note`。
- 这样反应式聊天和主动开口在 mood 维度完全对称：都能读 mood、都能更新 mood、都通过 chat-done/proactive-message 把更新后的 mood + motion 推给前端做动画。
- cargo check 通过。
- 已知未覆盖：Telegram 路径仍然走原 messages（不带 mood note），以免一次改两条链路。列入 Iter 13。

## 2026-05-02 — Iter 10：LLM 直接挑 motion group
- 后端：
  - 新增 `read_current_mood_parsed() -> Option<(String, Option<String>)>`，从 `[motion: X] free text` 格式解析。前缀缺失/损坏时 motion=None、text=raw，确保旧记忆不破。
  - 加防御：motion 标签长度 ≤ 16，避免 LLM 写出诡异长串塞坏 payload。
  - `ProactiveMessage` / `ChatDonePayload` 都加 `motion: Option<String>`。proactive prompt 末尾约束改为：description 必须以 `[motion: Tap|Flick|Flick3|Idle] 心情文字` 开头，并给出每个 group 对应的情绪映射示例。
  - mood_hint 注入 prompt 时用 `parsed.text` 而非原始 description，避免 `[motion:...]` 前缀污染上下文。
- 前端：
  - `useMoodAnimation.ts` 把 `pickMotionGroup` 拆成两层：先看 payload.motion 是否在 `VALID_GROUPS` 集合，命中直接用；不命中（缺失或拼错）才退回旧的关键词匹配。
  - `triggerMotion` 接受 `(motion, mood)` 两个参数，体现"优先级"语义。
  - Payload 接口加 `motion: string | null`。
- 这样 LLM 既能用语义直接挑动作，又有关键词做安全网；前端硬编码列表从"事实标准"降级为"兜底"。
- tsc + cargo check 双过。

## 2026-05-02 — Iter 9：反应式聊天也驱动 mood 动作
- 后端：
  - `proactive::read_current_mood` 改为 `pub` 以便 chat.rs 复用（避免重复实现）。
  - `commands/chat.rs` 引入 `tauri::AppHandle + Emitter`，`chat` 命令的签名加 `app: AppHandle`。
  - 新增 `ChatDonePayload { mood, timestamp }`；run_chat_pipeline 跑完后 `read_current_mood` 一次，emit `chat-done` 事件。
  - 即便反应式聊天暂时不主动改 mood（mood 还是 stale），用户与宠物对话时也能看到角色动起来。
- 前端：
  - `useMoodAnimation.ts` 抽出 `triggerMotion(model, mood)` 内部辅助；同时监听 `proactive-message` 和 `chat-done`，两者走同一逻辑。
  - 卸载时清两个 unlisten 句柄。
- tsc + cargo check 双过；视觉效果仍需实机验证。

## 2026-05-02 — Iter 8：mood 驱动 Live2D 动作
- 后端：`ProactiveMessage` 加 `mood: Option<String>` 字段；`run_proactive_turn` 在 `mark_proactive_spoken` 之后再次 `read_current_mood()`，把 LLM 刚写好的最新 mood 一起 emit 给前端。这样省一次额外的 IPC，也保证 mood 与本次 message 一一对应。
- 前端新增 `src/hooks/useMoodAnimation.ts`：
  - 监听 `proactive-message` 事件，按 payload.mood 做关键词匹配 → motion group。
  - 关键词分四组：HAPPY (开心/兴奋/...) → Tap，ENERGETIC (想分享/活泼/...) → Flick，RESTLESS (烦/焦虑/...) → Flick3，QUIET (低落/平静/...) → Idle。无 mood 或无匹配 → Tap（让主动开口至少有可见动作反馈）。
  - 调用 `model.motion(group, undefined, 2)`（priority NORMAL）。motion group 不存在时 catch 并 console.debug。
- `App.tsx` 在 `useChat` 后调 `useMoodAnimation(modelRef)`，复用已有的 modelRef。
- miku 模型只有 4 个 motion group（Tap/Flick/Flick3/Idle），没有 expression，因此本迭代用 motion 替代表情；后续若换更丰富的模型可在同一关键词表上扩展。
- tsc --noEmit / cargo check 都通过；未启 dev server 实测视觉效果（需要用户实机才能验证 motion 切换是否流畅）。

## 2026-05-02 — 设置面板：Proactive / Consolidate 配置 UI
- `useSettings.ts` 新增 TS 接口 `ProactiveConfig` / `MemoryConsolidateConfig`，扩到 `AppSettings`，`DEFAULT_SETTINGS` 也补上对应默认值，跟 Rust 端 `Default` 实现完全对齐（300s/900s/60s/1800s 和 6h/12 条）。
- `SettingsPanel.tsx` 模态框宽度从 260 升到 300，最大高度从 420 升到 560，避免新增字段挤压。
- 加两个分组段（"主动开口 (Proactive)" / "记忆整理 (Consolidate)"），每段一个 enabled checkbox + 几个 NumberField（两列网格排版）。
- 新增 `NumberField` 受控组件：label + `<input type="number">`，带 min 校验，NaN 拒收。
- `panel/PanelSettings.tsx` 的初始 `form` state 也补上 proactive / memory_consolidate 默认值，否则 TS2345 报错（一旦后端 get_settings 返回完整结构 form 会被覆盖，但 TS 静态类型要求初值完整）。
- tsc --noEmit / cargo check 都通过（仍是两条与本次无关的预存 warning）。
- 现在用户不必手编 config.yaml 就能开关主动开口和记忆整理；为后续暴露更多面板配置打好骨架。

## 2026-05-02 — Iter 7b：macOS 日历事件工具
- 新增 `src-tauri/src/tools/calendar_tool.rs`，定义 `GetUpcomingEventsTool`：
  - 参数 `hours_ahead`（默认 24，clamp 到 1–168 = 一周）。
  - macOS 走 `osascript` 调 Calendar.app `every event of c whose start date >= tStart and ≤ tEnd`，每条事件用 TAB 分隔字段（title / start / end / calendar / location），换行分隔记录。
  - Rust 解析 stdout 为 JSON 数组，最多返回 20 条，标记 `truncated`。
  - 失败时 stderr 透传出去并附 hint："去 System Settings 给 Calendars 授权"。
  - 非 macOS 返回明确的 unsupported 错误。
- `tools/mod.rs` 暴露 `calendar_tool`；`registry.rs` 注册到内置工具。
- `proactive.rs` 主动 prompt 工具列表加上 `get_upcoming_events`，注释"日程是私人内容不要原样念出"。
- 没有跑端到端验证（不应未授权读用户真实日历），但脚本与已工作的 `get_active_window` 同一 osascript 模式，cargo check 通过。
- cargo check 通过（仍是两条与本次无关的预存 warning）。

## 2026-05-02 — Iter 7a：天气工具（wttr.in）
- 新增 `src-tauri/src/tools/weather_tool.rs`，定义 `GetWeatherTool`：
  - 调用 `https://wttr.in/{city}?format=4`，返回紧凑一行（如 "Beijing: ⛅ 🌡️+18°C 🌬️↖4km/h"）。
  - 不传 city 时由 wttr.in 按 IP 自动定位。
  - 用 `reqwest::Client`（已在依赖里），10 秒超时，失败时返回结构化错误 + 200 字 body 预览。
  - 工具描述明确告诉 LLM "不要原样念出文本，要融到自然对话里"，避免机械感。
- `tools/mod.rs` 暴露 `weather_tool`；`registry.rs` 注册 `GetWeatherTool`。
- `proactive.rs` 主动 prompt 工具列表加上 `get_weather`，注释"偶尔用一次就好不要每次都查"以省 token。
- 现场用 curl 验证 wttr.in 可访问，singapore 反馈 "⛅ 🌡️+31°C 🌬️↖4km/h"。
- cargo check 通过（仍是两条与本次无关的预存 warning）。

## 2026-05-02 — Iter 6：定期记忆 consolidate
- 新增模块 `src-tauri/src/consolidate.rs`：独立的后台 tokio 循环，启动 120 秒后开始，每 `interval_hours`（默认 6 小时）跑一次。
- 触发条件：`enabled=true` 且 memory 总条目数 ≥ `min_total_items`（默认 12），否则只写一条 skip 日志，避免对空索引调用 LLM。
- 触发时把整个 memory 索引（YAML 序列化）丢给 LLM，明确指令它通过 `memory_edit` 工具进行合并/删除/扩充，并强调"保守，不确定就不动"，索引看起来已清爽时输出 `<noop>`。
- 通过 `run_chat_pipeline` + `CollectingSink` 复用现有工具调用基础设施，结束后日志记录 before/after 条目数和 LLM 总结的前 200 字。
- `MemoryConsolidateConfig` 新增到 `AppSettings`：`enabled` / `interval_hours` / `min_total_items`。默认关闭，避免开发期意外消耗 token。
- `lib.rs` 在 setup 末尾 `consolidate::spawn(app.handle().clone())`，与 proactive 并列启动。
- cargo check 通过（仍是两条与本次无关的预存 warning）。

## 2026-05-02 — Iter 5：主动发言节奏控制
- 重构 `InteractionClock` 内部状态：从单一 `last: Instant` 升到 `ClockInner { last, last_proactive, awaiting_user_reply }`，对外加 `mark_user_message` / `mark_proactive_spoken` / `snapshot` 三个明确语义的方法，原 `touch` 保留作为通用"刷一下时间"。
- `chat.rs` 入站调 `mark_user_message`（清掉 awaiting）；proactive 开口后调 `mark_proactive_spoken`（置 awaiting + 记 last_proactive）。
- `proactive.rs` spawn 主循环新增两道闸门，先于 idle/input_idle 检查：
  - **闸 1（awaiting）**：如果上一条 proactive 还没等到用户回复就跳过，写日志「skip — awaiting user reply」。
  - **闸 2（cooldown）**：如果距离上次 proactive 不到 `cooldown_seconds` 也跳过。
- `ProactiveConfig` 加 `cooldown_seconds: u64`，默认 1800。
- 删掉无用的 `InteractionClock::idle_seconds`（被 `snapshot()` 取代），保持 warning 计数不变。
- cargo check 通过（仍是两条与本次无关的预存 warning）。

## 2026-05-02 — Iter 4：宠物心情/状态持久化
- `proactive.rs` 新增常量 `MOOD_CATEGORY = "ai_insights"` / `MOOD_TITLE = "current_mood"`，统一描述 mood 在 memory 系统中的位置。
- 新增 `read_current_mood()` 辅助：通过 `memory::memory_list` 拉 `ai_insights` 分类，找到 title=`current_mood` 的项，返回它的 description。读不到返回 None。Rust 端不主动 create，bootstrap 完全交给 LLM 在第一次主动开口时用 `memory_edit` 自己写。
- `run_proactive_turn` 在构造 prompt 前读 mood：有则注入「你上次记录的心情/状态：「…」」；没有则提示「这是第一次」。
- 主动 prompt 末尾加一条新约束：开口后用 `memory_edit` 更新 `ai_insights/current_mood`（不存在 create，存在 update），description 写下当下心情、最近在想什么、对用户的牵挂。沉默不更新。
- 这样宠物的"心情"在多次主动开口之间形成连续状态，避免每次都从空白启动。
- cargo check 通过（仍是两条与本次无关的预存 warning）。

## 2026-05-02 — Iter 3：键鼠空闲门槛
- 新增 `src-tauri/src/input_idle.rs`：macOS 通过 `ioreg -c IOHIDSystem` 读 `HIDIdleTime`（纳秒）→ 秒。非 macOS 返回 `None`。不引新依赖，也不需要 Accessibility 权限。
- `ProactiveConfig` 加入 `input_idle_seconds`（默认 60，0 表示禁用门槛）。
- `proactive.rs` 触发逻辑改为：先满足"距上次互动 ≥ idle_threshold_seconds"，再读 HID idle，必须 ≥ `input_idle_seconds` 才会真的让 LLM 决定要不要开口；否则只写一条 skip 日志。
- 主动 prompt 把当前键鼠空闲时长也告诉 LLM，作为额外判断 context。
- cargo check 通过（仍是两条与本次无关的预存 warning）。
- 新增 `src-tauri/src/tools/system_tools.rs`，定义 `GetActiveWindowTool`：
  - macOS 下用 `osascript` + System Events 拿当前 frontmost 进程名 + 前窗口标题。
  - 失败时返回 JSON 错误并提示开启 Accessibility 权限。
  - 非 macOS 平台返回明确的 unsupported 错误。
- `tools/mod.rs` 暴露 `system_tools` 模块；`registry.rs` 把 `GetActiveWindowTool` 注册到内置工具列表。
- `proactive.rs` 的主动开口提示更新：明确告诉 LLM 在开口前可以先调 `get_active_window` 让话题贴合当下，并补充 `memory_search` 翻用户偏好。
- 现场验证 `osascript` 在该机器上可正常返回 `App|Window` 形式，无需额外授权（取决于具体 app）。
- cargo check 通过（仍是两条与本次无关的预存 warning）。

## 2026-05-01 — Iter 1：主动开口骨架
- 在 `AppSettings` 加入 `ProactiveConfig`（enabled / interval_seconds=300 / idle_threshold_seconds=900），默认关闭。
- 新增 `src-tauri/src/proactive.rs`：
  - `InteractionClock` 共享状态记录上次互动时间。
  - `spawn(AppHandle)` 后台 tokio 循环，每 tick 读 settings，若启用且 idle ≥ 阈值则触发主动检查。
  - 加载最新 session 历史 + SOUL，注入特殊 user 提示（`<silent>` 表示选择沉默）。
  - 复用 `run_chat_pipeline` + `CollectingSink` 调 LLM。非沉默回复持久化到 session，并通过 Tauri event `proactive-message` 推给前端。
- `chat` 命令在请求前后调用 `clock.touch()`。
- `useChat` 监听 `proactive-message` 事件，把 pet 主动消息加入 messages / items（后端已写盘，前端不再重复保存）。
- cargo check / tsc --noEmit 均通过（仅两条与本次无关的预存 warning）。

后续验证：开发期需打开 config.yaml 把 `proactive.enabled: true` 才会生效；面板 UI 留待 Iter 2+。
