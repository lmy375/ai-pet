# PanelChat session list 显非当前 session 新消息 badge

## 需求

宠物在后台 proactive 走完一轮 → 把回应写进某个老 session（不是当前
open 的那个）→ 用户切回 PanelChat 时不会知道哪个 session 多了新消
息。session list 当前只显 title + (N 条) + 日期，没有"未读"信号。
加一个轻量未读 dot badge。

## 实现

`src/components/panel/PanelChat.tsx`：

### state + persist

- `sessionLastSeen: Record<sessionId, ISO timestamp>`，localStorage
  key `pet-chat-session-lastseen`
- `markSessionSeen(id)`：set 当前 ISO timestamp 到 lastSeen[id]，写盘
- 默认空 map —— 新用户首次启动不会一打开就满屏 badge

### 写入时机

- `loadSession(id)` 末尾 `markSessionSeen(session.id)` —— 用户点开
  session 即视为读到当前时间
- 新 useEffect 监听 `[sessionId, items]` —— 当前 session 收到新消息
  / 用户自己发了一条 → items 变 → 推进 lastSeen，避免自己正在看的
  会话误标 unread

### 读取（render badge）

session list 每行 title 前条件渲染小圆点：
- 仅在 `s.id !== sessionId`（不是当前会话）
- 且 `sessionLastSeen[s.id]` 存在（用户至少访问过一次）
- 且 `s.updated_at > sessionLastSeen[s.id]`（确实有新内容）
- 三个条件全满 → 8×8 圆点，accent 色，hover tooltip 显"上次访问时
  间 vs 现在 updated_at"

未访问过的 session 不显 badge —— 用户首次打开 panel，老 session
按字面排序展示，badges 只在"我读过 → 后来又更新"的明确语义下出
现，不打扰首次用户。

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 首次打开 panel，sessionLastSeen={} → 无任何 badge
  - 切到 session A → markSeen(A) → A 在 list 里不显 badge（它现在
    是 current 也不显）
  - 切到 session B（A 不再 current）→ 如果 A 后来被宠物 proactive
    更新 → A 现 badge
  - 在 session B 里收到 assistant 回复 → items 变 → useEffect 推
    进 B 的 lastSeen → B 不会因为它自己的新消息显 badge
  - 重启 panel → localStorage 还原 sessionLastSeen → badge 状态延续
  - 私密浏览 / quota 满 → setItem 抛 catch 静默 → session 内仍生效

## 不在本轮范围

- 没显具体未读数量（"3 条新"），只显 dot —— "几条新"需要后端给
  session item index 差量；当前 API 仅给 updated_at；本轮先 boolean
- 没做点击 badge 单独"标记已读"操作：用户点 session 行进入 →
  自动 markSeen 已经足够
- 没做"全部标已读"批量操作：高频场景少（< 10 session 时手动点过
  一遍即可）；session 数膨胀后再补
- 没做 background polling 自动检测 session 更新：list_sessions 调
  用是用户 toggle 下拉时触发的，不刷 list 就看不到新 badge；这与
  现有"用户 toggle 才看 list"的 UX 一致，不算 regression

## TODO 池剩余

- PanelChat 消息里「任务标题」hover 显该 task 当前 status + last_update
- PanelMemory butler_tasks 单条 item "▶️ 现在跑一次" 按钮（需要后端新增 per-item fire 命令，工作量适中）
