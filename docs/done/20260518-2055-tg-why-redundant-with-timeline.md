# TG bot `/why <title>` — redundant with /timeline，pivot drop（iter #499）

## Discovery

提 TODO 时假设 `decision_log` 是「per-task 决策历史」（touch / snooze /
promote 等 owner action），实际它是：

- `src-tauri/src/decision_log.rs:33` — `DecisionLog` 是 in-memory ring
  buffer，cap = 16
- 存 proactive engine 全局决策：每 tick 的 `LoopAction`（`Silent` /
  `Skip` / `Run`）+ reason（如 quiet_hours / disabled / 各种 gate）
- 不按 task 索引；与 task title 无关
- 仅给 PanelDebug 「最近为啥没说话」audit 用

**per-task** 历史实际存在 `butler_history.log`（每条 task 的 create /
update / delete 事件 + description snapshot），已由 **/timeline** 完整
暴露：

- `src-tauri/src/telegram/commands.rs:5718` `format_timeline_reply` —
  emoji + ts + markers per event
- `compute_timeline_entries` 已去重连续无 marker 变化的 update 噪声
- cap 30 entries（远大于 /why 设想的 last N）

`/timeline` 已识别 markers：done / error / snooze / result / cancelled /
pinned / silent / blockedBy / archived — 这正是 owner action（pinned /
silent / snooze）+ LLM action（result）+ 状态机变化（done / cancelled）
**全套**。`/why` 想做的「last N action history」与 `/timeline` 取首 N
条等价。

## Decision

不实现 `/why` — `/timeline` 完全覆盖其语义且 already shipped。强行加
`/why` = 多 6+ surface 同步 + 测试维护 + autocomplete 噪音，无独立
信号增益。

procedure 改进（与 iter #498 find-replace pivot 同教训）：未来 propose
新需求前先 grep / fmt 确认是否已实现。

## Cross-reference

如果 owner 想要 "/timeline 的 TL;DR" 视图（last 5 而非 last 30），现
有路径：

- `/timeline <title>` 输出按时间序，**前 5 行即 last 5 events**（因为
  `compute_timeline_entries` 按 chronological 排，最新在底部，但
  `entries_newest_first` 反转过 — 实际是 oldest first）

实测一下 /timeline 排序：从 `compute_timeline_entries` 注释看 "input
newest-first → output chronological（旧→新）"。意味着 /timeline 显
的是从旧到新，最新在最底部。

如果未来 owner 真想要「最近 5 条 action one-line」入口（与 /timeline
长视图互补），可以加 `/recent_events <title> [N]` 而非 `/why` —
naming clearer，且 N 参数让 owner 控密度。但当前需求池无该痛点反馈，
暂不主动加。

## Verification

无代码改动。TODO 项删除，本 doc 作记录。
