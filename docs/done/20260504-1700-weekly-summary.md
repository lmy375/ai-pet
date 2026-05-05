# 周报合成 — 开发计划

> 对应需求（来自 docs/TODO.md「已确认」）：
> 周报合成：每周日由 consolidate 汇总本周任务/对话/情绪/陪伴指标，输出可读周报。

## 目标

每个 ISO 周末（默认周日 20:00 后），由 consolidate 后台循环触发一次"周报合成"：把本周的 **管家事件 / 主动开口 / 心情趋势 / 陪伴天数** 汇总成一段可读的 markdown，写入 `ai_insights/weekly_summary_YYYY-Www`，供用户在面板「记忆」标签里回看 / 让宠物在下周一开口时引用。

整条流水线**确定性**（不动 LLM），让"每周必有一份周报"成为可信契约 —— 即便 API key 失效或 consolidate 被禁用，周报仍按时落地。这与 `daily_review.rs` 的设计哲学一致（pure 门控 + 纯 IO 写入）。

## 非目标

- 不做语音 / TTS / 推送通知 — 周报只是个静态的 ai_insights 条目。
- 不在桌面气泡里弹出周报 — 用户在面板「记忆」标签自己看 + 周一第一次 proactive turn 自然带过即可。
- 不替代 daily_review — 两者互补：daily 是当晚摘录、weekly 是跨日折线。
- 本轮不接 LLM 改写 — 先把确定性的版本跑通，未来视效果再决定要不要二次 LLM 美化。
- 不做"按月 / 按季度"延展 — 周粒度先验证，更长粒度等 GOAL 确定后再上。

## 设计

### 触发门控（pure）

```rust
fn should_trigger_weekly_summary(
    now: NaiveDateTime,
    last_summary_week: Option<IsoWeek>,
    closing_hour: u8,           // 默认 20
) -> Option<IsoWeek>
```

返回 `Some(week)` 表示该 week 现在应当被合成；`None` 表示跳过。

判定语义：
- 找出"最近一个已结束的周"。"已结束"意味着 `now >= 该周周日 closing_hour:00`。
  - 若今天是周日 ∧ `now >= today closing_hour:00` → 该 week = `now.iso_week()`
  - 否则 → 找最近过去的周日（last week 或更早 — 但通常就是 last week）
- 若 `last_summary_week == target` → 已合成，跳过
- 否则返回 `Some(target)`

这套语义自然 cover：
- 周日晚上正常触发（loop 在 20:00 后第一次唤醒命中）
- 周日错过（mac 关机 / consolidate 被禁用 < 6h）→ 周一 / 周二的下次 loop 唤醒会因为 `last != target` 而**补发**
- 跨年 ISO 周（例如 2025-W53）—— `IsoWeek` 自带 (year, week) 唯一身份

### 数据收集（pure，输入注入）

```rust
struct WeeklyStats {
    week: IsoWeek,
    week_start: NaiveDate,        // 周一
    week_end: NaiveDate,          // 周日
    speech_count: u64,
    butler_create: u32,
    butler_update: u32,
    butler_delete: u32,
    completed_titles: Vec<String>,// description 含 "[done]" / "[cancelled" 的 update / delete 事件标题
    mood_top: Vec<(String, u64)>, // motion → count，降序，最多 3 条
    companionship_days: u64,
}
```

aggregator 函数都是 pure（输入是日志原文 + 日期范围，输出是 `WeeklyStats`）。IO 层负责读 `speech_history.log` / `butler_history.log` / `mood_history.log` 文本文件后调用 pure aggregator，便于单测。

### 输出格式（pure）

#### detail.md（写入 weekly_summary 条目的正文）

```markdown
# 周报 — 2026-W18 (4月27日 — 5月3日)

## 任务
本周管家事件 N 条（创建 X / 更新 Y / 删除 Z）。
完成或取消：
- 整理 Downloads
- ...

## 对话
本周主动开口 N 次。

## 情绪
本周心情主要是 Tap × 12、Idle × 8、Flick × 3。

## 陪伴
累计陪伴 K 天。
```

空段（如本周无心情记录）显示「（本周无记录）」而非省略 — 让用户能区分"功能没跑"vs"真的安静"。

#### description（一行索引）

```
[weekly] 主动开口 N 次，管家事件 M 条，陪伴 K 天
```

机器标签 `[weekly]` 与 daily_review 的 `[review]` 同形，便于未来 consolidate 整理时识别。

### 写入路径

- 类目：`ai_insights`
- 标题：`weekly_summary_YYYY-Www`（如 `weekly_summary_2026-W18`）
- detail：上面的 markdown
- description：上面的一行

### 集成点

`consolidate.rs::spawn` 的 loop 体里，**在 `cfg.enabled` 检查之前**插入：

```rust
maybe_run_weekly_summary(&app, chrono::Local::now()).await;
```

