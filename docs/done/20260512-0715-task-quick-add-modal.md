# PanelTasks ⌘N 全屏 quick-add 模态

## 需求

PanelTasks 顶部"新建任务"折叠表单展开后占大量垂直空间，把队列挤出视野。
加 ⌘N / Ctrl+N 全局快捷调出居中 modal 表单：用户填完一击即创建 + 自动
关 modal，队列视图永远完整可见。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 新 state `quickAddOpen: boolean`，与 createFormExpanded 并行（用户可单
  用模态 / 单用 inline 表单 / 同开都 OK）
- 复用既有 title / body / priority / due / creating / errMsg state：modal
  和 inline 是两个入口，state 是单一数据源；切换不丢草稿
- 新 useEffect 挂 window keydown：
  - ⌘N / Ctrl+N（无 alt / shift）→ preventDefault + setQuickAddOpen(true)
    + setTimeout(0) 让 modal 渲染后 focus titleInputRef
  - Esc + quickAddOpen → 关闭 modal（input focus 时也允许 Esc）
- `handleCreate` 成功后顺手 `setQuickAddOpen(false)` —— 创建即关，让用
  户立即看到队列里多出来的卡
- modal JSX：
  - position: fixed inset:0 + rgba 黑底 + 140ms 透明度淡入
  - 中心 card：520px maxWidth + 12px radius + 大阴影 + 180ms scale pop 动画
  - header 行 "⚡ 快速委托" + ✕ 关
  - 字段顺序：标题（autoFocus）/ 描述 / priority + due 两列
  - 底部按钮 row：创建任务 + 取消 + 右侧 "⌘Enter 创建 · Esc 关闭" hint
  - 错误文案在底部出（共享 errMsg state）
  - backdrop click 关闭、Esc 关闭、✕ 关闭三条路径

## 设计选择

- 同 titleInputRef 在 inline + modal 两处共用 OK：React 写入策略让最后渲
  染的元素掌握 ref；inline 在模态打开时虽然仍存在但行为退给模态；模态关
  闭后 React 重新 set ref 到 inline。focus 调用只在打开瞬间触发，无 race
- 不另起 createPortal：fixed inset:0 + 高 zIndex 已脱离正常文档流；引入
  Portal API 仅为 z-index 是 overkill
- 模态创建动画 180ms scale pop：与 macOS 系统弹窗节奏接近，不抢戏

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - PanelTasks tab 内按 ⌘N → 模态淡入；标题输入框自动 focus
  - 输入标题 + 描述 + ⌘Enter → 调 task_create → 模态关闭，列表多一条
  - 中途 Esc / 点背景 / 点 ✕ → 关闭，未提交的草稿仍在 state 里下次开还在
  - inline 折叠表单展开同时按 ⌘N → modal 也开（不冲突；state 共享）
  - 创建失败 → 模态内显红字 errMsg，模态不关
  - 全屏遮罩期间 panel 其它交互被 backdrop 拦截（点哪儿都关 modal）

## 不在本轮范围

- 没改全局快捷帮助层（KeyboardHelpOverlay）加 ⌘N 说明：那是另一处文档
  维护，本轮聚焦交互；后续可加进 / 弹起
- 没做"⌘N 在非任务 tab 时也唤起"：⌘N 监听在 PanelTasks 组件内，切到别
  的 tab 时该组件卸载 → 监听自然不挂；与"任务 tab 时是 quick-add"语义一致
- 没让模态记忆上一次输入的 priority / due（默认值）：state 共享意味着上
  次提交后会被清；用户填到一半切走再回来内容还在

## TODO 池

清空后按规则 #1 自主提出 5 条新需求。

## TODO 池新提案

1. PanelChat compose 拖入 .md / .txt 自动塞 textarea
2. ChatMini assistant bubble 单条"再回应"快捷
3. PanelTasks "now" 标记 + 桌面 nudge
4. PanelMemory 类目显示名重命名
5. PanelDebug 快照对比 diff
