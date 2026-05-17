# PanelTasks 行加「📅 创建 MM-DD HH:MM」hover chip（iter #518）

## Background

Row hover chip 家族已含多个时间维度 chip：
- ↺ +1d（snooze 推后）
- ⏰ reminderMin（提前 N 分软提醒）
- 📅 due 倒计时（还 N 天 X 小时）
- 💤 还 N 醒（snooze 倒计时）
- 🔄 sparkline（30 天事件分布）

但缺**「task 创建时间」直接 chip** — owner 想看「这条 task 多老了 /
是不是 stale backlog / 从创建到 done 多少天」时只能：
1. /show（看 raw_description，但 created_at 不直接显）
2. 展开 row 看 detail（不一定显 created_at）
3. 走 PanelMemory 找 item meta

本 iter 加 row hover 时显「📅 创建 MM-DD HH:MM」chip — 即时拿 task age
信号，click 复制「<title> 创建于 ... (N天前)」一行。

## Changes

### `src/components/panel/PanelTasks.tsx`

紧贴 💤 wake-countdown chip 之后插入：

```tsx
{taskPreviewHoverTitle === t.title &&
  t.created_at.length > 0 &&
  (() => {
    const cMs = Date.parse(t.created_at);
    if (isNaN(cMs)) return null;
    const ageMs = nowMs - cMs;
    if (ageMs < 0) return null;
    const rel = formatRelativeAge(t.created_at, nowMs);
    const short = t.created_at.length >= 16
      ? `${t.created_at.slice(5, 10)} ${t.created_at.slice(11, 16)}`
      : t.created_at;
    return (
      <button
        onClick={async (e) => {
          e.stopPropagation();
          const line = `「${t.title}」创建于 ${t.created_at.replace("T", " ").slice(0, 16)}（${rel}）`;
          await navigator.clipboard.writeText(line);
          setBulkResultMsg(`📅 已复制：${line}`);
        }}
        title={`task 创建于 ${t.created_at.replace("T", " ")}（${rel}）— 点击复制...`}
        style={{ ...common chip style + muted color... }}
      >
        📅 创建 {short}
      </button>
    );
  })()}
```

### Gates

- **`taskPreviewHoverTitle === t.title`**：500ms hover 同既有家族
- **`t.created_at.length > 0`**：极端兜底，正常 task 都有
- **`isNaN(cMs) === false`**：Date.parse 失败防御（极旧 yaml 字段格式
  异常时不渲）
- **`ageMs >= 0`**：clock drift 防御 — 未来 timestamp 不显
- **无 `!isFinished` gate**：done / cancelled 也显 — 让 owner 复盘
  「从创建到完成多少天」

## Key design decisions

- **「MM-DD HH:MM」紧凑格式**：与 hover chip 视觉密度协调；想看完整
  ISO 走 tooltip 或 click 复制（含完整 YYYY-MM-DD HH:MM）
- **复用 `formatRelativeAge` helper**：与既有 row updated 倒计 / preview
  hover relative time 同来源（formatRelativeAgeBuckets）— 单位/分级
  一致
- **不分 status 显**：done / cancelled 也显 — task age 在终态也是
  有效 audit 信号（「这条 done 是去年 backlog」/「这条 cancelled 当
  时为啥取消」追溯）
- **click 复制 full ISO + 相对时间**：粘 chat 时 owner 看到「创建于
  2026-04-01 09:00（48天前）」既知绝对又知相对，paste 不歧义
- **dashed border + muted color**：与既有非 active hover chip（📜 raw /
  📋 ref / ↗ refs / 📊 sparkline）同 muted 视觉层 — 区别于 tint-color
  的状态 chip（⏰ tinted blue / 💤 tinted purple 等）
- **不写 unit test**：纯 conditional render + Date.parse + 既有
  formatRelativeAge helper；逻辑 trivial。GOAL.md "meaningful tests
  only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.32s)
- 后端无改动 — 纯前端 hover chip
- 手测：
  - hover 任意 row 500ms → 看到「📅 创建 MM-DD HH:MM」chip
  - hover chip → tooltip 显完整 ISO + 相对时间
  - click → toast 「📅 已复制：「<title>」创建于 ... (N天前)」
  - 粘到 chat / detail.md 看到含两种时间格式的 line
  - done / cancelled row hover → chip 仍显（不分 status gate 验）

## Future iters (out of scope)

- 「创建到 done 持续天数」chip — 与「📅 创建」并行显「· 持续 N 天」
  仅 done task；后续 propose
- 「年份内 task age 颜色分级」— 0-7 天绿 / 8-30 天黄 / >30 天红 audit
  signal；当前 muted 灰 keep neutrality
