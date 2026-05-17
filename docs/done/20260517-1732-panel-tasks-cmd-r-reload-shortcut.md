# PanelTasks ⌘R 立即刷新 task list 快捷键（iter #317）

## Background

PanelTasks 当前刷新 task list 有两条路径：
- 30s 自动 setInterval（既有 nowMs tick，PanelTasks 不在那个 tick 拉
  task_list，需手动）
- 用户主动点 [...] 某动作时各 handler 内部触发的 reload

owner 想"刚 LLM 后端新建了 task / 改了状态，我立即想看更新"只能等 30s
被动 / 触发某动作让 reload 顺便发生。本迭代加 ⌘R / Ctrl+R 一键立即
拉新 task list，与 mac 浏览器 / Slack / Discord 的"⌘R = 刷新"直觉一致。

## Changes

### `src/components/panel/useTaskKeyboardNav.ts`

- 加新 arg field `handleReload: () => void`
- 加对应 ref + sync effect（与既有 7 个 handler ref pattern 一致）
- 在 ⌘F / ⌘K 块之后（仍在 tagName 守卫**之前**）加 ⌘R / Ctrl+R 分支：
  - 命中 `(e.metaKey || e.ctrlKey)` + key=='r' + 无 shift / alt
  - `e.preventDefault()` 吃浏览器默认"刷新整页"行为 — Tauri webview
    真重载会让 panel state 全丢（搜索框 / 焦点 / 展开 / 编辑器 dirty 内
    容等），必须拦
  - 调 `handleReloadRef.current()`
  - 跨 input context 工作（owner 在搜索 / 创建表单输入时也想能按 ⌘R
    看后端变化 — 与 ⌘F / ⌘K 同跨 input 行为）

### `src/components/panel/PanelTasks.tsx`

- 新 `handleReloadShortcut = useCallback(...)` 包既有 `reload`：
  - `setBulkResultMsg("⌘R 刷新中…")` 给即时视觉反馈
  - finally 后 `setBulkResultMsg("✓ 已刷新")` + 2s 清除
  - reload 失败时既有 setErrMsg 路径仍跑，不重复反馈
- `useTaskKeyboardNav({...})` 调用补 `handleReload: handleReloadShortcut`

## Key design decisions

- **跨 input context 工作（放在 tagName 守卫之前）**：与 ⌘F / ⌘K 同 —
  refresh 是全局动作不该被 input focus 限制。owner 在搜索框打字时按
  ⌘R 也该刷新（与浏览器 / Slack 行为一致）。⌘D 复制 title 不同：那
  个是 row-context 操作，必须有焦点行才有意义。
- **preventDefault 必须**：Tauri webview 默认 ⌘R = 重载整个 webview，
  会让 PanelTasks state 全丢（焦点 / 搜索 / 展开 / 编辑器 dirty 内容）。
  实测前端开发时 ⌘R 确实会重载 dev server — 拦住才能换成"刷数据 + 保
  state"。
- **toast `刷新中… → ✓ 已刷新`**：reload 通常 < 100ms 完成 — 没有可见
  视觉变化时 owner 会怀疑"按了没效果"。先 setBulkResultMsg 立即反馈，
  完成后切到 ✓ 标记。即使数据无变化 owner 也确认快捷键生效了。
- **无 shift / alt 修饰才响应**：⌘⇧R 留给未来扩展（VS Code 风格 "重启
  整个 panel"等）/ ⌘⌥R 同。modifier cluster 保留扩展空间。
- **不在 reload 内嵌 toast**：reload 是 useCallback 被多处调用（初次
  load / cancel 后 / retry 后 / 等）；嵌入 toast 会让所有调用方都拿到
  "已刷新" — 噪音。仅 ⌘R 入口包一层 toast 让反馈 scoped。
- **不复用既有的 30s tick interval**：30s tick 只更新 nowMs（"最近更
  新"绿点过期判断），不拉 task_list。是另一种"时间感知"维度，与刷新
  数据语义正交。
- **无 unit test**：键盘事件单测在 jsdom 难 stable mock（既有 d / r /
  p / ⌘D 也无单测）；行为通过 vite build + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.22s)
