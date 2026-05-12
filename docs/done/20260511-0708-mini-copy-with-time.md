# ChatMini 复制最近 N 条加"带时间"开关

## 需求

ChatMini 顶部 📋 弹框现在导出 `🧑 ...\n\n🐾 ...`，缺时间信息 —— 用户复盘
"今天下午 3 点宠物说了啥"时只能看见对话顺序。加一个 checkbox，启用后每条加
`[HH:MM]` 前缀。

## 设计

消息时间戳并不在现有协议里 —— useChat 的 ChatMessage 只有 role + content。
新发 user / assistant 消息时印 ISO ts；旧 session 加载回来的没 ts，时间显
`[?]` 让用户知道"这条来自历史，没法准估时间"，而不是凭空填一个 now()
误导。

## 实现

### useChat 印 ts

`src/hooks/useChat.ts`：

- ChatMessage interface 加 `ts?: string`，注释解释字段可选 + 后端 ChatMessage
  serde 不 deny_unknown_fields 所以多带 ts 字段 JSON 往返不破坏
- sendMessage：user message + 完成时的 assistant message 都印 `new Date().toISOString()`
- proactive-message listener：优先用 payload.timestamp（后端 chrono RFC3339）；
  缺失才 fallback now

### ChatMini 消费

- ChatMessage interface 同步加 `ts?`
- 新 state `copyIncludeTime`
- `formatTime(ts)`：parseable ISO → `[HH:MM]`；undefined / 解析失败 → `[?]`
- `copyRecentN` 拼前缀时根据 `copyIncludeTime` 加 `[HH:MM] glyph` 或纯 glyph
- popover 底部加 checkbox row（border-top 分隔与 N 选项段），勾选状态自身保持
  跨调用（用户偏好"我每次都要时间"会被记住直到关 panel）

### 持久化

state 只在组件生命周期内：刷 panel 重新打开则复位。用户偏好不放进 localStorage
是简化决策；如果用户多次反馈"每次都要勾"再加 localStorage 同步。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 新发消息 → ⌘+C 一条仍 work（不走带时间路径）；popover 选 5 条 → "🧑 xx\n\n🐾 yy"
  - 勾"带时间戳" → 选 5 条 → `[15:23] 🧑 ...\n\n[15:24] 🐾 ...`
  - 老 session 加载回来 → 勾时间戳后那些条显 `[?] 🧑 ...` 提示

## 不在本轮范围

- `[HH:MM]` 只到分钟精度：消息间隔通常以分钟为粒度，秒级噪音多。需要秒粒度
  的用户可以自己 ISO export（PanelChat 已经有 Copy MD）
- popover 没显示当前消息总数 → 选 N 时 N > 现有不报错（slice(-N) 自动截到实际
  长度）。如果用户经常想知道"我目前有 N 条历史"再加

## TODO 池剩余

- PanelTasks 任务卡片拖拽调 priority
- /image -n 局部成功失败混合反馈
