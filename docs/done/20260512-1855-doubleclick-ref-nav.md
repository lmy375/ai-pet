# PanelChat 双击 `「task title」` ref → 切 tab + 聚焦任务卡

## 需求

iter #178/#182 给 chat 加了 `「title」` ref token + hover 显 status
tooltip。下一步自然 affordance：双击 = 跳到该任务卡。让 ref token 从
"信号" 升级为"导航"，把 chat 与 PanelTasks 之间的回路打通。

## 实现

### 跨组件 state lift（PanelApp）

`src/PanelApp.tsx`：

- 新 state `pendingTaskFocusTitle: string | null` —— 跨 tab-switch 续传
  焦点意图的 source of truth
- `requestFocusTask(title)`：set pending + setActiveTab("任务")，把"切
  tab"与"指定 focus"两步打包成单一原子动作
- `<PanelChat />` 多传 `onRequestFocusTask={requestFocusTask}` prop
- `<PanelTasks />` 多传 `pendingFocusTitle` + `onConsumeFocus` 两个 prop

### PanelChat → ref span 双击

`src/components/panel/PanelChat.tsx`：

- `PanelChatProps` 加 `onRequestFocusTask?: (title: string) => void`
- 函数 destructure 加 `onRequestFocusTask`
- 两条 `<CopyableMessage>` 调用（user + assistant 分支）都加
  `onRefDoubleClick={onRequestFocusTask}`

`src/components/panel/panelChatBits.tsx`：

- `CopyableMessage` props 加 `onRefDoubleClick?: (title: string) => void`
- 传给 `renderContentWithTaskRefs(content, taskRefMap, onDoubleClick)`
- 该 helper signature 加 `onDoubleClick?: (title) => void`
- ref span：
  - cursor 从 "help" 变 "pointer"（仅当 callback 传入）
  - title tooltip 末尾追加 "\n\n双击跳到任务面板该卡片"（让用户知道
    可以双击；不传 callback 时不显此行）
  - onDoubleClick 命中 → `e.stopPropagation()` + 调 callback
  - 未在 taskMap 命中的 ref 也保留双击行为，title 文案改成"仍尝试跳
    到任务面板搜索此 title"（PanelTasks 找不到就静默 noop）

### PanelTasks 消费 prop

`src/components/panel/PanelTasks.tsx`：

- `PanelTasksProps` interface 加 `pendingFocusTitle?` + `onConsumeFocus?`
- 函数 destructure
- 新 useEffect 监听 `pendingFocusTitle`：非空 → 桥接进既有
  `pendingTitleFocus` state（共用现有 setFocusedIdx + scrollIntoView 路径，
  不重复实现）→ 调 onConsumeFocus 清空 prop

### Race 处理

PanelTasks 在 activeTab 切换那一刻才挂载（条件渲染）。如果用 CustomEvent
之类 transient 信号，事件在 PanelTasks mount 之前就 dispatch 完了，会
丢失。lift state 到 PanelApp 解决：
- 第一次 render：PanelApp setState → setActiveTab 触发 re-render
- 第二次 render：activeTab="任务" → PanelTasks 挂载 + 收 pendingFocusTitle prop
- useEffect 跑 → 消费 prop → onConsumeFocus 清 state

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 在 chat 消息正文有 `「整理 Downloads」` → 单击 hover 显 tooltip 含
    "双击跳到任务面板"
  - 双击 → 切到「任务」tab，自动滚到该任务卡 + 焦点高亮（既有 R94
    focus outline）
  - 任务被 filter 隐藏（如切了"仅进行中"但任务是 done）→ findIndex
    fail → 静默 noop，不滚不闪
  - 任务已归档 / 重命名（taskMap miss）→ ref 仍 underline muted 色，
    双击仍尝试跳但 PanelTasks 找不到 → noop
  - 双击行内任何文本 / 复制按钮 → stopPropagation 阻止 chat 行级冒泡
  - cursor 仅在传 onRefDoubleClick 时显 pointer（设计期就声明 affordance）
  - 同一 title 多次双击 → state 每次都清 → 重新 set → 重新滚（不需手
    动重置）

## 不在本轮范围

- 没做"双击后短暂高亮闪一次"（独立于既有 focus outline）：现有 outline
  + scrollIntoView 已经够直观；额外动画收益边际
- 没做"自动清掉 filter 让目标可见"：filter 是用户主动选择，自动改可
  能颠覆其它意图；tooltip 已经在 hover 时提示"任务可能不在当前过滤
  下"
- 没做反向跳（PanelTasks → chat）：当前没有 task→chat 的引用语义；
  与 ref token 单向流入 chat 一致
- 没做键盘 Enter 双击替代（用户按 Tab 焦点到 ref + Enter 跳转）：ref
  span 当前不可 tab focus（无 tabindex）；做的话 cursor / aria-role
  也要补，scope 翻倍

## TODO 池剩余

空。下一轮需自主提需求。
