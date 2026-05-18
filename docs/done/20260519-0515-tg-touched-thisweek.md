# TG bot 加 `/touched_thisweek` 命令（iter #536）

## Background

完成日 × 周复盘 scope ladder：

|                   | 仅 done           | 任意状态                |
|-------------------|-------------------|-------------------------|
| 今日              | /today_done       | /touched_today          |
| 昨日              | /yesterday        | /touched_yesterday      |
| 本周              | (无 /done variant)| **/touched_thisweek**   |

owner 写周报 / 周末整理本周产出 / 周一回顾上周末做了啥 — 都需要周维
度全谱视图。本 iter 加。

## Changes

### `src-tauri/src/telegram/commands.rs`

按 6+ surface 模式同步：

1. **Enum 变体** `TouchedThisweek`（无参，紧贴 TouchedYesterday 之后）
2. **`name()` arm** → `"touched_thisweek"`
3. **`title()` arm** → 加入无参 arm 集
4. **parser arm** `"touched_thisweek" => Some(TgCommand::TouchedThisweek)`
5. **en + zh registry** entries
6. **`ALL_HELP_TOPICS`** 加 `"touched_thisweek"`
7. **`format_help_for_topic("touched_thisweek")`** 详细文案（含周边界
   说明 + 三件套关系）
8. **`format_help_text`** 表格行
9. **两份 drift-defense test 列表**

#### 纯 formatter `format_touched_thisweek_reply`

```rust
pub fn format_touched_thisweek_reply(
    views: &[TaskView],
    week_start: chrono::NaiveDate,  // 本周一日期
) -> String {
    let week_start_str = week_start.format("%Y-%m-%d").to_string();
    let mut touched: Vec<&TaskView> = views
        .iter()
        .filter(|v| {
            // ISO 字典序 = 时间序 — updated_at >= week_start_str 即命中
            v.updated_at.len() >= 10 && &v.updated_at[..10] >= week_start_str.as_str()
        })
        .collect();
    if touched.is_empty() {
        return format!("📅 本周（{} 起）暂无动过的 task。\n用 /touched_today 看今日 / /tasks 看全清单 / /yesterday 看昨日完成。", week_start_str);
    }
    touched.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut out = format!("📅 本周（{} 起）动过 {} 条（按时间倒序）：", week_start_str, touched.len());
    for v in &touched {
        // 跨日 scope：行需 MM-DD HH:MM（仅 HH:MM 看不出哪天）
        let date_time = if v.updated_at.len() >= 16 {
            format!("{} {}", &v.updated_at[5..10], &v.updated_at[11..16])
        } else {
            String::new()
        };
        let emoji = match v.status { ... };  // 与 today/yesterday 同 emoji map + snooze 检测
        out.push_str(&format!("\n· {} {} {}", emoji, date_time, v.title));
        // done 附 result preview (40 char cap，与 today/yesterday 同)
        ...
    }
    out
}
```

与 today/yesterday split 区别：

- **MM-DD HH:MM per line**：跨日 scope 不能省 date（today/yesterday 是
  单日 scope，仅 HH:MM 就够）
- **header「本周（YYYY-MM-DD 起）」**：让 owner 一眼看周一日期，确认
  scope 起点准确
- **filter 用 prefix 比较**：`v.updated_at[..10] >= week_start_str` —
  ISO 字典序 = 时间序，所以 prefix 比较 = 日期 ≥ 周一即可（无需逐 task
  parse 完整 ISO）

### `src-tauri/src/telegram/bot.rs`

Handler 紧贴 TouchedYesterday 之后：

```rust
TgCommand::TouchedThisweek => {
    use chrono::Datelike;
    let views = read_tg_chat_task_views(chat_id.0);
    let today = chrono::Local::now().date_naive();
    // chrono 的 weekday().num_days_from_monday() 返 0..=6（Mon=0..Sun=6）
    let days_from_mon = today.weekday().num_days_from_monday() as i64;
    let week_start = today - chrono::Duration::days(days_from_mon);
    format_touched_thisweek_reply(&views, week_start)
}
```

`num_days_from_monday()` 是 chrono 标准 API — Mon=0 让算法直接得 days
to subtract。

## Key design decisions

- **周一起算（ISO weekday Mon=1）而非周日起算（US weekday Sun=0）**：
  与中国 / 欧洲 owner 常识一致 — 「本周」始于周一；周末仍算本周末。
  chrono `num_days_from_monday()` 正好返这个语义
- **MM-DD HH:MM per line**：跨日 scope 必须见 date；与 today/yesterday
  split 的 HH:MM-only 有意区分
- **`updated_at[..10] >= week_start_str`** prefix 比较：ISO 字典序 = 时
  间序，prefix 10 字符（YYYY-MM-DD）足够日比较；避免 parse 完整 ISO
- **空集教学不指 `/touched_thisweek` 自身**：avoid loop；指
  `/touched_today` / `/yesterday` / `/tasks` 让 owner 知道 narrower /
  broader scope alt
- **clone 不抽 generic**：与既有 today_done / yesterday split / digest
  split / touched_today/yesterday split 一致 — 单测点稳定 + 行内 < 80 行
- **5 个 unit tests pin 真实行为**：parser + 空集教学（验 3 alt 入口）+
  周内/外过滤（Mon-Sun included，上周日 excluded）+ MM-DD HH:MM 格式 +
  snooze 💤 优先
- **bug fixed: ASCII 直引号在 Rust 字面量内**（recurring 教训）：help-
  detail 文案写「"新本周"」ASCII 双引号在 Rust `"..."` 内 — parser
  中止。改用 fullwidth「「新本周」」修复（与 iter #514 oldest_done 同
  教训）

## Verification

- `cargo build`（src-tauri）— clean
- `cargo test --lib` — all 1670 tests pass（新 5 + 既有 1665）
- 三个 drift-defense test all pass
- 手测：
  - 周三发 `/touched_thisweek` → header「本周（周一日期 起）」+ Mon-Wed
    动作清单
  - 周日发 → 仍算本周（直到周一 00:00）
  - 周一发（早晨） → 仅显本周一今日动作（昨日已在上周）
  - 空集 → 三 alt 入口教学

## Future iters (out of scope)

- `/touched_lastweek` — 上周复盘；按需 propose
- `/digest_thisweek` — 仅 done + result preview 周版（与 /yesterday /
  /digest_yesterday 形成 done 维度的周对偶）；按需
- 周一日期可配置（owner 选周日起算 vs 周一起算）— 当前 Mon-start 是
  默认，配置化未来 iter
