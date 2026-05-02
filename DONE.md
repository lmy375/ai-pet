# DONE

记录每次迭代完成的实质性变化（按时间倒序）。

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
