# PanelChat 顶部「今日会话」chip

## 背景

owner 想一眼看到当天聊天活跃度。原 TODO 提的是"今日用户消息 N 条 + assistant M 条"—— 实现时发现 ChatItem 没有 per-message timestamp（消息时间戳不存在于 session items），无法逐条按日期过滤。降级到**会话级近似**：用 session 的 updated_at 字段判定，统计今日活跃过的会话数 + 它们累计 item_count。

## 改动

`src/components/panel/PanelChat.tsx`：在 🔍 搜索按钮和 📌 marks 按钮之间加 chip：

- 派生：今日本地日期 `YYYY-MM-DD` → 过滤 `sessionList` 里 `updated_at.startsWith(today)`
- 累加：`item_count ?? 0` 之和
- 渲染：`📅 N · M`（N 个会话 · M 条消息）
- 不交互（cursor default），title attribute 解释 caveat：
  - "今日活跃过 N 个会话（可能包含昨日开始的）"
  - "累计 M 条消息（含 user / assistant / tool / error，不含 system）"
  - "会话内单条消息没有时间戳，所以是会话级近似"
- 显隐：`sessionList.length === 0` 或 `todaySessions.length === 0` → 不渲染（启动空 / 当天无活动）

## 不做

- 不引入 per-message timestamp schema（要改 session.json 持久化格式 + 后端 + 历史迁移，超出本轮范围）
- 不区分 user / assistant / tool / error 分别计数（item_count 是 aggregate）
- 不持久化 / 不与 stats card 联动（独立 chip）

## 验收

- `npx tsc --noEmit` ✅
- 当天聊过 → chip 显示 📅 N · M
- 当天没动 → chip 不显（避免噪音）
- hover chip 看 caveat tooltip

## 完成

- [x] chip 派生 + 渲染
- [x] TODO.md 移除
- [x] 移到 docs/done/
