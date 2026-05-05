# PanelMemory 分类折叠（Iter R102）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory 分类折叠：每个 category section 标题加 ▾ / ▸ 切换按钮；items 多时（> 10 条）默认折叠到前 5 + "展开全部 N 条"，配合 R95 butler 历史折叠的视觉模式让长 cat 不抢屏。

## 目标

PanelMemory 列出 5 个 category，每个长度不一。`butler_tasks` 通常 ≤ 10
但 `general` / `ai_insights` 可能长到 30+ 条。短 cat 与长 cat 同屏混排时，
长 cat 把短 cat 推到屏幕底，需要滚动才能切换关注点。

加分类折叠：
- > 10 条的 cat 默认显前 5 条 + "展开全部 N 条"按钮
- ≤ 10 条不折叠（避免引入无用交互）
- section 标题旁加 ▾ / ▸ 按钮提供"显式折叠"入口（即使 ≤ 10 条用户也能手
  动折叠）

## 非目标

- 不持久化折叠状态 —— 与 R91 / R95 同语义（临时 session 视角）
- 不引入"无限滚动 / 虚拟列表"—— 5 vs 全部 一档切换够用
- 不动 butler 既有的"每日小结" / "最近执行"两段（R95 已折叠 history）—
  本轮针对 cat.items 主列表

## 设计

### state

```ts
const [expandedCategories, setExpandedCategories] = useState<Set<string>>(
  new Set(),
);
```

key 是 catKey（"butler_tasks" 等），Set 表"哪些 cat 用户主动展开了"。
默认 empty —— 所有 cat 走"自动折叠规则"。

### 折叠规则

```ts
const CATEGORY_FOLD_THRESHOLD = 10;
const CATEGORY_FOLD_PREVIEW = 5;

const isLong = cat.items.length > CATEGORY_FOLD_THRESHOLD;
const expanded = expandedCategories.has(catKey);
const shownItems =
  isLong && !expanded
    ? cat.items.slice(0, CATEGORY_FOLD_PREVIEW)
    : cat.items;
```

- isLong=false：始终显全部
- isLong=true + expanded=false：显前 5 + 展开按钮
- isLong=true + expanded=true：显全部 + 收起按钮

### 渲染

修改 `cat.items.map(...)` 改为 `shownItems.map(...)`；map 后追加展开/收起
按钮：

```tsx
{isLong && (
  <button
    type="button"
    onClick={() => {
      setExpandedCategories((prev) => {
        const next = new Set(prev);
        if (next.has(catKey)) next.delete(catKey);
        else next.add(catKey);
        return next;
      });
    }}
    style={{
      marginTop: 4,
      fontSize: 11,
      padding: "2px 8px",
      border: "none",
      background: "transparent",
      color: "var(--pet-color-accent)",
      cursor: "pointer",
      fontFamily: "inherit",
    }}
    title={
      expanded
        ? `折叠回前 ${CATEGORY_FOLD_PREVIEW} 条`
        : `展开后显示全部 ${cat.items.length} 条`
    }
  >
    {expanded
      ? `收起 (${cat.items.length})`
      : `… 展开全部 ${cat.items.length} 条`}
  </button>
)}
```

### 测试

无单测；手测：
- cat.items.length === 5 → 全显，无折叠按钮
- cat.items.length === 11 → 显前 5 + "展开全部 11 条"
- 点展开 → 显全部 + "收起 (11)"
- 切到搜索结果再退出（searchResults=null 重渲染）→ 折叠状态保留（不 reset）
- 切其它 panel 再回 → state 由 React tree 决定，PanelMemory 卸载就 reset
  （与 R91 / R95 同语义，无需特殊持久化）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + 常量 + render 改造 + 展开/收起按钮 |
| **M2** | tsc + build |

## 复用清单

- 既有 `expandedBodies` / `butlerHistoryExpanded` 折叠模式
- R95 inline 链接式按钮样式（accent 色 / 无 border / cursor pointer）

## 进度日志

- 2026-05-09 07:00 — 创建本文档；准备 M1。
- 2026-05-09 07:08 — M1 完成。`expandedCategories: Set<string>` state（默认 empty）；CATEGORY_ORDER.map 内 cat.items.map 改 IIFE 包裹：CATEGORY_FOLD_THRESHOLD=10 / PREVIEW=5 常量、isLong / expanded 决定 shownItems；map 后追加展开 / 收起 inline 链接式 accent 按钮（与 R91/R95 同款）。≤ 10 条不显按钮（避免无用交互）。
- 2026-05-09 07:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 同 R101 build 通过 (500 modules, 945ms)。归档至 done。
