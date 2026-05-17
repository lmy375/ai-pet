# detail.md 📜 popover「📁 .history」Finder 打开按钮（iter #322）

## Background

iter #305 加 detail.md 自动版本历史 + iter #308 加 popover 直接 restore。
但 owner 想"cherry-pick 某个老版本 / 备份导出整个 .history 目录 / 自己用
diff 工具看几版"时只能：
- 心算/复制路径 `~/.config/pet/memories/<cat>/<file>.md.history`
- 走 Finder → 一级一级进 dir → 终于到目录

太多步。本迭代加 popover 头部「📁 .history」按钮 — 后端 `open <dir>` /
`explorer <dir>` / `xdg-open <dir>` 一键 reveal 跨平台。

## Changes

### `src-tauri/src/commands/task.rs`

- 新 Tauri 命令 `task_reveal_history_dir(title)`：
  - find_butler_task 取 detail_path
  - `history_dir_for(<mem_dir>/<detail_path>)` 算 .history dir
  - `!exists()` → 友好 Err "尚无历史快照（save 过 detail.md 后才会有）"
  - canonicalize + 验证仍落在 memories_dir（与 reveal_detail_in_finder
    同安全检查 — 防 `..` / 绝对路径越权）
  - macOS `open <dir>` / Windows `explorer <dir>` / Linux `xdg-open <dir>`
    打开目录本身（不是 `open -R` 选中文件 — 那是 file reveal 模式，
    本场景要直接进目录）

### `src-tauri/src/lib.rs`

- 注册新 invoke handler `commands::task::task_reveal_history_dir`

### `src/components/panel/PanelTasks.tsx`

- 📜 popover 头部从单 `<div>` 文案改为 flex row：
  - 左侧保留既有"📜 save 前快照…"文案（flex: 1 撑开）
  - 右侧加「📁 .history」mini button
- onClick: invoke `task_reveal_history_dir` + 失败 setBulkResultMsg 3s
  toast
- tooltip 显式说明用途：owner cherry-pick 历史文件 / 备份导出 / 自己 diff

## Key design decisions

- **`open <dir>` 而非 `open -R <file>`**：reveal 模式（-R）会在父目录选
  中某个 file —— 本场景 owner 想直接进 .history 目录看 5 份快照内容，
  不是"选中"任何一份。打开目录本身更直觉。
- **gate "尚无快照" Err**：history dir 不存在时给友好文案而非 IO 错。
  常见原因：任务从未 save 过 detail.md（detail 还是空 / 创建后没保存）
  / cap=5 trim 把所有旧版清光（实际不会 — trim 保 5 份）。前一种是 owner
  会遇到的合理状态。
- **canonicalize 安全检查**：与既有 `memory_reveal_detail_in_finder`
  同 pattern — 防 detail_path 含 `..` 或绝对路径让命令越权打开任意目录。
  必须 starts_with memories_dir 才算合法。
- **复用 history_dir_for（iter #305 pure helper）**：computing `.history`
  sibling dir 的算法集中在一处 — task_detail_history / snapshot_before_
  write / 本新命令都走同一 helper，未来调命名约定（如改为 `.versions/`
  或 `<name>.history` 改 prefix）只改一处。
- **按钮位置在 popover 头部右侧**：与下方 entry list 视觉分离 — entry
  是"某具体版本"维度，按钮是"整个目录"维度 — 信息层级不同。flex row +
  `flex: 1` 让头部文案撑满，按钮自然贴右。
- **不引入 unit test**：跨平台 Command::spawn 在 CI 环境难 stable mock；
  既有 memory_reveal_detail_in_finder 同型函数也未单测；通过 cargo
  check + vite build + 手动验证。

## Verification

- `cargo check`（backend）— clean，无新 error
- `cargo test --lib detail_history`（backend）— 9 passed / 0 failed
  （既有 detail_history 单测仍通过，未引入回归）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.20s)
