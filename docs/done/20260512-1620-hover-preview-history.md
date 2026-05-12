# PanelTasks 任务卡 hover preview 加 "最近 3 条 history"

## 需求

iter #172 给任务卡加了 hover 500ms 浮 detail.md preview。但 detail.md
是"我做到哪了"的进度笔记，butler_history 里的事件流（create / update /
done / error）是"我什么时候做了什么"的时间线——两者互补。hover 时只
看 detail 缺时间维度。补 history。

## 实现

`src/components/panel/PanelTasks.tsx` 单文件：

### state 重构

- 删 `taskPreviewCache: Record<string, string>`（只缓存 detail.md 截断）
- 改用既存的 `detailMap: Record<string, TaskDetail>`（expand 路径同源
  缓存）—— hover 与 expand 双向复用，hover 后用户 expand 不重 fetch、
  反之亦然
- `startTaskPreviewHover` 改为 invoke `task_get_detail`（一次拿
  detail_md + history + 元数据），命中 detailMap 直接跳过
- 失败路径 silently 忽略（无 toast）—— hover 是被动行为，闪 error
  打扰；用户点 expand 时仍能看到详细错误

### tooltip render 重构

- 整个 tooltip 包在 IIFE：先算 `recentHistory = pd.history.slice(-3).reverse()`
  + `detailSnippet`（前端截 600 字符截断，与原 memory_read_detail 后端阈
  值一致），两段全空时不渲染 tooltip（避免空框）
- 上半段：🕒 最近 N 条事件 标题 + 每行 `timestamp action snippet`
  - timestamp 16 字符 "YYYY-MM-DD HH:MM"
  - action 用 accent 色突出（create / update / done / error 各动作）
  - snippet 单行 ellipsis（hover 是扫读，不让多行 snippet 撑爆 tooltip）
- 分隔：6px padding + 虚线 border-top
- 下半段：📄 detail_path 标题 + detail.md 内容（pre-wrap，与之前相同）
- maxHeight 260（之前 220，加了 history 行需要稍宽）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - hover 未 expand 任务 500ms → tooltip 浮，上半 history 三行 + 下半 detail.md
  - hover 已 expand 过的任务 → 立即浮（detailMap 命中，无网络/IO 等待）
  - history 为空 + detail 为空 → 不浮 tooltip（避免空框）
  - history 为空 + detail 有 → 只浮 detail 段（无 history 标题）
  - history 有 + detail 空 → 只浮 history 段（无 detail 段 + 无分隔）
  - 已 expand 任务 hover → 不浮（与原行为一致）
  - 切到归档 / 鼠标快速移动 → timer 取消，无 stale tooltip
  - 卡 hover 后点 expand → 详情区直接显（detailMap 已填）

## 不在本轮范围

- 没把 hover preview 抽成共享组件复用 PanelMemory：两边数据 shape 不同
  （memory 只有 detail.md，task 有 detail + history），抽组件需要
  generic 设计，性价比低
- 没在 tooltip 里渲染 detail.md 的 markdown（粗体 / 列表）：tooltip
  是扫读不是阅读，pre-wrap 纯文本 + 等宽字体已足够；要渲染走 expand
- 没显 history.action 配色（与 expand 视图 historyAction(action) 同套
  色板）：tooltip 简洁优先，单一 accent 色已能让用户区分 "ts / action /
  snippet" 三栏；要细颜色看 expand
- 没做 history.length > 3 时的 "… N more" 暗示：用户已经知道 hover 是
  preview，要看全部点 expand 即可

## TODO 池剩余

- PanelChat 消息里「任务标题」hover 显该 task 当前 status + last_update
- PanelMemory butler_tasks 单条 item "▶️ 现在跑一次" 按钮
- PanelChat session list 显非当前 session 自上次访问后的新消息 badge
