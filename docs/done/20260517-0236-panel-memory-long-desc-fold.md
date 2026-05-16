# PanelMemory items 长 description 行级折叠 + "展开 (N 字)" 按钮

## 背景

PanelMemory 行 description 之前以全文渲染，超长（500+ 字 ai_insights / persona_summary 等）会让 row 撑得极高。owner 滚长列表时连每条都得划过去看。

与 PanelTasks R91 长 body 折叠对偶，PanelMemory 也加 200 字阈值折叠 + 展开按钮。

## 改动

### `src/components/panel/PanelMemory.tsx`

#### 1. 新 fold state

```ts
const [expandedMemDesc, setExpandedMemDesc] = useState<Set<string>>(new Set());
```

key = `${catKey}::${title}`，与 pinnedKeys 同 key 模板。

#### 2. description 渲染 IIFE 内加 fold 逻辑

```tsx
{(() => {
  const FOLD_THRESHOLD = 200;
  const FOLD_PREVIEW = 120;
  const key = `${catKey}::${item.title}`;
  const isLong = displayDesc.length > FOLD_THRESHOLD;
  const expanded = expandedMemDesc.has(key);
  const q = searchKeyword.trim().toLowerCase();
  const matchInDesc = q !== "" && displayDesc.toLowerCase().includes(q);
  const folded = isLong && !expanded && !matchInDesc;
  const shown = folded ? displayDesc.slice(0, FOLD_PREVIEW) + "…" : displayDesc;
  return (
    <>
      {renderContentWithTaskRefs(shown, refTaskMap, onRequestFocusTask)}
      {isLong && !matchInDesc && (
        <button onClick={...toggle expandedMemDesc...} style={...}>
          {folded ? `… 展开 (${displayDesc.length} 字)` : `收起 (${displayDesc.length} 字)`}
        </button>
      )}
    </>
  );
})()}
```

## 关键设计

- **200 字阈值 + 120 字 preview**：与 PanelTasks R91 同阈值，跨 panel UX 一致。
- **search hit 强制展开**：折叠态会让 keyword 高亮命中点在 120 字外不可见，搜索 UX 会破。matchInDesc 强制 unfold。
- **e.stopPropagation 防 row 级 onClick 误触**：行其它区域 onDoubleClick 进编辑；按钮 click 不该冒泡。
- **行内 button 不脱离 parent flex 行**：marginLeft 6 + verticalAlign baseline 让按钮"接"在文末。
- **isLong && !matchInDesc gate 按钮**：搜索命中时按钮也隐 —— 此时 description 全文已显，按钮无意义。
- **ref token + URL 渲染仍兼容**：renderContentWithTaskRefs 接受 `shown`（截断或全文），同样路径渲；折叠时尾 "..." 也走文本不破坏其它 token 解析。

## 不做

- **不持久化 expandedMemDesc Set**：fold state 是临时 inspect 视图；跨 session 默认折叠让 panel 启动时 clean。
- **不写测试**：纯 conditional render；视觉验证（一条 > 200 字 description item → 默认折叠 + 按钮 → click → 展开全文）足够。
- **不批量展开/折叠**：scope creep；single-item toggle 已够日常用例。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.15s
- 改动 ~65 行（state 3 + IIFE 改写 55 + 注释）。既有 displayDesc 计算 / renderContentWithTaskRefs / refTaskMap / inline edit 路径完全不动。

## TODO 状态

剩 3 条留池：
- ChatMini bubble 双击 ref + audio bell
- detail.md toolbar 加 "🧠 ask LLM about selection"
- detail.md 编辑器 ⌘K 唤起 task quick-find palette

## 后续

- ⌥+click 按钮展开 / 折叠所有同 cat items 一次操作。
- 折叠态末尾自动加 "…+N 字"hint 让 owner 知道还有多少没看。
- 搜索 hit 时 highlight 命中点 + scroll-into-view 让长 desc 也能精准定位。
