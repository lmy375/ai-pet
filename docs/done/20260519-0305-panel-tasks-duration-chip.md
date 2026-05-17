# PanelTasks 行加「⏱ 历经 N」hover chip（iter #526）

## Background

iter #518 加了 `📅 创建` chip 显 task age。但缺**done task 产出周期量化**
信号 — owner 想看「这条 task 从创建到完成多久 / 哪些 done 拖了一周
+ / 整体节奏快慢」时仍要心算 created_at vs updated_at。

本 iter 加 `⏱ 历经 N` chip — done task hover 时显 create→done 持续
时长，分级 label（天 / 天+小时 / 小时+分 / 分），click 复制 line 含
两端日期。

## Changes

### `src/components/panel/PanelTasks.tsx`

紧贴 `📅 创建` chip 之后插入：

```tsx
{taskPreviewHoverTitle === t.title &&
  t.status === "done" &&
  t.created_at.length > 0 &&
  t.updated_at.length > 0 &&
  (() => {
    const cMs = Date.parse(t.created_at);
    const uMs = Date.parse(t.updated_at);
    if (isNaN(cMs) || isNaN(uMs)) return null;
    const durMs = uMs - cMs;
    if (durMs <= 0) return null;
    const totalMin = Math.floor(durMs / 60_000);
    const days = Math.floor(totalMin / 1440);
    const hours = Math.floor((totalMin % 1440) / 60);
    const mins = totalMin % 60;
    let label: string;
    if (days >= 7) label = `${days} 天`;
    else if (days > 0) label = `${days}d ${hours}h`;
    else if (hours > 0) label = `${hours}h ${mins}m`;
    else label = `${mins}m`;
    return (
      <button
        onClick={async (e) => {
          e.stopPropagation();
          const line = `「${t.title}」历经 ${label}（${t.created_at.slice(0, 10)} → ${t.updated_at.slice(0, 10)}）`;
          await navigator.clipboard.writeText(line);
          setBulkResultMsg(`⏱ 已复制：${line}`);
        }}
        title={`task 从创建到完成历经 ${label}...`}
        style={{ ...muted dashed chip... }}
      >
        ⏱ 历经 {label}
      </button>
    );
  })()}
```

### Gates

- **`taskPreviewHoverTitle === t.title`**：500ms hover 与既有 hover chip
  家族同节奏
- **`t.status === "done"`**：仅 done — cancelled / error 的「历经」语
  义不准（中断态，不是"产出周期"）；pending 还未完成无 duration 可言
- **`isNaN` + `durMs > 0` 双重防御**：日期 parse 失败或 clock drift
  反向 duration → 不渲染

### 分级 label

- ≥ 7 天：`{N} 天` — 周级 task 不必精到小时（粗粒度更易扫读）
- 0-6 天：`{N}d {H}h` — 短期 task 含小时让 "2 天 6 时" 这种精度可见
- < 1 天：`{H}h {M}m` — 当日完成的精度
- < 1 小时：`{M}m` — 快任务

## Key design decisions

- **仅 done task 显**：与既有 `📅 创建` chip（不分 status 显）有意区
  分 — 那个看「task 多老」（任意状态有效）；本 chip 看「产出周期」
  （done 专属）
- **分级 label 与既有 💤 wake-countdown / 📅 due 倒计时 一致**：相同
  天/小时/分级原则，owner 心智复用
- **click 复制含日期对**：`<title> 历经 N（YYYY-MM-DD → YYYY-MM-DD）`
  — paste 到 sprint review / 周报场景，对方一眼看到 "从哪天到哪天"
- **dashed border + muted color**：与既有非 active hover chip（📜 raw
  / 📋 ref / 📅 created）同 muted 视觉层
- **不写 unit test**：纯 conditional render + Date.parse + 分级 if-else
  字符串拼接；逻辑 trivial（既有 hover chip 家族同 algorithm pattern
  production 验证）。GOAL.md "meaningful tests only" 规则下不引装饰
  性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.32s)
- 后端无改动 — 纯前端 hover chip
- 手测：
  - hover done row 500ms → 看到「⏱ 历经 N」chip 在 `📅 创建` 之后
  - pending / error / cancelled row hover → chip 不显（status gate 验）
  - click → toast 「⏱ 已复制：「<title>」历经 N（YYYY-MM-DD → YYYY-
    MM-DD）」
  - 长 task (≥7d) 显「N 天」简化；短 task 显「H h M m」精度

## Future iters (out of scope)

- 「按 duration 排序」mode — sort done by 历经长度，audit "拖最久"vs"最
  快"；后续 propose
- 颜色分级（< 1d 绿 / 1-7d 黄 / >7d 红 audit signal）— 当前 neutral
  muted 灰 keep client-side neutrality
