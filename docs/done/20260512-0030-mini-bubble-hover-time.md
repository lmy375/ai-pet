# ChatMini bubble hover 时间戳角标

## 需求

ChatMini 单条 bubble 已有 `title` 原生 tooltip 显示时间戳，但 OS-level tooltip
触发要 hover ~500ms 才出，且文字小。用户想"扫一眼就知道这条是几点发的"。
加一个 absolute 浮在 bubble 上方的 `[HH:MM]` 小标，行级 hover 即显。

## 实现

`src/components/ChatMini.tsx`：

- `MINI_CHAT_STYLES` 加一组：
  ```css
  .pet-mini-row .pet-mini-row-time { opacity: 0; transition: opacity 120ms; }
  .pet-mini-row:hover .pet-mini-row-time { opacity: 0.55; }
  ```
  比 `.pet-mini-row-copy`（hover 升 0.7）更弱，因为时间戳是"次要监控信息"，
  存在感不应抢复制 / 反馈按钮
- visibleItems.map 内：
  - 算 `timeLabel = formatBubbleTimestamp(m.ts)` + `hasValidTime = timeLabel !== "[?]"`
  - 无效 ts（老 session 加载回来缺 ts）→ 不渲染 badge 避免占位噪音
  - 渲染 `<span className="pet-mini-row-time">`：
    - position absolute, top: -12, 对齐方向那侧 8px（user 右 / assistant 左）
    - 9px monospace，muted 字，card 底，2px 圆角，4px 内 padding
    - pointerEvents none 不挡 bubble 自身事件
- 与既有 absolute 顶部 👍 反馈块（top: -4 right: 0 仅最新 assistant 行）错
  位：time 在 top: -12 + 不同 z-stacking，互不重叠

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - hover 历史 bubble → 顶角浮 `[14:32]` 小标 0.55 opacity
  - hover assistant bubble → 时间在 bubble 左上方（左对齐侧）
  - hover user bubble → 时间在 bubble 右上方（右对齐侧）
  - 老 session 缺 ts 的 bubble → 无时间小标（不显 `[?]`）
  - 最新 assistant 行已有 👍 在 top:-4 right:0 → 时间在 top:-12 左侧（左对
    齐 assistant），不冲突
  - streaming bubble（独立分支）→ 不显时间（streaming 仍在变，时间无意义）

## 不在本轮范围

- 没改 ChatMini 顶部"复制 N 条"菜单的"带时间戳"复选项：那是导出语义；
  hover 时间是 UI 阅读语义，正交
- 没在桌面气泡（ChatBubble，proactive 弹的）加同款：那种气泡 < 5s 即关，
  不需要"看时间"功能

## TODO 池剩余

- PanelChat session tab 栏右键菜单
- PanelMemory 单条记忆"打开外部 markdown editor"
- PanelTasks detail.md markdown 预览
- PanelDebug 工具风险 inline 调整
