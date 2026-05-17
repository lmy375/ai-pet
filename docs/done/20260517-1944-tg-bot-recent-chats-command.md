# TG bot `/recent_chats [N]` 命令（iter #378）

## Background

owner 在手机端想「我刚才让 pet 做啥来着 / pet 上一句话说啥来着」，
需回桌面滚 ChatMini 才能看完整 chat 历史 — 缺手机端 audit 入口。

本 iter 加 `/recent_chats [N]` 让 TG 端列 active session 内最近 N
条 user ↔ pet 往返（过滤 tool_call / 系统行），与 /feedback_history
（隐性反馈 audit）/ /alarms（reminders audit）形成"三 audit 命令簇"。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. enum 变体（~line 193）

```rust
RecentChats { n: u32 },
```

#### 2. `name()` / `title()` arms

- name → "recent_chats"
- title() → "" (N-only 簇)

#### 3. parser

与 /alarms / /digest 同 clamp 模板 — N 缺省 5，clamp 1..=20。

#### 4. `format_recent_chats_reply` pure formatter

入参 `items: &[(role, excerpt)]`（caller 已 cap N + truncate excerpt
至 80 字）+ session_title + session_updated_at + n + total（含 N 之
外的 user/assistant 总条数，用于 overflow hint 算法）：

- 空 items → "💬 暂无聊天记录" + 引导 ChatMini / ChatPanel 创建
- 非空 → header "💬 最近 N 条 chat · 会话「title」最近活动 MM-DD
  HH:MM：" + 逐行 `<role glyph> <excerpt>` + overflow hint（如有）
- role glyph：🧑 user / 🐾 assistant — 与桌面 ChatPanel export
  markdown 同视觉锚
- session_title 超 24 字 → trim + …；空 title → "（无标题）" 兜底
- session_updated_at "YYYY-MM-DDTHH:MM:SS.sss" → 切 "MM-DD HH:MM"

```rust
pub const RECENT_CHATS_EXCERPT_CHARS: usize = 80;
pub fn format_recent_chats_reply(
    items: &[(String, String)],
    session_title: &str,
    session_updated_at: &str,
    n: u32,
    total: usize,
) -> String { ... }
```

#### 5. registry zh + en + format_help_text + format_help_for_topic
+ ALL_HELP_TOPICS + 两 drift-defense 列均加 "recent_chats"

### `src-tauri/src/telegram/bot.rs`

新 handler arm（紧贴 Alarms 之前）：

```rust
TgCommand::RecentChats { n } => {
    let idx = session::list_sessions();
    if idx.active_id.is_empty() {
        return format_recent_chats_reply(&[], "", "", n, 0);
    }
    match session::load_session(idx.active_id) {
        Ok(session) => {
            let mut all = session.items.filter_map(...).collect();
            let total = all.len();
            if all.len() > n { all.drain(0..all.len() - n); }
            format_recent_chats_reply(&all, &session.title,
                &session.updated_at, n, total)
        }
        Err(_) => format_recent_chats_reply(&[], "", "", n, 0),
    }
}
```

filter 逻辑：仅 `type == "user" | "assistant"`（跳过 tool_call /
系统行）；content 去 newline → 单空格折行；trim + take 80 chars +
… 截断。

### Tests（commands.rs，9 个新 unit test）

Parser（4 个）：
- 默认 N=5 / 显式 N=10 / clamp 上下界 / 非数字 fallback

Formatter（5 个）：
- 空 items → bootstrap
- 双 role glyph 渲染（🧑 / 🐾）+ session title + MM-DD HH:MM
- 长 title 超 24 字 truncate "…"
- overflow hint（total=10 / shown=3 → "还有 7 条更早"）
- no overflow hint when total === items.len()
- 空 title → "（无标题）" fallback

## Key design decisions

- **复用 active session 而非每个 TG chat 独立 session**：当前架构
  TG 与桌面共享 session（telegram/bot.rs line 377+ 把 TG msg 写到
  session.items 同 schema）。本命令读 active session 是"看 pet 现
  在的上下文"自然语义。
- **过滤 tool_call / 系统行**：owner audit "我说啥 / pet 答啥"，
  不想看 internal tool 调用噪音。与桌面 ChatMini 默认隐藏 tool 行
  同语义。
- **EXCERPT_CHARS = 80 而非 60**：chat 整句通常 > feedback_history
  excerpt（60），owner 看 chat 回顾时想要句子完整度更高。total N=20
  × 90 char ≈ 1800 < TG 4096 limit 内安全。
- **session 级 updated_at 而非 per-msg ts**：后端 schema 没存 per-msg
  ts；session 级 updated_at 是"最近活动"信号 — 与 ChatPanel /
  PanelTone 内 session info 同 source。help text 明示此限制。
- **role glyph 🧑 / 🐾**：与 panelChatBits.tsx export 路径 emoji 锚
  一致 — owner cross-surface 阅读心智统一。
- **input `n` 与 `total` 同时传 formatter**：让 formatter 算 overflow
  hint 不需后端 sort/cap 双层调用栈泄漏。caller 一次 cap，formatter
  一次渲染。
- **不引入 `_` 模式参数避免 `n` warning**：formatter 接 n 但未直接
  使用（cap 已由 caller 完成）— 加 `let _ = n;` 标记吸收 unused
  warning，保签名稳定（如未来需要 n 算 overflow 信号可直接用）。

## Verification

- `cargo check`（backend）— clean（一次 ASCII 双引号嵌 Rust 字面
  量报错 → 改 「」 修复，与历史 iter #371 同类型修复）
- `cargo test --lib`（backend）— **1328 passed / 0 failed**（+10
  新 recent_chats test，两 drift-defense 列也命中 "recent_chats"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
