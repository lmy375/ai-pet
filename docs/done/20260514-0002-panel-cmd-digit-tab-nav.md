# PanelApp `⌘1` – `⌘5` 跳 tab

## 背景

PanelApp 顶部有 5 个 tab（设置 / 聊天 / 任务 / 记忆 / 人格），切换全靠点。Slack / Linear / Chrome / Notion 都早就有 `⌘<digit>` 跳到第 N 个 tab/section 的肌肉记忆 —— 我们没有。

`?` 帮助层「Panel 全局」section 现在只有 `?` 和 `Esc`，加上 tab nav 后用户能快速在密集 panel 工作流里跳。

## 改动

`src/PanelApp.tsx`：

- 新增一个全局 `useEffect(keydown listener)`：
  - 修饰键：`metaKey || ctrlKey`，且 `!shiftKey && !altKey`（避免与未来组合冲突）
  - 键名：`"1"` – `"5"`，对应 `TABS[idx-1]`
  - **守门**：如果 `e.target` 是 `INPUT` / `TEXTAREA` / `contenteditable=true` 元素 → return（让用户在输入框里继续用 ⌘1（多数浏览器没绑） / ⌘C 等）
  - 命中 → `e.preventDefault()` + `setActiveTab(TABS[idx-1])`
- 注释里说明：与 Chrome ⌘1 jump tab、Slack ⌘1 jump channel 同模式

`src/components/panel/KeyboardHelpOverlay.tsx`：

- 「Panel 全局」section 追加一行：`["⌘1", "⌘2", "…", "⌘5"]` → `跳到对应 tab (设置 / 聊天 / 任务 / 记忆 / 人格)`

## 不做

- 不支持 `⌘0`：5 个 tab 用满 1-5，0 留作未来"回主页"或"重置 filter"语义
- 不限定平台（mac/win/linux）：⌘ 与 Ctrl 同时识别，Tauri webview 在 macOS 用 metaKey、在 Windows / Linux 用 ctrlKey，两个 OR 一致
- 不写测试：纯 keydown 路由，与 ⌘L focus / `?` 帮助层快捷键同类型逻辑

## 验收

- `npx tsc --noEmit` ✅
- 在面板「记忆」tab 按 `⌘2` → 跳到「聊天」；`⌘3` → 「任务」；`⌘5` → 「人格」
- 在 PanelChat textarea 输入时按 `⌘2` → **不切**（守门生效），用户继续打字
- `?` 帮助层第一栏多了一行 `⌘1-⌘5  跳到对应 tab`

## 完成

- [x] PanelApp.tsx keydown listener
- [x] KeyboardHelpOverlay 文档行
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
