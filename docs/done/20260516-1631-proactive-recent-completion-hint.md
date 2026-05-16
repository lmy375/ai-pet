# proactive prompt 加最近 24h 完成任务 hint

## 背景

TODO 上 auto-proposed 一条："proactive prompt 加『最近 24h 完成的 N 条任务』维度：让 LLM 主动开口时可『咱昨天搞定的 X 怎么样了』等连贯关怀。"

既有 `task_completion_hint` 仅追踪"上一 tick → 当前 tick 之间新转 done"的瞬时增量 —— fires once per transition，ephemeral。owner / pet 在过去 24h 完成的全集 LLM 看不到，错失"昨天搞定的 X 后续怎么样了 / 前几天那条 Y 看起来挺顺手"这类连贯关怀的抓手。

补一个 rolling 24h 窗口的"最近完成总览" hint，与 task_completion_hint 互补（"刚完成"瞬时 vs "过去 24h 总览"持久）。

## 改动

### `src-tauri/src/proactive.rs`

#### `CompletedTaskBrief` 加 derive

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedTaskBrief { ... }
```

让既有 struct 能在新测试 `assert_eq!` 用。`#[derive(Debug)]` 已 future-proof 其它 caller 也想 debug-print。

#### `compute_recent_completions(items, now)` 纯 helper

pure 函数：扫 `(title, description, updated_at)` 列表，筛 `classify_status == Done` + `updated_at` 落在 `[now - 24h, now]` 内的条目，按 updated_at 倒序输出 `CompletedTaskBrief` 列表。

```rust
pub fn compute_recent_completions(
    items: &[(String, String, String)],
    now: chrono::NaiveDateTime,
) -> Vec<CompletedTaskBrief> {
    let cutoff = now - chrono::Duration::hours(RECENT_COMPLETION_HINT_HOURS);
    let mut tuples: Vec<(chrono::NaiveDateTime, CompletedTaskBrief)> = Vec::new();
    for (title, desc, updated_at) in items {
        let (status, _) = classify_status(desc);
        if status != TaskStatus::Done { continue; }
        // 两种 timestamp 格式 fallback：with milliseconds + without
        let updated = parse_two_formats(updated_at);
        if updated < cutoff || updated > now { continue; }  // 防 corrupt 未来时间
        let result = parse_task_result(desc);
        tuples.push((updated, CompletedTaskBrief { title: title.clone(), result }));
    }
    tuples.sort_by(|a, b| b.0.cmp(&a.0));  // 倒序：最近完成在前
    tuples.into_iter().map(|(_, b)| b).collect()
}
```

#### `format_recent_completion_hint(items)` pure formatter

与既有 `format_task_completion_hint` 同模板，但 header 区分：

```rust
"[最近 24h 完成] 你和用户在过去一天里完成了下面这些 butler_task —— 可以用作连贯关怀的抓手（如「咱昨天搞定的 X 怎么样了？」/「前面那个 Y 看起来挺顺手」等），但别每条都点名："
```

cap 8 条 + "…还有 K 条" 溢出行。`result` 段截 80 字（与 task_completion_hint 共用 `TASK_COMPLETION_RESULT_CHARS` 常量）。

#### `build_recent_completion_hint(now)` IO 包装

```rust
pub fn build_recent_completion_hint(now: chrono::NaiveDateTime) -> String {
    let tuples = crate::db::butler_tasks_as_memory_items()
        .into_iter()
        .map(|i| (i.title, i.description, i.updated_at))
        .collect();
    let recent = compute_recent_completions(&tuples, now);
    format_recent_completion_hint(&recent)
}
```

#### run_proactive_turn 接入

```rust
let task_completion_hint = build_task_completion_hint();
let recent_completion_hint = build_recent_completion_hint(now_local.naive_local());
// ...
PromptInputs {
    ...,
    task_completion_hint: &task_completion_hint,
    recent_completion_hint: &recent_completion_hint,
    ...
}
```

### `src-tauri/src/proactive/prompt_assembler.rs`

`PromptInputs` 加新字段 `recent_completion_hint: &'a str`，注释说明与 `task_completion_hint` 互补关系。

`build_proactive_prompt` push 顺序：

```rust
push_if_nonempty(&mut s, inputs.task_completion_hint);
push_if_nonempty(&mut s, inputs.recent_completion_hint);  // 新
push_if_nonempty(&mut s, inputs.deadline_hint);
```

紧贴"刚完成"后 + "deadline"前，让 LLM 视野按"队列 → 心跳 → 刚完成 → 24h 全景 → 截止"递进。

