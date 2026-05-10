# 三件套：类目排序 / 归档视图 / 本地数据目录

> 对应需求（来自 docs/TODO.md）：
> 1. PanelMemory 类目排序固化：活跃类目（butler_tasks / todo / ai_insights）排前。
> 2. PanelTasks 增加「查看归档」入口：点按钮切到只读 task_archive 列表。
> 3. 设置页显示宠物本地数据目录并加「在 Finder 打开」按钮。

## 1. PanelMemory 类目排序

`src/components/panel/PanelMemory.tsx`：

- `CATEGORY_ORDER` 改为 `["butler_tasks", "todo", "ai_insights",
  "task_archive", "general", "user_profile"]`。把 task_archive 显式
  插在中间，避免 fallback append 把它甩到最末。
- 注释解释顺序意图：活跃在上、慢变 / 归档在下，让首屏先看到有动态的内容。

## 2. PanelTasks 「查看归档」入口

`src/components/panel/PanelTasks.tsx`：

- 新 state：`archiveExpanded` / `archiveLoaded` / `archiveLoading` /
  `archiveItems` / `archiveError`。
- `reloadArchive` 走 `memory_list({ category: "task_archive" })`，把
  items 按 `updated_at` 字典序倒排（同 RFC3339 字符串与时序一致）。
- 在主队列底部加一段折叠区「📦 归档」：默认 collapsed；首次展开 lazy fetch；
  「刷新」按钮强制重拉。
- 只读视图：每条显「YYYY-MM-DD（归档日）」chip + 原 title（剥前缀） +
  完整 description（含 `[archived: ...]` 前缀和原 [done]/[cancelled] 标记）。
  无 checkbox / 无 retry / 无 cancel —— 归档是回看视图。

## 3. 本地数据目录

后端：

- `src-tauri/src/commands/settings.rs`：
  - `get_pet_data_dir() -> Result<String, String>` 返回 `~/.config/pet/`
    的绝对路径（`config_dir()` 内部 ensure 父目录可拼）。
  - `open_pet_data_dir() -> Result<(), String>` 在系统文件管理器里打开。
    macOS 用 `open <path>`，Windows 用 `explorer`，其它走 `xdg-open`。
    打开前先 `create_dir_all`，避免首次启动还没写盘时 Finder 拒绝。
- `lib.rs` 注册两条命令。

前端：

- `PanelSettings`：新增「本地数据目录」SearchableSection，挂在设置搜索
  框下面、Live2D 之上，永远可见。
  - 显示绝对路径（user-select: all）。
  - 「在 Finder 中打开」按钮调 `open_pet_data_dir`。
  - 「复制路径」按钮 `navigator.clipboard.writeText`，1.5 秒「已复制」反馈。
  - tail 一段 11px 灰字解释目录下分别是 `config.yaml` / `SOUL.md` /
    `memories/` (含 task_archive 归档) / `sessions/`。

## 验证

- `tsc --noEmit` 干净。
- `vite build` 干净。
- `cargo check` 干净。
- 现有 892 测试不动；新代码均为 UI / IO 包装，无独立逻辑可单测。
