# TG bot `/sleep` 命令（8 小时静音 + 友好"晚安"）（iter #332）

## Background

owner 睡前 / 长会议 / 想 deep work 时想"让宠物安静一段时间不要打扰"。
当前命令是 `/mute 480`（手敲数字）。但：
- 480 是抽象数字，不直觉（要算 8h * 60 = 480）
- mute reply 文案是中性"已静音 N 分钟"，不带情感色调

本迭代加 `/sleep` 一键 8h mute + 专属"晚安"语气 reply — 让"睡前 mute"
场景的情感与 owner 心境对得上。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum `Sleep` 无参变体（与 `Mood` / `Now` / `Today` 同模板）
- `name()` → "sleep"；`title()` 归入无参桶
- 解析器："sleep" 分支无参；多余尾部一律忽略
- 新常量 `SLEEP_MUTE_MINUTES = 480` (8 * 60)
- 新 pure formatter `format_sleep_reply(until_local)`：
  - 🌙 emoji 头
  - "宠物去睡了 —— 8 小时静音，HH:MM（次日 / 8h 后）自动醒。"
  - "晚安！想立刻叫醒发 /mute 0。" 尾
  - until None → dash 占位
- registry zh + en 都加 ("sleep", desc)
- format_help_text 全表加 `/sleep` 行（/random 之后）
- format_help_for_topic 加 "sleep" key + /mute 交叉引用
- ALL_HELP_TOPICS 加 "sleep"
- 两 drift-defense 名单同步加 "sleep"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Sleep` handler arm（在 Random arm 之前）：
  - `crate::proactive::set_mute_minutes(SLEEP_MUTE_MINUTES)`（复用既有
    /mute 后端 — 同 hook record_mute_engaged 让 "🔕 今日 mute" chip 计
    数同步）
  - `until_local = chrono::Local::now() + Duration::minutes(480)`
  - 调 `format_sleep_reply(Some(until_local))`

### Tests（4 个新 unit test）

- parser：无参 / 多余尾部容忍 / 大小写不敏感
- formatter：友好 tone + until 时间 + 8 小时 label + /mute 0 解除 hint
- until None 时 dash placeholder
- 常量 SLEEP_MUTE_MINUTES = 480 校验（防误改）

## Key design decisions

- **复用 set_mute_minutes 同后端**：不引新 backend 命令 — sleep 就是
  mute 8h 的语义糖。让既有 hook (record_mute_engaged) / 桌面 ChatMini
  "🔕 今日 mute" chip 计数 / PanelDebug "⚙️ mute" 等全部自动跟进。
- **8 小时（480 分）默认**：典型一晚睡眠时长；short-term 不够（NREM 周
  期 90min × N），long-term 太久（会议 / 会前缓冲场景不需要）。owner
  想要精确控制走 `/mute N`，本命令是"无脑常用值"。
- **format_sleep_reply 独立而非加 minutes 参数到 format_mute_reply**：
  /sleep 的情感色调与 /mute 中性数字 reply 应分离。共用就会让 owner
  按 `/sleep` 收到 "已静音 480 分钟" 这种冷冰冰文案 — 失去命令的存在
  意义。两个 formatter 独立维护各自风格。
- **保留 /mute 等价路径**：/sleep 不替换 /mute — owner 想精确控制时
  /mute 5 / /mute 60 / /mute 0 仍可用。两命令按"语气 / 精度"分流。
- **/mute 0 解除提示**：让 owner 看到 reply 时知道"想提前叫醒怎么办"。
  与既有 format_mute_reply 末尾相同的引导。
- **until_local 注入而非 caller 自己算**：与 format_mute_reply 同 pure
  pattern — 单测稳定（不依赖运行机 Local::now）。

## Verification

- `cargo test --lib`（backend）— 1201 passed / 0 failed（4 新 sleep 测
  试通过；两 drift-defense 也命中新加的 "sleep"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.23s)
