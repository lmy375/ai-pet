# PanelTasks 任务行 hover「↗ N refs」chip（iter #424）

## Background

iter #414 加 PanelMemory item 🔗 inline ref 按钮生成 `[[cat/title]]`
wiki-link token；owner 可在 detail.md 内粘贴作交叉引用。但**消费
视图**缺位 —「这条 task 有多少 outgoing 引用」没显，owner 想 audit
不知从哪 squint。

本 iter 加 PanelTasks 任务行 hover「↗ N refs」chip — 扫
detailMap.detail_md 内 `[[cat/title]]` token 和 `「title」` task ref
token 计数。0 时不渲；与 ⏱ / 📂 / 📊 chip 同 hover-only 节奏。

同时丢 TODO「PanelMemory item 📝 N 分钟前 updated_at chip」一行 —
功能已存在为 PanelMemory item 展开视图内的「📅 创建 X 前 · 🔄
更新 Y 前」chip（line 5814-5861），TODO 描述的「N 分钟前」chip
是 same data 不同 layout（chip 化 vs muted 文本）；不重复加。

## Changes

### `src/components/panel/PanelTasks.tsx`（紧贴 📂 detail size 之后）

```tsx
{taskPreviewHoverTitle === t.title && (() => {
  const detail = detailMap[t.title];
  if (!detail) return null;
  const text = detail.detail_md ?? "";
  if (text.length === 0) return null;
  const wikiRefs = (text.match(/\[\[[^[\]\n]+\]\]/g) ?? []).length;
  const taskRefs = (text.match(/「[^「」\n]+」/g) ?? []).length;
  const total = wikiRefs + taskRefs;
  if (total === 0) return null;
  return (
    <span
      title={`detail.md 内含 ${wikiRefs} 条 [[cat/title]] inline ref + ${taskRefs} 条「title」task ref（heuristic）...`}
      style={dashed-mono-chip}
    >
      ↗ {total} refs
    </span>
  );
})()}
```

设计要点：
- **复用 hover preview 触发的 detailMap**：与 iter #421 📂 chip 同
  IO 复用 — 零额外 invoke
- **两类 ref 都计**：`[[cat/title]]` (iter #414 inline ref 协议) +
  `「title」` (既有 task ref 协议，renderContentWithTaskRefs / ⌘K
  picker 用)。两者覆盖 owner 在 detail.md 内可能用的所有引用形式
- **heuristic regex**：`\[\[[^[\]\n]+\]\]` 严格匹配 wiki-link 双括
  号 + 不跨行 + 不嵌套；`「[^「」\n]+」` 同模式。简单稳定不 parse
  AST
- **total === 0 时不渲**：避免「↗ 0 refs」噪音；新 task / 无引用
  detail 不显
- **hover-only**：与 ⏱ / 📂 / 📊 同 taskPreviewHoverTitle gate；
  500ms 触发后所有 chip 同时浮起视觉节奏一致
- **monospace + dashed border**：与 📂 chip 同视觉重量

## Key design decisions

- **不显 incoming refs（"哪些 task 引用了我"）**：incoming 需要扫所
  有其它 task 的 detail.md，IO 重 + 缓存策略复杂；outgoing 直接读
  当前 task detail 零开销。需要 incoming audit 时走 PanelTasks 顶
  部 search 输入「[[cat/<title>]]」即可（既有 search 支持子串）
- **不去重 ref token**：同 title 引用两次显 2 次 refs — 反映 detail
  内强调密度。owner 想知道唯一 ref 数走详情面板看 detail.md 内容
- **不为单 chip 引 unit test**：regex 模式 + setState；build pass
  + 手测足够（hover 含 [[...]] 的 task → 看 ↗ chip 出 + 计数正确；
  含「title」task ref → 也计入）
- **drop PanelMemory updated_at chip TODO**：functionally 已被
  expanded 视图内的「📅 创建 X 前 · 🔄 更新 Y 前」覆盖；chip vs
  muted 文本是 cosmetic 偏好不重复

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.40s)
- 后端无改动 — 纯前端 regex 派生
