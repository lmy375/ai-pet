# detail.md 大纲浮窗 active heading 高亮

## 背景

TODO 上 auto-proposed 一条："detail.md 大纲浮窗 active heading 高亮：IntersectionObserver 监听 preview pane scroll 位置，自动高亮『我在哪节』对应的 outline item。"

近一轮 ship 的 📑 大纲浮窗已能列 H1-H3 + click 跳节。但 owner 在长 detail.md 里滚阅读时，大纲并不告诉"你现在看到哪节"—— 用户得自己心算"这是第几节，哪个 outline item 对应"。

IntersectionObserver 监听所有 heading 元素，把"最靠上可见"那个映射到大纲对应 item 加 tint bg + 加粗，让"我在哪节"成 ambient 反馈。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### state + IntersectionObserver effect

紧贴 `detailOutlineOpen` state 之后：

```ts
const [activeHeadingCounter, setActiveHeadingCounter] = useState<number | null>(null);

useEffect(() => {
  if (!detailOutlineOpen || !editingDetailTitle) {
    setActiveHeadingCounter(null);
    return;
  }
  if (detailViewMode === "edit") {
    // edit 模式没 preview pane 渲染 heading（id 不存在）
    setActiveHeadingCounter(null);
    return;
  }
  // 扫所有 heading id (pet-detail-<title>-h<N>)
  const prefix = `pet-detail-${editingDetailTitle}-h`;
  const elements: HTMLElement[] = [];
  let counter = 1;
  while (true) {
    const el = document.getElementById(`${prefix}${counter}`);
    if (!el) break;
    elements.push(el);
    counter += 1;
  }
  if (elements.length === 0) return;

  const visibility = new Map<number, number>();
  const obs = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        const m = entry.target.id.match(/-h(\d+)$/);
        if (!m) continue;
        const n = parseInt(m[1], 10);
        if (entry.isIntersecting) {
          visibility.set(n, entry.intersectionRatio);
        } else {
          visibility.delete(n);
        }
      }
      if (visibility.size === 0) return;
      // 取最小 counter（emit 顺序最早 = DOM 最靠上）作 active
      let minCounter = Infinity;
      for (const k of visibility.keys()) {
        if (k < minCounter) minCounter = k;
      }
      if (Number.isFinite(minCounter)) {
        setActiveHeadingCounter(minCounter);
      }
    },
    {
      rootMargin: "0px 0px -70% 0px",
      threshold: [0, 0.1, 0.5, 1],
    },
  );
  for (const el of elements) obs.observe(el);
  return () => obs.disconnect();
}, [detailOutlineOpen, editingDetailTitle, editingDetailContent, detailViewMode]);
```

#### outline item active 样式

map 改 arrow body 计算 `isActive`：

```tsx
{headings.map((h) => {
  const isActive = h.counter === activeHeadingCounter;
  return (
    <button
      style={{
        background: isActive ? "var(--pet-tint-blue-bg)" : "transparent",
        color: isActive ? "var(--pet-tint-blue-fg)" : "var(--pet-color-fg)",
        fontWeight: isActive ? 600 : 400,
        // ...
      }}
      onMouseOver={(e) => {
        if (isActive) return;  // active 时不让 hover 覆盖 tint
        e.currentTarget.style.background = "var(--pet-color-bg)";
      }}
      onMouseOut={(e) => {
        if (isActive) return;
        e.currentTarget.style.background = "transparent";
      }}
      title={`跳到「${h.text}」（H${h.level}）${isActive ? " · 当前节" : ""}`}
    >
      ...
    </button>
  );
})}
```

## 关键设计

- **`rootMargin: "0px 0px -70% 0px"`**：把 IntersectionObserver 观察区缩到视口顶部 30%。语义："只有 heading 进入顶部 30% 才算 active"。
  - 防止视口尾部多个 heading 同时算 active（在长 page 滚到底时 5 个 heading 同时可见的情况）。
  - 顶部 30% 与人阅读"我正在看哪节"的注意力位置一致 —— 用户的视线焦点通常在视口偏上。
- **取最小 counter 作 active**：DOM 中越早 emit 的 heading 越靠上，counter 越小（与 parseMarkdown 内部 counter 一致）。"最靠上的可见 heading" = active 是 IDE / 文档站通用 pattern (MDN / GitBook / docs.rs 都这样)。
- **`detailViewMode === "edit"` 短路**：edit 模式 textarea 不渲染 heading 元素（没 id），observer 拿不到东西。短路省 IO + 显式 null active 不让 stale 旧值闪一下。
- **`editingDetailContent` 入 deps**：用户改内容（增删 heading）时 observer 自动重建。重建是 `obs.disconnect()` + `new IntersectionObserver` —— `O(N)` 成本，N <= ~50 typical detail.md，每次 keystroke 都跑也无感知。要更省的 deferred update 可加 debounce，但当前 v1 不必。
- **active 时跳过 hover 覆盖**：active 的 tint blue 是强信号；hover 覆盖到 `pet-color-bg` 会让"我在哪节"瞬间消失。owner 移光标到 active 项上时仍要看到它高亮。
- **title attr 加 "· 当前节" 后缀**：让 hover tooltip 也反映 active 状态，提升 affordance。

## 不做

- **不用 `root: previewPaneEl`**：preview pane 在桌面 panel 内不一定独立 scroll，传 root 还要 ref-track 元素 + 处理 split 双 pane（两个 preview）。默认 viewport root 在 Tauri webview 下行为正确 + 简洁。
- **不防 debounce / throttle**：observer 重建是 ~O(50) 同步操作；50 个元素的 disconnect + observe 在 1ms 内完成。owner 打字的 keystroke 频率 (~10/sec) 远低于此成本。
- **不写测试**：IntersectionObserver 在 jsdom 下需要 polyfill；既有 lazy-load 图片 / Esc / ⌘O 等同模式的 keydown / observer 路径都视觉验证。手动测：长 detail.md 含 5+ heading，滚 preview → 大纲高亮跟随滚动 → 视觉确认。
- **不让大纲自动滚动到 active item**：当前大纲 panel maxHeight 200px + overflowY auto，超长 outline 用户得自己滚找。可以加 active.scrollIntoView({block: "nearest"}) 让 active 自动滚进可见区，但本 iter 简化不做（owner 平时大纲只 5-15 节，少超 200px）。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.24s
- 改动 ~80 行（state + useEffect 60 + outline map 改写 + isActive 样式分支 20）；既有 detail.md 编辑器 / parseMarkdown / 既有大纲 click-to-scroll 路径完全不动。

## TODO 状态

empty —— 6 条 auto-proposed 全部完成。下次启动 TODO 流程进入 auto-propose 分支。

## 后续

- active item 自动 `scrollIntoView({block: "nearest"})` 让长大纲（10+ 节）的 active 始终可见。
- "我滚到 active heading 时"的浮窗自动暂时高亮 1.5s + fade —— 让 active 切换更有动态反馈感。
- 同款 IntersectionObserver 模式扩到 PanelChat 长 session（message list 中跳 N 条前 / N 条后等 quick-jump 时 active 高亮）—— 但 chat 消息没 outline 概念，价值有限。
