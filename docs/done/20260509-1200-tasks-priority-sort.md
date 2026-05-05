# PanelTasks "按 priority 降序" 排序模式（Iter R107）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks 加"按 priority 降序"排序模式：现 sortMode 只 "queue" / "due"；加 "priority" 选项让多选 priority 过滤 (R104) 之后能快速排出"高优在顶"，配合复盘视图。

## 目标

PanelTasks `sortMode` 现支持 `"queue"`（backend compare_for_queue 综合序）/
`"due"`（按 due 升序）两档。R104 加了 priority 多选过滤后，用户经常在
"P0 + P3"组合下想"按 priority 降序看"——但当前 queue / due 都不专门按
priority 排，得肉眼扫 priority badge。

加 `"priority"` 排序模式：unfinished 任务按 priority 降序（高优先级在顶）。
priority 相同时 JS stable sort 保留 queue 综合序作为 tie-break。finished
不动（按 R94 始终 updated_at 降序）。

## 后端 priority 方向核实

`src-tauri/src/task_queue.rs:594-596`：
```rust
if a.priority != b.priority {
    return b.priority.cmp(&a.priority);
}
```
backend 用 `b.priority.cmp(&a.priority)` —— 数值大 = 优先级高。所以 P9 =
最紧急，P0 = idea drawer（最低）。前端按相同方向 `b.priority - a.priority`。

## 非目标

- 不持久化 sortMode —— 与现有 queue / due 同语义（重启回 queue 默认）
- 不动 finished 段排序 —— R94 已固定 updated_at desc
- 不引入"priority asc"—— 用户日常诉求是"先看高优先级"，加 asc 增加 UI 选
  项但场景模糊（看 idea drawer 用 priorityFilter 选 P0 即可，比加 sort 模式
  通用）

## 设计

### type 扩展

```diff
-const [sortMode, setSortMode] = useState<"queue" | "due">("queue");
+const [sortMode, setSortMode] = useState<"queue" | "due" | "priority">("queue");
```

### sort 改造

```diff
 const sortedUnfinished = (() => {
   const unf = filteredTasks.filter((t) => !isFinished(t.status));
-  if (sortMode !== "due") return unf;
-  return unf.slice().sort((a, b) => { /* due asc */ });
+  if (sortMode === "due") {
+    return unf.slice().sort((a, b) => { /* due asc 不变 */ });
+  }
+  if (sortMode === "priority") {
+    // 数值大 = 优先级高（与 backend compare_for_queue 一致）。JS sort
+    // 是 stable，priority 相同的任务保持原 queue 综合序，让"P3 内部"仍
+    // 是 backend 推荐的处理顺序。
+    return unf.slice().sort((a, b) => b.priority - a.priority);
+  }
+  return unf;
 })();
```

### toggle 按钮

```diff
-<div style={{ display: "flex", gap: 4 }} title="切换队列默认排序 vs 按截止时间升序">
-  {(["queue", "due"] as const).map((mode) => {
+<div style={{ display: "flex", gap: 4 }} title="切换排序模式：默认综合 / 按截止 / 按优先级">
+  {(["queue", "due", "priority"] as const).map((mode) => {
       ...
-      {mode === "queue" ? "队列" : "due ↑"}
+      {mode === "queue" ? "队列" : mode === "due" ? "due ↑" : "P ↓"}
   })}
```

### 标题文案

```diff
-队列{sortMode === "queue" ? "（按宠物处理顺序）" : "（按 due 升序）"}
+队列
+{sortMode === "queue"
+  ? "（按宠物处理顺序）"
+  : sortMode === "due"
+    ? "（按 due 升序）"
+    : "（按优先级降序，高 → 低）"}
```

### 测试

无单测；手测：
- 默认 queue：与原行为一致
- 切到 priority：P9 → P8 → ... → P0；同 priority 内保留原综合序
- 切到 due：与原行为一致
- 切回 queue：恢复
- 多选 priorityFilter（P0 + P3）+ sort=priority：P3 在前，P0 在后
- 与 dueFilter / search / tag 多层 AND 叠加：sort 在 filter 之后跑

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | type 扩展 + sort 分支 |
| **M2** | 按钮 array + 标题文案 |
| **M3** | tsc + build |

## 复用清单

- 既有 sortMode toggle 按钮组
- 既有 sortedUnfinished IIFE
- 既有 priority u8 字段语义（与后端一致）

## 进度日志

- 2026-05-09 12:00 — 创建本文档；准备 M1。
- 2026-05-09 12:08 — M1 完成。先 grep 验后端 priority 方向（task_queue.rs:594 `b.priority.cmp(&a.priority)` 即数值大 = 优先级高）；sortMode type 加 "priority"；sortedUnfinished IIFE 内三分支：due → 按 due 升序；priority → `b.priority - a.priority` 降序（JS sort stable 保留 queue tie-break）；其它 → unf。
- 2026-05-09 12:11 — M2 完成。toggle 按钮 array 加 "priority"，label "P ↓"；标题文案三分支（队列 / due 升序 / 优先级降序，高 → 低）。
- 2026-05-09 12:14 — M3 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
