# Pinned 任务在 proactive prompt 里给 LLM 优先级 boost

## 背景

TODO 上 auto-proposed 一条："pinned 任务在 proactive prompt 里给 LLM 优先级 boost：让宠物自动倾向先取 pinned 单做，与 owner 标注意图同步。"

前两轮分别落地了 `[pinned]` marker + 桌面 chip 过滤 / 右键 toggle，以及双端 `/pin` `/unpin` slash 命令。但 owner 钉住任务后，LLM 在 proactive 选单时**完全看不到** pinned 信号 —— `format_butler_tasks_block` 仍按 due / updated_at 排，pinned 任务可能沉到第 5、6 条被截断。owner 标注的"关键任务"对 LLM 不可见就成了仅 UI 的视觉装饰。

本轮把 pinned 信号注入到 proactive prompt 的 butler_tasks 块：让钉住任务上浮到列表顶 + 加 「📌 钉住」 marker + header 透明计数 + 教学文案告诉 LLM"钉住的事请这轮就做"。

## 改动

### `src-tauri/src/proactive/butler_schedule.rs`

#### sort key

原 sort 是 `(due, updated_at asc)`，扩展为 `(pinned, due, updated_at asc)` —— pinned 排最前。

```rust
annotated.sort_by(|(a, a_pin, a_due, _), (b, b_pin, b_due, _)| match (a_pin, b_pin) {
    (true, false) => Less,
    (false, true) => Greater,
    _ => match (a_due, b_due) {
        (true, false) => Less,
        (false, true) => Greater,
        _ => a.2.cmp(&b.2),
    },
});
```

#### header 拼接

原 header 是 4 分支笛卡尔积（`(due_count, err_count)` 的 4 种组合）。加 pinned 后变 8 种，太冗。改成「片段拼接」：每个非零信号各占一段，按 owner 钉住 → 到期 → 失败 顺序追加。

```rust
let mut parts: Vec<String> = Vec::with_capacity(3);
if pin_count > 0 { parts.push(format!("{} 条由 owner 钉住（优先做）", pin_count)); }
if due_count > 0 { parts.push(format!("{} 条到期", due_count)); }
if err_count > 0 { parts.push(format!("{} 条上次执行失败需要复查", err_count)); }
let header = if parts.is_empty() {
    format!("用户委托给你的管家任务（共 {} 条，按最早委托排在前）：", n)
} else {
    format!(
        "用户委托给你的管家任务（共 {} 条，其中 {}，按 钉住 → 到期 → 最早委托 排在前）：",
        n, parts.join("、")
    )
};
```

#### per-task line marker

`📌 钉住 · ` 排在 `❌ 错误` / `⏰ 到期` 前面 —— owner 标注先于系统信号。

```rust
if *pinned { marker.push_str("📌 钉住 · "); }
if *errored { marker.push_str("❌ 错误 · "); }
if *due { marker.push_str("⏰ 到期 · "); }
```

#### footer 教学

在既有「看到「⏰ 到期」就该这一轮优先处理它」之后追加：

> 看到「📌 钉住」是 owner 显式标的「关键任务」 —— 优先级在到期之上，请这一轮就开始推进（哪怕做一小步也好），不要冷落 owner 反复钉的事。

让 LLM 明确：钉住 ≠ 装饰，是 directive cue。"哪怕做一小步"是为避免"任务大→拖延一整轮"的 LLM 倾向。

#### 新 3 个单测

- `format_butler_tasks_block_pinned_task_bubbles_to_top_with_marker` —— pinned 在前 + marker + header 计数。
- `format_butler_tasks_block_pinned_dominates_due_in_ordering` —— pinned + due 都触发时 pinned 优先；双 marker / 双 count 都正确显示。
- `format_butler_tasks_block_no_pinned_means_no_pin_phrase_in_header` —— 无 pinned 时 header 不出现"钉住"字样（只 footer 教学文案带，是常驻 instruction）。

## 关键设计

- **pinned 优先级 > due**：owner 的显式标注覆盖系统的时间窗信号。若 LLM 同时看到 due + pinned 的两个任务，先做 pinned —— 那是 owner 反复盯的，做了它对 owner 价值最大。文档化在 footer + 测试。
- **per-task marker + header count 双层透明**：列内 prefix 「📌 钉住」 让 LLM 一眼判定每条任务的特殊性；header 「N 条由 owner 钉住」让总览即时可见。两个信号叠加，免单一通道 ablation。
- **footer 用「哪怕做一小步」**：防 LLM 看到 pinned 是大任务（"重构整个 panel"）就因"做不完"而干脆不动 —— 显式鼓励"推进"而非"完成"。任务的最小可见进展才能让 owner 感到宠物在用心。
- **片段拼接 header**：避免 due/err/pinned 三维笛卡尔积爆炸（2³=8 个 match arm），用 `parts: Vec<String>` 累积非零信号 + 末尾 `parts.join("、")`，新增第 4 维 signal 时只需 `if … push`。
- **sort 不破"最老任务不沉底"原则**：tertiary key 仍是 `updated_at asc`。pinned 任务集合内部 / 非 pinned 集合内部 都按最老在前 —— 与原 "don't let tasks rot" 不变量一致。
- **errored 仍不参与 primary 排序**：保持原决策 —— 失败常伴随 due 已浮顶；单独把 errored 排顶反而让 stale error 抢占。

## 不做

- **不做"pinned 数量上限"防 LLM 被 owner 滥用**：相信 owner —— 钉太多 → owner 自己看 chip 数量也会自我调节。LLM 端的 8 条 max_items cap 已自然约束注入量。
- **不让 LLM 主动 unpin**：pin/unpin 是 owner 的标注权。LLM 完成 pinned 任务后应 mark done（既有路径），但不该 strip owner 的 `[pinned]` —— 那是判断"重要性"，不是宠物的工作。
- **不动 PanelTasks 排序**：UI 列表 sort 仍由 owner 控制；prompt block sort 是给 LLM 看的"建议序"，两个独立。

## 验证

- `cargo test --lib` ✓ **997 / 997 通过**（+3 新 pinned sort / marker / header 测试）
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.21s
- 改动 ~120 行（butler_schedule.rs 50 + tests 70）；既有 16 个 butler_tasks_block 测试 + due/error/blocked/snooze 路径全部不动。

## TODO 状态

5 条候选 auto-proposed 已完成 2 条，余 3 条留池：
- 任务批量勾选 → 批量 pin / unpin
- detail.md 编辑器 `⌘S` 保存键
- detail.md 工具栏「📅 当前时间」按钮
- 任务行 chip 区显 created_at 相对时间

（pin 系列的"自我进化"维度已闭环：owner 钉 → LLM 优先做 → 完成回流 → owner 评估。）

## 后续

- pinned 任务 stale 提醒：连续 N 天 pinned 但 updated_at 没动时让 proactive 一句"这条钉住 N 天了，要不要拆 / 取消？"
- TG `/tasks` 列表在 pinned 任务前显 📌 figure，让 owner 在手机端也能一眼看到"哪些钉了"（之前 `/pin /unpin` ship 时已列后续，此处再确认）。
- pinned 任务"完成时给 owner 一个特别确认"：done 回流 TG / mini chat 时附 "这是你钉住的 ✨" 区分普通任务，强化"宠物记得 owner 在意什么"的体感。
