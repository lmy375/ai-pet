# 任务行最近更新绿点 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务行最近更新指示：updated_at < 5 分钟时在标题旁加一个绿色 ● 表示"刚动过"，让用户一眼看到刚被宠物或自己改过的任务。

## 目标

「任务」面板长队列里看不出哪条刚被宠物或自己 retry / 改 priority / 加 detail
笔记。本轮在每行标题旁加一个微小绿点 ●，只在 `updated_at` 距今 < 5 分钟时
显示，hover tooltip 给"X 分钟前更新"。

## 非目标

- 不做"已读 / 未读"状态 —— 那需要 user-side 的 ack；本轮只是被动新近性指示。
- 不做颜色梯度（10 秒前 / 1 分钟前 / 4 分钟前 不同色）—— 阈值内一律绿点，
  统一即可。
- 不做整行背景色高亮 —— 与搜索黄背景 / 焦点蓝边冲突；点位足够小不抢主视觉。
- 不写 README —— 任务面板可见性微调。

## 设计

### 状态

`now: number` —— `Date.now()` 快照，30s 一次 setInterval 更新。
理由：`updated_at` 是 RFC3339 静态字符串，到期"过 5 分钟"得靠 client clock
推进。30s 是 task_overdue_count 已用的同周期，节奏一致。

### Pure helper

```ts
function isRecentlyUpdated(updatedAt: string, now: number): boolean {
  const ts = Date.parse(updatedAt);
  if (Number.isNaN(ts)) return false;
  const ageMs = now - ts;
  return ageMs >= 0 && ageMs < 5 * 60 * 1000;
}
```

边界：
- 解析失败 → 不显示（不是脏数据问题，不强行展示）
- 时钟漂移 → ageMs 负数 → 不显示（保守：未来的 updated_at 不算"刚动过"）
- 阈值 5 分钟与 README 早安简报 / 后端 stale_*_hours 数量级独立，无耦合

### 渲染

在 itemTitle 内 `{t.title}` 之后插入：

```tsx
{isRecentlyUpdated(t.updated_at, now) && (
  <span
    title={formatRecentlyTooltip(t.updated_at, now)}
    style={{ color: "#22c55e", fontSize: "8px", marginLeft: 6, ... }}
  >●</span>
)}
```

tooltip 文案：
- `< 60s` → "刚刚更新"
- 否则 → `${Math.floor(ageMs/60000)} 分钟前更新`

### 测试

逻辑全 pure helper 内，但项目无前端 vitest。helper 极小（5 行），靠 tsc +
手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | helpers + now 状态 + interval |
| **M2** | 行 JSX 加绿点 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 30s polling pattern（PanelApp `OVERDUE_POLL_MS` 同周期）
- `s.itemTitle` flex 容器

## 待用户裁定的开放问题

- 阈值 5 vs 10 分钟？本轮 5（"刚动过"实战感受偏短）。如反馈想长再调。

## 进度日志

- 2026-05-06 00:00 — 创建本文档；准备 M1。
- 2026-05-06 00:15 — 完成实现：
  - **M1**：`PanelTasks.tsx` 加 `nowMs: number` 状态 + 30s setInterval 刷新（与 PanelApp 任务过期徽章同周期）。新增 file-level helper `isRecentlyUpdated(updatedAt, now)` + `formatRecentlyUpdatedHint(updatedAt, now)`：Date.parse RFC3339；解析失败 / age 负（未来 ts）一律 false 防时钟漂移。
  - **M2**：行 itemTitle 内 `{t.title}` 后插入条件渲染的绿点 `<span color=#22c55e fontSize=8>●</span>` + hover tooltip（`< 60s` 显示"刚刚更新"，否则"X 分钟前更新"）。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 任务面板可见性微调。
  - **设计取舍**：5 分钟阈值（"刚动过"实战感受短）；时钟未来 ts 不显示绿点（防时钟漂移误报）；30s 刷新（与 PanelApp `OVERDUE_POLL_MS` 同周期，节奏一致）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；helpers 极小（5+3 行）逻辑由 tsc 保证。
