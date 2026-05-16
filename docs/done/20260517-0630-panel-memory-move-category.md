# PanelMemory item「🏷 改类目」按钮（iter #259）

## Background

owner 整理 memory 时常发现条目"归错类" —— `user_profile` 里塞了一条 general
的随笔，或 `general` 里写了一条更适合 `user_profile` 的偏好事实。当前唯一改
分类的路径：在源类目删除 → 在目标类目"新建"重写描述 + detail。

本迭代加跨 category 移动按钮 + 后端 `memory_move_category` 命令，让 owner
一键挪条目。同时为避免镜像表（butler_tasks / todo / ai_insights / task_archive）
状态错乱，restrict 只支持两端都是非镜像 category（如 general ↔ user_profile）。

## Changes

- `src-tauri/src/commands/memory.rs`：新增 `memory_move_category(title,
  old_category, new_category)` tauri 命令。
  - 前置校验：源 cat 存在 + 含 title / 目标 cat 存在 + 无 title 冲突 /
    两端都非镜像 / 不是 current_mood
  - 计算新 detail_path（target_cat 子目录 + title_to_filename + 碰撞加 `_N`）
  - 创建 target dir → 移文件（rename 或 写空文件兜底）→ index.items
    remove + push + 更新 detail_path / updated_at
  - 失败前 ensure 不动 state（pre-validation 全部通过才 mutate）
  - 同 category 视为 noop（`Ok("No change.")`）

- `src-tauri/src/lib.rs`：注册 `memory_move_category` 命令。

- `src/components/panel/PanelMemory.tsx`：
  - 新增 `MIRRORED_CATEGORIES` 常量（与后端 is_mirrored 同步 4 项）
  - 新增 `moveCatPicker: { catKey, title } | null` state + `moveCatBusy`
    busy flag + outside-click / Esc 关闭 useEffect
  - 在每条 item 的 📋📄（复制绝对路径）按钮之后插 🏷 按钮（仅非镜像 cat
    的 item 显）
  - click 切换 popover：列出所有合法目标 cat（CATEGORY_ORDER + 自定义，去镜像，
    去当前 cat）— 显示中文 label + 灰字 catKey
  - 选目标 → invoke `memory_move_category` + `loadIndex()` + 3s message

## Key design decisions

- **仅允许非镜像移动**：butler_tasks / todo / ai_insights / task_archive 镜像
  到 SQLite，跨 kind 移动需要 delete 源镜像 + create 目标镜像 + 重置 queue
  序号 / 归档计数器；任一环节失败状态可能漂移。v1 用 hard refuse + 文档化
  workaround（在目标 cat 重建 + 源 cat 删除）。owner 实际诉求"挪 user_profile
  / general 之间"完全覆盖。
- **pre-validation 全部通过才 mutate index**：rust `read_index()` 返回的是
  fresh copy；先做 immutable borrows 校验全部分支，再 single shot mutable
  borrow 改 items。失败路径不动数据避免半成品状态。
- **popover 模板复用 snooze chip / 调期 popover 同样的 outside-click + Esc**：
  与既有交互语言一致，owner 学新按钮但操作模式熟悉。
- **busy flag 防双触**：invoke 期间 disable 按钮 + 阻打开新 picker，避免
  连点产生多次移动 race（如先移到 general 又快速移到 user_profile）。

## Verification

- `cargo check` ✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
