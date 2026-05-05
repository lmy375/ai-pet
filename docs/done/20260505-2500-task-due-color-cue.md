# 任务行 due 颜色紧迫度提示 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务行 due 颜色提示：due 在 24 小时内显示 due 文字为橙色；已过期红色；正常灰色。让队列扫读时一眼分辨紧迫程度。

## 目标

任务行 itemMeta 段当前 `截止 2026-05-06 18:00` 与`创建于 ...`同色（默认灰）—
扫长队列要逐个解析时间字符串才能分辨"今晚要做"vs"下周才到期"。本轮根据当
前时刻自动着色：
- 已过期 → 红 `#dc2626`
- ≤ 24h 内 → 橙 `#ea580c`
- > 24h → 默认灰（不动）
- 无 due → 不渲染（既有行为）

## 非目标

- 不做 7 天 / 30 天等更细粒度梯度 —— 三档已覆盖"现在就做 / 抓紧 / 还早"的扫
  读语义；多档反倒让色彩成为噪音。
- 不做"过期 N 天"的额外文案（"已过期 2 天"）—— 红色 + 原 due 时间字符串足够，
  用户能从字符串本身推算。
- 不为终态任务（done / cancelled）改色 —— 那些 due 已无意义，红橙反倒让人误
  以为还需处理。
- 不写 README —— 任务面板视觉微调。

## 设计

### Pure helper

```ts
type DueUrgency = "overdue" | "soon" | "normal";
const SOON_THRESHOLD_MS = 24 * 60 * 60 * 1000;
function dueUrgency(due: string, now: number, status: TaskStatus): DueUrgency {
  // 终态：保持 normal（无紧迫感）
  if (status === "done" || status === "cancelled") return "normal";
  // 后端 due 是 `YYYY-MM-DDThh:mm` 无时区本地协议；前端按 datetime-local
  // 直接拼上 ":00" 当本地时间 parse。
  const ts = Date.parse(`${due}:00`);
  if (Number.isNaN(ts)) return "normal";
  const delta = ts - now;
  if (delta <= 0) return "overdue";
  if (delta <= SOON_THRESHOLD_MS) return "soon";
  return "normal";
}

function dueColor(urgency: DueUrgency): string | undefined {
  switch (urgency) {
    case "overdue":
      return "#dc2626";
    case "soon":
      return "#ea580c";
    case "normal":
      return undefined; // 走父级 itemMeta 默认色
  }
}
```

`status` 入参让终态任务走 normal —— 与其它视觉提示（如绿点 / 焦点蓝边）的
"终态保持中性"原则一致。

### 应用

`{t.due && <span ...>截止 {formatDue(t.due)}</span>}` 改为带条件 style：

```tsx
{t.due && (
  <span style={{ color: dueColor(dueUrgency(t.due, nowMs, t.status)) }}>
    截止 {formatDue(t.due)}
  </span>
)}
```

`nowMs` 已在 30s setInterval 刷新（与"刚动过"绿点共享时钟），无需新增轮询。

### 测试

helpers 全 pure；项目无 vitest，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `dueUrgency` + `dueColor` pure helpers |
| **M2** | itemMeta JSX 接入 |
| **M3** | tsc + build + cleanup |

## 复用清单

- `nowMs` 状态（已存在，30s setInterval 刷新）
- `formatDue` 字符串渲染（不动）
- 任务行 status / due 字段（既有）

## 待用户裁定的开放问题

- "soon" 阈值 24h vs 12h vs 6h？本轮 24h（与"明天就到期"对应，最直觉）。如反
  馈想更紧再调。
- 终态任务保持 normal（即"灰"）vs 完全不渲染颜色提示？保持 normal 一致 ——
  与"完成"绿徽章 + "取消"灰徽章相邻，正色不冲突。

## 进度日志

- 2026-05-05 25:00 — 创建本文档；准备 M1。
- 2026-05-05 25:10 — 完成实现：
  - **M1**：`PanelTasks.tsx` 加 `dueUrgency(due, now, status)` + `dueColor(urgency)` pure helpers。终态任务（done / cancelled）走 normal；overdue（delta ≤ 0）→ 红 `#dc2626`；soon（≤ 24h）→ 橙 `#ea580c`；normal → undefined（走父级默认色）。`Date.parse(\`${due}:00\`)` 解析失败一律 normal 防御。
  - **M2**：itemMeta JSX 把 `<span>截止 ...</span>` 包成 IIFE 注入 dueColor + 加重 fontWeight（非 normal 时） + hover tooltip（overdue / soon 解释）。复用既有 `nowMs` 30s setInterval（与"刚动过"绿点共享时钟）。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 任务面板视觉微调。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；helpers 全 pure，由 tsc + 既有 nowMs 复用模式保证。
