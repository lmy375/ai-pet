# PanelDebug 工具调用频次 top-K 卡片（Iter R97）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug 工具调用频次 top-K 卡片：从既有 toolCallHistory ring buffer 派生"最近 N 次调用里 top 5 工具"，加在 PanelStatsCard 旁让用户感知"宠物最依赖什么"。

## 目标

PanelDebug 已经有"工具调用历史"section（按 risk_level 过滤、可展开看 args /
result），但用户要回答"宠物最常调哪些工具"这种统计问题，得手动数 N 行。

加一个轻量 top-K 卡片：从 `toolCallHistory` ring buffer（cap=N，由后端 R4
持久化）派生"调用次数 top 5"列表。直接显在 PanelStatsCard 之后，与发言
统计 + tone 走廊形成"宠物画像"三件套。

## 非目标

- 不改后端 ring buffer 容量 / 不持久化新统计 —— 派生自既有数据
- 不做按 risk_level / review_status 分组统计 —— 工具历史 section 已支持
  这两维过滤；top-K 关心的是"频次"维度
- 不做 sparkline / time-series 趋势 —— 单卡 row 4-5 个 chip 已传达 90%
  信息；趋势属于另一类 feature

## 设计

### 计数

```ts
const counts = new Map<string, number>();
for (const r of toolCallHistory) {
  counts.set(r.name, (counts.get(r.name) ?? 0) + 1);
}
const top = [...counts.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 5);
```

ring buffer 现在 cap 远小于 1000（实际几十条），单次扫廉价；不需要 useMemo
（state 变化时整 panel 都重渲染，节流意义不大）。

### 渲染：新组件 PanelToolsTopK

新文件 `src/components/panel/PanelToolsTopK.tsx`（mirror PanelStatsCard 的
存在即"独立小卡片"模式）。

```tsx
import type { ToolCallRecord } from "./panelTypes";

interface Props {
  history: ToolCallRecord[];
}

export function PanelToolsTopK({ history }: Props) {
  if (history.length === 0) return null;
  const counts = new Map<string, number>();
  for (const r of history) counts.set(r.name, (counts.get(r.name) ?? 0) + 1);
  const top = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5);
  if (top.length === 0) return null;
  return (
    <div style={{
      padding: "8px 16px",
      borderBottom: "1px solid var(--pet-color-border)",
      background: "var(--pet-color-bg)",
      display: "flex",
      alignItems: "baseline",
      gap: 12,
      flexWrap: "wrap",
      fontSize: 12,
    }}>
      <span style={{ color: "var(--pet-color-muted)", fontSize: 11 }}>
        🔧 最常用工具（近 {history.length} 次）
      </span>
      {top.map(([name, count], i) => (
        <span key={name} style={{ display: "inline-flex", alignItems: "baseline", gap: 4 }}>
          <span style={{ color: "var(--pet-color-muted)", fontSize: 10 }}>#{i + 1}</span>
          <span style={{ fontFamily: "'SF Mono', 'Menlo', monospace", color: "var(--pet-color-fg)", fontWeight: 500 }}>
            {name}
          </span>
          <span style={{ color: "var(--pet-color-accent)", fontWeight: 600 }}>× {count}</span>
        </span>
      ))}
    </div>
  );
}
```

### 类型抽取

`ToolCallRecord` 当前定义在 PanelDebug.tsx 函数体内（line 91-）。要让新
组件 import 这个类型，需要把它挪到 `panelTypes.ts`。不过更简单：给新组件
inline 定义 props 接口需要的字段（只用 `name`），不依赖完整 ToolCallRecord：

```ts
interface Props {
  history: { name: string }[];
}
```

这样新组件解耦于 ToolCallRecord 完整 shape，未来后端 schema 改动只要 name
字段保留就不影响。

### PanelDebug 接入

```diff
       <PanelStatsCard ... />

+      <PanelToolsTopK history={toolCallHistory} />

       <PanelToneStrip tone={tone} />
```

放在 StatsCard 之后、ToneStrip 之前，让"发言统计 → 工具画像 → tone 信号"
三层依次呈现。

### 测试

无单测；手测：
- toolCallHistory 空 → 卡片不渲染（避免空 placeholder 占行）
- 触发几次 reactive chat → 各种 tool 入队 → top 5 出现
- 同名 tool 多次调用计数累加
- 切换 dark 主题 → bg / fg / muted / accent 都跟切

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 新建 PanelToolsTopK.tsx + import + 接入 PanelDebug |
| **M2** | tsc + build |

## 复用清单

- 既有 toolCallHistory state（PanelDebug.tsx）
- 既有 token 系统（border / bg / fg / muted / accent）

## 进度日志

- 2026-05-09 02:00 — 创建本文档；准备 M1。
- 2026-05-09 02:08 — M1 完成。新建 `src/components/panel/PanelToolsTopK.tsx`：单 export，Props 解耦只取 `name` 字段；空历史返 null 不渲染；border / bg / fg / muted / accent 全走 token；title hover tooltip 解释 ring buffer 来源。PanelDebug 加 import + 在 `<PanelStatsCard />` 与 `<PanelToneStrip />` 之间插 `<PanelToolsTopK history={toolCallHistory} />`。
- 2026-05-09 02:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules — 新文件计数 +1, 934ms)。归档至 done。
