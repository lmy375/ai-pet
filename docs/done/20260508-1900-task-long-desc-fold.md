# 任务长描述默认折叠（Iter R91）

> 对应需求（来自 docs/TODO.md）：
> 任务长描述默认折叠：itemBody 超过 200 字的描述默认显前 120 字 + "展开"按钮，避免单条任务卡片在列表里占据过大视觉空间。

## 目标

PanelTasks itemBody 现在显全文 description。委托型任务（如 butler_tasks
里写一段长 prompt）/ 用户写的"备忘任务"经常 200+ 字，单条卡片占据 5-10
行，挤压列表 skim 体验。

加默认折叠：超过 200 字的描述显前 120 字 + ` "…"` + "展开 (N 字)" 按钮；
点击切换到全文 + "收起 (N 字)"。短描述（≤200 字）不动。

## 非目标

- 不改后端字段 / 存储 —— 折叠纯前端展示决策
- 不动 result / error_message / cancelled reason 这些"短文本字段" —— 它们
  被其它消息行展示，风格不同，长度也少超阈
- 不持久化展开状态 —— 临时浏览决策，session 内即可（`useState`）；用户
  关闭面板后展开态丢失，下次打开默认折叠（与"跨会话搜索清状态"等其它
  panel 同语义）

## 设计

### 阈值

- `FOLD_THRESHOLD = 200`：≤ 200 字不折叠
- `FOLD_PREVIEW = 120`：折叠时显前 120 字

200/120 都是直觉值（约 4-5 行 vs 2-3 行）。中文 ~3 char/token，120 字 ≈
40 tokens，足够 skim 一句完整中文。

### state

```ts
const [expandedBodies, setExpandedBodies] = useState<Set<string>>(new Set());
const bodyKey = (t: TaskView) => `${t.title}-${t.created_at}`;
```

key 沿用 list `<div key>` 同款，与 react reconciliation 对齐。

### 折叠规则

- body 长 ≤ 200 字 → 显全文，无按钮
- body 长 > 200 字 + 用户没展开 + **没在搜索 OR 搜索 keyword 不在 body 里**
  → 折叠到 120 字 + 展开按钮
- 命中条件之一不成立 → 显全文 + 收起按钮

"搜索 keyword 命中 body 时强制展开" 是关键 UX 防线：用户搜了关键词进任务，
HighlightedText 高亮 body 内匹配的子串；若匹配位置在 120 字之外，折叠态
就看不到高亮，搜索体验崩溃。强制展开避免这种 inconsistency。

### 渲染

```tsx
{t.body && (() => {
  const isLong = t.body.length > FOLD_THRESHOLD;
  const key = bodyKey(t);
  const expanded = expandedBodies.has(key);
  const matchInBody =
    search.trim() !== "" &&
    t.body.toLowerCase().includes(search.trim().toLowerCase());
  const folded = isLong && !expanded && !matchInBody;
  const shown = folded ? t.body.slice(0, FOLD_PREVIEW) + "…" : t.body;
  return (
    <div style={s.itemBody}>
      <HighlightedText text={shown} query={search} />
      {isLong && !matchInBody && (
        <button
          type="button"
          onClick={() => toggleBodyExpansion(key)}
          style={s.bodyToggleBtn}
          title={folded ? `展开全部 ${t.body.length} 字` : "折叠到前 120 字"}
        >
          {folded ? `… 展开 (${t.body.length} 字)` : `收起 (${t.body.length} 字)`}
        </button>
      )}
    </div>
  );
})()}
```

button 用 inline 链接式样式（小字、accent 色、no border、cursor: pointer）
让"展开 / 收起" 看着像可点的辅助操作而不抢主体视觉。

```ts
bodyToggleBtn: {
  marginLeft: 4,
  fontSize: 11,
  padding: 0,
  border: "none",
  background: "transparent",
  color: "var(--pet-color-accent)",
  cursor: "pointer",
  fontFamily: "inherit",
  whiteSpace: "nowrap",
},
```

### 测试

无单测；手测：
- 创建短描述任务（< 200 字）→ 全文 + 无按钮
- 创建长描述（300 字）→ 显前 120 + "… 展开 (300 字)"
- 点展开 → 全文 + "收起 (300 字)"
- 长描述任务：搜索 keyword 命中第 250 字 → 强制展开（无折叠/收起按钮）
- 长描述：搜索不命中 → 折叠不变
- 切到 finished 视图（show 已完成）→ 折叠状态保留
- 关闭面板再开 → 默认折叠（state 不持久化）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + bodyKey + render 改动 + style 加按钮样式 |
| **M2** | tsc + build |

## 复用清单

- 既有 `s.itemBody` 容器样式
- 既有 `HighlightedText` + search 高亮路径
- 既有 react key 模式 `${title}-${created_at}`

## 进度日志

- 2026-05-08 19:00 — 创建本文档；准备 M1。
- 2026-05-08 19:08 — M1 完成。模块顶部加 `BODY_FOLD_THRESHOLD = 200` / `BODY_FOLD_PREVIEW = 120` 常量；`expandedBodies: Set<string>` state（key = `${title}-${created_at}` 与 list key 同款）；s style 表加 `bodyToggleBtn`（inline 链接式 accent 文字、no border）；t.body render 改成 IIFE：判断 isLong / matchInBody / folded → 显前 120 字 + "… 展开 (N 字)" 或全文 + "收起 (N 字)"；搜索 keyword 命中 body 时强制展开（防止 high light 被折叠）。
- 2026-05-08 19:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 同 R90 build 通过 (499 modules, 965ms)。归档至 done。
