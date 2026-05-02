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
