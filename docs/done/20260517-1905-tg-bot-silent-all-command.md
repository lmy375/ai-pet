# TG bot `/silent_all [minutes]` 命令（iter #371）

## Background

桌面 PanelMemory「⏸ 全部 silent 1h」按钮（iter #366）走 frontend
timer + localStorage 自动撤回。本 iter 加 TG bot `/silent_all [minutes]`
对偶 — 手机端 owner 开会 / 集中写作时一键挡 butler_task picker。

TG 不能用 frontend timer（无前端进程持续），改 backend tokio timer：
spawn task + generation counter 防 race，到时自动 `task_set_silent
(false)` 撤回。

## Changes

### `src-tauri/src/telegram/bulk_silent.rs`（新模块）

核心：`BulkSilentState { titles, expires_at, generation }` + 静态
`STORE: OnceLock<Store>`：

- `snapshot()` — 当前 active 窗口克隆（None = 无）
- `arm(titles, minutes) -> Result<State>` — 先 release_active 释放
  prior，再 set_silent(true) 应用 markers，spawn tokio 计时任务 +
  generation 计数器
- `release_active() -> Option<Vec<String>>` — 撤销 markers + 清
  STORE + 返被撤销的 titles
- spawned timer 用 `generation == captured_gen` 判断"我还是 current"
  — 防 race：第二次 arm 会让旧 timer noop（不影响新 snapshot）

不持久化 — app restart 丢 timer，markers 留在原地。help text 明示
此限制 + 给清理路径（重启后 `/silent_all` 或 `/silent_all 0`）。

3 个 module unit tests：snapshot 起始 None、release_active 无 active
返 None、generation 单调递增。arm() 集成 (含 task_set_silent + 文件
IO) 留给手测 / e2e。

### `src-tauri/src/telegram/mod.rs`

注册 `pub mod bulk_silent`。

### `src-tauri/src/telegram/commands.rs`

#### 1. enum 变体（~line 175）

```rust
SilentAll { minutes: i64 },
```

#### 2. `name()` / `title()` arms

- name → "silent_all"
- title() → "" (Mute / Digest 同 N-only 簇)

#### 3. parser（~line 1010）

与 /mute 同 clamp 0..=10080；缺省 60；非数字 fallback；0 = release
intent（与 /mute 0 同协议）；负数 clamp 到 0。

#### 4. `format_silent_all_reply(armed, released, minutes, until)` pure

4 输出态：
- minutes=0 + released=0 → "✨ 当前无 silent 窗口可解除"
- minutes=0 + released>0 → "🔊 已解除 N 条"
- minutes>0 + armed=0 → "✨ 暂无可 silent 任务"
- minutes>0 + armed>0 → "⏸ 已 silent N 条·M 分钟后自动解除（到 HH:MM）"
  + 上轮 prior 非零时附"（先解除上轮 X 条）"

duration label 复用 mute reply 同分桶（< 60min / < 24h / ≥ 24h），
保命令风格一致。

#### 5. registry zh + en + format_help_text + format_help_for_topic
+ ALL_HELP_TOPICS + 两 drift-defense 列均加 "silent_all"

### `src-tauri/src/telegram/bot.rs`

新 handler arm：
- minutes==0 → `release_active()` + reply
- minutes>0：
  1. 记录 prior 窗口的 titles.len() 作 released_count
  2. 扫 butler_tasks pending && !silent candidates（memory_list
     filter `![done]` + `![silent]`）
  3. `arm(candidates, minutes)` 走模块路径
  4. Ok → reply armed=state.titles.len()；Err → reply armed=0 兜底

### Tests（commands.rs，12 个新 unit test）

Parser（6 个）：
- 默认 60 / 显式 30 / 显式 120 / 0 = release / clamp 99999 → 10080 /
  clamp 负数 → 0 / 非数字 → 60

Formatter（6 个）：
- minutes=0 release 无 active → bootstrap
- minutes=0 release 有 active → 解除 5 条
- minutes>0 armed=0 → 友好兜底
- minutes>0 armed=7 + 60min → 含 19:30 + 1 小时 + /silent_all 0
- minutes>0 armed=5 + released=3 → 含 "先解除上轮 3 条"
- 1440 → "1 天" label

## Key design decisions

- **backend tokio timer + generation counter 而非 schema 改造**：
  考虑过加 `[silent_until: <ts>]` 新 marker 让 expiry 嵌入数据，但
  会触及 task_queue parser / proactive engine / 所有 `\[silent\]`
  regex 命中点 — scope 失控。tokio timer + 既有 `[silent]` marker
  的组合 KISS，只触一个新模块。
- **不持久化（trade-off）**：app restart 丢 timer，markers 留原地。
  trade-off 是"零 schema 改造 + 复用既有 marker"。help text 明示限
  制 + 给清理路径。
- **generation counter 防 race**：第二次 arm 先 release_active 旧
  窗口 + 新 expires_at；同时旧 timer 还在睡。captured_gen 比对让
  旧 timer 醒来时 noop（不会把新 snapshot 的 titles 错误 unsilent）。
- **桌面 vs TG 不共享 state**：iter #366（frontend timer +
  localStorage）和本 iter（backend tokio）是两个独立 surface 各自
  管理。owner 在桌面 click + 手机 /silent_all 会有两个窗口同时进
  行 — 实际无害（[silent] marker 单值 idempotent，谁先 release 谁
  生效），保 surface 独立。
- **`/silent_all 0` 协议**：与 `/mute 0` 同 — "0 minutes = 立即解
  除" 是 owner 心智一致点，避免引入新的 `/silent_all clear` /
  `/silent_all_release` 命令膨胀。
- **不抽 `bulk_silent` 到顶层 module**：当前只 TG 单 surface 用，
  放 `telegram::bulk_silent` 表语义归属。如未来桌面 iter #366 重
  写也走这条路径，再提升到 `crate::bulk_silent`。
- **arm() 同步而非 async**：set_silent 是同步函数（sqlite + 文件
  IO blocking）；arm 内部不需要 await。`tokio::spawn` 在 arm 内
  同步调用即可（spawn 自身是同步的 spawn handle 返回）。

## Verification

- `cargo check`（backend）— clean
- `cargo test --lib`（backend）— **1308 passed / 0 failed**（+15
  = 3 module test + 12 cmd test）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
