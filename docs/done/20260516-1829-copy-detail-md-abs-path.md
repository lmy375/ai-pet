# PanelTasks 行右键加「🔗 复制 detail.md 绝对路径」+ 上轮 TODO 池替换

## 背景

### 上轮"切 Live2D 模型"TODO 替换说明

上一轮 auto-propose 的 6 条里最后一条「桌面 pet 右键菜单加切 Live2D 模型子菜单」实际无法落地：
- 项目仅内置 `miku` 一个模型（`public/models/miku/`），submenu 列出来就是 "miku ✓" 一个 row，无切换价值
- 让 owner 拖自定义模型进 `~/.config/pet/live2d/` 加载需要 Tauri asset protocol 设置 + 权限 scope + 跨平台 path 适配 + Live2D loader 对 file:// URL 适配 —— 工作量超出一轮 iter

按 TODO.md workflow "任务不能过度复杂"，移除该项并补 5 条新需求：
1. PanelTasks 行右键「🔗 复制 detail.md 绝对路径」（本 iter 实现）
2. PanelTasks 顶 "+ 新建" 绑 ⌘N
3. ChatMini bubble 底"⏱ N 分前"hover chip
4. PanelMemory item 行右键「📅 显创建时间」
5. butler_task `[silent]` marker

### 本 iter 实现 #1

owner 已有「📂 在 Finder 显示」一键 reveal detail.md 文件，但有时想：
- 在 VSCode ⌘P 直接打开（需要绝对 path）
- 在 IntelliJ ⇧⌘O 跳转
- 在 Finder ⇧⌘G goto folder（接受绝对 path）
- shell `open <path>` / `code <path>` / `cat <path>` 流水线

都需要绝对路径串。当前没渠道拿到 —— 必须先 reveal in Finder + 在 path bar 复制 / option-click 复制 path。两步。

加一条「🔗 复制 detail.md 绝对路径」右键菜单一键搞定。

## 改动

### `src-tauri/src/commands/memory.rs` — 新 `memory_detail_abs_path` 命令

```rust
#[tauri::command]
pub fn memory_detail_abs_path(detail_path: String) -> Result<String, String> {
    let trimmed = detail_path.trim();
    if trimmed.is_empty() {
        return Err("detail_path is empty".to_string());
    }
    if trimmed.contains("..") || trimmed.starts_with('/') {
        return Err("invalid detail_path".to_string());
    }
    let mem_dir = memories_dir()?;
    let full = mem_dir.join(trimmed);
    if full.exists() {
        let mem_canon = fs::canonicalize(&mem_dir).map_err(|e| ...)?;
        let full_canon = fs::canonicalize(&full).map_err(|e| ...)?;
        if !full_canon.starts_with(&mem_canon) {
            return Err("detail_path escaped memories_dir".to_string());
        }
        return Ok(full_canon.to_string_lossy().into_owned());
    }
    // 文件不存在 → 直接拼，让 owner 在 detail.md 还没写入时也能预先拿 path
    Ok(full.to_string_lossy().into_owned())
}
```

- 安全：`..` / 绝对路径拒；存在时 canonicalize 后必须落 memories_dir 内
- 不存在时不报错：让 owner "detail.md 还没写过" 的 task 也能拿到 "未来会落地" 的目标 path，预先 open IDE 占位
- 复用 memories_dir() 既有 helper

### `src-tauri/src/lib.rs` — 注册

```rust
commands::memory::memory_detail_abs_path,
```

### `src/components/panel/PanelTasks.tsx` — 右键菜单按钮

紧贴既有 「💬 复制为引用块」 之后插入：

