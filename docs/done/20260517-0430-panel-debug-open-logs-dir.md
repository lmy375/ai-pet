# PanelDebug「📂 logs 目录」按钮（iter #248）

## Background

owner 排查 LLM 调用 / proactive 决策 / 任务调度等异常时常需要看
`~/.config/pet/logs/` 下的 `app.log` / `llm.log` 文件。PanelDebug 本身能在
界面里浏览 in-memory log，但要 grep、tail、拖到第三方 viewer 时还得手敲完整
路径。

设置面板有「打开宠物数据目录」按钮（`open_pet_data_dir`），但那打开的是数据
根，logs 是其子目录之一，再点进去仍是手动操作。本迭代加一键直达 logs 子目录
的按钮，与既有"在 Finder 显示 detail.md"模式同源。

## Changes

- `src-tauri/src/commands/debug.rs`：新增 `open_logs_dir` tauri 命令。
  跨平台 `open` / `explorer` / `xdg-open`；目录不存在时先 `create_dir_all`
  防 Finder 拒打开空路径（与 `open_pet_data_dir` 同模式）。

- `src-tauri/src/lib.rs`：在 `invoke_handler` 注册 `open_logs_dir`，紧跟
  `clear_logs` 后。

- `src/components/panel/PanelDebug.tsx`：toolbar 在「清空」按钮后新增
  「📂 logs 目录」按钮 → `invoke("open_logs_dir")`，失败 console.error
  （不弹 toast — 用户能立即从"目录没打开"反馈知道失败，不值得占视觉空间）。

## Key design decisions

- **专用命令而非 reuse `open_pet_data_dir`**：让 owner 直达 logs 子目录，
  少一步操作；命令实现 9 行，复用成本比写 callsite 拼路径低。
- **平台分流复用既有 cfg 模板**：与 settings.rs `open_pet_data_dir` 同结构，
  未来若加 `xdg-open` 退化策略也只需改一处常量。
- **不强制 logs/ 已被写过**：`create_dir_all` ensure 路径存在，让首次启动还
  没产生任何 log 的用户也能打开（看到空目录，比 Finder 报错好）。

## Verification

- `cargo check` ✅（在 src-tauri/ 下）
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.18s)
