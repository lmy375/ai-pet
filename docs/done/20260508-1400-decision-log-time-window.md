# 决策日志时间窗快捷过滤（Iter R86）

> 对应需求（来自 docs/TODO.md）：
> 决策日志加时间窗快捷过滤：filter chip 行新增"近 10m / 30m / 1h"三键，与现有 kind / reason 过滤叠加，方便 debug 短时间内事件。

## 目标

PanelDebug 决策日志现在能按 kind 多选 + reason 子串过滤，但无法按"最近 N
分钟"快速聚焦。debug 一个刚发生的"为什么 5 分钟前宠物突然开口"场景时，
用户需要肉眼扫整个 ring buffer 找时间戳，太低效。

加 3 个时间窗 chip（10 / 30 / 60 分钟），与 kind + reason 过滤叠加。

## 非目标

- 不做自定义 N 分钟输入框 —— 三个固定阶位已覆盖 90% debug 场景；过细的
  自由输入反而增加心智负担
- 不做"超过 N 分钟"反向过滤 —— 现有 kind / reason 已能屏蔽 noise，反向
  时间窗用得少
- 不持久化到 localStorage —— 临时 debug 视角，与 kind / reason 同语义
- 不联动后端 ring buffer 改设容量 —— 只在当前 fetch 到的 ≤16 条上过滤；
  超出时间窗的更早条目本来就不在 buffer 里

## 设计

### state

```ts
const [decisionTimeWindow, setDecisionTimeWindow] = useState<
  "all" | "10m" | "30m" | "1h"
>("all");
```

单选（time window 互斥），默认 "all" = 不过滤。

### useMemo 扩展

`filteredDecisions` 现有 kind + reason 双过滤的基础上插入时间窗：

```ts
const filteredDecisions = useMemo(() => {
  let f = decisionKinds.size === 0
    ? decisions
    : decisions.filter((d) => decisionKinds.has(d.kind));

  if (decisionTimeWindow !== "all") {
    const windowMs =
      decisionTimeWindow === "10m" ? 10 * 60_000 :
      decisionTimeWindow === "30m" ? 30 * 60_000 :
      60 * 60_000;
    const cutoff = Date.now() - windowMs;
    f = f.filter((d) => {
      const ts = Date.parse(d.timestamp);
      return !isNaN(ts) && ts >= cutoff;
    });
  }

  const q = decisionReasonSearch.trim().toLowerCase();
  if (q === "") return f;
  return f.filter((d) => {
    const haystack = `${d.kind} ${d.reason} ${localizeReason(d.kind, d.reason)}`.toLowerCase();
    return haystack.includes(q);
  });
}, [decisions, decisionKinds, decisionTimeWindow, decisionReasonSearch]);
```

`Date.now()` 在 useMemo 内调用 → 仅在依赖变化时重算，过滤"快照在那一刻"。
新决策入队时 decisions 引用变化触发重算，时间窗自动滑动；用户静默盯着时
不会自动剔除恰好越界的条目（minor staleness, 不影响 debug）。

### 渲染

3 个按钮放在 kind chip 行末尾（kind chip 之后、reason 搜索之前），共用
现有 `chipStyle` 函数 —— accent 用统一灰 `#475569`（与"全部"chip 同色族，
表示"非 kind 的过滤维度"）。

```tsx
const timeOptions: { value: typeof decisionTimeWindow; label: string; title: string }[] = [
  { value: "10m", label: "近 10m", title: "只看最近 10 分钟内的决策" },
  { value: "30m", label: "近 30m", title: "只看最近 30 分钟内的决策" },
  { value: "1h",  label: "近 1h",  title: "只看最近 60 分钟内的决策" },
];
{timeOptions.map((opt) => {
  const isActive = decisionTimeWindow === opt.value;
  return (
    <button
      key={opt.value}
      type="button"
      onClick={() => setDecisionTimeWindow(isActive ? "all" : opt.value)}
      style={chipStyle(isActive, "#475569")}
      title={isActive ? `再次点击关闭时间窗（${opt.title}）` : opt.title}
    >
      {opt.label}
    </button>
  );
})}
```

再点同一 chip = 关闭（回到 "all"）；点别的 time chip = 切换到那一档（互斥）。

### 测试

无单测；手测：
- 默认 "all"：与切换前完全一致
- 点 "近 10m"：只剩近 10 分钟决策；count `N / M` 缩小；
- 与 kind 多选叠加：只显示符合 kind 的近期条目
- 与 reason 搜索叠加：三层 AND 过滤
- 再点同 chip：回到全部
- 点不同 time chip：切换互斥，不累加

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + useMemo 扩展 |
| **M2** | 3 个 chip 按钮渲染 |
| **M3** | tsc + build |

## 复用清单

- 既有 `chipStyle` 函数 + accent border `${accent}66` (Iter R84)
- 既有 `decisionsNewestFirst` / `decisionReasonSearch` filter 模式

## 进度日志

- 2026-05-08 14:00 — 创建本文档；准备 M1。
- 2026-05-08 14:08 — M1 完成。`decisionTimeWindow` state（"all" / "10m" / "30m" / "1h"）默认 "all"；filteredDecisions useMemo 在 kind 过滤后插入时间窗：windowMs 三档 const 选取，`Date.now() - cutoff` 在 useMemo 体内计算（依赖含 decisionTimeWindow）；reason 串过滤跑在最后保持原序。
- 2026-05-08 14:11 — M2 完成。3 个 chip 按钮在 kind chip 行末尾渲染（kind 之后 / search 之前）；accent 用统一灰 #475569（与"全部"chip 同色族表非-kind 维度）；单选互斥（再点同 chip 关闭回 "all"）；title 视 isActive 切换文案。
- 2026-05-08 14:14 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (499 modules, 1.05s)。归档至 done。
