# DONE

记录每次迭代完成的实质性变化（按时间倒序）。

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
