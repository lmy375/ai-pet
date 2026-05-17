# PanelDebug「🗄 detail .history 占盘」chip（iter #328）

## Background

iter #305 ship 了 detail.md 自动版本历史（snapshot 前一版到 sibling
`.history` 目录，cap=5）。这是 safety net，但磁盘成本对 owner 不可见 —
随着 task 数量 / 编辑次数累积，`.history` 目录占盘会增大。

本迭代加 PanelDebug toolbar chip — 扫所有 `.history` 目录算总字节数 +
目录数 + 文件数，让 owner 看 safety-net 磁盘成本，决策"该不该清 / 调小
HISTORY_CAP"。

## Changes

### `src-tauri/src/detail_history.rs`

- 新 struct `DetailHistoryDiskUsage { total_bytes, file_count, dir_count }`
  + Default + Serialize
- 新 pure helper `scan_history_disk_usage(mem_dir: &Path) ->
  DetailHistoryDiskUsage`：
  - 递归扫 mem_dir
  - 子目录名以 `.history` 结尾 → 算 `.history` dir，累加内部文件大小，
    `dir_count += 1`
  - **不递归进 .history 内部子目录**（防御嵌套场景把不相关 file 误算）
  - 普通子目录继续递归找 `.history`
  - mem_dir 不存在 / 不可读 → 全 0；单 file metadata 失败容忍
- 4 个新 unit test：empty dir / missing dir / 聚合多 .history 跨 cat /
  不递归进 .history 内部子目录

### `src-tauri/src/commands/memory.rs`

- 新 Tauri 命令 `detail_history_disk_usage()` → wraps `scan_history_disk_
  usage(memories_dir())`

### `src-tauri/src/lib.rs`

- 注册 invoke handler `commands::memory::detail_history_disk_usage`

### `src/components/panel/PanelDebug.tsx`

- 新 import `formatBytes` from utils
- 新 `HistoryDiskUsage` 接口 + state + `fetchHistoryDisk` callback
- 新 useEffect 首屏自动 fetch 一次
- toolbar 在「📂 logs 目录」之后插 chip button：
  - 未加载：显「🗄 .history —」
  - 加载中：显「🗄 刷新中…」+ disabled
  - 已加载：显「🗄 .history 12 KB · 3 dir」(formatBytes + dir count)
  - 点击立即刷新 + 详细 tooltip 含 dir / file 数

## Key design decisions

- **递归扫 + 名后缀匹配**：`.history` 目录是 sibling 模式（`<file>.md.
  history`），不固定在某层 — 必须递归扫每个 cat 子目录。后缀匹配
  `ends_with(".history")` 与 history_dir_for 算法对偶。
- **不递归进 .history 内部**：防御未来扩展可能给 .history 加 sub-dir
  （如 daily archive bucket）；当前 inner is_file() 仅算 history dir 第
  一层文件 — 防误把不相关嵌套 file 算进体积。
- **首屏自动 fetch**：chip 没数据时显「—」无意义；自动 fetch 一次让首
  次进 PanelDebug 即看到数字。比纯 click-to-load UX 更直接。
- **点击刷新 + disabled-during-fetch**：避免 owner 连点引发并发 IO；
  与 PanelDebug 其它 fetch chip (cacheStats / llmLatencies / taskStats
  refresh) 同 pattern。
- **chip 文案两段**：「🗄 .history N KB · M dir」给一眼看的"体积 +
  dir 数"；tooltip 补完整 N KB · D dir · F file + 刷新 hint。让 chip
  本身紧凑，详情在 hover。
- **chip 位置紧贴 logs 目录**：两者都是"safety / debug 类磁盘 / 系统资
  源"信息 — adjacency 让 owner 一眼扫到相关一族。
- **不引入 PanelMemory 对应 chip**：PanelMemory 已经显 memory_disk_usage
  全集；本 chip 是 PanelDebug 专属（debug 信号），避免在 panel 各处重
  复同维度数字。

## Verification

- `cargo test --lib detail_history`（backend）— 13 passed / 0 failed
  （4 新 scan_disk_usage 测试通过）
- `cargo test --lib`（backend）— 1197 passed / 0 failed
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.22s)
