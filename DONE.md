# DONE

记录每次迭代完成的实质性变化（按时间倒序）。

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
