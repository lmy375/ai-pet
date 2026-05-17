# TG bot 加 `/touched_today` 命令（iter #510）

## Background

既有 today-scoped TG 视图：
- `/today` — 今日叙事：pending+due_today + done+updated_today 两段
- `/today_done` — 仅 done + updated_at 今日 — 「完成产出」单维度
- `/yesterday` — 昨日 done — 复盘维度

但缺**「今天动过哪些」全谱视图** — owner 今天 pin/silent/snooze/touch/
edit 过的 pending task / 失败的 task / 取消的 task 都 updated_at 在今
日但 /today_done 看不见（仅 done）。sprint 中段「我今天到底做了 / 调
了 / 推后了哪些」audit 缺入口。

本 iter 加 `/touched_today` — 列任意状态、updated_at 命中今日的 task，
按时间倒序 + HH:MM 时间前缀。

## Changes

### `src-tauri/src/telegram/commands.rs`

按 6+ surface 模式全量同步：

1. **Enum 变体** `TouchedToday`（无参，紧贴 RecentEvents 之后）
2. **`name()` arm** → `"touched_today"`
3. **`title()` arm** → 加入无参 arm 集（Tasks/Pinned/Silenced/Markers/
   Snippets/Tags/...）
4. **parser arm** `"touched_today" => Some(TgCommand::TouchedToday)` —
   多余尾部容忍（与 /today / /yesterday / /today_done 同协议）
5. **en + zh registry** entries
6. **`ALL_HELP_TOPICS`** 加 `"touched_today"`
7. **`format_help_for_topic("touched_today")`** 详细文案（含与
   /today_done / /today 三视图区别 + 场景说明 + 状态 emoji 表）
8. **`format_help_text`** 表格行
9. **两份 drift-defense test 列表**

#### 纯 formatter `format_touched_today_reply`

```rust
pub fn format_touched_today_reply(
    views: &[TaskView],
    today: chrono::NaiveDate,
) -> String {
    let t_str = today.format("%Y-%m-%d").to_string();
    let mut touched: Vec<&TaskView> = views
        .iter()
        .filter(|v| v.updated_at.starts_with(&t_str))
        .collect();
    if touched.is_empty() {
        return format!(
            "📅 今日（{}）暂无动过的 task。\n用 /today 看今日 due / /today_done 看今日完成 / /tasks 看全清单.",
            t_str
        );
    }
    touched.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut out = format!("📅 今日（{}）动过 {} 条（按时间倒序）：", t_str, touched.len());
    for v in &touched {
        let hm = if v.updated_at.len() >= 16 { &v.updated_at[11..16] } else { "" };
        let emoji = match v.status {
            TaskStatus::Done => "✅",
            TaskStatus::Error => "⚠️",
            TaskStatus::Cancelled => "🚫",
            TaskStatus::Pending => {
                if v.raw_description.contains("[snooze:") { "💤" } else { "⏳" }
            }
        };
        // line render + done 状态附 result preview (40 char cap)
        ...
    }
    out
}
```

差异点 vs `format_today_done_reply`：
- 不限 status — 任意状态都显
- pending + `[snooze:]` marker 用 💤 单独区分（owner 一眼看 "今天被推
  后" vs "今天仍活着"）
- 每行加 HH:MM 时间前缀（done 文案没显时间因为 result 段就在）

8 个 unit tests pin 真实行为：parser（含尾部容忍）/ 空兜底 / status 不
限过滤 / 时间倒序 / 4 种 status emoji 各自 / snooze 💤 + ⏳ 互斥 /
HH:MM 前缀 / done result preview。

### `src-tauri/src/telegram/bot.rs`

Handler 紧贴 TodayDone 之后：

```rust
TgCommand::TouchedToday => {
    let views = read_tg_chat_task_views(chat_id.0);
    let today = chrono::Local::now().date_naive();
    crate::telegram::commands::format_touched_today_reply(&views, today)
}
```

与 TodayDone 同 read path + 同 today_str 解析；区别仅在 formatter。

## Key design decisions

- **任意 status 都显**：本命令的 audit 价值就是「全谱」— 排除 pending
  /error/cancelled 就退化成 /today_done。HH:MM 前缀让密集状态扫读时
  仍可读
- **pending + [snooze:] → 💤**：snooze 在 backend 是 pending status
  （等到点重激活），但 owner 心智里它是「我推后了」— 单 emoji 区分
  比单 ⏳ 更准
- **done 状态显 result preview**：与 /today_done 同模板（40 char cap）
  让本命令既能当 daily audit 入口，也能当 "今天完成了啥 + 怎么完成"
  daily summary 入口
- **不显 result preview for non-done**：error 的 error_message / pending
  的 body / cancelled 的 reason 都是不同 axis；single one-line view 想
  保紧凑，complex cases 走 /show / /timeline / /peek
- **按 updated_at 倒序**：最新动作在前 — owner 想"刚才我刚 touch 的
  是啥"自然在顶部，与 ChatMini bubble 滚动方向一致
- **空集 friendly + cross-ref**：empty 时点 /today / /today_done /
  /tasks 三 alt 入口教学，避免 owner 看到空状态没下一步
- **8 个 unit tests pin 真实行为**（满足 GOAL.md "meaningful tests
  only"）：每条覆盖一种独立语义路径（parser tolerance / empty fallback /
  status non-filter / time sort / 4 emoji map / snooze override / HH:MM
  prefix / result preview）

## Verification

- `cargo build`（src-tauri）— clean
- `cargo test --lib` — all 1618 tests pass（新 8 + 既有 1610）
- 三个 drift-defense test all pass
- 手测：
  - `/touched_today` → 「📅 今日（YYYY-MM-DD）动过 N 条」+ 多 emoji 多
    状态 task 按时间倒序
  - 今天 nothing touched → 友好兜底 + 三 alt 入口
  - pending + snooze → 💤；普通 pending → ⏳

## Future iters (out of scope)

- 「按 owner-action only」filter（剥 LLM update 引发的 result 写）— 当
  前/timeline 已 dedup「无 marker 变化」update，本入口看全谱仍有价值；
  下游入口可考虑
- `/touched_yesterday` 对偶 — 后续 propose
