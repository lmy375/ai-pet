# PanelTasks 行加「💤 还 N 醒」hover countdown chip（iter #513）

## Background

既有 PanelTasks 行已有 always-visible 💤 chip（line ~10942）显
`💤 至 MM-DD HH:MM`（绝对时刻 + click 弹 preset popover 改 / 解除）。但
缺**相对倒计时视角** — owner 想「还多久醒，现在该等还是该 unsnooze」时
要心算 "现在 14:50 → 醒在 16:30 → 还 1h40m"。

本 iter 加 row hover countdown chip — 计算 `snoozed_until - now` →
显「💤 还 Nm」/「💤 还 Hh Mm」/「💤 还 Dd Hh」分级 label。click 复制
`「title」还 N 醒（YYYY-MM-DD HH:MM）` 一行到剪贴板（粘到 chat / 通报
同事场景）。

## Changes

### `src/components/panel/PanelTasks.tsx`

紧贴 `📋 ref` hover chip 之后插入：

```tsx
{taskPreviewHoverTitle === t.title &&
  !isFinished(t.status) &&
  t.snoozed_until &&
  (() => {
    const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(t.snoozed_until!);
    if (!m) return null;
    const target = new Date(+m[1], +m[2]-1, +m[3], +m[4], +m[5]);
    const deltaMs = target.getTime() - nowMs;
    if (deltaMs <= 0) return null; // 已过点不显
    const totalMin = Math.floor(deltaMs / 60_000);
    const days = Math.floor(totalMin / 1440);
    const hours = Math.floor((totalMin % 1440) / 60);
    const mins = totalMin % 60;
    let label: string;
    if (days > 0) label = `${days}d ${hours}h`;
    else if (hours > 0) label = `${hours}h ${mins}m`;
    else label = `${mins}m`;
    return (
      <button
        onClick={async (e) => {
          e.stopPropagation();
          const line = `「${t.title}」还 ${label} 醒（${t.snoozed_until!.replace("T", " ")}）`;
          await navigator.clipboard.writeText(line);
          setBulkResultMsg(`💤 已复制：${line}`);
        }}
        title={`还 ${label} 才醒（至 ${t.snoozed_until!.replace("T", " ")}）...`}
        style={{ ...purple-tint-fg dashed border... }}
      >
        💤 还 {label}
      </button>
    );
  })()}
```

### Gates

- **`taskPreviewHoverTitle === t.title`**：500ms hover state — 与 📂 /
  ↗ / 📊 / ↘ / ⏭ / 🔁 / 📅 / 📜 / 📋 ref / ⏰ reminderMin 同节奏
- **`!isFinished(t.status)`**：done / cancelled 不显 — 终态 snooze 无
  意义
- **`t.snoozed_until`**：仅 snoozed 任务显
- **`deltaMs > 0`**：已过点的 stale snooze（backend should clean）兜底
  不显，避免「还 -5m 醒」诡异

### 分级 label

- `< 1 小时`：`{mins}m` — 最常用「快醒了」场景
- `< 24 小时`：`{hours}h {mins}m` — 当日推后
- `≥ 24 小时`：`{days}d {hours}h` — 长程推后 / sleep

## Key design decisions

- **与既有 💤 chip 互补不替代**：既有 always-visible chip 显绝对时刻
  + click 改 preset；本 hover chip 显相对倒计时 + click 复制 line。两
  axis（when wakes vs how long left）分开
- **复用 `nowMs` 既有 1s tick**：与 due 倒计时 chip 同 nowMs（line 1530
  附近的 `useEffect` 每 1s 刷新）— 自然秒级同步、不引新 timer
- **purple-tint-fg + dashed border**：与既有 💤 chip purple 色族对齐 —
  视觉群一致；dashed 与其它 hover chip 同 dashed border 模板
- **click 复制 line 含绝对时间**：让 paste 到 chat 时同事看到 "还 30m
  醒（2026-05-19 09:00）" 双信息 — 相对方便扫读，绝对避免歧义
- **不复用 ⏰ due chip 算法**：那个 due 是 `t.due` 字段；snooze 是
  `t.snoozed_until`。底层 regex 同形（YYYY-MM-DDThh:mm）但语义不同 —
  保两 chip 独立 inline 避免抽象成本
- **已过点 silent skip**：极端情况 backend 还没清 stale snooze（race
  cond / clock drift）— 不渲染避免误导，等下次 build_task_view 清掉
- **不写 unit test**：纯 React conditional render + Date 算术；逻辑
  trivial（既有 due countdown chip 同 algorithm + nowMs 既有 1s tick
  production 验证）。GOAL.md "meaningful tests only" 规则下不引装饰
  性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.30s)
- 后端无改动 — 纯前端 hover chip
- 手测：
  - 用 `/snooze <title> 30m` 设个 30 分钟后 snooze
  - hover row 500ms → 看到 「💤 还 30m」紫色 chip
  - click → toast 「💤 已复制：「<title>」还 30m 醒（...）」
  - 等到 snooze 过点（或 unsnooze）→ chip 消失
  - 长程 snooze（如 /snooze <title> 2d）→ 「💤 还 2d 0h」

## Future iters (out of scope)

- 「💤 ⏰ 醒来时 due chip」— 若 task 同时有 due + snoozed_until，显两
  者交叉关系；当前两 chip 分开
- 「批量 snooze countdown view」— 顶部工具栏看所有 snoozed task 的
  countdown 列表；后续 propose
