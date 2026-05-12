# ChatMini 双击进面板的过渡视觉

## 需求

桌面 mini chat 双击 → 调 `open_panel` 后用户进了面板，但 panel 的 session
header 没有任何"我桌面聊的就是这条"的视觉反馈 —— 用户得肉眼读 session
title 确认。在 session bar 上加 1.2s 黄底脉冲，让"刚从桌面跳过来"语境连续。

## 实现

### `src/App.tsx`

- `openPanel()` 在调 `invoke("open_panel")` 之后做两件事：
  1. `emit("pet-focus-from-mini", { ts })`：panel 已开时即时触发 listener
  2. `localStorage.setItem("pet-focus-from-mini-ts", ts)`：首次开 panel 时
     listener 还没挂上，事件会丢；用 ts 兜底
- emit / localStorage 都 try/catch 包 —— 失败不影响 invoke 主流程

### `src/components/panel/PanelChat.tsx`

- 新 state `focusFromMiniPulse: boolean`，1.2s 自动落回
- 挂载 useEffect：
  - 读 localStorage ts；如果 `Date.now() - ts <= 3000` → trigger pulse；
    无论命中与否读后都 remove key（避免 stale 时戳重复触发）
  - `listen("pet-focus-from-mini", trigger)` 注册，return 时 unlisten
- 既有 `<style>` 块加 keyframes `pet-session-bar-focus-pulse`：
  - 0%: 黄底 + box-shadow 0 0 0 0 rgba(250,204,21,0.45)
  - 60%: 同黄底，shadow 扩到 6px 0 alpha
  - 100%: transparent + shadow 收回
- `.pet-session-bar-pulse` class 仅在 focusFromMiniPulse 时挂；用 box-shadow
  + background 不改 layout 维度，避免脉冲抖动周边元素
- `@media (prefers-reduced-motion: reduce)` 退化为常亮 1.2s（仍由 React 控
  制 unmount）

## 为什么用 localStorage 兜底

Tauri `emit` 是即时事件，订阅者必须已挂载才能收。openPanel 首次调用时
panel 的 webview 才开始建，React 挂 listener 至少要等 DOM ready —— 此时
事件已发出去落地了。

不用 sessionStorage：sessionStorage 是 per-window 隔离的，跨窗口看不到。

不写后端 emit 通道：纯前端就能解，避免 Rust 改动。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - panel 已开（任意 tab）：桌面双击 mini → panel session bar 黄底脉冲
    1.2s（即时）
  - panel 未开：桌面双击 → panel 新建 → 挂载 → 读 localStorage ts → 同样
    脉冲 1.2s
  - 用户 3s 后才切到 chat tab：ts 已过期，无脉冲（避免长 stale 触发）
  - panel 单纯重启 / 切 tab 不触发 mini emit → 无脉冲
  - 减弱动画系统偏好下：黄底常亮 1.2s 然后由 React 卸 class

## 不在本轮范围

- 没让 panel 强制切到 chat tab：用户切 tasks 看任务时不希望被打断；脉冲只
  做"信号"不做"导航"。若后续用户反馈"我想自动回到 chat"，可在 emit
  payload 加 `forceTab: 'chat'`，PanelApp 监听切 active tab
- 没复用 highlightedItemIdx / 现有 session row hover 样式：那两个是行级 +
  hover 态，sessionBar 是全行级单触发，独立 keyframe 更清晰
- 没改 mini 自身的视觉：双击 → panel 开是用户主动行为，mini 端不必额外反馈

## TODO 池剩余

- PanelTasks 卡片"按住拖拽改 priority"
