# PanelMemory ⌘F 聚焦搜索 input

## 需求

PanelTasks 早就有 ⌘F / Ctrl+F 聚焦搜索 input 的快捷键（useTaskKeyboardNav
hook 内部），与 mac Finder / 浏览器 / Notion 直觉一致。PanelMemory 顶
部也有 search input 但没快捷键 —— 用户从 PanelTasks 切到 PanelMemory
后想搜还要鼠标点 input box。补齐 ⌘F 让两个 panel 行为对称。

## 实现

`src/components/panel/PanelMemory.tsx`：

- 新增 `searchInputRef: useRef<HTMLInputElement>(null)`，挂到顶部
  memory_search input 上（`ref={searchInputRef}`）
- 新增 useEffect 挂 `window` keydown 监听：
  - 条件 `(metaKey || ctrlKey) && !shiftKey && !altKey && key.toLowerCase() === "f"`
  - 触发：`e.preventDefault()` + `el.focus()` + `el.select()`
- placeholder 加 `（⌘F / Ctrl+F 聚焦）` 提示，与 PanelTasks search
  placeholder 同款 affordance

视野上没冲突：PanelApp 用 `activeTab === "记忆" && <PanelMemory />`
条件渲染，PanelMemory 与 PanelTasks 同时只挂一个，两个面板的 ⌘F
监听不会同时存在。

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 切到「记忆」tab → 按 ⌘F → search input 聚焦 + 已有内容全选
  - 输入 "整理" → Enter → memory_search 调用，命中渲染
  - 按 ⌘F 再次 → 焦点保持 search input，select 让用户能直接覆盖输入
  - 浏览器原生 ⌘F 不再弹（preventDefault 吃掉）—— webview 单页应用
    里原生 ⌘F 几乎无用，让位给搜索面板更直观

## 不在本轮范围

- 没加 `/` 单键聚焦：PanelTasks useTaskKeyboardNav 加了 `/` 是因为
  那 hook 已经分层处理 INPUT / TEXTAREA tag 守卫；PanelMemory 没这层
  hook，单独加 `/` 监听容易拦下用户在编辑 modal 里输入 `/`，本轮不
  扩 scope
- 没加 ⌘K 全局搜索（PanelTasks 是 ⌘K / ⌘F 双绑）：⌘K 在 PanelChat
  刚被 task ref picker 占用，记忆里再来一个 ⌘K 与 chat 行为不一致
  反而困惑；⌘F 一个就够

## TODO 池剩余

- PanelChat 消息里「任务标题」hover 显该 task 当前 status + last_update
- PanelTasks 任务卡 hover preview tooltip 加 "最近 3 条 history" 行
- PanelMemory butler_tasks 单条 item "▶️ 现在跑一次" 按钮
- PanelChat session list 显非当前 session 自上次访问后的新消息 badge
