# mood sparkline 点格子查当日详情 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> mood_history sparkline 可点格子跳详情：sparkline 中某天柱状被点 → 弹出该日 mood_history 该日所有 motion entry 的展开列表，回看"今天为什么这么躁"。

## 目标

「人格」标签 sparkline 当前只展示按天聚合的 motion 频次柱。用户看到"周三全是
Flick3（焦虑）"想知道具体是哪几条，得去文件系统翻 mood_history.log。本轮让
sparkline 柱可点 → 在 sparkline 下方展开当天所有 mood_history entry 列表
（时间戳 + motion 颜色块 + 文本）。

## 非目标

- 不做编辑 / 删除单 entry —— 已有「管理」入口可清掉过去 N 天，单 entry 编辑
  价值低 + 复杂度高。
- 不显示当日完整 LLM reply 内容（mood_history 只存 motion 注释，不存说话）—
  那是 speech_history 范畴。
- 不写 README —— sparkline 内嵌交互补强。

## 设计

### 后端

`mood_history.rs` 加：

```rust
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MoodEntry {
    pub timestamp: String,  // 原始 RFC3339 ts
    pub motion: String,
    pub text: String,
}

/// pure: 从 mood_history 全文按 `target_date` (本地时区) 过滤出当天所有 entry。
/// malformed / ts 解析失败行 silent 跳过（与 summarize_motions_by_day 同语义）。
pub fn entries_for_date(content: &str, target_date: chrono::NaiveDate) -> Vec<MoodEntry>;

#[tauri::command]
pub async fn get_mood_entries_for_date(date: String) -> Vec<MoodEntry>;
```

注册到 lib.rs。

### 前端

`MoodSparkline` 加 `selectedDate: string | null` 状态 + `entries: MoodEntry[]`
缓存 + onClick 处理：
- 点 SparklineBar → setSelectedDate(d.date)，fetch 当日 entries
- 再点同一柱 / 点 close 按钮 → setSelectedDate(null)
- selectedDate 非 null 时 sparkline 下方渲染 entries 列表（每条：HH:MM 灰色 + motion 颜色块 + text）

`SparklineBar` 加 `onClick?: () => void` prop + 视觉态：selected 时柱外 1px outline。

### 测试

后端 `entries_for_date` 是 pure：单测覆盖正常 / 跨日边界 / malformed 跳过 /
空输入。

前端无 vitest，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `MoodEntry` + `entries_for_date` pure 函数 + 单测 |
| **M2** | `get_mood_entries_for_date` Tauri 命令 + 注册 |
| **M3** | MoodSparkline 选中状态 + entries fetch + 下方列表渲染 |
| **M4** | cargo test + tsc + build + cleanup |

## 复用清单

- 既有 `parse_motion_text` / `read_history_content`
- 既有 `MOTION_META` 配色
- 既有 `MoodSparkline` 布局

## 进度日志

- 2026-05-05 44:00 — 创建本文档；准备 M1。
- 2026-05-05 44:30 — M1 完成。`mood_history.rs` 加 `MoodEntry` struct + `entries_for_date` pure 函数；新增 5 个单测（target_day filter / malformed skip / no match / empty / preserve order）。`cargo test --lib mood_history` 28 通过。
- 2026-05-05 44:45 — M2 完成。`get_mood_entries_for_date` Tauri 命令注册到 `lib.rs`。
- 2026-05-05 45:00 — M3 完成。`MoodSparkline` 加 `selectedDate` / `dayEntries` state + useEffect fetch；`SparklineBar` 加 `selected` / `onClick` props 与 1px outline 高亮 + `cursor: pointer`。柱下方按时间倒序展开当日 entries（HH:MM + motion 颜色块 + 文本 + 关闭按钮）。
- 2026-05-05 45:10 — M4 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 954ms)。归档至 done。
