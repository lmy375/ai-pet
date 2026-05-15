# 后端 `task_stats` 命令 + 桌面 /stats 切换调用

## 背景

上轮加了桌面 PanelChat `/stats`，前端遍历 `task_list` 在 TS 里计 5 个数（pending / overdue / done_today / error / cancelled_today）。同模式 TG bot 的 `/stats` 在 Rust 里也算了一遍。

两份独立实现：
1. "今日"的语义在 TS / Rust 各自定义 —— 容易 drift（已经在 TS 那边踩过 `toISOString()` 是 UTC 的坑）
2. 没单测覆盖：纯 TS 在前端走不了 cargo test；TG 那条有测但全局桌面这条没有
3. 后续如果想加 PanelDebug stats 卡片 / pet 窗"今日完成 N"绿 pill，又得抄第三遍

把"算 stats"下沉到后端单一函数，多 surface 共用同一份逻辑。

## 改动

### `src-tauri/src/db.rs`

新增：
```rust
#[derive(Debug, Clone, Serialize)]
pub struct TaskStats {
    pub pending: u32,
    pub overdue: u32,
    pub done_today: u32,
    pub error: u32,
    pub cancelled_today: u32,
}

/// 纯计算 + sql，testable with in-memory conn。
fn compute_task_stats(conn: &Connection, now: NaiveDateTime) -> Result<TaskStats, rusqlite::Error>;

#[tauri::command]
pub fn task_stats() -> Result<TaskStats, String>;
```

实现：
- `pending` / `error`：单 COUNT 查询（butler_tasks.status 已有 index）
- `done_today` / `cancelled_today`：COUNT + `updated_at LIKE 'YYYY-MM-DD%'`（用本地时间格式，与写盘端一致）
- `overdue`：SELECT description FROM pending → `parse_task_header` 取 due → < now 计数（butler_tasks 没有独立 `due` 列；走 description marker 解析，与 PanelTasks 既有行为一致）

Tauri command 是薄包装：`now = chrono::Local::now().naive_local()` + `with_db(|c| compute_task_stats(c, now))`。

注册到 `lib.rs::invoke_handler`。

### `src-tauri/src/db.rs` 测试

3 个新单测（in-memory SQLite + insert 几条 fixture row）：
- `task_stats_counts_each_status` —— 一条 pending（未逾期）、一条 done（今天）、一条 done（昨天）、一条 error、一条 cancelled（今天） → 验证 5 个字段
- `task_stats_overdue_picks_pending_with_past_due` —— 一条 `[task pri=3 due=2020-01-01T10:00] foo` pending → overdue == 1
- `task_stats_all_zero_on_empty_table` —— 空表 → 全 0

### `src/components/panel/PanelChat.tsx`

`case "stats"` 重写：

```ts
try {
  const s = await invoke<{
    pending: number; overdue: number; done_today: number;
    error: number; cancelled_today: number;
  }>("task_stats");
  const allZero = !s.pending && !s.overdue && !s.done_today && !s.error && !s.cancelled_today;
  pushLocalAssistantNote([
    allZero ? "📊 任务状态（今日很安静 ✨）" : "📊 任务状态",
    `○ 待办：${s.pending}`,
    `🔴 逾期：${s.overdue}`,
    `✓ 今日完成：${s.done_today}`,
    `⚠️ 出错：${s.error}`,
    `🗑 今日取消：${s.cancelled_today}`,
  ].join("\n"));
} catch (e) { ... }
```

删掉本地遍历 / `toLocaleDateString("sv-SE")` / Date.parse(due) 这堆细节 —— 都进后端了。

### TG bot 不动

`format_stats_reply` 保持原状：TG 那条是**单 chat** 视图（origin 过滤），与桌面"全集"语义不同，强行共用就要往 `compute_task_stats` 加 origin filter 参数，反而把通用 API 污染。先各自存活。

## 不做

- 不给 butler_tasks 加 `due` 列：影响 v5 migration + 双写 path + backfill，盘子大；现在 description marker 解析每次都跑一遍 ≈ 50µs / 条，pending 通常 < 20 条，可忽略
- 不让 task_stats 接 origin 参数：当下唯二 caller 用法不同；YAGNI
- 不删 TG /stats 的 `format_stats_reply`：那边自带 view filter + tests

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅（含 3 新测试）
- `npx tsc --noEmit` ✅
- 桌面聊天 `/stats` 输出仍是 6 行，数值与上轮一致；TG `/stats` 不变
- 检查 PanelChat `/stats` case 行数缩到 ~15 行（原 ~50）

## 完成

- [x] db.rs: TaskStats 结构 + compute_task_stats + task_stats command
- [x] lib.rs: 注册
- [x] db.rs 单测：3 个新 case
- [x] PanelChat.tsx: /stats 改 invoke 后端（~50 行 → ~22 行）
- [x] `cargo build --release` 通过
- [x] `cargo test --lib` 通过（898 passed，+3 新）
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
