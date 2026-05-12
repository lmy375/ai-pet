# ChatMini 双击气泡进 panel

## 需求

桌面气泡的 ⛶ 按钮在右上角小尺寸。每次想"看完整历史 / 多会话"都要把鼠标精
确移到右上角点 ⛶。双击气泡本体打开 panel —— 用户视线已经在气泡上，距离最短。

## 实现

`src/components/ChatMini.tsx`：bubble div 加 `onDoubleClick={() => onOpenPanel?.()}`
+ tooltip 暗示。cursor 维持默认（不改成 pointer），保持气泡的"text-selectable"
心智 —— 单击仍能正常选词复制；双击才打开 panel。

`src/components/common/ImageThumb.tsx`：img 的 onClick / onDoubleClick 加
`stopPropagation`。否则用户快速双击图片想看 lightbox 时，事件冒泡到外层 bubble
触发"打开 panel"，造成"我只想放大图却切到 panel"的体验割裂。

ImageThumb 是共享组件 —— PanelChat / ToolCallBlock / ChatMini 都用。这两处之
前不依赖事件冒泡（CopyableMessage 父级 `pet-chat-row` 没有 onClick；
ToolCallBlock 的 image grid 在 header 之外的子 div），加 stopPropagation 不破
坏既有行为。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 桌面气泡上双击 → 立即开 panel
  - 单击仍能选文字 / 拖选 → 文本依然可选
  - 双击图片 → lightbox 打开（不连带打开 panel）
  - 双击图片旁边的文字 → panel 打开（图片 stopPropagation 不影响纯文本路径）
  - onOpenPanel 未传（极端 case）→ 无 tooltip + 双击 noop

## 不在本轮范围

- 没改 PanelChat / ToolCallBlock 的图像 hover 行为 —— stopPropagation 是新增，
  与现有 listener 兼容
- 没在桌面气泡加 cursor 提示 —— 单击的"文字选择"功能比"双击进 panel"高频，
  cursor 类型按高频路径选 default 不变；title tooltip 已足够发现性
