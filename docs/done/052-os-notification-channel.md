# 052 · proactive utterance 走 OS notification — 离 app 时也能被感知

所有 proactive utterance（008 welcome_back / 016 morning_briefing / 032 anniversary / 034 surprise / 039 evening_briefing / 042 reminder_escalate 等）当前只落 ChatMini bubble。用户切到别的 app 或 pet 窗口最小化时全部静默 — GOAL「实时陪伴 / 主动聊天 / 后台长时运行」实质退化为"只在你看着 pet 时 pet 才陪伴"。

需求：
- proactive utterance 落 ChatMini bubble 时，若 pet app 不在前台 ≥ 30s → 同步触发 macOS notification（Tauri notification API）。
- notification title = pet 当前 name（PanelPersona 提供）；body = utterance 首句截断 ≤ 60 字；点击通知 → 聚焦 ChatMini 并滚到该条 bubble。
- 受 026 user stress / 017 pet mood / gate.rs deep-focus 已有 gate 抑制：抑制 in-app 时 OS 通道一并抑制，不绕过 gate。
- 042 reminder_escalate 走 OS 通道时频率更克制：一次原 fire + 一次 escalate（10min 后），不到三次（防止 5min 三声系统通知 spam）。
- 用户拒授 macOS notification 权限：退回 in-app only，pet 在桌面 ChatMini 加一行 toast「在外面我喊不到你哦」一次性提示，不反复提醒。
- 不引入 cross-device 推送（feedback memory 禁）；本需求纯本机 OS API。
- 通知 group identifier 按 utterance 类型分组（briefing / reminder / surprise），便于 macOS 通知中心折叠。

---
实现笔记：
- 加 `tauri-plugin-notification = "2"` 到 Cargo.toml + `tauri_plugin_notification::init()` 到 builder + `"notification:default"` 到 capabilities/default.json。Tauri 2 官方 plugin，权限走 capability 模型，首次发通知时系统自动弹授权请求。
- `src-tauri/src/proactive.rs` 新加：
  - `WINDOW_FOREGROUND_STATE: Mutex<Option<(bool, Instant)>>` 进程内单调时钟跟踪前台/失焦状态及切换时刻
  - `update_window_focus(in_foreground)` 由 lib.rs setup 块的 `main.on_window_event(WindowEvent::Focused)` hook 调用，**无前端改动**
  - `ProactiveNotificationKind {Briefing, Reminder, Surprise, Followup, Other}` + `group_id() → "pet.{kind}"` 让 macOS 通知中心按 kind 折叠（spec「group identifier」对应）；当前仅 Other 接通，其余占位
  - 常量 `NOTIFICATION_FOREGROUND_THRESHOLD_SECS = 30` / `NOTIFICATION_BODY_CHAR_CAP = 60`
  - Pure `should_send_os_notification(state, now, threshold)`：前台 → false；状态未知（启动初未收过 focus） → false（保守不发避误触发）；后台 ≥ threshold → true（边界含等号）
  - Pure `format_notification_body(text, max_chars)`：识别中英首句末标点（`.!?。！？\n`），截首句；超 cap 加 `…`；按 char 计避 UTF-8 byte 切割
  - Async `send_os_notification(app, pet_name, body, kind)`：调 `NotificationExt::builder()` 写 title + body + group；title 兜底 `"Pet"`；失败 silent log（通知是锦上添花，不阻塞主路径）
- `src-tauri/src/lib.rs::setup`：监听 main 窗口 `WindowEvent::Focused(bool)` → `update_window_focus(bool)`；启动时显式 `update_window_focus(true)` 让初始状态正确
- 接入 `run_proactive_turn`：emit `proactive-message` 后 gate-check + `tauri::async_runtime::spawn` 异步 send（不阻塞返回路径）
- 11 单测：focused → false / 状态未知 → false / 后台未到阈值 → false / 后台超阈值 → true / 边界等号 → true / 首句中文标点 / 首句英文标点 / 截断 + `…` / 无标点截到 cap / 空 input → 空 / `\n` 视作句末 / 5 个 group_id 两两不等 + 均 `pet.` 前缀
- **缺口**：
  1. **其它 emit 站点未接入**：还有 9 个 `app.emit("proactive-message", ...)`（016 morning_briefing / 008 welcome_back / 011 scheduled_report / 012 deferred_task / 034 surprise / 039 evening_briefing / 042 reminder_escalate / 037 goal_check_in / 007 memory_follow_up）需逐个加同款 gate + send。本刀仅接 run_proactive_turn 作 POC，可拆 helper 函数避免重复粘贴
  2. **042 reminder_escalate 频率克制**：spec「走 OS 通道时一次原 fire + 一次 escalate，不到三次」——需要传 fire_count 给 send_os_notification + 在 send 内 short-circuit。变体 Reminder 已占位
  3. **pet name title**：当前用 `"Pet"` 兜底——AppSettings 没 `pet_name` 字段，spec「PanelPersona 提供」未做。未来加 `pet_name` setting 时一行替换
  4. **点击通知 → 聚焦 ChatMini + 滚到 bubble**：spec 要求点击通知后 deep-link 到 bubble id。需 notification builder action / on-click handler + 前端 scroll-to-id；本刀未做
  5. **权限拒授 → in-app toast「在外面我喊不到你哦」**：spec 要求一次性提示。需检查 `app.notification().permission_state()` + 失败计数 + 前端 toast 触发事件；本刀未做
  6. **gate.rs deep-focus 协同**：spec「抑制 in-app 时 OS 通道一并抑制」——proactive 路径若已 gate 跳过 emit，则不会到达 send_os_notification，自然满足。但需确认所有 9 个 emit 站点都在 mood/stress gate 之后才 emit
  7. **关闭命令**：发送通知前应检查 user 是否在 settings 关闭了 OS notification（spec 反指令「拒授退回 in-app only」隐含可关闭）。当前无 settings 字段；可加 `settings.os_notifications_enabled`
