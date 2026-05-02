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
