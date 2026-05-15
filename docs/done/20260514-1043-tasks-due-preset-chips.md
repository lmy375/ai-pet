# PanelTasks 新建表单加 due 快捷预设 chips

## 背景

TODO 第一项：

> PanelTasks 新建表单加 due 快捷预设 chips（今晚 18:00 / 明天 9:00 / 周一 9:00 / 一周后），手敲 datetime-local 在高频场景太繁琐。

`datetime-local` input 体感差：要点开日历挑日期、再敲 4 位小时分钟，少打一个 0 就报无效。日常派给宠物的任务 due 95% 落在「今晚 / 明天上午 / 周一开工 / 一周内」几个 bucket，一键填能省 5-10 秒 × 高频。

## 改动

### `src/components/panel/PanelTasks.tsx`

**新增纯函数 helpers**（放在 `formatDue` 旁边，与 due 相关计算同段）：

- `formatDueInput(d: Date): string` — 把 Date 渲染成 datetime-local 的 `YYYY-MM-DDThh:mm`（无时区，本地组件取值；不走 `toISOString` 防 UTC 偏移）。
- `dueTonight(now: Date)` — 今晚 18:00；若 now 已过 18:00 跳明晚同点（避免点了反而退回过去 due 的 footgun）。
- `dueTomorrow(now: Date, hour=9, minute=0)` — 明早 09:00。
- `dueNextMonday(now: Date)` — 下个周一 09:00（周日 +1，其它工作日按 `7 - getDay() + 1`；今天周一 09:00 之前点也跳下周一，让"周一"语义稳定 = 下周第一天）。
- `dueOneWeek(now: Date)` — +7 天，保留 now 的小时分（不强制 09:00，"一周后"语义里"现在加一周"更直觉）。

所有 helper 都 `export` —— 未来 PanelTasks 拆分或别处复用都拿得到。

**UI**：在原 `<input type="datetime-local">` 下方加一行 chip 群（圆角 999、accent 6% 底、border + 3x10 padding、11px 字号）。

- 4 个预设："今晚 / 明天 / 周一 / 一周后"，每个 chip 上有 `title` tooltip 解释具体落点。
- `due` 非空时额外渲染右对齐"清除"chip（红 tint 边色 + red-fg 文字），与赋值 chip 视觉分离。
- chips 是 `<button type="button">`，不会被表单 ⌘Enter 误触发提交。
- 每次点击都 `new Date()` 重新求值，避免 cache 旧 "now"。

## 不做

- **没改 edit / bulk due 路径**（line 3906 / 6549 也有 datetime-local）。本次只覆盖"新建任务"的高频入口；edit 是 1:1 替换语义，"今晚"语义不再合适（已有 due 的任务再点"今晚"可能不是用户想要的语义）。如果 demand 起来再另开一波。
- **没引入测试框架**。前端 repo 当前没 vitest / jest 配置；为 4 个 5-行 helper 加测试框架成本不匹配收益。helpers 设计成 pure + Date 注入便于将来 vitest 落地直接 unit test。
- **未做"自定义预设"**。如果常用 due 落点偏离这 4 个，再加 customizable preset；当下默认覆盖 90%+ 场景。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.17s（与改动前体量一致）
- 改动是纯叠加 — 既有 due 行为不变（input 仍 controlled，chips 仅赋值不拦截）

## 后续

- 同一 chip 群可叠加到 edit-task / bulk-due UI 上（语义对照 demand）。
- 预设语义"今晚 / 明天"等如果接入用户区域化偏好可由 settings 控制；当前固定 ZH 文案。