```tsx
{t && t.detail_path && (
  <button
    style={itemBtn}
    onClick={async () => {
      setTaskCtxMenu(null);
      try {
        const abs = await invoke<string>("memory_detail_abs_path", { detailPath: t.detail_path });
        await navigator.clipboard.writeText(abs);
        setBulkResultMsg(`已复制 detail.md 绝对路径`);
      } catch (e) {
        setBulkResultMsg(`复制 path 失败：${e}`);
      }
      window.setTimeout(() => setBulkResultMsg(""), 3000);
    }}
    title="把 detail.md 的绝对路径（含 ~/.config/pet/memories/... 前缀）复制到剪贴板..."
  >
    🔗 复制 detail.md 绝对路径
  </button>
)}
```

- gate 在 `t.detail_path` 非空：从未 writeDetail 过的 task 没 path 可复制
- 与既有 📂 在 Finder 显示 / 📑 复制为 Markdown / 💬 复制为引用块 形成菜单内 file 操作组

## 关键设计

- **gate `t.detail_path` 非空**：detail_path === "" 时按钮不渲染。任务还没 writeDetail 过则没意义（按下也只能复制一个 sketch path）。
- **后端 path 存在 → canonicalize**：resolve symlink + 验证落 memories_dir 内；与既有 `memory_reveal_detail_in_finder` 安全检查同模板。
- **后端 path 不存在 → 直接拼**：用 `mem_dir.join(trimmed).to_string_lossy()` 拼接。让 owner 在 detail.md 没存盘时也能拿到目标 path，预先用 IDE 打开占位 / 创建文件夹结构等。
- **错误 toast 在 bulkResultMsg slot**：与既有 「📑 复制为 Markdown」 / 「💬 复制为引用块」 等复制操作同 message 区，UX 一致。
- **menu label 标 🔗**：与 既有「🔗 复制为 ref（「title」）」chain emoji 同 family —— 都是"轻量 token / 链接形态" 复制。
- **tooltip 列具体 IDE 用法**：VSCode ⌘P / IntelliJ ⇧⌘O / Finder ⇧⌘G / shell open —— 教学性 tooltip 让 owner 学会"path 怎么用"，不只是复制完一脸懵。

## 不做

- **不写 macOS / Windows / Linux per-platform path 风格转换**：`Path::to_string_lossy()` 已生成本机风格 path（`/Users/...` mac / `~` 不展开）。owner 在自己平台用，path 直接生效。
- **不显 path 字面量在 button label 内**：> 50 字符的 path 会撑爆 menu 列，sub-tooltip 显示已足够。
- **不缓存 path（每次都 IPC 调一次后端）**：detail.md 文件 path 可能因 task rename 改名，缓存反而易脏。IPC 一次也就几毫秒，按钮 click 同步等待 OK。
- **不写单测**：纯 path join + canonicalize；既有 `memory_reveal_detail_in_finder` 同 path 安全模板无单测；视觉验证（在 PanelTasks 右键任意 task → 点本按钮 → 粘到 VSCode 应直接打开 detail.md）足够。

## 验证

- `cargo check` ✓
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.23s
- 改动 ~75 行（backend command 35 + lib.rs 注册 1 + frontend button 30 + 注释 9）。既有 detail_path / 📂 在 Finder 显示 / 复制按钮链路 / pendingTitleFocus / detail.md hover preview 全部不动。

## TODO 状态

剩 4 条留池：
- PanelTasks 顶 "+ 新建" 绑 ⌘N
- ChatMini bubble 底 "⏱ N 分前" hover chip
- PanelMemory item 行右键「📅 显创建时间」
- butler_task `[silent]` marker

## 后续

- ⌘⇧L 全局快捷"复制当前选中 task 的 detail.md path"，让 menu 抓不到的"已展开 detail in-place 编辑" 场景也可用键盘抓 path。
- 加一个对偶按钮 「📋 复制 detail.md 内容」 —— 一键 fetch + 复制整段 detail body，方便贴进 IM。
- detail.md path 复制时如果文件不存在追加一行 hint "(尚未写盘 - 在 IDE 打开时会自动创建)"。
- ~/.config/pet/memories 路径在 PanelSettings 「本地数据目录」 section 已显，与此 path 复制功能形成"位置 known + 文件 path 复制"互补。
