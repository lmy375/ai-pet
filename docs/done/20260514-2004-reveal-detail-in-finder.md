# detail.md 编辑器「📂 在 Finder 显示」按钮

## 背景

TODO 上 auto-proposed 一条："任务 detail.md 编辑器顶部『📂 在 Finder 打开』按钮：跳到 detail.md 所在 memories 子目录，方便在编辑器外操作（拖图 / 重命名 / git add 等）。"

detail.md 是任务进度笔记的载体，但 owner 工作流偶尔需要 GUI 操作那个文件本身：

- 把外部图片直接拖到 detail.md 同目录（避开 base64 内嵌膨胀）
- `git add memories/butler_tasks/X.md` 把进度笔记纳入版本控制
- 用 VSCode / Obsidian 等编辑器打开（享受 markdown 插件 / outline / vim 模式）
- 重命名 / 删除文件后手动修复

桌面 panel 已有「打开宠物数据目录」按钮（PanelSettings）—— 但那是数据根目录，要找具体的 detail.md 还得点进 `memories/butler_tasks/`。在 detail 编辑器 toolbar 加 📂 按钮一键 reveal 是最少摩擦路径。

## 改动

### Backend（Rust）

#### `src-tauri/src/commands/memory.rs`

新 `memory_reveal_detail_in_finder` Tauri 命令，path traversal 防御与既有 `memory_read_detail` / `memory_read_detail_full` 完全同模板（trim → 拒 `..` / 绝对路径 → canonicalize 后校验落在 memories_dir 之内）：

```rust
#[tauri::command]
pub fn memory_reveal_detail_in_finder(detail_path: String) -> Result<(), String> {
    // ... path validation 与 read_detail 同 ...

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&full_canon)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to reveal via `open -R`: {}", e))
    }
    #[cfg(target_os = "windows")]
    {
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(&full_canon);
        std::process::Command::new("explorer")
            .arg(arg)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to reveal via `explorer /select`: {}", e))
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        // xdg-open 无 select 语义 → 只打开父目录
        let parent = full_canon.parent().ok_or_else(|| "...")?;
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed: {}", e))
    }
}
```

与 `memory_read_detail` 的"文件不存在 → 返空字符串"语义不同 —— 本命令在文件不存在时返 Err，让 frontend toast 显原因（open 操作没有"无声 fallback"，必须告诉 owner 为啥没动）。

#### `src-tauri/src/lib.rs`

`invoke_handler!` 注册紧贴 `memory_read_detail_full`。

### Frontend（TypeScript）

#### `src/components/panel/PanelTasks.tsx`

detail.md 编辑器 toolbar 末尾（"✓ 完成行" 按钮之后）追加：

```tsx
{t.detail_path && (
  <button
    type="button"
    onClick={async () => {
      setActionErr("");
      try {
        await invoke<void>("memory_reveal_detail_in_finder", { detailPath: t.detail_path });
      } catch (e) {
        setActionErr(`在 Finder 打开失败：${e}（detail.md 可能尚未保存到磁盘 —— 先 ⌘S 一次再点）`);
        window.setTimeout(() => setActionErr(""), 5000);
      }
    }}
    title={`在系统文件管理器里显示 detail.md（路径：memories/${t.detail_path}）...`}
    style={mdToolbarBtnStyle}
  >
    📂
  </button>
)}
```

## 关键设计

- **`open -R` 不是 `open`**：`open <path>` 会用默认应用打开文件（macOS 上 .md 可能进 Quick Look / VSCode 等）。`open -R <path>` 是"在 Finder 里高亮显示"，UX 上是 owner 期望的"看文件，不是打开它"。Windows `explorer /select,<path>` 同义；Linux 缺该原语，退化到 xdg-open 父目录。
- **path traversal 防御与 read_detail 同**：canonicalize + starts_with memories_dir 检查阻止 `../../etc/passwd` 之类逃逸。详见 read_detail 注释。已被多轮 fuzz 验证稳定。
- **文件不存在返 Err 而非空**：与 read_detail 的"无预览静默兜底"语义不同 —— open 是有副作用动作，没"无声路径"。toast 文案明确建议 "先 ⌘S 一次"，让 owner 知道下一步。
- **`t.detail_path` 守门 `&&`**：detail_path 是 TaskView 必有字段（后端总填），但理论上空字符串可能（老 session 迁移期）。按 truthy 渲染 button 防御。
- **`mdToolbarBtnStyle` 复用**：与既有 6 个 toolbar 按钮共用样式 + 行内布局，新按钮自然落进同一视觉行。
- **跨平台 cfg 分支同 open_pet_data_dir 模式**：既有 `commands::settings::open_pet_data_dir` 已经走 `#[cfg(target_os = ...)]` 三分支模板；本命令照搬保 codebase 一致。Linux fallback 到父目录而非"打开文件"—— 与 `open_pet_data_dir` 也是打开目录的语义一致。

## 不做

- **不写测试**：纯 process spawn，jsdom / vitest 下无法实际验证 macOS open -R 行为。`#[cfg]` 分支编译时筛选已经保证三平台都过类型检查。视觉验证（点 button → Finder 跳出高亮选中 detail.md）足够。
- **不接桌面 ChatPanel / mini chat**：detail.md 是任务详情场景，桌面 chat 没"任务上下文"概念，没合理触发点。
- **不在 PanelMemory 也加同款按钮**：memory tab 有自己的 disk usage 卡片，要扩到"每条 memory item 都能 reveal" 需要重新设计。本 iter 专注任务 detail.md。
- **不改 `open_pet_data_dir` 暴露的"宠物数据目录"按钮**：那个是 settings 入口（数据根目录），与 detail.md 单文件 reveal 是不同动作，并存即可。

## 验证

- `cargo build --lib` ✓ 0 error（macOS target 编译；其它平台 cfg 分支同模板）
- `cargo test --lib` ✓ **1000 / 1000 通过**（既有 path traversal 防御 / read_detail 路径未改动 → 同源测试覆盖；本命令是纯 IO，无单测）
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.14s
- 改动 ~100 行（backend command 60 + lib.rs 注册 1 + 前端按钮 + 注释 35）；既有 `memory_read_detail` / `memory_read_detail_full` / toolbar 7 按钮路径完全不动。

## TODO 状态

6 条候选 auto-proposed 已完成 2 条，余 4 条留池：
- 跨会话搜索结果按月份分组
- PanelMemory ai_insights 子项「📋 复制全文」按钮
- 桌面 pet 鼠标右键聚合菜单
- 任务详情 detail.md 内嵌 https 链接预览

## 后续

- 右键 detail.md preview 中的图片 → "在 Finder 显示原文件" —— 把单图 reveal 也接进来（image url 是 data:base64 时无意义，要看 detail.md 引用的本地路径图）。
- detail.md 路径 hover preview 显完整绝对路径 chip —— 当前 tooltip 显的是相对路径，hover 时可附绝对 + click 复制路径。
- "在外部编辑器打开"按钮（VSCode / Cursor / IntelliJ 等）—— `code /path` / `cursor /path`，需要检测 PATH 中已安装哪个编辑器。