这条调用与 consolidate 主体解耦：周报独立运行，与 LLM 整理无关。loop 唤醒间隔为 `interval_hours.max(1) * 3600`（默认 6h），周日 20:00 后基本必中下一次唤醒。

跨进程幂等性：
- 进程内 `LAST_WEEKLY_SUMMARY_WEEK: Mutex<Option<IsoWeek>>` 静态
- 跨重启：写入前 `read_ai_insights_item("weekly_summary_YYYY-Www")` 校验

### 配置

新增字段（`MemoryConsolidateConfig`，与现有 stale_* 字段同位）：

```rust
#[serde(default = "default_weekly_summary_closing_hour")]
pub weekly_summary_closing_hour: u8,  // 0 = 禁用；默认 20
```

0 关闭周报；其它值视为周日触发的小时。不暴露面板 UI（默认值合理；用户想关 / 调可以直改 yaml）。

## 阶段划分

| 阶段 | 范围 | 状态 |
| --- | --- | --- |
| **M1** | `weekly_summary.rs` 纯模块（gate + aggregator + format）+ 单测 | ✅ 完成（19 条新单测，全套 cargo test 724/724） |
| **M2** | `consolidate.rs::maybe_run_weekly_summary` IO 层 + 接入 loop | ✅ 完成 |
| **M3** | settings 字段 + 收尾（README / TODO / done/） | ✅ 完成 |

## 复用清单

- `chrono::IsoWeek` / `Datelike::iso_week` —— 周身份
- `commands::memory::{memory_edit, read_ai_insights_item}` —— 写入 / 幂等校验
- `speech_history` / `butler_history` / `mood_history` 各自现有的解析约定（每行带 ISO 时间戳）
- `companionship::companionship_days`（async）—— 陪伴天数
- `proactive::butler_schedule::parse_updated_at_local` —— 复用 RFC3339 → NaiveDateTime
- `proactive::daily_review::format_yesterday_recap_hint` 模式 —— description 用 `[标签] 内容` 形式

## 待用户裁定的开放问题

1. **触发时刻**：默认 20:00 是不是太早？太晚（如 23:00）会与 daily_review (22:00) 抢同一波 LLM 调用 / 决策日志；20:00 留出"周日傍晚回顾今晚还能聊"的窗口。先用 20:00 看反馈。
2. **是否需要 LLM 二次合成成更顺口的中文**：本轮不做，留 todo："周报 LLM 渲染"作为后续选项。
3. **周报里要不要加 deep_focus 数据**（已有 active_app 模块）：暂不加 —— 先把核心四块（任务 / 对话 / 情绪 / 陪伴）跑顺。

## 进度日志

- 2026-05-04 17:00 — 创建本文档；准备进入 M1。
- 2026-05-04 17:45 — M1-M3 一次性合到 main：
  - **M1**：`src-tauri/src/weekly_summary.rs` 落 `should_trigger_weekly_summary` + 三个 aggregator (`aggregate_speech_count` / `aggregate_butler_events` / `aggregate_mood_top`) + `format_weekly_summary_detail` / `_description` + `weekly_summary_title`。19 条单测覆盖：closing hour 0/无效、周日 closing 触发、周日 closing 前跳过 / 补发、周一早晨补上周、catch-up 周四仍能补、ISO 周边界、speech 区间过滤、butler 三个 action 计数 + completed/cancelled 抽取 / 去重、mood top 排序 + truncate、detail 三段空白文案、description 一行格式、ISO 周标题 zero-pad。
  - **M2**：`speech_history` / `butler_history` / `mood_history` 各加 pub `read_history_content() -> String` —— 把"读 .log 全文"暴露成统一异步接口，避免 path 细节散在 consolidate 里。`consolidate.rs::maybe_run_weekly_summary(app, now, closing_hour)` 在 loop 体最前面调用，**与 cfg.enabled 解耦** —— 即便 LLM 整理被禁用，周报仍照常合成。跨进程幂等：先看 `LAST_WEEKLY_SUMMARY_WEEK` 静态，再读 `read_ai_insights_item` 验标题，两道关都未命中才落盘。
  - **M3**：`MemoryConsolidateConfig.weekly_summary_closing_hour: u8`（默认 20，0 = 关）；`useSettings.ts` / `PanelSettings.tsx` 默认值同步。README 加亮点；TODO 移除条目；本文件移入 `docs/done/`。`cargo test --lib` 724/724 通过；`tsc --noEmit` 干净。
- **开放问题答复**：
  - Q1 触发时刻：保留 20:00。后续如发现 daily_review 22:00 与周报落盘的"几乎同时"会让 ai_insights 类目在 panel 列表抖动，再考虑挪到 21:00 或加 staggering。
  - Q2 LLM 二次合成：仍不做。确定性版本已能给用户"上周做了哪些事"的事实回看；让 LLM 改写反而引入幻觉风险（特别是数字字段）。
  - Q3 deep_focus 数据：暂不加。后续如果用户反馈想看"上周专注分钟"，再补一段 active_app 聚合即可，本架构留出空位。
