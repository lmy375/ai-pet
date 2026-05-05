# PanelMemory butler 每日小结折叠（Iter R143）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory butler 每日小结折叠：与 R95 butler 最近执行折叠同模式；> 5 条时默认显前 5（最新）+ "展开全部 N 条"按钮；长跑用户多日累积避免占屏。

## 目标

butler_tasks section 的"每日小结"现 unbounded —— 长跑用户累积几十条 daily
summary（每天 1 条），全部渲染挤压下方"最近执行" + 任务列表。R95 已对
"最近执行"加同款折叠；本轮镜像到"每日小结"。

加 `butlerDailyExpanded` state：> 5 条时默认显前 5（reversed 后即最新 5
天）；用户点 "展开全部 N 条" 切到 unbounded。

## 非目标

- 不持久化 expanded 状态（与 R95 / R102 / R109 同语义）
- 不动 yellow tint section 配色 / 边框
- 不联动 butler 最近执行的 expanded 状态 —— 两段独立

## 设计

### state

```ts
const [butlerDailyExpanded, setButlerDailyExpanded] = useState(false);
```

放 `butlerHistoryExpanded` 旁边（mirror R95 pattern）。

### 折叠规则

```ts
const HISTORY_FOLD_THRESHOLD = 5;
const reversed = butlerDaily.slice().reverse();
const isLong = butlerDaily.length > HISTORY_FOLD_THRESHOLD;
const shown =
  isLong && !butlerDailyExpanded
    ? reversed.slice(0, HISTORY_FOLD_THRESHOLD)
    : reversed;
```

复用 R95 的常量名 / 模式，让两段折叠逻辑视觉对齐。

### 渲染

把现有 `{butlerDaily.slice().reverse().map(...)}` 改为 IIFE 包裹 + 折叠
按钮：

```tsx
{(() => {
  const HISTORY_FOLD_THRESHOLD = 5;
  const reversed = butlerDaily.slice().reverse();
  const isLong = butlerDaily.length > HISTORY_FOLD_THRESHOLD;
  const shown = isLong && !butlerDailyExpanded
    ? reversed.slice(0, HISTORY_FOLD_THRESHOLD)
    : reversed;
  return (
    <>
      {shown.map((line, i) => {
        // 现有 map 内部不变
      })}
      {isLong && (
        <button
          type="button"
          onClick={() => setButlerDailyExpanded((v) => !v)}
          title={butlerDailyExpanded ? "折叠回最新 5 条" : `展开后显示全部 ${butlerDaily.length} 条历史小结`}
          style={{
            marginTop: 4,
            fontSize: 11,
            padding: 0,
            border: "none",
            background: "transparent",
            color: "var(--pet-tint-yellow-fg)",
            cursor: "pointer",
            fontFamily: "inherit",
          }}
        >
          {butlerDailyExpanded
            ? `收起 (${butlerDaily.length})`
            : `… 展开全部 ${butlerDaily.length} 条`}
        </button>
      )}
    </>
  );
})()}
```

按钮 color 用 `var(--pet-tint-yellow-fg)` 与 section 标题色一致；R95
最近执行用 `var(--pet-tint-blue-fg)`，两段配色互不冲突。

### 测试

无单测；手测：
- butlerDaily.length === 5：不显折叠按钮
- butlerDaily.length === 6：显前 5 + "… 展开全部 6 条"
- 点展开 → 显全部 + "收起 (6)"
- 切到其它 cat（非 butler_tasks）→ section 不渲染（既有逻辑）
- 关 panel 再开 → 默认折叠（state 重置）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + IIFE + 渲染改造 |
| **M2** | tsc + build |

## 复用清单

- 既有 butlerDaily state
- R95 butler 最近执行折叠模式
- yellow section 配色

## 进度日志

- 2026-05-11 00:00 — 创建本文档；准备 M1。
- 2026-05-11 00:08 — M1 完成。`butlerDailyExpanded` state 加在 butlerHistoryExpanded 旁；现 `butlerDaily.slice().reverse().map(...)` 改 IIFE 包裹：HISTORY_FOLD_THRESHOLD=5 + reversed slice(0,5) when !expanded；map 后追加 toggle button (yellow tint fg 与 section 标题色一致)。
- 2026-05-11 00:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
