# PanelChat 输入框 `@` task ref picker

## 背景

已有 ⌘K modal task picker 走 fuzzy search → 插入 `「title」` ref token。
但 IM 用户的直觉是"敲 @"立刻看到一段下拉，与平时聊天工具一致。

## 改动

`PanelChat.tsx`：

1. 新增 helper `extractMentionContext(input, cursorPos)`：cursor 前向回扫到最近 `@`，命中（且 `@` 之前是 boundary）返回 `{ start, query }`，否则 null。遇到空白 / 角引号 / 起始非 boundary 时放弃。
2. 抽取 `fuzzyMatchTaskTitle(query, target)` —— 与 ⌘K modal 内 inline `fuzzyMatch` 同算法（char-order 子序列 + span*100+firstMatch 评分）。⌘K 入口内的 IIFE 暂未替换（不属本次改动 scope，跟随既有重构再说）。
3. 新增 state：`composeCursorPos` + `mentionSelectedIdx`。`mentionContext` / `mentionFilteredTasks` / `mentionMenuVisible` 用 useMemo 派生。
4. textarea：`onChange` / `onSelect` 双 hook 同步光标位置（IME / 鼠标点击都覆盖）。
5. `handleInputKeyDown` 加 mention 分支（顶部，优先级高于其它）：↑↓ 选 / Enter+Tab 选定 / Esc 退出（删掉 `@<query>` 段，避免下次 keystroke 重触发）。
6. `pickMention(title, ctx)`：把 `@<query>` 段替换成 `「title」 `（与 ⌘K 完全同形态）。
7. Render：与 SlashCommandMenu 同锚位（form relative + `bottom:100%`），空命中显友好提示。tasks 列表复用 `chatTaskMap`（挂载已刷新，每个 ref token render 也共用）。
8. Placeholder 文案补充 `@`。

## 不做

- 不在`@`触发时去 invoke `task_list` —— chatTaskMap 挂载已刷新；少一次 IO 抖动，且 picker 打开成本接近 0。
- 不与 slash menu 同时显示 —— `slashMenuVisible` 优先（input 开头为 `/`），mention 派生时直接 short-circuit。
- 不写 keyboard hint chip / docs page —— placeholder 文案足够发现。
- 不写测试 —— extractMentionContext 是 pure helper 但行为简单；端到端的 UI 编排也无明确"真实行为"可 pin（与 GOAL.md 的"不写没用的测试"一致）。

## 验收

- 在 input 中敲 `这条要参考 @整` → 浮窗出现，过滤含 char-order `整` 的任务。
- ↑↓ 选 + Enter → `@整` 被替换成 `「整理 Downloads」 `（含尾空格）。
- Esc → `@整` 整段被清掉，光标落回 `@` 之前。
- 句中 `email@example.com` 的 `@` 不触发（前一字符 `l` 非 boundary）。
- input 以 `/` 开头时 mention 不抢 slash 菜单。

## 完成

- [x] PanelChat.tsx
- [x] TODO.md 移除该行
- [x] 移入 docs/done/
