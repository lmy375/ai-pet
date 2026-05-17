# PanelMemory 段标题右键「📁 reveal cat dir」（iter #466）

## Background

PanelMemory 既有「📂 在 Finder 显示 detail.md」item-level entry — 单
条 item 的 detail.md 文件可一键 Finder 打开。但缺一个 cat-level 入口：
**一键打开整个 category 的子目录** — owner 在调 file structure / audit
「这 cat 实际写了哪些 .md 文件 / 是不是有 stale 文件」时要切到终端
`open ~/.config/pet/memories/<cat>/` 多步。

本 iter 加 cat label 右键 → 直接 Finder 打开 `memories/<cat_key>/`
子目录。

## Changes

### `src-tauri/src/commands/memory.rs`

#### 1. `memory_reveal_cat_dir(cat_key)` Tauri 命令

紧贴 `memory_reveal_history_dir` 之后：

```rust
#[tauri::command]
pub fn memory_reveal_cat_dir(cat_key: String) -> Result<(), String> {
    let trimmed = cat_key.trim();
    if trimmed.is_empty() { return Err("cat_key is required".to_string()); }
    if trimmed.contains("..") || trimmed.contains('/') || trimmed.contains('\\') {
        return Err("invalid cat_key".to_string());
    }
    let mem_dir = memories_dir()?;
    let cat_dir = mem_dir.join(trimmed);
    if !cat_dir.exists() {
        return Err(format!("category 子目录不存在（cat 还没生成 detail.md 文件）：{}", trimmed));
    }
    let canon = fs::canonicalize(&cat_dir).map_err(|e| ...)?;
    let mem_canon = fs::canonicalize(memories_dir()?).map_err(|e| ...)?;
    if !canon.starts_with(&mem_canon) {
        return Err("cat dir escaped memories_dir".to_string());
    }
    // macOS `open` / Win `explorer` / Linux `xdg-open` 多平台分支
}
```

- path-traversal 防御：cat_key 必非空 + 不含 `..` / `/` / `\\`（与
  既有 reveal_history_dir / resolve_safe_detail_path 同 pattern）
- `canonicalize` 后必落在 `memories_dir` 内（双层 sandbox 防御）
- 子目录不存在 → 友好错误（cat 还没建过任何 item → 没生成子 dir）
- 多平台 OS-open（macOS `open` / Windows `explorer` / Linux `xdg-open`）—
  与既有 reveal_history_dir 相同跨平台模板

注册到 `lib.rs::invoke_handler!`。

### `src/components/panel/PanelMemory.tsx`

在既有 cat 名 `<span>` 上加 `onContextMenu`：

```tsx
<span
  onDoubleClick={...既有 rename}
  onContextMenu={async (e) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      await invoke("memory_reveal_cat_dir", { catKey });
    } catch (err: any) {
      setMessage(`📁 打开 cat 目录失败：${err}`);
      setTimeout(() => setMessage(""), 3000);
    }
  }}
  title={`双击改显示名 · 右键 → 📁 在 Finder 打开 cat 子目录（memories/${catKey}/）调试 file structure`}
>
  {categoryLabels[catKey] || cat.label}
</span>
```

设计：
- **`preventDefault` + `stopPropagation`**：吃浏览器默认 ctx menu（Tauri
  webview 已禁但兜底）+ 防上层 drag handler 误触
- **失败走 setMessage**：复用既有 PanelMemory 3s toast 通道（subdir 不
  存在 / IO 错时显原因）
- **tooltip 同 span 含双重 hint**：双击改名 + 右键 reveal — 让 owner
  在 hover 时一眼发现两条入口

## Key design decisions

- **挂在 label span 而非起新 chip**：right-click 是隐性入口（discover-by-
  try），避免再加显式 chip 让 header 视觉过密；既有 「📊 字 / 📊 schedule
  / 📋 titles / 🗑 清空 / + 新建」chip 已 5+ 个。新功能用现有 surface
  扩展更稳
- **不写 unit test**：纯 Tauri command + OS process spawn（无法 mock 文
  件系统外 Finder/Explorer）；path-traversal 防御与 memory_reveal_history_dir
  同模板（已 production 验证）。GOAL.md "meaningful tests only" 规则下
  不引装饰性测试
- **不引「📁 chip」按钮入口（仅 ctx）**：与既有"双击 rename"惯例一致 —
  cat label span 是「与该 cat 整体相关的快速入口」自然集中地。owner
  心智「右键 = cat-wide actions」可以未来扩展（如「右键 → 导出本段 .md」/
  「右键 → 切到本 cat-only filter view」等）
- **subdir 不存在友好错误**：cat 在 index.yaml 注册但还没创建任何
  item → cat_dir 不存在；不是异常状态。错误文案明示「还没生成 detail.md」
  让 owner 不会以为是 bug
- **path-traversal 严格防御 + canonicalize sandbox**：本命令对 owner 信
  任度高（cat_key 来自 index.yaml 已 trusted），但保 defense-in-depth
  让未来 cat_key 源扩展（如 URL deeplink / TG bot 命令）时无需重写安
  全逻辑

## Verification

- `cargo build --lib` — clean
- `cargo test --lib`（全表）— unchanged（无新测试 / 无破坏既有）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.28s)
- 手测：PanelMemory 右键 "butler_tasks" cat 标题 → Finder 打开
  `~/.config/pet/memories/butler_tasks/`；右键空 cat（如还没用过的
  general）→ setMessage 显「category 子目录不存在」
