# PanelDebug「📋 复制 logs 路径」chip（iter #357）

## Background

PanelDebug 已有「📂 logs 目录」chip — 走 Tauri opener 在 Finder
里 reveal logs 目录。但 owner 真实用法不全是 Finder：grep / tail /
VSCode ⌘O 打开单文件 / shell `cd` / 第三方 log viewer — 都需要的是
**绝对路径字符串**，不是 reveal-in-Finder。本 iter 加「📋 复制 logs
路径」chip 互补 — 一键剪贴板拿到 `~/.config/pet/logs/` 绝对路径，
随处粘贴。

## Changes

### `src-tauri/src/commands/debug.rs`（line 326-335）

新 `#[tauri::command] pub fn get_logs_dir_path() -> String`：

```rust
pub fn get_logs_dir_path() -> String {
    log_dir().to_string_lossy().to_string()
}
```

- 复用既有 `log_dir() -> PathBuf` helper（与 open_logs_dir 同源）
- `to_string_lossy().to_string()` lossless 路径转字符串 — log_dir
  内部走 PathBuf 拼接，正常 UTF-8 合法
- 不带参数 / 无错误路径 — 失败可能性极低（path always exists），不
  抽 Result 包装

### `src-tauri/src/lib.rs`（line 149）

`invoke_handler!` 注册 `commands::debug::get_logs_dir_path,` —
紧跟 `open_logs_dir,` 之后，体现 chip 互补关系。

### `src/components/panel/PanelDebug.tsx`（line 2185-2207）

「📂 logs 目录」chip 之后插新「📋 logs 路径」chip：

```tsx
<button
  onClick={async () => {
    try {
      const path = await invoke<string>("get_logs_dir_path");
      await navigator.clipboard.writeText(path);
      console.log(`已复制 logs 路径：${path}`);
    } catch (e) {
      console.error("copy logs path failed:", e);
    }
  }}
  style={toolBtnStyle}
  title="..."
>
  📋 logs 路径
</button>
```

- 复用既有 `toolBtnStyle` — chip row 风格统一
- title 详释互补语义：「📂 reveal Finder vs 📋 copy path for non-
  Finder tools」
- 复用 navigator.clipboard.writeText — 与 PanelDebug 其他 copy
  chip 同 pattern（no extra state 反馈，console.log 即可）

## Key design decisions

- **新增独立 Tauri command 而非复用 open_logs_dir**：open_logs_dir
  是 side-effect（启动 Finder），不返回字符串。新 command 单纯
  query path — 职责单一。
- **不在前端硬编码 `~/.config/pet/logs/`**：log_dir() 在 Rust 端
  resolve（走 dirs::config_dir() 或 HOME fallback），前端硬编码会
  drift。owner 改 config base dir 后前端会失同步 — 走 IPC 拿真值
  最稳。
- **不加 toast UI 反馈**：PanelDebug 是 power-user / debug 面板，
  其他 copy chip（如 disk usage 复制 #328）也只 console.log。一致
  风格 — 不为单个 chip 引 toast 系统。owner 看 console / Finder
  粘一下就知道有没有成功（粘上即成功）。
- **title attr 详释互补关系**：避免 owner 困惑「📂 与 📋 各做什
  么」— hover title 一行说清「reveal Finder vs 拷贝路径给其他工具
  用」。
- **不抽 helper merge open_logs_dir + get_logs_dir_path**：两 fns
  side-effect 模型不同（一个调 opener crate，一个纯 query），merge
  会引参数标志 — 单独短 fns 更清晰。

## Verification

- `cargo check`（backend）— clean，仅遗留 7 个 dead-code warnings
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.22s)
