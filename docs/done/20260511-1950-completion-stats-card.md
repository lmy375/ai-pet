# PanelTasks 完成任务统计小卡

## 需求

PanelTasks 队列 header 已有一行小字 `今日完成 X · 近 7 天 Y`（completionStats
useMemo 派生 done 任务，rolling 7×24h 窗口），但只是 read-only 文字。用户想
看"具体哪几条完成"得手动开 showFinished + 滚找。把它升级成可点小卡，展开
列出 title，点 title 直接跳到该行。

## 实现

### `src/components/panel/PanelTasks.tsx`

- `completionStats` useMemo 扩展：在 today / week 计数同时收集
  `todayList / weekList`（`{ title, ts }[]`，按 ts 降序）
- 新 state：
  - `completedListExpanded: boolean` —— 小卡展开 / 折叠
  - `pendingTitleFocus: string | null` —— 跨 render 定位 by title
- JSX：原 `<div>今日完成 X · 近 7 天 Y</div>` 改成 `<button>` + 下方
  position: absolute 浮窗：
  - 按钮 hover 提示 + ✅ 前缀 + ▾/▸ 箭头表展开态
  - 展开后浮窗按"今日"/"近 7 天（早些）"分两段，每段 title 列表（最长
    280px 高 + overflowY auto）
  - 每个 title 是 `<button>`，hover 加 `--pet-color-bg` 灰底；点 title 触发：
    - 清所有 filter（search / tags / due / priority）
    - `setShowFinished(true)` 让 finished 任务出现在 visibleTasks
    - `setPendingTitleFocus(title)`
    - close 浮窗
- 新 useEffect 消费 pendingTitleFocus：visibleTasks 重算后 findIndex →
  setFocusedIdx，触发既有 `[focusedIdx]` effect 的 scrollIntoView
- 新 useEffect outside-click + Esc 关浮窗（与既有 priority / status / ctxmenu
  picker 同模式）：setTimeout(0) 挂 mousedown 防"同次 click 既开又关"；
  popover 容器 `onMouseDown stopPropagation` 防内部 click 误关

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 入页：小卡显 ✅ 今日完成 N · 近 7 天 M ▸（M=0 时无箭头）
  - 点小卡 → 浮窗展开，分"今日 (X)" / "近 7 天（早些）(Y)"两段列 title
  - 点某 title → 浮窗关 + 清 filter + showFinished 切开 → 行滚到视口 +
    既有 focused outline 蓝色高亮
  - 点小卡外部 / Esc → 浮窗关
  - 浮窗内空白处点击不关（stopPropagation 拦截）
  - 长列表（一周内完成 20+ 条）→ 浮窗内部 overflow auto，外面不抖动

## 不在本轮范围

- 没在聊天 tab 顶加同款（TODO 描述提到"聊天 tab 顶部 / tasks 顶部"）：聊天
  tab 关注当前对话，不必让任务统计干扰；后续若长用户反馈再加
- 没做"按 priority 分组"：扁平 today / 近 7 天两段足够，再多分维度会让 panel
  滚得手忙
- 没做"周对比"（上周 vs 本周）：饼图 / 趋势会扩大成另一个 tab，本轮聚焦
  list & nav
- 没把 cancelled 计入：cancelled 是放弃，与"产出"语义冲突；保持只数 done

## TODO 池剩余

- PanelSettings 主题色 accent 自定义
- PanelMemory 一键导出 .md zip
- ChatMini ⌘F inline 搜历史消息