测试 fixture `base_inputs()` 加 `recent_completion_hint: ""` 默认 empty。

### 新 8 个单测

`recent_completion_*` 测试族（与 `task_completion_format_*` 同 cluster）：

1. **empty** → 空串
2. **filters_non_done_status**：pending / error / cancelled 全跳过，仅 done 进
3. **24h_window_cutoff**：< 24h 入，> 24h 跳
4. **sorts_by_recency**：返回顺序按 updated_at 倒序
5. **skips_unparseable_timestamps**：corrupt timestamp 不让 hint 整段炸
6. **includes_result_marker**：`[result: ...]` 段被 parse 进 brief.result，format 含"产物：" 段
7. **caps_list_with_overflow_line**：超 cap 时"…还有 K 条"溢出行
8. **future_timestamp_is_skipped**：updated_at > now 视作 corrupt 跳过

## 关键设计

- **rolling 24h vs 单 tick 增量**：两 hint 互补共存。task_completion_hint 是"自上次 tick 转 done 的"，触发后即消失；本 hint 是"过去 24h 内完成的全部"，持续可见。重叠不冲突 —— header 区分（"刚完成" vs "最近 24h 完成"）让 LLM 知道这是两种语义。
- **`recent_completion_hint` 放在 `task_completion_hint` 之后**：视野递进序"刚 → 一天景观"。
- **筛选纯 done 状态**：cancelled / error 不算"完成"，不该让 LLM 误认为是 owner accomplishments。
- **future timestamp 防御**：updated_at > now 通常是数据 corrupt 或时钟 reset，跳过避免 LLM 看到荒诞值。
- **double timestamp format parse**：chrono Local 序列化输出 `"%Y-%m-%dT%H:%M:%S%.f"` 带 ms，但偶发 yaml 手写 / migration 形态丢 ms。两种 format 都接受。
- **`compute_recent_completions` pure + tests-injectable now**：参数化 wall clock 让单测不依赖系统时间。8 个 `ndt(y,m,d,h,m)` 测试 fixture 覆盖各边界。
- **cap=8 条 + 溢出 "…还有 K 条"**：与 task_completion_hint cap=5 不同 —— 24h 全景比单 tick 增量自然多一些。8 是经验值（一天 typical 完成 0-10 件）。format 函数内 cap，compute helper 不 cap（让 caller 拿原始全集，cap 是展示决策）。
- **复用 `TASK_COMPLETION_RESULT_CHARS` (80 chars)**：两 hint 都截 result 段同长度，视觉风格一致。

## 不做

- **不去重 `task_completion_hint` 与 `recent_completion_hint` 之间**：单 tick 内同一任务可能同时进两个 hint。LLM 提示是 robust 的 — 头部 header 区分就够，去重逻辑反增复杂度。
- **不导出 settings 控制 24h / cap**：当前常量足够。若用户反馈"想看 48h" / "想看 12h" 再加。
- **不接 cancelled / error 也展示**："最近完成" 是正向 accomplishments 信号；失败 / 取消是另一维度，已在 task_completion_hint 内（如果该 tick 有转 error / cancelled 也会被 LLM 看到）。
- **不动 task_completion_hint 行为**：完全独立的两个 hint。

## 验证

- `cargo test --lib` ✓ **1008 / 1008 通过**（+8 新 recent_completion_* 测试）
- `cargo test --lib recent_completion` ✓ 8 / 8 全过
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.18s
- 改动 ~280 行（compute_recent_completions + format_recent_completion_hint + build_recent_completion_hint 100 + PromptInputs / assembler push / test fixture 默认值 10 + 8 单测 170）；既有 task_completion_hint / butler_tasks_hint / prompt 其它分支 / 所有其它 hint 路径完全不动。

## TODO 状态

6 条 auto-proposed 已完成 5 条（其中 1 条 stale 移除：PanelSettings 🔌 测试 LLM 连通性 — 早已存在），余 1 条留池：
- PanelChat 顶部按 priority / updated_at / due 的 sort chip

## 后续

- prompt rules 检测"recent_completion_hint 非空 + idle 久"时加一条 "聊聊昨天那条做完的"建议，让 LLM 在 owner 沉默期主动用这维度。当前仅展示数据，rule 层不显式 boost。
- recent_completion_hint 与 daily_review 合流：每晚 R12 consolidate 时把 24h 内完成清单写进 ai_insights/daily_review_YYYY-MM-DD，让"今日总结"自然包含 accomplishments 段。
- chip 数据可在 Persona tab 显"过去 7 天完成趋势 sparkline" —— frontend 数据源同源，pet sense 自我画像更全。
