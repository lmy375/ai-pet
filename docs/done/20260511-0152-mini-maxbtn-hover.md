# 桌面气泡 ⛶ 按钮加 hover 反馈

## 需求

ChatMini 右上角 ⛶ "在面板里打开" 按钮目前完全没有 hover 反馈：颜色 / 边框 / size 全恒定。用户没法立刻判断它"是个能点的按钮"还是"装饰"，影响 affordance。

## 实现

`src/components/ChatMini.tsx`：

- MINI_CHAT_STYLES 加 `.pet-mini-maxbtn` CSS 类：
  - transition: transform / border / shadow / color，120ms ease-out
  - :hover：scale(1.12) + 边框 / 文字换 var(--pet-color-accent) + box-shadow 加深
- 按钮加 `className="pet-mini-maxbtn"`，inline style 不动 —— 基态完全不变；只 hover 时新 CSS 类生效

transform 比改 width / height 便宜（不触发 layout，仅 compositor 层）；border-color / color 走 accent token，light / dark 主题自动适配。

## 不在本轮范围

TODO 描述"与 PanelChat 的 ⛶ 风格统一" —— grep 后发现 PanelChat 没有 ⛶ 按钮，描述中的对比是错的。本轮就改 ChatMini 的；PanelChat 的 session 列表 dropdown 切换 / 关闭已经有自己的 hover 风格，不再扩。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - hover ⛶ → 0.12s 内放大 12% + 边框 / icon 变 sky 蓝 + 阴影加深
  - 移开 → 平滑回原位
  - light + dark 都走主题 accent token，深色下也能看清
