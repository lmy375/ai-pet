# 决策日志统计条 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 决策日志统计条：filter 行末显 `当前过滤命中 N / 全 M`，让用户在长 timeline 里直观感知过滤强度。

## 目标

PanelDebug 决策日志加了多 chip + reason search 后，过滤可能很复杂。
用户现在难感知"我的过滤是不是太严"——只能往下滚到底数行数。本轮在 chip
+ search 行的最末尾加一段灰色 `命中 N / 全 M` 文案，实时反映过滤强度。

## 非目标

- 不动统计 placement（在已有 ↓最新在底 toggle 之后；与排序按钮同 row）。
- 不做 percent 显示 —— `N / M` 已直观。
- N === M 时也显示 —— 让用户始终知道总数（"当前我看到的全部决策有 M 条"）。

## 设计

### 重构：把 filter 计算提到 useMemo

现在 `kindFiltered` / `filtered` 在 IIFE 内计算，header 看不到 length。提
到 useMemo（依赖 `decisions / decisionKinds / decisionReasonSearch`）让
header + 渲染共享同一份。

```ts
const filteredDecisions = useMemo(() => {
  const kindFiltered = decisionKinds.size === 0
    ? decisions
    : decisions.filter((d) => decisionKinds.has(d.kind));
  const q = decisionReasonSearch.trim().toLowerCase();
  if (q === "") return kindFiltered;
  return kindFiltered.filter((d) => {
    const haystack = `${d.kind} ${d.reason} ${localizeReason(d.kind, d.reason)}`.toLowerCase();
    return haystack.includes(q);
  });
}, [decisions, decisionKinds, decisionReasonSearch]);
```

IIFE 改成读 `filteredDecisions`，去重计算。

### 统计文案

filter 行末，"↓ 最新在底" 按钮之后插：
```tsx
<span style={{ fontSize: 11, color: "#94a3b8", whiteSpace: "nowrap" }}>
  {filteredDecisions.length} / {decisions.length}
</span>
```

只显数字 + 分隔符 `/`，不写"命中 / 全"标签 —— mono 字体下 `12 / 47` 已能
被理解为分数；title hover 加详细解释 "当前过滤命中 / 决策总数（来自 in-
memory ring buffer）"。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 提出 filteredDecisions useMemo + IIFE 改读 |
| **M2** | filter 行末加 `N / M` 数字 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 filter 逻辑（提取到 useMemo 后保持语义不变）
- 既有 chip / search / 排序行容器

## 进度日志

- 2026-05-07 14:00 — 创建本文档；准备 M1。
- 2026-05-07 14:05 — M1 完成。`filteredDecisions` 提到 useMemo（依赖 decisions / decisionKinds / decisionReasonSearch）；IIFE 改读，去重。补充 useMemo 入 react import。
- 2026-05-07 14:10 — M2 完成。filter 行排序按钮后加 `{N} / {M}` mono 灰字；title hover 解释 "当前过滤命中条数 / 决策总数（来自 in-memory ring buffer）"。
- 2026-05-07 14:15 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 927ms)。归档至 done。
