# DONE

记录每次迭代完成的实质性变化（按时间倒序）。

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
