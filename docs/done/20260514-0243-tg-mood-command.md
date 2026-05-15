# TG bot `/mood` 命令 — 手机端"宠物现在感觉如何"

## 背景

桌面 pet 窗有 MoodWidget 持续显示宠物心情；TG 端**完全感知不到** —— 用户在外不能问"宠物现在是开心还是失落"。`/stats` 看的是任务，与心情正交。

加 `/mood` 让用户一行命令拿到当前心情快照（text + 可选 motion group）。

## 改动

### `src-tauri/src/telegram/commands.rs`

- `TgCommand::Mood` variant
- `name()` / `title()` 接上
- `parse_tg_command` 加 `"mood" => Some(TgCommand::Mood)`，多余尾部忽略
- `tg_command_registry_localized` zh/en 各加一行（中文 "查看宠物当前心情" / en "Show the pet's current mood"）
- 新 pure fn `format_mood_reply(parsed: Option<(String, Option<String>)>) -> String`：
  - Some((text, Some(motion))) → "🐾 心情：text\n  动作组：motion"
  - Some((text, None)) → "🐾 心情：text"
  - None → "🐾 宠物还没记心情；一会儿主动开口时会写一笔。"
  - text 为空时（如 motion 已设但 text 空）→ "🐾 心情：（无文字）"
- `format_help_text` 补一行 `/mood  —  查看宠物当前心情`

### `src-tauri/src/telegram/bot.rs`

handler：

```rust
TgCommand::Mood => {
    let parsed = crate::mood::read_current_mood_parsed();
    crate::telegram::commands::format_mood_reply(parsed)
}
```

### 单测

- `parses_mood`：`/mood` → `TgCommand::Mood`
- `parses_mood_ignores_trailing`：`/mood now?` 仍命中（与 /tasks /stats 同模式）
- `tg_command_registry_covers_all_user_facing_commands`：assert contains "mood"
- format 三态：含 motion / 无 motion / None

## 不做

- 不暴露完整 mood history：那是桌面 MoodWidget hover 的活；TG 端只回当前快照
- 不分 chat 过滤：心情是宠物全局状态，对所有 chat 一样（不像 tasks 有 origin）
- 不加 emoji 映射 motion → 拟人形容词：motion 字符串本身（如 `happy_idle`）足够清晰，再翻译反而增加歧义

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅（含 4 新测试）
- TG 发 `/mood` → 6 行内的简短回复
- 没心情记录时 → 友好的"还没记"反馈，不是空字串

## 完成

- [x] commands.rs: enum + parser + registry + format_mood_reply + tests
- [x] bot.rs: handler
- [x] format_help_text 加一行
- [x] `cargo build --release` 通过
- [x] `cargo test --lib` 通过（905 passed，+6 新）
- [x] README 一行
- [x] 移到 docs/done/
