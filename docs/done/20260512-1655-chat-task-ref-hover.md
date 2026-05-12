# PanelChat 消息「task title」hover 显 status + last_update

## 需求

iter #178 给 PanelChat 加了 ⌘K task 引用选择器，把 `「title」` 插入
正文。但 ref token 是 dead text —— 用户翻几天前的聊天，看到 `「整理
Downloads」` 不知道这条 ref 现在还是不是"活"任务（可能已完成 / 归档 /
重命名）。hover 显当前 status + 最近更新时间，让 ref 在聊天里仍然是
"活"信号。

## 实现

### `src/components/panel/panelChatBits.tsx`：

- 新 helper `renderContentWithTaskRefs(content, taskRefMap)`：
  - 正则 `/「([^「」]+)」/g`（严格匹配 ⌘K picker 插入格式的全角直角引
    号，不抓普通文本里偶发的半角 ASCII 引号 / 类似符号）
  - 每段命中 → 渲一个 `<span>` 带 dotted underline + native `title=`
    属性（拼 `「title」\n状态：xxx\n最近更新：YYYY-MM-DD HH:MM`）
  - 未在 taskMap 命中（任务归档 / 重命名）→ underline 走 muted 色 +
    tooltip 提示 "已完成归档 / 被重命名 / 不存在"
  - 段落间空段调既有 `parseUrls` 渲染（不打破 URL 蓝色下划线 / 多模态
    路径）
  - 全无 ref 命中（out.length === 0）退到 parseUrls fast path 不浪费
    Fragment 渲

- `CopyableMessage` 新增 optional prop `taskRefMap?: Record<title,
  {status, updated_at}>`：
  - bubble 内文本渲染三段优先级：
    1. `highlightKeyword`（搜索结果，最优先）
    2. `taskRefMap` 非空 → renderContentWithTaskRefs
    3. parseUrls 默认
  - 三档互斥避免渲染冲突 / 重复计算

### `src/components/panel/PanelChat.tsx`：

- 新 state `chatTaskMap: Record<title, {status, updated_at}>`
- `refreshChatTaskMap()`：async invoke `task_list`，把 tasks 拉成 map
  - 失败时保留旧 map（不退化所有 ref 为 muted "已归档"提示）
- 挂载 useEffect 刷一次
- `openTaskPicker` 改造：fetch 完后顺手 set 一份 chatTaskMap（与 picker
  数据同一份，避免双 fetch）
- 把 `taskRefMap={chatTaskMap}` 传给 user + assistant 两条 CopyableMessage
  分支

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 用 ⌘K 插入「整理 Downloads」→ 发送 → 消息渲出 dotted underline；
    hover → native tooltip 显状态 + 最近更新
  - 翻历史会话里旧的「整理 Downloads」消息 → 同样能 hover（chatTaskMap
    挂载即加载）
  - 任务已完成归档 → ref 仍 underline 但走 muted 色，tooltip 提示
    "已归档 / 重命名 / 不存在"
  - 跨会话搜索高亮命中后 → 取 keyword 高亮路径，不并行渲 task ref
    （高亮优先级最高）
  - 再次 ⌘K 打开 picker → chatTaskMap 同步刷新 → 消息里所有 ref 状态
    immediate 更新

## 不在本轮范围

- 没做"双击 ref 跳转到 PanelTasks 该任务卡"：与 hover preview 不冲突，
  可作下一轮独立改进
- 没做实时 watch task_list 变化（pet 完成任务时自动更新）：openTaskPicker
  + mount 两个 trigger 已覆盖主要 UX；后续可挂 Tauri 事件监听
- 没集成 keyword 高亮与 task ref 同时渲：搜索 + ref 同时出现的概率低，
  keyword 路径独立工作即可；如需可后续抽 segment merge
- 没让 ref token 在状态变化时改 underline 颜色（pending=蓝 / done=灰 /
  error=红）：本轮先用 accent 单色 + tooltip 文本，避免无故彩色化所有
  ref；后续可按需扩
- 没做 client-side title fuzzy 匹配（容错重命名后的旧 ref）：保守，
  让 muted 提示明确告诉用户"这条 ref 过期了"，比误命中相似名错任务安全

## TODO 池剩余

- PanelMemory butler_tasks 单条 item "▶️ 现在跑一次" 按钮（需后端新增
  per-item fire 命令，工作量适中，独立 iter）
