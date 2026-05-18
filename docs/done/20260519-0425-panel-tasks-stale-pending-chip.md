# PanelTasks 行加「⏳ pending Nd」stale chip（iter #532）

## Background

既有 hover chip 家族覆盖：
- `📅 创建`：任意状态显 created_at（中性灰）
- `⏱ 历经`：done 显 create→done duration（中性灰）
- `📅 due`：active row 含 due 时倒计时
- `💤 还 N 醒`：snoozed 倒计时

但缺 **「stale pending backlog 告警」** 信号 — owner 想看「哪条 pending
我搁了一个月没动」时需要心算或走 `/oldest_n`。本 iter 加 hover chip
（红 tint）专显 pending ≥ 14 天的「老任务告警」。

## Changes

### `src/components/panel/PanelTasks.tsx`

紧贴 `⏱ 历经` chip 之后插入（hover chip 时间维度家族延续）：

```tsx
{taskPreviewHoverTitle === t.title &&
  t.status === "pending" &&
  t.created_at.length > 0 &&
  (() => {
    const cMs = Date.parse(t.created_at);
    if (isNaN(cMs)) return null;
    const ageMs = nowMs - cMs;
    if (ageMs < 0) return null;
    const days = Math.floor(ageMs / 86_400_000);
    if (days < 14) return null;
    const veryStale = days >= 30;
    return (
      <button
        onClick={async (e) => {
          e.stopPropagation();
          const line = `「${t.title}」pending ${days} 天（since ${t.created_at.slice(0, 10)}）`;
          await navigator.clipboard.writeText(line);
          setBulkResultMsg(`⏳ 已复制：${line}`);
        }}
        title={veryStale ? "⚠ stale backlog ..." : "长期 backlog 信号 ..."}
        style={{
          ...red-tint chip with optional bold + bg fill,
        }}
      >
        ⏳ pending {days}d
      </button>
    );
  })()}
```

### Gates 与分级

- **`status === "pending"`**：done 已闭环（走 ⏱ 历经 chip）；
  error/cancelled 已中断（与 stale 语义不符）
- **`days >= 14`**：≤ 13 天 pending 不算 stale — 普通工作节奏
- **`days >= 30`** → 「very stale」加粗 + bg fill — 一个月还没动的 task
  几乎肯定要决策（推 / cancel / 重排 priority）
- 防御性：`isNaN` 跳过；`ageMs < 0`（clock drift）跳过

### tooltip 文案分级

- 14-29 天：「本 task pending N 天 — 长期 backlog 信号」中性
- ≥ 30 天：「⚠ stale backlog：考虑 /done / /cancel / /promote 推动决策」
  含 action hint

## Key design decisions

- **red-tint warning 色**：与既有 due overdue chip 同 var(--pet-tint-red-fg)
  — 视觉一致表达「需要 owner 关注」
- **14 / 30 双阈值**：14 天是「不正常但可能合理」，30 天是「确定需要
  决策」— 加粗 + bg fill 让 very stale 一眼可见
- **不分等级显其它颜色**：保 binary warn — owner 心智「红 = 老 task」
  单一信号；中间渐变易扰
- **复用 hover 家族 pattern**：500ms hover gate / dashed border / mono
  font / setBulkResultMsg 2.5s toast — 与 ⏱ 历经 / 📅 创建 同视觉
- **不写 unit test**：纯 React conditional render + Date.parse + 阈值
  分级；逻辑 trivial（既有 `⏱ 历经` chip 同 algorithm pattern
  production 验证）。GOAL.md "meaningful tests only" 规则下不引装饰
  性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.33s)
- 后端无改动 — 纯前端 hover chip
- 手测：
  - hover < 14 天 pending row → 无 chip（gate 验）
  - hover 15 天 pending row → 「⏳ pending 15d」红字 dashed
  - hover 35 天 pending row → 「⏳ pending 35d」红字加粗 + 红 bg
  - hover done / cancelled / error row → 无 chip
  - click → toast 「⏳ 已复制：「<title>」pending N 天（since DATE）」

## Future iters (out of scope)

- 「sort by stale-pending desc」mode — 按 pending 时长排，让 owner 一
  眼看最老 task；后续 propose
- /oldest_pending TG 命令 — 已有 /oldest_n 覆盖（pending sort by
  created asc 一致）
- error / cancelled 类似 chip — 「error stale」「cancelled forever ago」
  意义不大（error 应 retry / cancelled 是终态）
