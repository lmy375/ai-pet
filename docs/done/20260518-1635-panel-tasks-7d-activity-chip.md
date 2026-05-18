# PanelTasks row hover「🔄 7d N」活动次数 chip（iter #578）

## Background

TODO 提案 row hover「🔄 update 次数」chip：扫 butler_history 算本 task
近 7d update event 数。

ambient signal「这条 task 近期有多活跃」让 owner 一眼区分「热门 task
（多次 update）」 vs「冷 task（仅 1 次 update）」。

## Data substrate

既有 `sparklineBuckets: Record<string, number[]>` (reload 时 batch
fetch 自 `task_history_sparklines` Tauri 命令)：每 task 10 桶 × 3
天 = 30 天事件分布，oldest=[0] / newest=[9]。

直接复用 — 取末 2 桶 `buckets[8] + buckets[9]` ≈ 近 0-6 天 ≈ 7d 窗口。

**注：sparkline 覆盖 update + create + rename + delete 全谱 event，
不精确单做 update 过滤**。本 iter 选「不加 backend 命令」路径，接受
全谱语义 + tooltip 明示。owner 想精确「update only」count 可未来 lift
新命令。

## Change

`PanelTasks.tsx` 紧贴既有 sparkline chip 加 hover-gated 🔄 chip：

```tsx
{taskPreviewHoverTitle === t.title && (() => {
  const buckets = sparklineBuckets[t.title];
  if (!buckets || buckets.length < 10) return null;
  const recent = (buckets[8] || 0) + (buckets[9] || 0);
  if (recent === 0) return null;
  return (
    <button onClick={async (e) => {
      const line = `「${t.title}」近 7 天 ${recent} 次 history event`;
      await navigator.clipboard.writeText(line);
      setBulkResultMsg(`🔄 已复制：${line}`);
    }} title="...含 update/create/rename 全谱..."
       style={{ fontSize: 10, dashed border, tint-blue, mono, ... }}>
      🔄 7d {recent}
    </button>
  );
})()}
```

## Key design decisions

- **复用 sparklineBuckets**：避免新增 backend command — sparkline 已
  扫 butler_history 一次得 bucket，再加 update-only count 命令重复扫；
  ROI 不值
- **取末两桶 buckets[8]+[9] = 0-6 天**：最接近 7d 窗口的整桶组合。
  桶边界对齐天而非 7d 精确切；偏离 ≤ 3 天，ambient 信号可接受
- **全谱 event 而非仅 update**：tooltip 明示「含 update / create /
  rename / 全谱不限 update-only」让 owner 知道 chip 数字语义。
  semantically 它是「活跃度」而非「编辑次数」
- **hover gate + 0 时不渲**：与 row hover chip family 一致 — 仅 hover
  时浮，且数 ≥ 1 才挂避免 dead chip
- **tint-blue dashed border**：与 stale 类 chip（💤 / ⏳ red）/ fresh
  badge（🔥 green）色族错开。blue = 中性 info 信号
- **click 复制单行**：与 ⏳ / 💤 / ⏱ 等 hover chip family 同 pattern

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — chip 加在熟悉 row 位置（紧贴 sparkline），同
  hover gate / 同样式 family

## Future iters (out of scope)

- **精确 update-only count**：新 backend Tauri 命令 `task_update_counts_recent(days)`
  返回 Map<title, count> 仅 filter action=="update"。avoid 多次扫
  log — bulk command。如 owner 反馈「全谱含创建/重命名让数字虚高」
  再做
- **🔄 30d cousin**：30d 全谱 = 全 sparkline 总和；冗余 — 与既有
  sparkline tooltip 内总数等价。propose 时合并 sparkline 增强
- **chip click 不复制而展 modal**：弹小 popover 显「7d N events:
  - 2026-05-15 update [done]; 2026-05-16 rename; ...」详情列表。
  需读 raw history 行 — 加 backend 命令或读 task_get_detail
