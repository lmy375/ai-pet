# PanelTasks 行加「📅 due 倒计时」hover chip（iter #485）

## Background

PanelTasks 既有 dueUrgency 三档色 chip（normal / soon / overdue）显
紧急程度 + meta line 显 ISO 形 due 字符串。但 owner 想精确知「还差
多少」/ 「已过多久」时只能心算从 ISO 到 now — friction。

本 iter 加 hover chip「📅 N 天 X 小时后」/「📅 N 分前已过」 — 给精
确倒计时数字让 owner 决策（"还有 2 小时还是 5 分钟？"）。

## Changes

### `src/components/panel/PanelTasks.tsx`

紧贴 🔁 复制 schedule chip 之后插：

```tsx
{taskPreviewHoverTitle === t.title && !isFinished(t.status) && t.due && (() => {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(t.due);
  if (!m) return null;
  const target = new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5]);
  const diffMs = target.getTime() - Date.now();
  const absMin = Math.abs(diffMs) / 60_000;
  const days = Math.floor(absMin / (60 * 24));
  const hours = Math.floor((absMin % (60 * 24)) / 60);
  const mins = Math.floor(absMin % 60);
  let label: string;
  if (absMin < 60) label = `${mins} 分`;
  else if (absMin < 60 * 24) label = `${hours} 小时${mins > 0 ? ` ${mins} 分` : ""}`;
  else label = `${days} 天${hours > 0 ? ` ${hours} 小时` : ""}`;
  const isOverdue = diffMs < 0;
  const display = isOverdue ? `📅 ${label}前已过` : `📅 ${label}后`;
  return (
    <button
      onClick={async (e) => {
        e.stopPropagation();
        try {
          await navigator.clipboard.writeText(t.due!);
          setBulkResultMsg(`📋 已复制 due: ${t.due}`);
        } catch (err) {
          setBulkResultMsg(`复制 due 失败：${err}`);
        }
        window.setTimeout(() => setBulkResultMsg(""), 2500);
      }}
      title={... countdown context + 点击复制 ISO ...}
      style={{... overdue: red+bold; future: muted ...}}
    >
      {display}
    </button>
  );
})()}
```

设计：
- **手动 regex parse 本地时间**：与 ⏭ +1d chip 同 parse 算法 — 跨浏
  览器 `new Date("YYYY-MM-DDTHH:MM")` 行为不一致（有的 UTC，有的
  Local，有的报错）。手动 regex + Date constructor 保跨平台稳定
- **3 段 label**：< 60min → "N 分" / < 24h → "N 小时 M 分" / ≥ 24h →
  "N 天 N 小时"。粒度自适应 — 5 分钟显「5 分」精确；2 天显「2 天 3 小
  时」省噪音
- **overdue 红 + bold**：与既有 dueUrgency overdue chip 配色 family；
  visual emphasis 提示紧迫
- **click 复制 ISO**：与 PanelMemory 📅 created chip click copy ISO 同
  语义（iter #453）— 让 owner 引用具体时刻时一键 copy

## Key design decisions

- **hover-only via `taskPreviewHoverTitle`**：与既有 📂 / ↗ / 📊 / ↘
  / ⏭ / 🔁 等 hover chip 同 gate 节奏，避免 always-visible 视觉密度
- **`!isFinished(t.status)` gate**：done / cancelled 任务的 due 已无意
  义（不会再 fire）— 与既有 ⏭ +1d / 📌+⏰ combo / 🔁 复制 schedule 同
  gate
- **`t.due` 存在 gate**：无 due 的 task 不渲染 chip（与既有 schedule
  chip 同 gate — 信号无意义时隐藏）
- **不引「click 直接弹 📅 调期 popover」**：本 chip 是 audit / 复制入
  口 — owner 想改 due 走既有 📅 调期 chip 或 ⏭ +1d chip。两职责分开
  让 chip family 各司其职
- **三级 label 阈值（60 / 24h）通用**：与 ChatMini ⏱ pet 沉默 chip /
  TG /last_speech 相对时间 chip 同 tiered display protocol
- **不写 unit test**：纯 regex parse + Date 算术 + Render；逻辑 trivial
  （与 ⏭ +1d chip / parse_sleep_until_time 算法 production 验证）。
  GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.30s)
- 后端无改动 — 纯前端 hover chip
- 手测：PanelTasks pending row with due → hover → chip 出现 → 显「N 天
  X 小时后」（future）或「N 分前已过」（overdue 红 bold）→ click → toast
  「📋 已复制 due: 2026-05-19T10:00」；不带 due 的 row hover 时本 chip
  不渲染
