# Settings 显示 app 版本号

## 背景

PanelSettings「本地数据目录」section 末尾 chip 行已显 `pet.db {KB/MB} · schema vN · butler_tasks: N …`。但是 **app 自己的版本号没在任何地方暴露给用户** —— 用户想知道"我跑的是哪个版本"得回 Cargo.toml / 改 GitHub release page。

加一个 `pet vX.Y.Z` 段到既有 chip 行最前面，最低成本一眼自检。

## 改动

### `src-tauri/src/commands/app.rs`（新模块）

```rust
//! App-level meta commands（version / build-time 等）。Per-tauri-command 风格，
//! 与 commands/window.rs 同层；不归到 db.rs 因为不依赖 SQLite。

#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
```

`commands/mod.rs` 加 `pub mod app;`。`lib.rs::invoke_handler` 加 `commands::app::app_version`。

### `src/components/panel/PanelSettings.tsx`

- 新增 `appVersion: string | null` state
- 挂载时 `invoke<string>("app_version").then(setAppVersion).catch(() => setAppVersion(null))`
- dbStats chip 行最前面插一段：`<span>pet v{appVersion}</span>`（fontWeight 600 与 `pet.db` 同 emphasis）；`appVersion == null` 时不显（容错旧 backend）

## 不做

- 不暴露 build_date / commit hash：build.rs 嵌入需要额外脚手架（git2 / 编译期 env 变量 + 工具链支撑），ROI 低。版本号 + schema 已够日常自检
- 不渲染 release notes / 上游对比：避免 panel 沦为 release dashboard；版本号是"知情权"，更新走 GitHub release / brew upgrade
- 不动调试导出文案：buildDebugMarkdownSnapshot 在 PanelDebug 已经会 fetch debug context；可后续 iter 把 app_version 也拼进 snapshot 顶部，本轮不做

## 验收

- `cargo build --release` ✅
- `npx tsc --noEmit` ✅
- 切到「设置」tab，滚到「本地数据目录」section → 末尾 chip 行最前面看到 `pet v0.1.0`（与 Cargo.toml 一致）

## 完成

- [x] commands/app.rs 新增（含 1 单测） + mod.rs 注册
- [x] lib.rs invoke_handler 加 app_version
- [x] PanelSettings.tsx: appVersion state + 渲染
- [x] `cargo build --release` 通过
- [x] `cargo test --lib` 通过（899 passed，+1 新）
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
