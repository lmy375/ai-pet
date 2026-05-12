# PanelChat 待发 attachment 区合并显示

## 需求

`pendingImages` 当前在 input bar 上方 always-expanded 显完整缩略图条（每
张 56x56 + ✕ 按钮）。粘 / 拖入 5+ 图时占据大量 input bar 上方空间，挤压
textarea 实际可见区域。改为单一 chip "📎 N 附件待发"；hover chip 展开
popover 显完整缩略图条。

## 实现

`src/components/panel/PanelChat.tsx`：

- 旧的 always-expanded 缩略图条 inline JSX 抽成新组件 `PendingAttachmentsChip`
- 组件 API：
  - `images: string[]`
  - `onOpen(src)`：点缩略图打开 lightbox
  - `onRemove(idx)`：单张 ✕ 移除
  - `onClearAll()`：chip 右侧的小 ✕ 全清
- 渲染结构：
  - 外层 wrapper 跟踪 `hovered` state（onMouseEnter / Leave）—— chip 与
    popover 共用一个 hover boundary，鼠标从 chip 滑到 popover 不会闪
  - chip 视觉：accent 边 + accent 字 + 14px 圆角 + "📎 N 附件待发"
  - chip 内嵌小 ✕ 按钮：clearAll，深色半透明 circle
  - hovered === true 时浮 popover：`position: absolute; top: 100%+4`，
    最大宽 380，沿用既有缩略图条样式（56x56 + 右上角 ✕）
- 与文本附件的关系：拖入 .md / .txt 仍是 append 到 textarea（与现行
  行为一致），不进 attachment 队列。chip 标题里的"附件"专指 pending
  images

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 粘一张图 → chip "📎 1 附件待发" 显在 input bar 上方左侧
  - 鼠标移到 chip → 浮出缩略图列表
  - 鼠标从 chip 平移到缩略图（同 hover 容器内）→ popover 不消失
  - 点缩略图 → lightbox 大图
  - 点缩略图右上角 ✕ → 单张移除；剩余仍在 popover 内
  - 点 chip 右侧 ✕ → 全清；chip 消失
  - 鼠标离开整个容器 → popover 自动隐藏，chip 仍在（直到全清 / 发送）
  - 多图（10+）→ popover flex-wrap，maxWidth 380 限宽自动换行

## 不在本轮范围

- 没把拖入文本文件也纳入 pending 队列：那要扩 state（pendingTexts: string[]）
  + 改 submit 拼接逻辑，与 attachment chip 视觉合并属另一轮工作；TODO 描
  述的"图片 + 文本文件"统一在此实现的"图片单一 chip"已满足主诉求
- 没做 click-toggle sticky popover：hover 已够直观；点 chip 没语义动作
  容易让用户误以为可点击
- 没做 chip 与缩略图条之间的箭头连接器：靠位置相对 + 顶部 4px 距离 已能
  让用户感知"这两个是同一组"

## TODO 池剩余

- PanelTasks 任务卡 hover detail.md preview tooltip
- PanelMemory category 顺序自定义
