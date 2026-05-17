# PanelMemory item action row「📜 历史快照」按钮（iter #335）

## Background

iter #305 ship 了 detail.md 自动版本历史 — memory_edit("update") 中央
hook 给所有 category 留 `.history` 快照。PanelTasks 已有 📜 popover
（iter #305 + #308 restore + #322 Finder 按钮）；PanelMemory 端没对应
入口 —— owner 想查 general / ai_insights / 等其它 cat 的 detail history
只能去 Finder 自己找 `.history` 目录。

本迭代加 PanelMemory item action row 📜 按钮 + popover，与 PanelTasks 对
偶（list 历史 + click copy + Finder 直达）。

## Changes

### `src-tauri/src/commands/memory.rs`

- 新 helper `resolve_safe_detail_path(detail_path) -> Result<PathBuf,
  String>` 抽出与 `memory_reveal_detail_in_finder` 同 pattern 的安全检
  查（禁 `..` / 绝对路径；caller 后续 canonicalize）— 复用给两个新命令
- 新 Tauri 命令 `memory_detail_history(detail_path) ->
  Vec<DetailHistoryEntry>`：与 `task_detail_history` 对偶但 category-
  agnostic — 走 detail_path 直接索引而非 task title 查找
- 新 Tauri 命令 `memory_reveal_history_dir(detail_path)`：与
  `task_reveal_history_dir` 对偶但 category-agnostic

### `src-tauri/src/lib.rs`

- 注册两新 invoke handler

### `src/components/panel/PanelMemory.tsx`

- 新 `PanelMemoryHistoryEntry` 接口 + state：
  - `historyPicker: {catKey, title, detailPath} | null`
  - `historyEntries: PanelMemoryHistoryEntry[]`
  - `historyCopiedTs: string | null`（短时✓反馈）
  - `historyBusy: boolean`
- outside-click / Esc close（与既有 moveCatPicker / reminderQuickPicker
  pattern 同源）
- `openHistoryPicker(catKey, title, detailPath)` async callback：invoke
  `memory_detail_history`
- 在 item action row 既有「📋 复制 detail.md 全文」之后插「📜」按钮：
  - 仅 `item.detail_path` 非空时渲染
  - click toggle popover；popover 内列每条快照（与 PanelTasks 同视觉：
    ts 格式化 MM-DD HH:MM:SS + 内容前 50 字 preview）
  - 头部右侧 mini button「📁 .history」一键 reveal 目录
  - busy 时显"拉历史中…"；空时显"尚无历史快照"友好兜底
  - click entry → clipboard.writeText(content) + 临时 ✓ 已复制 2.5s
    视觉反馈

## Key design decisions

- **走 detail_path 而非 (category, title)**：解耦 frontend 调用面与
  backend memory item 查找 — caller 已经有 detail_path 字段，直接传入
  更轻量。也让 task_detail_history（butler-only）/ memory_detail_history
  （任意 cat）两路径并存而不互相纠缠。
- **不引入 inline restore**：PanelMemory 没像 detail editor 那样的 inline
  textarea — restore 语义在这里不适用（owner 看 history 是为了"知道改
  过啥 / 拷回去手动粘"，不是"撤销")。click 复制就够。
- **`📁 .history` mini button 在头部右侧**：与 PanelTasks 📜 popover
  iter #322 同 layout — 让两端 UX 完全对偶，owner 跨 panel 不必学新交互。
- **抽 `resolve_safe_detail_path` helper**：旧 `memory_reveal_detail_in_
  finder` 安全检查代码块直接复制了两遍（reveal_in_finder + detail_abs_
  path），本次新加两命令前抽出公共逻辑避免第 3 / 第 4 份拷贝。
- **不引入 unit test**：后端两个命令是薄包装层；下层 detail_history::
  list_history / scan_history_disk_usage / history_dir_for 等 pure helper
  已有完整单测（iter #305 / #328），新命令的 happy path 通过 cargo
  check + frontend 真实交互验证。
- **busy / empty 兜底 state**：与 PanelTasks 同 polish — popover 打开
  时 sync UI 不会出现"看似空但其实在 fetch" 的歧义状态。

## Verification

- `cargo check`（backend）— clean
- `cargo test --lib`（backend）— 1208 passed / 0 failed（未引入新单测；
  既有 detail_history 13 个测试仍通过）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.22s)
