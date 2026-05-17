# PanelMemory 顶部「📊 cat 总览」chip 横条（iter #410）

## Background

PanelMemory 默认按 cat 段竖向排，owner 想"哪些 cat item 多 / 字符
占用大"扫读不便 — 需折叠所有段、逐段看 badge 数。"长尾 cat"分布
不直观（owner 不知道某条 ai_insights 比 butler_tasks 总字数多吗
之类）。

本 iter 加顶部横条 chip strip：每非空 cat 一个 chip，显
`<label> <items 数> · <总字符数>`。chip click 自动展开该 cat 段
+ scrollIntoView 滚到位 — 既是 audit 视图又是 quick navigation
入口。

## Changes

### `src/components/panel/PanelMemory.tsx`

#### 1. chip 横条 JSX（紧贴 Search 行之前）

```tsx
{index && (() => {
  type CatStat = { key, label, items, chars };
  const stats: CatStat[] = [];
  // 按 CATEGORY_ORDER + fallback 顺序遍历，空 cat 跳过
  for (const k of [...CATEGORY_ORDER, ...other_keys]) {
    const cat = index.categories[k];
    if (!cat || cat.items.length === 0) continue;
    let chars = 0;
    for (const it of cat.items) {
      chars += Array.from(it.description).length;     // unicode 字符数
      chars += detailSizes[it.detail_path] ?? 0;       // detail.md 字符数
    }
    stats.push({ key: k, label: cat.label || k, items: cat.items.length, chars });
  }
  if (stats.length === 0) return null;
  // fmtChars: ≥ 10000 用 "1.2k 字"；否则 "N 字"
  return (
    <div style={chipStripStyle}>
      <span>📊 总览</span>
      {stats.map(st => (
        <button onClick={...scroll-to-cat...}>
          <span>{st.label}</span>
          <span>{st.items} · {fmtChars(st.chars)}</span>
        </button>
      ))}
    </div>
  );
})()}
```

#### 2. chip click 行为：展开 + scrollIntoView

```ts
setExpandedCategories(prev => {
  const next = new Set(prev);
  next.add(st.key);
  localStorage.setItem("pet-memory-expanded-cats", JSON.stringify([...next]));
  return next;
});
setTimeout(() => {
  const el = document.querySelector(`[data-memory-cat="${st.key}"]`);
  if (el instanceof HTMLElement) {
    el.scrollIntoView({ block: "start", behavior: "smooth" });
  }
}, 50);
```

setTimeout 50ms 让 React 把展开态渲完再 query DOM — 防 section 内
容还没挂出 `querySelector` 落空。

#### 3. section header 加 `data-memory-cat={catKey}` 属性

让 chip click 的 querySelector 命中目标。属性名与既有
`expandedCategories` localStorage key 协议同源（`pet-memory-expanded-cats`），
避免引第二条标识协议。

## Key design decisions

- **复用 detailSizes 缓存而非新 IO**：`memory_detail_sizes` 已在
  loadIndex 时拉过 — 每 item.detail_path 对应 unicode 字符数；chip
  strip 渲染零 IPC，纯前端聚合
- **chars 包含 description + detail.md**：owner 关心"这个 cat 占
  用多重"是整体感知，分开两个数字反而读不出 — 加总更直觉
- **`Array.from(s).length` 取 unicode 字符数**：JS `string.length`
  是 UTF-16 code unit 数（emoji / surrogate pair 算 2）；用 spread
  cast 到 code point — 与 backend `chars().count()` 同语义
- **空 cat 跳过**：与既有 size chip / disk usage chip 同模式 — 没
  数据的 cat 显 "0 · 0 字" chip 是视觉噪音
- **horizontal flex-wrap**：典型 5-8 个 cat（CATEGORY_ORDER 6 + 自
  定义 1-2），横条放得下；窄屏 wrap 到第二行也优雅
- **不显占比百分比**：跨 cat 比较时百分比有意义但加列让 chip 拥
  挤；owner hover chip 看 tooltip 即可对比
- **click chip 展开 + 滚动而非仅滚动**：owner 看 chip 点进去通常
  想看内容；折叠态滚到位仍要展开是双步骤，自动展开是更短路径
- **不为单 chip strip 引 unit test**：纯渲染 + 聚合算法已有边界
  条件（空 cat / 空 detailSizes / 缺 cat key）；build pass + 手测
  足够（折叠所有 cat → 看 strip → 点 butler_tasks chip → 看自动
  展开 + 滚到 section → 验数字正确性可对照 hover badge）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 复用 memory_detail_sizes（既有 IPC）
