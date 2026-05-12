# PanelTasks 归档独立 tab

## 需求

任务归档原本藏在 PanelTasks 底部 collapsible (📦 归档 + 点击展开 + 数量)。
长队列 panel 滚到底才看得到，且与上方"队列"分不出层次。提到顶部 tab"队
列 / 归档"双标签让"我现在在看哪类"一眼可读。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 新 state `taskViewTab: "queue" | "archive"`，默认 "queue"
- 创建表单 section 下方插一行 tab 头：
  - 两个按钮：📋 队列 (N) / 📦 归档 (M)
  - active 下划线 2px accent，inactive muted；button:focus 视觉与既有
    PanelApp tab bar 同模式
  - 队列计数取活动态 `tasks.filter((t) => !isFinished(t.status)).length`
  - 归档计数：`archiveLoaded ? archiveItems.length : null`（未加载显
    "归档" 不带括号数字）
  - 点 📦 归档：`setTaskViewTab("archive")` + 若未 loaded 自动
    `setArchiveExpanded(true) + void reloadArchive()`
- 原 queue section 用 `{taskViewTab === "queue" && (...)}` 包裹整段（含
  chip 过滤 / 排序 / 任务列表 / 拖拽逻辑等所有内容）
- 原底部归档 collapsible 块 stripped 出来用 `{taskViewTab === "archive"
  && (<div style={s.section}>...)}` 包裹，作为独立 section 渲染
  - marginTop:0（不再需要顶部 dashed 分隔线）
  - 内部 header 仍保留 ▾/▸ 折叠按钮（用户可以临时折叠看上下文）

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 入页：默认 "队列" tab active，所有原 queue 视图完整呈现
  - 点 📦 归档 → tab 切换，自动 reloadArchive；归档列表显
  - tab badge 显数量（队列 N / 归档 M）
  - 切回 队列 tab → archive content 隐藏，队列视图恢复
  - 创建任务表单 / quick-add modal 在两个 tab 都可用（在 tab strip 上方
    渲染）
  - ⌘N quick-add 在两个 tab 都能开（事件挂在 window）

## 不在本轮范围

- 没让归档支持 search / 时间窗过滤：归档本就是只读回看视图，过滤不在主用
  例上；后续若有需要再加单独 chip
- 没做"归档项右键 restore"：把归档恢复到 butler_tasks 现役队列需要
  backend memory_edit 跨 category move，工程量大；本轮聚焦视图分离
- 没存 taskViewTab 偏好到 localStorage：用户多数情况都在队列，归档是临时
  回看；session 内即可

## TODO 池剩余

- ChatMini 桌面气泡 markdown 块级语法
