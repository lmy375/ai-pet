# TG bot `/consolidate_now confirm` 命令（iter #479）

## Background

桌面已有两个 force-consolidate 入口：
- PanelMemory「立即整理」 — 主显眼按钮
- PanelDebug「🧹 force consolidate」 — debug 视图就近入口（iter #456）

但 TG 端无对应入口。owner 在 TG sprint 整理 / 调 prompt 后想立即 audit
consolidate 行为时只能切桌面 — 跨设备 / 远程使用场景 friction。

本 iter 加 `/consolidate_now confirm` — confirm-token 模板（与
/promote_all_p7 / /touch_all_p7 / /pin_all_p7 / /cancel_all_error 同
家族）；调既有 `trigger_consolidate(app)` 后端共享同 sweep pipeline。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::ConsolidateNow { confirmed: bool }` 变体

紧贴 `PinAllP7`（同 confirm-batch 家族）。

#### 2. 解析（与 /promote_all_p7 等同 confirm-token 模板）

```rust
"consolidate_now" => {
    let confirmed = title.trim().eq_ignore_ascii_case("confirm");
    Some(TgCommand::ConsolidateNow { confirmed })
}
```

#### 3. `format_consolidate_now_reply` pure 函数

```rust
pub fn format_consolidate_now_reply(
    confirmed: bool,
    result: Option<Result<String, String>>,
) -> String;
```

3 态：
- `!confirmed` → usage hint 含 LLM-heavy warning + 推 PanelDebug ETA chip
- `confirmed + Ok(summary)` → 「🧹 {summary}」（summary 含 elapsed_ms +
  LLM summary snippet —— 与桌面入口同 backend 返回）
- `confirmed + Err(reason)` → 区分用户 cancel（友好兜底）/ 实际错误
  （显原因）

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `PinAllP7` / 前于 `CancelAllError`：

```rust
TgCommand::ConsolidateNow { confirmed } => {
    if !confirmed {
        format_consolidate_now_reply(false, None)
    } else {
        let app = state.app.clone();
        let result = trigger_consolidate(app).await;
        format_consolidate_now_reply(true, Some(result))
    }
}
```

- **复用 `trigger_consolidate(app)`**：与桌面入口同后端 — 一处真实施
  加点；返 `Result<String, String>`（既有 LLM summary / 错误原因）直
  接传 formatter
- **不在 handler 转译 cancel / 错误细节**：formatter 解析 Err 字符串内
  容；handler 仅 stitching
- **不引 timeout / 超时保护**：trigger_consolidate 内部 sweep 各步骤有
  cancel checkpoint；handler 是 awaiting end-to-end，超长 sweep（>2min）
  时 TG bot 一直 await 是预期行为 — owner 已通过 confirm token 明确同
  意

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift defense

- 双 lang registry 各加一条（紧贴 pin_all_p7）
- ALL_HELP_TOPICS 紧贴 "pin_all_p7"
- format_help_for_topic 加详细文案（含与桌面入口对比 + PanelDebug ETA
  chip 交叉引用）
- format_help_text 全表加 `/consolidate_now confirm` 一行
- 两处 drift-defense 测试列表加 "consolidate_now"

### 7 单元测试

parse（无参 / confirm 大小写 / 其它 trailing）× 3
formatter（unconfirmed-shows-usage / confirmed-ok / confirmed-cancel /
confirmed-error）× 4

## Key design decisions

- **confirm-token 模板**：与 /promote_all_p7 / /touch_all_p7 / /pin_all_p7
  / /cancel_all_error 同 family — `confirm` trailing case-insensitive。
  consolidate 是 LLM-heavy（~30s..2min + 烧 token），confirm 防误触尤
  其关键
- **复用 trigger_consolidate 而非新 backend**：与桌面两入口共享 single
  source of truth — 行为完全一致（含 progress event emit / cancel flag
  共享）。owner 在 TG 触发后桌面 PanelMemory「整理进度条」也会显（如
  果同时开着）
- **formatter 3 态显式区分 cancel / error**：用户主动 cancel（如桌面
  按✕）应友好显「已取消」非"失败"；其它 error 显原因。formatter 通过
  字符串 contains 检测 cancel — backend 已用「用户取消」标记，与桌面
  PanelMemory「立即整理」错误处理同模板
- **不 emit progress 给 TG**：consolidate progress event 是桌面 panel
  专用（带 phase / progress / total 数）；TG 端无 progress UI 支持，
  end-to-end await 后一次性返结果更直觉。owner 想看 progress 在桌面
  PanelMemory 看
- **不引入 throttle / "1 hour 内只能一次" 限流**：confirm token 本身
  足够防误触；owner 想短时间内连续触发是他的选择（如 prompt tweak
  调试）。throttle 引入额外 state 维护
- **不写 unit test on async handler**：handler 仅是 stitching（await +
  Option<Result> wrap + invoke formatter）；formatter + trigger_consolidate
  各自有 tests。GOAL.md "meaningful tests only" 规则下不引装饰性 handler
  test

## Verification

- `cargo build --lib` — clean
- `cargo test --lib telegram::commands::tests::consolidate_now` — 3/3 通过
- `cargo test --lib telegram::commands::tests::format_consolidate_now`
  — 4/4 通过
- `cargo test --lib`（全表）— 1555/1555 通过（+7 from 1548）
