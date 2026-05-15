# 桌面任务 pill 加 💤 暂停段

## 背景

TODO（本轮 auto-proposed）：

> 桌面任务 pill 加 💤 暂停段：TaskStats 增 snoozed 计数；pill 文本动态拼"🔴 N · ✓ M · 💤 K"让用户一眼看到队列三种状态。

任务 pill 现仅显逾期 + 今日完成两段。snooze marker 上线（20260514-1228）+ snooze 菜单（20260514-1318）+ TG /snooze（20260514-1337）后，"暂停中的任务"成了独立维度——但 pill 看不到它们。用户暂停了 5 条后看 pill 仍是空（这 5 条不在 overdue / done / pending-active 任一段），容易疑惑"我的任务都去哪了"。把 snoozed 也加进 pill 完整反映队列状态。

## 改动

### Backend（Rust）

#### `src-tauri/src/db.rs`

**1. `TaskStats` 增字段 `snoozed: u32`**

```rust
pub struct TaskStats {
    pub pending: u32,
    pub overdue: u32,
    pub done_today: u32,
    pub error: u32,
    pub cancelled_today: u32,
    pub snoozed: u32,  // pending 子集中 [snooze: 未来] 的条数
}
```

**2. `compute_task_stats` 合并扫描循环**

既有循环已遍历 pending 行的 description（算 overdue）；在同一循环里加 `parse_snooze(&d)` 调用 + 比较 now，避免两次 prepare/query_map。snooze 只在 pending 状态有意义（done/cancelled 是终态，marker 自动失效）—— 与 SQL 过滤 `WHERE status = 'pending'` 一致。

```rust
let mut snoozed: u32 = 0;
for desc in rows {
    let d = desc?;
    // ...既有 overdue 计算
    if let Some(snooze_until) = crate::task_queue::parse_snooze(&d) {
        if snooze_until > now {
            snoozed += 1;
        }
    }
}
```

**3. 测试**

- `task_stats_all_zero_on_empty_table` 加 `assert_eq!(s.snoozed, 0)` 行
- 新 `task_stats_snoozed_counts_pending_with_future_snooze`：覆盖 future snooze / past snooze / no marker / done-state with future snooze 四种 fixture

### Frontend（TypeScript）

#### `src/App.tsx`

**1. usePollingState fetch 扩字段**

```ts
const { data: taskStats } = usePollingState(
  () => invoke<{ overdue: number; done_today: number; snoozed: number }>("task_stats"),
  60_000,
  { overdue: 0, done_today: 0, snoozed: 0 },
);
```

**2. pill 渲染逻辑：3 段动态拼接 + tint 三档**

```ts
const segments: string[] = [];
if (hasOverdue) segments.push(`🔴 ${overdueLabel}`);
if (hasDone) segments.push(`✓ ${doneLabel}`);
if (hasSnoozed) segments.push(`💤 ${snoozedLabel}`);
let text = segments.join(" · ");
if (segments.length === 1) {
  text = hasOverdue ? `🔴 N 逾期`
       : hasDone ? `✓ M 今日完成`
       : `💤 K 暂停`;
}
const tint = hasOverdue ? "red" : hasDone ? "green" : "blue";
```

**tint 优先级**：紧迫度 > 庆祝度 > 提醒度。snoozed-only 用 blue tint（与 PanelTasks 💤 chip 同色族 purple/blue 区分但接近，让两处视觉关联）。

**1-段附后缀**：单段时附中文后缀（"逾期 / 今日完成 / 暂停"）让初见者直接读懂；多段时 `🔴 N · ✓ M · 💤 K` 风简洁。

**3. tooltip 三段拼接**

```ts
const tipParts: string[] = [];
if (hasOverdue) tipParts.push(`${overdue} 条任务已过期`);
if (hasDone) tipParts.push(`今日完成 ${done} 条`);
if (hasSnoozed) tipParts.push(`${snoozed} 条暂停中`);
const tooltip = `${tipParts.join(" · ")} · 点开「任务」tab`;
```

## 不做

- **不为 snooze-only 加专属 deeplink filter**。当前点 pill 走"逾期 → overdue / 否则 → all"，snooze 任务在 all 视图里能看到 💤 chip，足够定位。新增 `dueFilter: "snoozed"` 是独立 PanelTasks 改造，与本 pill 视觉补段独立。
- **不动 deeplink 行为**：仅扩 pill 显示，不改 click 路径。
- **不让 pill 加进 sparkle 触发条件**：sparkle 是任务"完成 +1"的庆祝信号，snooze 不该 trigger sparkle（暂停不是成就）。
- **不动 PanelDebug task stats strip**：那是 chip-row 显示器，与 pill 是不同 surface；如要补 snoozed 段是独立改动。

## 验证

- `cargo test --lib db::tests` ✓ 17 / 17 通过（含新 task_stats_snoozed 测试）
- `cargo test --lib` ✓ **988 / 988 通过**（987 → 988）
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.14s
- 改动 ~70 行（backend struct + helper + 2 tests + frontend pill render 重构）；既有 deeplink click / sparkle 触发 / 60s 轮询路径全部不动。

## 后续

- PanelDebug task stats strip 也补 💤 段。
- pill 显「💤 5 暂停中」单段时 click 跳一个新的 `dueFilter: "snoozed"` —— 但要先在 PanelTasks 加 snooze 过滤 chip。
- 测试 task_stats_snoozed 多增"snooze 在 error 行" / "snooze 与 done 共存" 等边界（当下覆盖 4 种 fixture 已是主路径）。

## TODO 状态

- 本轮提了 5 条新 TODO（pill snoozed / 标题 LLM 重写 / Memory inline edit description / preview 引用块视觉 / pet 顶栏陪伴 chip），实现 1 条。
- TODO 剩 4 条。
