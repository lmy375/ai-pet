# ChatMini ambient hint chip click deep-link 到 PanelDebug（iter #395）

## Background

iter #383 加 ChatMini 顶部 ambient hint 行（📝 transient_note + ⏰
alarms + 🔇 mute），但 chip 只显数字 + hover 提示，无 click 入口跳
查看 / 编辑 → owner 想"点 chip 直接看详情"需切到 debug 窗手动滚。

本 iter 实现 chip click → deep-link 跳 PanelDebug 对应卡片（开 debug
窗 + 切 "应用" tab + scrollIntoView 锚点元素 + 短暂 flash 高亮）。

## Changes

### `src/DebugApp.tsx`

#### consumeDebugDeeplink 消费

新 `pet-debug-deeplink` localStorage 通道，shape `{ tab, scrollAnchor?,
ts }`，TTL 10s（防过期触发）。与既有 `pet-panel-deeplink` 同模板但
key 独立。

```ts
const consumeDebugDeeplink = useCallback(() => {
  // 读 localStorage + JSON.parse + TTL check
  // 若 tab 字段 → setActiveTab
  // 若 scrollAnchor 字段 → setTimeout 50ms 等渲染 → 找
  //   id=`pet-debug-anchor-<value>` → scrollIntoView({ behavior:
  //   smooth, block: start }) + 600ms accent flash 高亮
}, []);
```

两路径触发：
- mount useEffect：未开 debug 窗 → open_debug 后 DebugApp 首次 mount
- storage useEffect：已开 debug 窗 → pet 窗 setItem 触发跨窗口 storage
  event → 立即消费

### `src/components/panel/PanelDebug.tsx`

加 2 个 anchor id：
- `pet-debug-anchor-tone-strip` 包 PanelToneStrip wrapper（含
  transient_note + mute chips）
- `pet-debug-anchor-pending-reminders` 在既有 reminders block 上

### `src/components/ChatMini.tsx`

3 chips（📝 transient / ⏰ alarms / 🔇 mute）改 `<span>` 为
`<button>`：
- 📝 / 🔇 → tab="应用", scrollAnchor="tone-strip"
- ⏰ → tab="应用", scrollAnchor="pending-reminders"

onClick 写 localStorage + `invoke("open_debug")`；button 加 cursor:
pointer + border: none 保持原视觉 + 加 hover title 提示 "点击 → 打开
debug 窗 + 滚到 XXX"。

## Key design decisions

- **新 deeplink key `pet-debug-deeplink` 而非复用 pet-panel-deeplink**：
  避免与 panel 路径冲突 — panel 处理 panel 内 tab + dueFilter；debug
  处理 debug 窗 tab + scrollAnchor。命名空间分开让两 deeplink 独立演
  进。
- **TTL 10s + JSON 解析双重容错**：与既有 panel deeplink 同模板。owner
  click 后 10s 内开/未开 debug 窗都能触发；过期则 stale 不触发。
- **scrollIntoView 后 600ms accent flash**：让 owner 看到目标位置
  （单纯滚到位时容易看不出"哪条 chip / 哪段 card"已 land）。flash
  风格与 PanelMemory mem-flash / PanelChat chat-match 高亮一致。
- **setTimeout 50ms 等渲染**：mount 路径下 setActiveTab + 首次渲染
  完成需一帧；50ms 余量给 children 完成 first paint 防 getElementById
  null。已开窗路径同样需要这个 buffer（tab 切换 → re-render）。
- **button 模拟 chip 视觉**：cursor:pointer + border:none 让 chip
  形态保留；fontFamily/fontSize/inherit 确保字体一致。视觉上 owner
  看不出是 button — 但 keyboard tab + Enter 可激活（accessibility）。
- **3 chip 都 deeplink 到 "应用" tab**：所有 ambient 信号都在 PanelDebug
  "应用" tab；不需多 tab 路由。scrollAnchor 区分 tone-strip vs reminders
  两个具体卡片。
- **不为单 fn 引 unit test runner**：行为是 IO + DOM ops；build pass
  + 手测足够（chips click → debug 窗自动开 + 滚 + flash 三场景验）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 复用既有 `open_debug` Tauri 命令
