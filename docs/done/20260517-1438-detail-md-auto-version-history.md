# detail.md save 自动版本历史 + 📜 chip popover（iter #305）

## Background

owner 在 PanelTasks detail.md 编辑器 ⌘S 保存后没有撤销 / 还原入口 ——
意外覆盖整段内容（如误删全选、粘错文本、@ 触发自动补全失误）后只能凭
记忆重写。dirtySinceRef / cancelEditArmed 等 safety nets 都只防"在写到
一半丢失"，不防"已保存后的覆盖"。

本迭代加 detail.md 自动版本历史 — 每次保存前后端 snapshot 旧版到
`<detail_path>.history/<ts>.md`（cap=5）。前端 📜 chip 弹 popover 让
owner 一键复制某版到剪贴板回滚。

## Changes

### `src-tauri/src/detail_history.rs`（新文件）

- `HISTORY_CAP: usize = 5` 常量
- `history_dir_for(detail_path) -> PathBuf`：返 `<detail_path>.history`
  sibling dir
- `timestamp_now() -> String`：返 `YYYYMMDD-HHMMSS`（字典序 = 时间序）
- `snapshot_before_write(full_detail_path)`：读现版（不存在 / 空 → 直接
  return），mkdir 历史 dir，写 `<ts>.md`，自动 trim 到 cap。全 best-effort
  — 任一 IO 失败静默吞，不阻塞 caller 主路径
- `trim_history(dir, cap)`：list + sort lexicographic + drop oldest
- `list_history(full_detail_path) -> Vec<DetailHistoryEntry>`：按 ts 倒序
  返最多 HISTORY_CAP 份 (ts, content) 给 Tauri 命令用
- 9 个 unit test：history_dir / snapshot noop missing / noop empty /
  writes versioned / trim keeps cap newest / trim noop under cap / list
  desc order / list empty when missing / trim caps at HISTORY_CAP

### `src-tauri/src/commands/memory.rs`

- `memory_edit("update")` 在 `fs::write(&full_path, &content)` 之前调
  `crate::detail_history::snapshot_before_write(&full_path)`。覆盖全部
  category（butler_tasks / todo / ai_insights / task_archive / general /
  user_profile），不只 butler — detail.md 是所有 cat 共有概念

### `src-tauri/src/commands/task.rs`

- 新 Tauri command `task_detail_history(title) -> Vec<DetailHistoryEntry>`：
  通过 `find_butler_task` 拿 detail_path 后调 `list_history`。task 不
  存在 → Err；history 目录不存在 / 空 → Ok([])

### `src-tauri/src/lib.rs`

- 注册 `mod detail_history;`
- 注册 invoke handler `commands::task::task_detail_history`

### `src/components/panel/PanelTasks.tsx`

- 新 `DetailHistoryEntry` 接口（ts + content）
- 新 state：`historyEntries` / `historyPopoverOpen` / `historyCopiedTs`
- `refreshDetailHistory(taskTitle)` callback：invoke 后端命令 → setState
- effect on `editingDetailTitle` 切换：开新任务 → 拉一次；关闭 → 清状态
- `handleSaveDetail` 成功后 fire-and-forget `refreshDetailHistory` 让下次
  chip 点击显刚 snapshot 的新版
- 状态栏 ⏰ 编辑用时 chip 之后插 📜 chip（仅 historyEntries.length > 0
  时渲染）。点击 toggle popover；popover 列每条 ts (格式化 `MM-DD HH:MM:SS`)
  + 内容前 50 字预览；click 任一行 → navigator.clipboard.writeText(content)
  + 临时 ✓ 已复制 视觉反馈 2.5s
- popover 绝对定位 right=0 top=100% 跟在 chip 下方；z-index 100 避免被
  textarea / preview pane 遮挡

## Key design decisions

- **best-effort snapshot 不阻断主写路径**：safety net 性质 — snapshot
  失败时 owner 仍能保存。任一 fs::write / create_dir_all 失败都静默吞，
  主路径错误信息保持纯净。
- **复制到剪贴板而非自动 restore**：自动覆盖当前 dirty 编辑内容风险高
  （正在写新版的 owner 一点 "v2"→ 当前进度被强行 reset）。剪贴板路径
  让 owner 主动决策粘回 — 可选择全文粘 / 拷一段粘进当前文本，灵活且无
  破坏性。
- **cap=5 不可调**：5 份覆盖典型"几小时内多次 save 撤回"需求；过多让
  `.history` 目录吵杂 + 占磁盘（一份 detail.md 可能几 KB / MB），cap
  写死避免 owner 误配大数字。
- **TS 字典序 = 时间序**：`YYYYMMDD-HHMMSS` 格式让文件名 sort 直接
  = 时间序，不需 mtime / stat — 跨平台 / 跨文件系统都稳定。
- **覆盖全 category**：snapshot 在 memory_edit("update") 中央位置 hook，
  自动覆盖 butler_tasks / todo / ai_insights / task_archive / general /
  user_profile。当下只有 PanelTasks UI 暴露 chip；future iter 可在
  PanelMemory 复用同 task_detail_history 命令（或新建 memory 维度命令）。
- **同一秒内重复 save 文件名撞**：让后到的盖前面 — owner 不会感知
  1 秒内连点保存的细微差异；不引入 .1 / .2 后缀规避（会让 cap 计数失
  准 + 文件名不可读）。
- **lazy fetch + post-save refresh**：编辑器打开时拉一次 + 每次 save
  后再拉。不在每次 textarea onChange 都拉（list_history 是磁盘 IO，频
  繁拉无意义）。

## Verification

- `cargo test --lib`（backend）— 1138 passed / 0 failed（9 新
  detail_history 测试都通过）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.20s)
