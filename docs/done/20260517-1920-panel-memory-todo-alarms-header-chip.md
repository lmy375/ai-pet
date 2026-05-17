# PanelMemory todo 段头「⏰ N pending」chip + 倒计时清单 popover（iter #374）

## Background

iter #372 加桌面 ⏰ alarm chip（在 PanelMemory item 行）创建 todo 段
reminder；iter #373 加 TG `/alarms` 手机端 audit 入口。但桌面 owner
回看 "我设了哪些 alarm 队列"目前要展开 todo 段逐条 grok description —
缺一个"一眼看到全部 pending"的汇总入口。

本 iter 在 todo 段头加 ⏰ N pending chip + click 弹倒计时清单
popover。完成 alarm 三 surface 闭环（item 创建 chip + 段头 audit
chip + TG audit 命令）。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. state（~line 958，紧贴 silentOnlyCats）

```ts
const [alarmsPopoverOpen, setAlarmsPopoverOpen] = useState(false);
```

outside-click + Esc 关闭 useEffect，与 reminderQuickPicker /
alarmPickerKey 同模板。状态不持久化（临时 audit 视图）。

#### 2. chip + popover（segment header，~line 3938，📊 字数 chip 之后）

- `catKey === "todo"` 段才渲
- 扫 `cat.items` description，regex 匹配 `[remind: ...]` 协议（两
  种格式：`[remind: HH:MM]` / `[remind: YYYY-MM-DD HH:MM]`）
- 0 条 → 不渲 chip
- chip 显 `⏰ N`，blue tint；open 态实底 + 白字
- popover：列表按 target ms 升序，每行 `MM-DD HH:MM (剩/已逾期 N
  分) topic`；overdue 条目红 tint bg 让 owner 一眼分辨

#### 3. 前端 regex 简化

不复用后端 `proactive::parse_reminder_prefix`（Rust 函数无法直接
在前端调）— 写前端简化版 regex 匹配。容忍 invalid time 边界场景
（前端多算几条但仅影响 count 精度，无副作用 — 后端 fire 时仍走严
格 parse_reminder_prefix gate）。

## Key design decisions

- **chip 位置紧贴 items 数 badge / 📊 字数 chip 之后**：与既有
  butler_tasks 段 silent/snooze chip 同设计原则 — 段头是"这段全局
  摘要" 信息层，类目特定 chip 紧凑排列。
- **0 条不渲 chip（vs disabled）**：与 butler_tasks silent chip 同
  策略。dead button 增 UI 噪音。
- **popover sort 按 target ms 升序**：最近 fire 在顶 — 与 TG /alarms
  同心智，owner 第一眼看到"下一个是什么"。
- **overdue 红 tint**：owner 一眼分辨"还会 fire" vs "已错过应该手
  动 ack"。Hover title 给 source item title 保溯源。
- **前端简化 regex 而非调后端**：完整一致性要在 chip 上"加 invoke
  `get_pending_reminders`" 轮询；但 chip 是常驻 UI，每次 panel 重渲
  调 IPC 开销大。tradeoff：前端宽松 parse → 多算 count 概率极低
  （格式严格的 regex 已过滤大多数 noise），换零 IPC 开销。
- **状态不持久化**：popover 是临时 audit 视图，跨 session 默认关
  更符合直觉。
- **popover 显简化时刻字符串而非 Date 对象**：JS Date 跨日 / 跨 DST
  边界格式化有奇怪 fallback；预格式化 `MM-DD HH:MM` 字符串保渲染
  稳定。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 仅复用 todo 类目数据 + 前端 regex
