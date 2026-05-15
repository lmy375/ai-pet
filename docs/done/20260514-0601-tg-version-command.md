# TG bot `/version` 命令

## 背景

桌面 PanelChat 有 `/version`（输出 `🐾 pet vX.Y.Z` + `schema vN` + `平台 ...`），Settings chip 有 `pet vX.Y.Z`（点击复制完整字符串）。TG 端没有版本入口 —— bug report 写"什么版本"得切桌面看 Settings。

加 TG `/version` 让手机端能就地拿到版本信息。

## 改动

### `src/telegram/commands.rs`

- `TgCommand::Version` variant
- `name()` / `title()` 接上
- parser `"version" => Some(TgCommand::Version)`（多余尾部忽略）
- registry zh/en 各加一行
- 新 pure fn `format_version_reply(app_version: &str, schema_version: i32) -> String`：

  ```
  🐾 pet vX.Y.Z
  schema vN
  ```

  schema=0 时省略该行（旧 backend 兼容）；app_version 空时降级为 "pet（版本号缺失）"。
- `format_help_text` 加 `/version` 行
- 单测：parse / parse-with-trailing / format 各几个

### `src/telegram/bot.rs`

handler：

```rust
TgCommand::Version => {
    let app_version = env!("CARGO_PKG_VERSION");
    let schema_version = crate::db::with_db(|conn| {
        let v: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _migrations",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Ok(v)
    }).unwrap_or(0);
    crate::telegram::commands::format_version_reply(app_version, schema_version)
}
```

简单 SQL 查 schema_version，与 `get_db_stats` 同模式但只取一字段 —— 不引入新的 Tauri 命令。

## 不做

- 不包括"平台" / navigator.platform：那是 webview/browser API，Tauri 后端没有；ought to log OS via std::env::consts::OS 但这又是新维度，先不加（用户主要关心 app 版本 + schema）
- 不让 /version 触发 setMyCommands 重注册等副作用：纯只读

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅（含 3 新测试）
- TG 发 `/version` → "🐾 pet v0.1.0\nschema v4"
- `/help` 输出含 `/version` 行

## 完成

- [x] commands.rs: enum + parser + registry + format_version_reply + 5 测试
- [x] bot.rs: handler
- [x] format_help_text 加 /version 行
- [x] `cargo build --release` 通过
- [x] `cargo test --lib` 通过（920 passed，+5 新）
- [x] 移到 docs/done/
