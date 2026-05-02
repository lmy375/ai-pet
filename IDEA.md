# IDEA — 实时陪伴型 AI 桌面宠物的设计思考

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
