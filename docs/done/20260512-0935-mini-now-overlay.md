# ChatMini hover NOW 任务浮窗

## 需求

上一轮加的 PanelTasks "⚡ 标 NOW" 60s 浮顶 + 桌面 nudge 是两次独立反馈
（mark 时一条、过期时一条）。中间 60s 用户在桌面不知道还有几条 mark
着、剩多久。在 ChatMini 顶部加 "⚡ NOW · N" 角标，hover 弹列表显 title
+ 倒计时。

## 实现

### `src/App.tsx`

- 新 state `nowTasks: Map<title, expiresAtMs>`
- 既有 `task-now-mark` 事件 listener 内：
  - 标记瞬间 `setNowTasks(prev => prev + {title, Date.now() + 60_000})`
  - 60s timer 内既 appendAssistant nudge，也 `setNowTasks(prev => prev - title)`
- 把 `nowTasks` 作为 prop 传给 `<ChatMini />`

### `src/components/ChatMini.tsx`

- Props 加 `nowTasks?: Map<string, number>`
- 新 state `nowOverlayHover: boolean` + `nowTick: number`
- useEffect: 仅当 `nowTasks.size > 0` 时启动 1s `setInterval(() => setNowTick(t+1))`，
  让倒计时数字真的动；列表为空时 interval 不开
- 渲染：top:14 / left:20 / zIndex:13 的橙底角标 "⚡ NOW · N"
  - 仅 `nowTasks.size > 0` 时显（与右上 ⛶ / 📋 / 🔍 错位不挤）
  - onMouseEnter / Leave 切换 `nowOverlayHover`
  - hover 时浮 320 max-width 弹层：每条一行 "title （右侧 Ns 倒计时）"
  - 倒计时 ≤ 15s 红字加粗，其余 muted gray，monospace + tabular-nums
- IIFE 内 `void nowTick;` 让 React 把 tick 计入依赖触发重渲（每秒 Date.now()
  重算所有 entry 的 secLeft）

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 没 mark 任何 NOW → ChatMini 顶部无角标，零视觉占用
  - panel 标 1 条 NOW → 桌面 mini 立刻出 "⚡ NOW · 1"
  - hover 角标 → 浮出列表，唯一一行 + "60s" / "59s" / ... 实时倒数
  - 倒计时进 15s → 数字变红 + 加粗
  - 标多条 → "⚡ NOW · 3"，hover 看 3 行（按 expiresAt 升序，即将过期的
    排前）
  - 过期 → 自动从列表移除；全 0 时角标消失 + interval 关停
  - 关 panel 不影响：mark 一次就独立 60s 在 pet 端

## 不在本轮范围

- 没让 hover 列表可点（跳到任务 panel）：mini 是桌面 quick read 视图，
  点击切 panel 是 ⛶ 按钮职责，重复给同一动作多入口反而混乱
- 没存 nowTasks 到 localStorage：60s 短暂状态；持久化会让陈旧 mark 缠绕
- 没做"鼠标悬停时暂停 tick"：interval 间隔仅 1s，不暂停也无性能问题

## TODO 池剩余

- PanelChat session ⑂ fork 按钮
- PanelMemory consolidate 进度 + cancel
- PanelDebug 统计窗口快速切换 1d / 3d / 7d
