# 桌面 ChatPanel / PanelChat compose 缩略图条加 lightbox

## 需求

粘贴 / 拖入图片后，compose 区只能看 44×44（桌面） / 56×56（panel）缩略图 +
✕ 删除。**发送前**用户没法看清"我刚拖的是不是这张图"。点击缩略图弹 lightbox
看大图、看完关掉 → 自然预览路径。

不加 hover 📋 复制：缩略图本来就是用户刚 paste / drop 进来的源图，剪贴板 /
finder 里还有副本，复制按钮对 compose 区零增量价值；而且 44×44 已经被 ✕ 占
角，再叠 📋 视觉太挤。

## 实现

### 桌面 ChatPanel

`src/components/ChatPanel.tsx`：

- import `ImageLightbox`
- 加 `lightboxSrc: string | null` state
- 缩略图 `<img>` 加 `onClick={() => setLightboxSrc(src)}` + `cursor: zoom-in` + tooltip "点击查看大图"
- 根 `</>` 前挂一次 `<ImageLightbox src={lightboxSrc} onClose={() => setLightboxSrc(null)} />`

### PanelChat

`src/components/panel/PanelChat.tsx`：同样改造。`composeLightboxSrc` 名字与
`CopyableMessage` 内部自管的 lightbox 解耦 —— 后者只服务历史气泡的图，前者
服务发前预览，两条路径互不抢 state。lightbox 挂在 root `</div>` 前。

两边都复用上一轮做的 `ImageLightbox`，自带 Esc 关 + backdrop 关 + 📋 复制
（虽然 compose 场景复制需求低，但已经在 lightbox 里 free 提供了）。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 桌面：拖图 → 缩略图条 → click → 全屏大图 → Esc / 点暗背景 → 关
  - PanelChat：粘图 → 缩略图条 → click → lightbox → ✕ 删图 / 📋 复制 / Esc 关
  - lightbox 关后回到 compose 区，pendingImages 还在，按 Enter 还能发

## 不在本轮范围

- 没换成 ImageThumb 共享组件：ImageThumb 自带 hover 📋 角标会与 compose 区
  的 ✕ 删除按钮抢同角；保留独立 `<img>` 渲染让 ✕ / lightbox 各司其职
- 没做"批量预览缩略图"：用户一次粘 1-3 张是常见场景，逐张 click 即可
