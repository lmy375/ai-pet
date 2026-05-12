# ChatMini 96px 缩略图加快速复制

## 需求

桌面气泡的 96px 缩略图只能点开 lightbox 再复制 / 关 lightbox 再回桌面，比"hover → 📋 一键"多两步。常见路径是用户看完图想直接拖到飞书 / Notion；hover 出 📋 把这条路压到最短。

## 实现

### ImageThumb 自带 hover CSS injection

之前 ImageThumb 的 hover 表现依赖 PanelChat `<style>` 里的 `.pet-image-thumb:hover .pet-image-thumb-copy { opacity: 0.92 !important }` —— 这个 style 只在 panel webview 里生效，桌面 webview 没有，所以 ChatMini 不能直接复用 ImageThumb。

修：让 ImageThumb 自己注入这段 CSS：

```ts
let stylesInjected = false;
function injectStyles() {
  if (stylesInjected || typeof document === "undefined") return;
  stylesInjected = true;
  const tag = document.createElement("style");
  tag.dataset.petComponent = "ImageThumb";
  tag.textContent = `
    .pet-image-thumb:hover .pet-image-thumb-copy { opacity: 0.92 !important; }
    .pet-image-thumb .pet-image-thumb-copy:hover { opacity: 1 !important; }
  `;
  document.head.appendChild(tag);
}
```

Panel 与桌面是不同 webview window，各自独立 document —— 模块级 boolean 单 window 内就足够，每个 window 首次渲染 ImageThumb 时自己注入；跨 window 不共享（也不该共享）。

PanelChat 原来的 `.pet-image-thumb` 规则删掉一行注释代替（"由 ImageThumb 自己 inject"），DRY 修干净。

### ChatMini 用 ImageThumb 替换 inline `<img>`

`src/components/ChatMini.tsx`：

```tsx
{imgs.map((src, j) => (
  <ImageThumb
    key={j}
    src={src}
    onOpen={() => setLightboxSrc(src)}
    maxSize={96}
  />
))}
```

替代了原来 15 行的 inline `<img>` + onClick + style 块。ImageThumb 自带 hover 📋 + idle/done/err state + zoom-in cursor + lightbox 触发。`maxSize={96}` 让缩略图收到桌面气泡的紧凑尺寸。

## 验证

- `npx tsc --noEmit` clean
- 行为（桌面气泡）：
  - hover 96×96 缩略图 → 右上角浮 📋
  - click 图本体 → lightbox 弹出（与之前一致）
  - click 📋 → 短闪绿 ✓ → 飞书粘贴 → 出图
  - hover 移开 → 📋 fade out
- 行为（PanelChat 历史 / give_image）：与之前完全一致，CSS 注入只搬了位置，没变行为

## 不在本轮范围

- 没把 ChatPanel 的 44×44 compose 预览换 ImageThumb —— 已经在上一轮的 done 里说明：✕ 删图按钮和 📋 复制会抢同角，先保持 ✕ 独占

## 剩余 TODO

- PanelTasks 任务详情解析 image markdown 渲缩略图
- /image 历史 prompt 召回
