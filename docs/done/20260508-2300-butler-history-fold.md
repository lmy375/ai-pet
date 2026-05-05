# PanelMemory butler 最近执行折叠（Iter R95）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory butler 最近执行折叠：butler_tasks 的"最近执行"section 现在显示全部 N 条；> 5 条时太占屏，默认显前 5 条 + "展开全部 N 条"按钮（仿 R91 长描述折叠）。

## 目标

`butler_tasks` category 内的"最近执行"section 现在 unbounded —— 把 backend
缓冲全部塞出来（实测可达 50+ 条）。在 PanelMemory 里它直接挤压下方"任务
列表"的视觉空间，特别是当 butler_tasks 本身只有 5 条任务、最近执行却堆
30+ 条时，比例严重失衡。

加默认折叠：> 5 条时只显前 5（最新 5 次执行），下面附"展开全部 N 条"
按钮；点开切换全文 + "收起" 按钮。≤ 5 条时不动（无需折叠）。

## 非目标

- 不持久化展开状态 —— 临时浏览，与 PanelTasks 长描述折叠（R91）同语义
- 不改后端 / butler_history 容量
- 不引入"分页 / 虚拟滚动" —— 5 vs 全部 一档切换够用

## 设计

### state

```ts
const [butlerHistoryExpanded, setButlerHistoryExpanded] = useState(false);
```

放在 PanelMemory 函数顶部，与既有 `butlerHistory` state 同区。

### 折叠规则

`butlerHistory` reverse 后取前 5 条（最新 5 次）：

```ts
const reversedHistory = butlerHistory.slice().reverse();
const HISTORY_FOLD_THRESHOLD = 5;
const isLongHistory = butlerHistory.length > HISTORY_FOLD_THRESHOLD;
const shownHistory =
  isLongHistory && !butlerHistoryExpanded
    ? reversedHistory.slice(0, HISTORY_FOLD_THRESHOLD)
    : reversedHistory;
```

> 5 条且未展开 → 切前 5；其它 → 显全部。

### 渲染

替换现有 `{butlerHistory.slice().reverse().map(...)}`：

```tsx
{shownHistory.map((line, i) => { ... })}
{isLongHistory && (
  <button
    type="button"
    onClick={() => setButlerHistoryExpanded((v) => !v)}
    style={historyToggleStyle}
    title={
      butlerHistoryExpanded
        ? "折叠回前 5 条最新执行"
        : `展开后显示全部 ${butlerHistory.length} 条历史执行`
    }
  >
    {butlerHistoryExpanded
      ? `收起 (${butlerHistory.length})`
      : `… 展开全部 ${butlerHistory.length} 条`}
  </button>
)}
```

按钮样式（inline 链接式，蓝色 accent，与 R91 PanelTasks bodyToggleBtn 同款）：

```ts
const historyToggleStyle: React.CSSProperties = {
  marginTop: 4,
  fontSize: 11,
  padding: 0,
  border: "none",
  background: "transparent",
  color: "var(--pet-tint-blue-fg)",
  cursor: "pointer",
  fontFamily: "inherit",
};
```

`--pet-tint-blue-fg` 与最近执行 section 的蓝标题同色族 → "section 内的次级
操作"语义。

### 测试

无单测；手测：
- butlerHistory 长度 5：不显折叠按钮
- butlerHistory 长度 6：显前 5 + "… 展开全部 6 条" → 点击 → 显全部 + "收起 (6)"
- butlerHistory 空：section 整体不渲染（既有逻辑 `butlerHistory.length > 0`）
- 切到其它 category（非 butler_tasks）→ section 不渲染（既有 `catKey === "butler_tasks"`）
- 关面板再开 → state 重置（默认折叠）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + render + style |
| **M2** | tsc + build |

## 复用清单

- 既有 `butlerHistory` state
- 既有蓝色 tint section 视觉
- R91 PanelTasks 长描述折叠的 inline 链接按钮模式

## 进度日志

- 2026-05-08 23:00 — 创建本文档；准备 M1。
- 2026-05-08 23:08 — M1 完成。`butlerHistoryExpanded` state；section 内 IIFE：reverse 后切片（`isLong && !expanded ? slice(0, 5) : full`）；map 关闭后条件渲染 toggle 按钮（inline 链接式 var(--pet-tint-blue-fg) accent，与 R91 PanelTasks bodyToggleBtn 同款）；`isLong` 守卫覆盖 ≤ 5 条不出按钮。
- 2026-05-08 23:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 同 R94 build 通过 (499 modules, 947ms)。归档至 done。
