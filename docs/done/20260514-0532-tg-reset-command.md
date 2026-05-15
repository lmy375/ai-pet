# TG bot `/reset` — 清掉 LLM 对话上下文

## 背景

TG bot 维护 `session_messages: Vec<serde_json::Value>`（共享一个 Telegram session）。聊久了 / chat 跑偏后，**用户没有手机端入口重置**——只能开桌面切到「聊天」tab 走 `/clear`。

加 `/reset` 让手机端能就地重置：

- 保留首条 `role: "system"`（人设 / SOUL.md 内容）
- 删其它所有消息
- save_session 持久化
- 回复 "🔄 已重置对话上下文（保留人设）"

不叫 `/clear` 是因为桌面 `/clear` 有 5s armed 二次确认；TG 端要复刻 armed 模式不太顺（用户两次输入间隔可能很长 / 不同设备 / etc）。**用不同的词**（`/reset`）让用户一眼明白这是 TG-特有的单击行为。

## 改动

### `src/telegram/commands.rs`

- `TgCommand::Reset` variant
- `name()` / `title()` 接上
- parser 加 `"reset" => Some(TgCommand::Reset)`，多余尾部忽略
- registry zh/en 各加一行
- 新 pure fn `format_reset_reply() -> String`：固定文案 "🔄 已重置对话上下文（保留人设/系统提示）。"
- `format_help_text` 加 `/reset` 行
- 测试：parse + parse-with-trailing + registry coverage

### `src/telegram/bot.rs`

handler：

```rust
TgCommand::Reset => {
    let kept_system = {
        let mut msgs = state.session_messages.lock().await;
        let system: Vec<serde_json::Value> = msgs
            .iter()
            .filter(|m| {
                m.get("role")
                    .and_then(|r| r.as_str())
                    .map(|r| r == "system")
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        *msgs = system.clone();
        system
    };
    // 持久化：写回 session 文件，下次启动加载仍是 system-only
    let now = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3f")
        .to_string();
    let s = crate::commands::session::Session {
        id: state.session_id.clone(),
        title: "Telegram".to_string(),
        created_at: String::new(), // backend preserves on save_session for existing
        updated_at: now,
        messages: kept_system,
        items: vec![],
    };
    if let Err(e) = crate::commands::session::save_session(s) {
        eprintln!("session save after /reset failed (best-effort): {e}");
    }
    crate::telegram::commands::format_reset_reply()
}
```

不依赖 `created_at` —— `session::save_session` 对已有 session 保留 backend 的 created_at（与 desktop /clear 同模式）。

## 不做

- 不加 armed 二次确认：桌面 /clear 走 5s armed 是因为单一 webview 内可控；TG 跨设备 / 多用户文化下 5s 窗口不适用。**用 `/reset` 名字区分**让用户预期单击即生效
- 不清 `last_tasks_titles` / `last_tasks_response`：那是 /tasks 的去重缓存，与 LLM 上下文正交，重置后下次 /tasks 仍能正常显
- 不动 desktop `/clear`：两边语义不同，名字也分开

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅（含 3 新测试）
- TG 发几条普通消息让上下文积累 → 发 `/reset` → 收到 🔄 反馈
- 之后再聊 → 行为如新会话（LLM 看不到旧消息）
- 重启 bot → session 仍是 system-only

## 完成

- [x] commands.rs: enum + parser + registry + format_reset_reply + 3 测试
- [x] bot.rs: handler
- [x] format_help_text 加 /reset 行
- [x] `cargo build --release` 通过
- [x] `cargo test --lib` 通过（915 passed，+3 新）
- [x] README 一行
- [x] 移到 docs/done/
