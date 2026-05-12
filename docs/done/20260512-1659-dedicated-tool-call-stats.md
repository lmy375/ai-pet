# LLM 专用工具调用占比统计

## 背景

SQLite v11/v12 加了 `butler_task_edit` / `todo_edit` 专用工具 + 用 prompt 引导 LLM 使用。但实际效果是否生效（LLM 是否真的切到新工具，还是继续用 `memory_edit(butler_tasks)` fallback）没有 telemetry。owner 看不到引导是否成功。

## 改动

### Backend：`src-tauri/src/tool_call_history.rs`

新增：

- **`DedicatedToolStats` struct**：5 字段
  - `butler_task_edit_count` / `memory_edit_butler_count` / `todo_edit_count` / `memory_edit_todo_count` / `total_records`

- **`compute_dedicated_tool_stats(records) -> Self`**：pure fn
  - 遍历 ring buffer records
  - `butler_task_edit` / `todo_edit` 直接 count++
  - `memory_edit` 解析 `args_excerpt` JSON 取 `category` 字段 → butler_tasks 或 todo 分别 count++
  - JSON 解析失败（截断 / 非 JSON）静默忽略不 panic

- **`#[tauri::command] get_dedicated_tool_stats()`**：透传 wrapper

注册到 `lib.rs` invoke_handler。

### Backend 单测（3 个）

合并到既有 `tests` mod：
- `dedicated_vs_legacy_counts` —— 7 条 mixed records 验各计数
- `dedicated_stats_empty_records` —— 空输入 → default()
- `dedicated_stats_ignore_non_json_args` —— 非 JSON / 截断 args 不 panic 不计入

### Frontend：`src/components/panel/PanelDebug.tsx`

- 新 state `dedicatedToolStats` + 30s polling useEffect
- 渲染条件：`total_records > 0`（启动后没调过任何工具不显）
- chip strip 位置：PanelChipStrip 下方、Toolbar 上方
- 内容布局：
  - `🛠 专用工具占比（窗口 N）：`
  - `butler_task_edit X% (a / total)`
  - `todo_edit Y% (a / total)`
- title attribute 解释指标用途（"判断 prompt 引导效果"）
- 命令未注册时（旧 backend）静默退化为 null，不渲染

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 **883 通过**（原 880 + 3 新）
- `npx tsc --noEmit` ✅
- 打开调试窗口后能看到一行 monospace chip 显示当前比例
- LLM 用 `butler_task_edit` 后比例上升；用 `memory_edit(butler_tasks)` 后比例下降
- 比例越高 → prompt 引导越成功 → 后续可决定何时安全撤掉 memory_edit 对 butler_tasks/todo 的 fallback

## 完成

- [x] DedicatedToolStats + compute fn + tauri command
- [x] 3 backend 单测
- [x] PanelDebug chip strip + polling
- [x] TODO.md 移除该行
- [x] 移到 docs/done/
