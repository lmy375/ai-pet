# detail.md 内嵌图片懒加载

## 背景

TODO 上 auto-proposed 一条："任务详情 detail.md 内嵌图片懒加载：IntersectionObserver 控制 `<img>` `src` 延后注入，长 detail 含 5+ 截图时滚动顺滑不卡。"

detail.md 内嵌图片通常是 base64 data URL（来自前一轮"任务详情粘贴图片自动压缩"的 256 KiB / 1600 px / JPEG 0.85 路径）。Native HTML `loading="lazy"` 只 defer 网络 fetch —— 对 data URL 无效，浏览器仍会在 `<img src=data:...>` mount 瞬间 eager decode 整张 base64 + 渲染。一条 detail.md 含 8 张 200 KB JPEG，打开 task 详情时 decode 1.6 MB base64 一次性发生在主线程，明显卡 paint。

IntersectionObserver-based lazy load 让 `<img>` 只在 wrapper 接近 viewport 时才 mount，把 decode 摊到滚动时机；首次 open 详情只 decode 第一屏可见的几张。

## 改动

### `src/components/common/ImageThumb.tsx`

新增 `lazy?: boolean` 可选 prop（默认 false，opt-in 不影响 ChatMini / 工具卡片等小集合场景）：

```tsx
const [shouldLoad, setShouldLoad] = useState<boolean>(!lazy);
const wrapperRef = useRef<HTMLDivElement>(null);

useEffect(() => {
  if (!lazy || shouldLoad) return;
  const el = wrapperRef.current;
  if (!el) return;
  if (typeof IntersectionObserver === "undefined") {
    setShouldLoad(true);  // 老环境 fallback
    return;
  }
  const obs = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          setShouldLoad(true);
          obs.disconnect();
          break;
        }
      }
    },
    { rootMargin: "300px 0px" },
  );
  obs.observe(el);
  return () => obs.disconnect();
}, [lazy, shouldLoad]);
```

未 load 时渲染占位 div：

```tsx
<div
  ref={wrapperRef}
  className="pet-image-thumb"
  style={{ position: "relative", display: "inline-block" }}
>
  {shouldLoad ? (
    <img src={src} ... />
  ) : (
    <div
      onClick={() => { setShouldLoad(true); onOpen(); }}
      title="懒加载中 — 点击立即加载并放大"
      style={{
        width: maxSize,
        height: Math.round(maxSize * 0.6),  // 16:10 横屏截图近似
        background: "color-mix(in srgb, var(--pet-card-bg) 70%, transparent)",
        border: "1px dashed color-mix(in srgb, var(--pet-fg-muted) 35%, transparent)",
        display: "flex", alignItems: "center", justifyContent: "center",
        cursor: "zoom-in",
        color: "var(--pet-fg-muted)",
        fontSize: 11,
        gap: 4,
      }}
    >
      <span style={{ fontSize: 18 }}>🖼</span>
      <span>懒加载</span>
    </div>
  )}
  {shouldLoad && <button ... 📋 复制 ... />}
</div>
```

### `src/components/panel/PanelTasks.tsx`

`parseDetailMdWithImages` 里 ImageThumb 加 `lazy`：

```tsx
<ImageThumb src={url} onOpen={() => onOpenImage(url)} lazy />
```

## 关键设计

- **opt-in 而非默认开**：ChatMini / give_image 工具卡片 / 既有聊天历史里的 ImageThumb 全部 ≤ 几张图，没有性能问题；强加占位反而 hurts UX。仅 detail.md 这种"可能含十几张高清截图的长 markdown"才需要。
- **rootMargin 300px**：距 viewport 300px（~1/3 屏）时就触发，让滚动到位前已 decode 完毕；体感"看到时已 ready"。再大浪费、再小肉眼看得到"加载中→图片"瞬间。
- **mount 后不卸载**：IO 命中后 `obs.disconnect()` —— 即便 wrapper 后续滚出 viewport 也保持 `<img>` 已挂载。避免来回滚动反复 decode + 闪烁。代价是内存占用，但 Tauri WKWebView 上 1.6 MB 图缓存可接受。
- **占位 16:10 比例**：未知真实图片尺寸 → 用 `maxSize × 0.6` 近似横屏截图比例。layout reservation 接近真值，加载完成时位移最小（避免 cumulative layout shift）。
- **点击占位 = 强制加载 + 打开 lightbox**：让用户能主动戳穿懒加载（即使屏外）。`onOpen()` 直接调 ImageLightbox —— lightbox 接 src 字符串本身可读 data URL，不依赖 thumb 实际 load 状态。
- **copy 按钮跟着 shouldLoad 渲染**：placeholder 阶段不显 📋 —— 视觉上"还没加载完的图不能复制"符合直觉；用户想复制就先点占位戳穿。
- **fallback to eager**：缺 `IntersectionObserver`（SSR / 老 webview）→ `setShouldLoad(true)` 直接立即加载。功能正确性优先于性能；当前 Tauri WKWebView macOS 上 IO 一定可用，这条只是双保险。

## 不做

- **不做 `loading="lazy"` HTML 属性**：对 data URL 无效，混淆设计。
- **不做"距 viewport N 屏外卸载 img"**：会让来回滚动反复 decode + 闪烁，得不偿失；内存占用比反复 decode 划算。
- **不写测试**：jsdom 下 IntersectionObserver / DOM layout 不真实，覆盖率有限；UX 验证才是关键。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.22s
- 改动 ~80 行（ImageThumb 70 + PanelTasks 1 + comment）；既有 ChatMini / 工具卡片调用点未传 lazy，行为不变。

## TODO 状态

5 条候选 auto-proposed 已完成 3 条，余 2 条留池：
- 任务行 hover detail 预览
- pinned 任务过滤 chip

## 后续

- 把"图片渲染数量"加到 detail 顶部状态栏（如 "（含 5 张图，已加载 2 张）"），让用户对懒加载进度心里有数。
- 配合 `prefers-reduced-motion`：当前已经无 transition；不必额外处理。
- 占位 click 立即加载后再不存 cache —— 若同一张图在多处出现（理论上仅 detail.md，但若以后多端共享），可考虑模块级 LRU dataURL → ImgBitmap 复用。
