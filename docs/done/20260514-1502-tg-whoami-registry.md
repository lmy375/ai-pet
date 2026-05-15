# TG `/whoami` 命令进入 setMyCommands registry + 防漏注册测试加固

## 背景

TODO（auto-proposed 之前）：

> TG `/whoami` 加入 tg_command_registry 让 TG 的 slash autocomplete 列表能浮（命令本身已实现但未注册到 setMyCommands 列表）。

20260514-1217 实现 TG `/whoami` 时漏了一步：parser 接受、handler 工作、format_help_text 列出，但 `tg_command_registry_localized` 这张表（用于 Telegram setMyCommands API）没补 `/whoami` 这一行。结果：手机 TG 客户端敲 `/` 时 autocomplete bubble 里看不到 `whoami`，只能靠用户记得这命令存在。同样的漏注册问题之前也波及 `/snooze` / `/unsnooze`（20260514-1337 实现时一并注册了，OK）。本轮把 `/whoami` 补上 + 加测试钉死，防止下次新增命令时重蹈覆辙。

## 改动（backend Rust only）

### `src-tauri/src/telegram/commands.rs`

**1. 注册到 `tg_command_registry_localized`**

zh / en 两个 arm 各加一行（放在 `mood` 与 `today` 之间，与 chat slashCommands 的位置呼应）：

```rust
// zh
("whoami", "宠物自我介绍（陪伴 / 心情 / 自我画像 / 近常用工具）"),
// en
("whoami", "Show pet's whoami digest (companionship / mood / persona / top tools)"),
```

**2. 测试加固：用列表 + for 循环替代手敲 11 个 assert**

```rust
#[test]
fn tg_command_registry_covers_all_user_facing_commands() {
    let names: Vec<&str> = tg_command_registry()
        .into_iter().map(|(n, _)| n).collect();
    for expected in [
        "task", "tasks", "cancel", "retry", "done", "stats", "mood",
        "whoami", "snooze", "unsnooze", "today", "reset", "version", "help",
    ] {
        assert!(
            names.contains(&expected),
            "registry missing user-facing command `{}`",
            expected,
        );
    }
}
```

新命令加入时只需在两处（registry + 本测试列表）同步；漏一处测试就挂。比"逐条 assert"列表更难悄悄漏。

## 不做

- **不动 format_help_text 文案**。其早已含 `/whoami` 行（20260514-1217 已修），与本次 registry 注册独立。两条 surface 都正确反映 /whoami 存在。
- **不写 TG 端 e2e 测试** —— setMyCommands 是远端 API 调用，pet 启动时一次性 push；本地单测只能保证 payload 完整。
- **不动 /whoami 实际逻辑**。dispatch + format_whoami_reply 早已就位。

## 验证

- `cargo test --lib telegram::` ✓ 184 / 184 通过（含更新后的 coverage test）
- `cargo test --lib` ✓ **978 / 978 通过**
- 改动 ~10 行（2 个新 registry entry + 测试列表化）；无运行时行为变化（除 setMyCommands payload 多一条）。

## 后续

- 加入 TG bot 自动测试基础设施时（mock teloxide），可断言 setMyCommands 实际发出的 payload 长度 == registry.len()。
- 长期看 /help format_help_text 也走 registry source-of-truth 让两 surface 同步（当前是两份各自维护，registry 是 setMyCommands 用、format_help_text 是 /help 输出用）。

## TODO 状态

- 本轮实现 1 条。
- TODO 剩 2 条：ChatMini 跳到 Panel deeplink / PanelDebug session size 卡片。
