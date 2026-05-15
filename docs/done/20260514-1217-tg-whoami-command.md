# TG bot `/whoami` 命令 —— 与桌面 chat 对偶

## 背景

TODO（auto-proposed 本轮）：

> TG bot `/whoami`：与桌面 chat `/whoami` 对偶（陪伴 / 心情 / 自我画像头 / 近常用工具 top 3），手机端也能让宠物自报家门。

上一波（20260514-1113）实现了桌面 chat `/whoami`：四源聚合让宠物自报家门。TG 端缺这个对偶 —— TG 已有 `/mood` / `/today` / `/stats` 等单源命令，但没有"composite 自我介绍"。手机用户也想问宠物"你是谁"。

## 改动（backend Rust only）

### `src-tauri/src/telegram/commands.rs`

**1. `TgCommand::Whoami` 变体**

放在 `TgCommand::Mood` 旁（语义相邻），无参；`name()` 返 `"whoami"`；`title()` 与其它无参命令一致返空串。

**2. parser case**

```rust
"whoami" => Some(TgCommand::Whoami),
```

放在 `"mood"` 之后（与 enum 顺序对齐）；多余尾部一律忽略（让 `"/whoami please"` 也能命中），与既有无参命令同模式。

**3. `format_whoami_reply` pure formatter**

签名：

```rust
pub fn format_whoami_reply(
    user_name: &str,
    companionship_days: Option<u64>,
    mood: Option<(String, Option<String>)>,
    persona_summary: &str,
    top_tools: &[(String, u64)],
) -> String
```

与桌面 `case "whoami"` 完全对齐：emoji 顺序（🪪 / 🐾 / 📅 / 💗 / 🪞 / 🛠）+ 首段切分（按 `\n\n` 取头） + 90 字截断 + 工具 top 3 含频次 + 全空兜底"还没攒到自我介绍的素材"。每段独立可缺失（None / 空 / trim 后空），单源缺失只省略该行不挂整段。

**4. `format_help_text` 列入**

`/help` 文案在 `/mood` 后插一行 `/whoami  —  宠物自我介绍（陪伴 / 心情 / 自我画像 / 近常用工具）`，让发现性与其它命令对齐。

**5. 9 个新单测**

- `parses_whoami` / `parses_whoami_ignores_trailing` —— parser case
- `whoami_reply_full_signal_renders_all_lines` —— happy path
- `whoami_reply_zero_days_says_today` —— 0 天初识文案
- `whoami_reply_skips_missing_sources` —— 单源缺失独立忽略
- `whoami_reply_all_empty_falls_back_to_friendly_line` —— 全空兜底
- `whoami_reply_truncates_long_persona_summary` —— 90 字截断 + `…`
- `whoami_reply_persona_first_paragraph_only` —— 双空行后段不出现
- `whoami_reply_top_tools_caps_at_three` —— top 4-5 时不渲染

### `src-tauri/src/telegram/bot.rs`

dispatch handler 紧跟 `TgCommand::Mood`：

```rust
TgCommand::Whoami => {
    let user_name = crate::commands::settings::get_user_name();
    let companionship_days = Some(crate::companionship::companionship_days().await);
    let mood = crate::mood::read_current_mood_parsed();
    let persona_summary = crate::commands::memory::read_ai_insights_item("persona_summary")
        .map(|i| i.description)
        .unwrap_or_default();
    let top_tools: Vec<(String, u64)> = crate::tool_call_history::get_top_tools_used()
        .into_iter()
        .map(|s| (s.name, s.count))
        .collect();
    crate::telegram::commands::format_whoami_reply(
        &user_name, companionship_days, mood, &persona_summary, &top_tools,
    )
}
```

5 个 IPC 都是廉价同步 read（< 1ms），不开 async / spawn —— 单 `companionship_days().await` 是已经在 `async fn handle` 上下文里的天然 await，与其它 TgCommand 路径节奏一致。

## 不做

- **不动桌面 chat `/whoami`**。已存在；本轮只补 TG 对偶。
- **不抽公共 helper 把桌面 / TG 两侧的 fetch + format 合一**。两端走完全不同的 IPC / TS 模板字面量、emoji 字符在 TG 是普通 string 而桌面是 React subdued bubble 渲染 —— 强行抽象代价大于收益。语义对齐通过单测和 README 文案手动同步。
- **不让 LLM 调用 `/whoami`**。是用户面向 slash，不是 tool；后端不需要新增 IPC。
- **不缓存结果**。每次都重新 fetch；信号变化即时反映（与 `/mood` / `/stats` 同模式）。
- **不在 TG 加 markdown 渲染**。当前 TG bot 输出走纯文本（HTML 模式仅在 errors 等特定场景）；`` ` `` 字符在 TG 显示就是反引号，不会渲染成 code。

## 验证

- `cargo check` ✓ 0 error
- `cargo test --lib telegram::` ✓ 168 / 168 通过（含 9 个新增 whoami 测试）
- `cargo test --lib` ✓ **943 / 943 通过**

## 后续

- 桌面端 `/whoami` 和 TG 端如果未来对齐文案要改一处，记得手动同步两侧 `format_whoami_reply` 与桌面 `case "whoami"`。可考虑加一个 single-source `WhoamiSignals` 结构 + 两个独立 formatter，让"signals 数据"统一而"渲染"分离。
- `/whoami` 高频使用可让 proactive 偶尔自报家门（与 `morning_briefing` 同模式）。
