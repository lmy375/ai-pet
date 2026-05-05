# PanelMemory 分类标题显示最新更新相对时间（Iter R92）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory category section header 显示最新更新相对时间：每个分类标题右侧附"最近更新 X 天前"小字（取 cat.items 最新 updated_at），让用户感知哪些 category 在活跃迭代、哪些是死库存。

## 目标

PanelMemory 现有多个 category section（facts / preferences / butler_tasks
等），仅显类目名 + 条数 badge。看不出"哪些区域在最近被用 / 哪些一年没动
过"。

加一段小字"最近更新 X 天前"贴在 badge 之后，配合 R87 / R89 形成的"流量
计"族（任务相对时间、完成率），给 memory 区域同样的时态感知。

## 非目标

- 不改 backend / 持久化字段 —— 现有 cat.items 已含 updated_at，前端取最新
  即可
- 不区分 created vs updated —— "最新更新" 已经是用户记忆"被动过的最近时刻"
  含义，足以分辨活跃度
- 空 cat.items 不渲染（与"暂无记忆"placeholder 互补）
- 不做"按更新时间排序" —— CATEGORY_ORDER 是固定的语义顺序（todos / facts
  / preferences ...），重排会让习惯定位失效

## 设计

### 计算

每个 category section 内联 IIFE 算最新 ts（`useMemo` 在 .map 里不能用 ——
hooks 规则要求每帧同序调用）：

```ts
const latestTs = (() => {
  let latest: number | null = null;
  for (const item of cat.items) {
    const ts = Date.parse(item.updated_at);
    if (Number.isNaN(ts)) continue;
    if (latest === null || ts > latest) latest = ts;
  }
  return latest;
})();
```

性能：cat.items ≤ 10 在所有情况，每条只跑一次 Date.parse —— 廉价。

### helper

```ts
function formatLastUpdated(latestTs: number, now: number): string {
  const age = now - latestTs;
  if (age < 60_000) return "刚刚更新";
  if (age < 3_600_000) return `${Math.floor(age / 60_000)} 分钟前更新`;
  if (age < 86_400_000) return `${Math.floor(age / 3_600_000)} 小时前更新`;
  return `${Math.floor(age / 86_400_000)} 天前更新`;
}
```

复用与 PanelTasks `formatRelativeAge` / `formatRecentlyUpdatedHint` 同款的
分级语义（minute / hour / day），用 "更新" 后缀贴 cat 语义（vs Tasks "前
创建"贴 task 语义）。

### 渲染

```diff
   <div style={s.sectionTitle}>
     {cat.label}
     <span style={s.badge}>{cat.items.length}</span>
+    {latestTs !== null && (
+      <span
+        style={{ fontSize: 11, color: "var(--pet-color-muted)", fontWeight: 400 }}
+        title={`最新一条 item 的 updated_at = ${new Date(latestTs).toLocaleString()}`}
+      >
+        最近 {formatLastUpdated(latestTs, now.getTime())}
+      </span>
+    )}
     {/* overdue 按钮 + 新建 按钮（marginLeft: auto） */}
```

`now` 已经在循环内 line 548 创建（`const now = new Date()`），复用。

文案 "最近 X 分钟前更新" / "最近 X 天前更新"——"最近" 前缀保持与"已经"
对比的轻读感（不是"最后一次"那么强硬）。

### 测试

无单测；手测：
- 新建一条 item → "最近 刚刚更新"
- 5 分钟后 → "最近 5 分钟前更新"
- 跨午夜 → "最近 1 天前更新"（按 24h 量纲，非日历日）
- 空 category（cat.items.length === 0）→ 不渲染该 span（latestTs===null）
- 该字段挤入 sectionTitle flex row 后 + 新建 button 仍 marginLeft auto 至右

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | helper + render 改动 |
| **M2** | tsc + build |

## 复用清单

- 既有 `cat.items[].updated_at`
- 既有 `s.sectionTitle` flex layout（gap: 8）
- 既有 PanelTasks `formatRecentlyUpdatedHint` / `formatRelativeAge` 同款分级
  设计

## 进度日志

- 2026-05-08 20:00 — 创建本文档；准备 M1。
- 2026-05-08 20:08 — M1 完成。CATEGORY_ORDER.map 内 inline 算 latestTs（cat.items ≤ 10 廉价；不能用 useMemo —— hooks 规则要求每帧同序调用）；section header 在 badge 之后插 muted 11px span，渲染 `最近 ${formatLastUpdated(latestTs, now.getTime())}`；空 cat（latestTs===null）不渲染该 span。helper `formatLastUpdated` 加在文件末（HighlightedText 上方），与 PanelTasks formatRelativeAge 同款 4 量级（minute / hour / day），后缀"更新"贴 cat 语义。
- 2026-05-08 20:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
