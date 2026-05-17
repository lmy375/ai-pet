# pet ChatMini「🔕 今日 mute」计数 chip + 后端 mute_count 模块（iter #275）

## Background

owner 临时按下「⚙️ mute N min」或 TG `/sleep N` 让宠物闭嘴是高频操作（专
注 / 接电话 / 不想被打断），但缺少 audit 入口 — owner 想知道"今天我打断
了宠物几次"，作为是否过度静音的自我反思信号。

本迭代加 in-process 今日 mute engaged 计数 + 桌面 chip 显计数（钉在 🐾 主
动开口 chip 左侧）。

## Changes

后端：

- `src-tauri/src/mute_count.rs`（新文件）：
  - `DailyMuteCount { date, count }` static Mutex（用 `Mutex::new` const
    构造，无 once_cell 依赖）
  - `record_mute_engaged()`：跨午夜 reset 后 +1
  - `today_count_from(date, count, today)` pure helper：date 不匹配返 0
  - `get_today_mute_count()` tauri 命令
  - 3 单元测试覆盖 reset 跨日 / 同日返存值 / record 后能读到累计

- `src-tauri/src/lib.rs`：注册 `mod mute_count` + 命令 `get_today_mute_count`。

- `src-tauri/src/proactive.rs`：`set_mute_minutes` 在 `minutes > 0` 路径
  调 `crate::mute_count::record_mute_engaged()`（mute clear 路径 minutes==0
  不计 —— chip 语义是"我打断了几次"而非"按了 mute 按钮几次"）。

前端：

- `src/App.tsx`：
  - 新增 `todayMuteCount` polling state（10 分钟轮一次 — mute 是低频操作）
  - 在 🐾 todaySpeechCount chip 之后插「🔕 N」chip：仅 `todayMuteCount > 0`
    时显；位置 right 动态算（有 🐾 chip 时偏更左 `76+56=132px`，避开
    overlap）；hover opacity 0.6 → 1 与既有 chip 同模板；tooltip 解释来源
    路径 + 进程重启清零

## Key design decisions

- **in-process 计数 + 进程重启清零**：owner 实际诉求是当日感知。持久化到
  磁盘 / SQLite 增加 IO + race / 跨进程冲突，对此低频 audit 信号收益不大。
- **跨午夜自动 reset 在 record 路径 + read 路径都校验**：date 不匹配时
  record 先 reset 再 +1；read 直接返 0（不污染陈旧值进 chip）。
- **仅数 engage 不数 clear**：chip "🔕 N" 字面是"今天 mute 了 N 次"，clear
  是反操作不该 +1。owner 多次 engage + clear 仍只记 engage 次数（连续 4 次
  调试 mute / unmute = 4 mute）。
- **fallback -1 → 不显 chip**：与 todaySpeechCount 同模板，未抓到时静默不
  噪音。
- **polling 10 min**：mute 操作低频（一天 0-5 次），10 min 节奏跟得上又不
  浪费 IPC。同 todaySpeechCount。

## Verification

- `cargo check` ✅
- `cargo test`（含 3 新 mute_count 测试 + 全表 1067 通过）✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
