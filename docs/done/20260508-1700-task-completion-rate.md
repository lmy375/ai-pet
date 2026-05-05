# 任务面板完成率指示（Iter R89）

> 对应需求（来自 docs/TODO.md）：
> 任务面板加完成率指示：列表头加小字"今日完成 X / 本周完成 Y"（unfinished 排序时仍显），让用户一眼看到产出节奏，配合 created_at 相对值形成"流量计"。

## 目标

PanelTasks 现在能看到积压（pending / error）和已结束（done / cancelled）
两栈，但产出节奏没数字化呈现。配合 R87 的 created_at 相对值，加一行
"今日完成 X · 近 7 天 Y" 让用户快速感知"我最近做完多少 / 是否在堆积"。

## 非目标

- 不做"完成率百分比"（done/(done+pending)）—— 容易误导：长期积压老欠债
  压低分母，看着像"完成率高"实则没干新活
- 不做日历视图 / 趋势图 —— 单行小字足够"流量计"用途，趋势图属于另一类
  feature
- 不区分"自己关掉"vs"宠物完成"—— TaskView 没分这个维度，靠 history 行
  挖掘成本太高且对用户感知度无帮助

## 设计

### 计数口径

- **今日完成**：`status === "done"` 且 `updated_at` 落在本地今日（00:00 起）。
  `done` 的 updated_at 即标记"完成时刻"（任务进入终态后不再被宠物更新）
- **近 7 天完成**：滚动 7 × 24h 窗口（now - 7d 起），与"今日"两个维度互补。
  不用日历周（"周一开始"在 CN 不太一致；rolling 更直觉）
- **cancelled** 不计入完成 —— 那是"用户主动放弃"，不算产出
- 实现：useMemo 单次扫 `tasks` 全集（含 finished，因为 showFinished=false 时
  显示列表过滤但 tasks 状态本身仍含全部）

```ts
const completionStats = useMemo(() => {
  const now = Date.now();
  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const todayMs = todayStart.getTime();
  const weekAgoMs = now - 7 * 86_400_000;
  let today = 0;
  let week = 0;
  for (const t of tasks) {
    if (t.status !== "done") continue;
    const ts = Date.parse(t.updated_at);
    if (Number.isNaN(ts)) continue;
    if (ts >= todayMs) today += 1;
    if (ts >= weekAgoMs) week += 1;
  }
  return { today, week };
}, [tasks]);
```

### 渲染

把 "队列" 标题段从单行改成 stacked 双行（保持右侧 sort 按钮 flex-end 不动）：

```tsx
<div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 8 }}>
  <div>
    <div style={{ ...s.sectionTitle, marginBottom: 0 }}>
      队列{sortMode === "queue" ? "（按宠物处理顺序）" : "（按 due 升序）"}
    </div>
    <div
      style={{ fontSize: 11, color: "var(--pet-color-muted)", fontWeight: 400, marginTop: 2 }}
      title="status=done 且 updated_at 在窗口内的任务数（cancelled 不计；近 7 天为 rolling 窗口）"
    >
      今日完成 {completionStats.today} · 近 7 天 {completionStats.week}
    </div>
  </div>
  <div style={{ display: "flex", gap: 4 }}>
    {/* 现有 sort 按钮 */}
  </div>
</div>
```

### 测试

无单测；手测：
- 完成 1 个任务 → 今日 +1 / 近 7 天 +1
- 取消 1 个任务 → 不计入
- 4 天前的 done 任务 → 今日 0 / 近 7 天 +1
- 8 天前的 done 任务 → 都 0
- 完成日跨午夜后下一个日历日：今日归 0（依靠 setHours(0,0,0,0) 本地午夜）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | useMemo + 标题段双行渲染 |
| **M2** | tsc + build |

## 复用清单

- 既有 `tasks` state（已含 finished 全集）
- 既有 `s.sectionTitle` 字号 / 字色

## 进度日志

- 2026-05-08 17:00 — 创建本文档；准备 M1。
- 2026-05-08 17:08 — M1 完成。`completionStats` useMemo 在 p0Count 之后加：单次扫 tasks 全集、status==="done" 才计数、依赖 [tasks, nowMs] 让 30s tick 时今日窗口跨午夜自动滚动。"队列" 标题段从单 div 改成 stacked 双行（左侧 div 包标题 + 完成率小字），右侧 sort 按钮 group 不动。
- 2026-05-08 17:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 同 R88 build 通过 (499 modules, 961ms)。归档至 done。
