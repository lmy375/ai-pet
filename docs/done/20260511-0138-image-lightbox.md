# 聊天气泡图片支持点开放大

## 需求

聊天历史 / ChatMini / give_image 工具结果里的图片缩略图最大 160px / 96px，看不清细节。点开放大、Esc / 点背景关掉，是 web app 里图片的最低预期交互，目前没有。

## 实现

### 共享 Lightbox 组件

`src/components/common/ImageLightbox.tsx`（新文件）：

- props: `src: string | null` + `onClose: () => void`；src null = 不渲染
- 用 `createPortal` 挂到 `document.body`，避免被父级 `overflow: hidden` 切掉，z-index 9999 盖住所有 panel
- 黑底 0.85 透明度；点暗背景关闭，img 自身 stopPropagation 防误关
- Esc 键监听走 `useEffect` 在 src 非 null 时挂，null 时解绑
- `cursor: zoom-out` on backdrop / `cursor: default` on img 暗示交互
- 短淡入动画（fade-in 140ms）

### 三处接入点

各组件内部维护自己的 `lightboxSrc` state（lightbox 一次只显一张，不需要全局 store）：

1. `panelChatBits.tsx` `CopyableMessage`：用户 / 助手 bubble 内的图，onClick → setLightboxSrc(src)，缩略图 cursor 改 zoom-in
2. `ChatMini.tsx`：桌面气泡的 user / assistant 图同上；lightbox 挂在 `</>` 外层根
3. `ToolCallBlock.tsx`：give_image 等工具结果的 _attachments 图同上

每处的 `<img>` 加 `cursor: zoom-in` + `title="点击放大"` tooltip。

## 不在本轮范围

- 没做"上一张 / 下一张"键盘导航 —— 当前用户图集多是 1-2 张，方向键属于"图库"心智，超出 lightbox 职责
- 没做下载按钮（右键已自带"图片另存为"）

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 桌面 ChatMini 用户气泡里的图 → click → 全屏黑底 + 居中大图 → Esc → 关闭
  - PanelChat 历史 user / assistant 气泡 → 同
  - give_image 工具卡片里的缩略图 → 同
  - 多张图：每张都可单独点开
- 加载中的 panel 切 tab 时 lightbox 也跟着 unmount（state 跟组件生命周期一致）
