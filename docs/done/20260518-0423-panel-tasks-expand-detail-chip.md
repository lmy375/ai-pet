# PanelTasks 行加「↘ 展开 detail」chip（iter #451）

## Background

PanelTasks 行点击 row header 展开 detail.md 段（含 description / 进度
笔记 / 历史时间线）。但当 row 处于 viewport 中段时，展开后 detail 段
向下扩展超出 viewport，owner 要手动滚屏才能读到。常见 UX 损耗：
点 → 等加载 → 手动滚，三步。

本 iter 加 hover-only chip「↘ 展开 detail」 — 一键完成「展开 +
滚 row 头顶到 viewport top」两动作，让 detail 段紧随展开就在视区
可读。已展开时 chip 自动隐藏（动作语义无意义）。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. 紧贴 📂 detail size chip 之前插 ↘ 展开 detail chip

```tsx
{taskPreviewHoverTitle === t.title && !expanded && (
  <button
    onClick={(e) => {
      e.stopPropagation();
      void handleToggleExpand(t.title);
      // 双 rAF 等 React commit + paint 完成；scrollIntoView 滚 row
      // 头顶到 viewport top。detail 段紧随 row 渲染于可视区。
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          const el = document.querySelector(`[data-task-idx="${idx}"]`);
          el?.scrollIntoView({ block: "start", behavior: "smooth" });
        });
      });
    }}
    title={`一键展开「${t.title}」detail 段 + 滚动到顶端 — 省「点 row → 等加载 → 手动滚」三步。`}
    style={{
      fontSize: 10,
      padding: "0 5px",
      marginLeft: 6,
      border: "1px dashed var(--pet-color-border)",
      borderRadius: 3,
      background: "transparent",
      color: "var(--pet-color-muted)",
      cursor: "pointer",
      fontFamily: "'SF Mono', 'Menlo', monospace",
      lineHeight: 1.5,
      verticalAlign: "middle",
      whiteSpace: "nowrap",
    }}
  >
    ↘ 展开 detail
  </button>
)}
```

设计：
- **`taskPreviewHoverTitle === t.title && !expanded` 双门控**：复用既有
  500ms hover state（与 📂 / ↗ refs / 📊 sparkline chip 同节奏）+
  `!expanded` 避免已展开时显「展开」按钮（语义矛盾 → 自动隐藏）
- **复用 `handleToggleExpand`**：与 row click 同路径（fetch detail +
  setExpandedTitle + 更新 lastview）。一处实现避免 drift
- **双 rAF + querySelector(`[data-task-idx="…"]`)**：React commit
  setExpandedTitle 后第 1 rAF 后 React 完成 commit；第 2 rAF 后浏览器
  完成 paint，DOM 可滚。`block: "start"` 把 row 头顶贴到 viewport
  top，detail 段紧随其下渲染于可视区。`behavior: "smooth"` 让 UX 流畅
- **detail 异步加载也兼容**：detail_get 在 handleToggleExpand 内 await
  task_get_detail；即便 detail 内容还在 loading，row 头顶已滚到 top —
  loading state UI 在可视区，加载完后内容顺延填充 row 下方，无需
  re-scroll
- **样式与既有 chip 一致**：`1px dashed border + fontFamily monospace +
  marginLeft 6` 与 「📂 字」「↗ refs」「📊 sparkline」同 chip family
  视觉语言

## Key design decisions

- **复用 taskPreviewHoverTitle 500ms 门控（而非新增 hover state）**：与
  既有 📂 / ↗ / 📊 chip 同节奏 — owner 在 row 上停留 500ms 才整段 hover
  chip 出现，避免快速划过时闪烁噪音
- **已展开时藏 chip**：与「点 row」语义一致：再点 row 是折叠；保 chip
  只表达「展开 + 滚」单方向语义。如果 chip 在已展开时显「折叠」会让
  按钮语义来回切换，owner 心智成本增加
- **双 rAF 而非 setTimeout(0) / 单 rAF**：单 rAF 在某些场景下 paint
  还未完成（layout 数据 stale）；双 rAF 是 React + 浏览器 paint 都完
  成的稳妥时机。setTimeout 比 rAF 慢且 frame-unaligned，鼠标视觉感
  受会有 ~30ms 跳动。这种"完成后做事"模式是 React + DOM 操作标准
- **`block: "start"` 而非 `"nearest"` / `"center"`**：start 把 row 头
  顶贴到 viewport top，detail 段直接占据下方主视区 — 这是「读 detail」
  最理想布局。nearest 不动（如 row 已在 top 就不滚）；center 把 row
  推到屏幕中央，detail 反而只占下半屏。start 一致性最好
- **不写 unit test**：纯 DOM scroll 副作用 + render condition；逻辑
  trivial 但 jsdom scrollIntoView mock 不实在；GOAL.md "meaningful
  tests only" 规则下不引装饰性测试。`tsc` + `vite build` clean 即够
- **stopPropagation 防 row click bubble**：chip 是 button，本就吃掉
  click 不冒到 row 的 click handler；但保 e.stopPropagation 显式更稳
  （防未来 row 加 capture-phase handler 抢键）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.31s)
- 后端无改动 — 纯前端 UI 增强
- 手测：PanelTasks 中段 row hover 500ms → 看「↘ 展开 detail」chip 浮
  出 → 点击 → row 头顶滚到 viewport top → detail 段紧随渲染于可视
  区；已展开 row hover 时 chip 不显（自动隐藏）
