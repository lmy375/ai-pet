# 任务-记忆联动 — 开发计划

> 对应需求（来自 docs/TODO.md「已确认」）：
> 任务-记忆联动：任务输入与产物自动写入记忆库并打 tag，供后续回忆与周报引用。

## 目标

让任务从"队列里的一条 description"升级为"长期可检索的工作记录"。两条具体路径：

1. **打 tag**：description 里支持 `#xxx` 词法的标签（如 `#organize` `#文件整理`）。面板列表展示为小 chip；后续 `memory_search` 关键字命中自动；周报按 tag 聚合"本周哪些主题在推进"。
2. **抽产物**：description 支持 `[result: 整理了 38 个文件]` 形式的产物标记。LLM 在标 `[done]` 时同时写一句"做了什么"。面板在已完成任务下显式展示产物行；周报"任务"段升级为"完成清单（标题 + 产物）"。

两条路径都**复用现有 `butler_tasks.description` 这条字符串协议**——不引入新表 / 新内存类目。description 仍是真相源；新增的只是"description 内可被识别的轻量标记"。

## 非目标

- 不做"产物文件链接"——结构化产物（文件路径、URL）等用户反馈了再加 schema。
- 不做"按 tag 筛选 / 搜索 UI"——v1 只展示与聚合。后续如果用户想点 tag 看相关任务再加。
- 不强制 LLM 必须写 result——纯软鼓励。任务即便没 result 也能正常完成，周报里那一条直接省掉产物行。
- 不做跨任务关联（"这一条任务延续了上周的 X"）——超出 MVP，留 TODO。

## 设计

### Tag 词法

约定：`#` 起始 + 一段连续字符（letters / digits / `_` / `-` / 中文）。空白、标点、`]`、其它 `#` 都终止 tag。例：

```
[task pri=2] 整理 Downloads #organize #文件整理 #weekly
```

→ tags = `["organize", "文件整理", "weekly"]`（去掉 `#`）。

不区分大小写在搜索层（memory_search 自带 case-insensitive），存储层保持原样。

### Result 标记

约定：`[result: 自由文本]`（中文冒号也接受），单条 description 里至多一条。例：

```
[task pri=2] 整理 Downloads [done] [result: 把 30 天前的 38 个文件归档到 ~/Archive/2026-04/]
```

→ result = `"把 30 天前的 38 个文件归档到 ~/Archive/2026-04/"`

LLM 写 result 是软约定 —— `format_butler_tasks_block` 里加一句"完成时除了 `[done]`，建议补一行 `[result: 你具体做了什么]`，让用户能在面板和周报里看到产物"。

### 纯函数（task_queue.rs）

```rust
pub fn parse_task_tags(description: &str) -> Vec<String>
pub fn parse_task_result(description: &str) -> Option<String>
```

两个都是 pure。tags 去重 + 保持首次出现顺序；result 取首个 `[result: ...]` 的内容（trim 后非空才返回 Some）。

### TaskView 字段扩展

`TaskView` 加：

```rust
pub tags: Vec<String>,
pub result: Option<String>,
```

`build_task_view` 调上面两个解析器。同时 `strip_origin_marker` 那一行扩到也剥 `[result:]` 段——避免 result 在 body 里重复出现（产物会单独在 result 字段展示）。tag 不剥（让用户在 body 里也看得到 `#` 词，便于阅读）。

### Panel UI

- 任务行：标题旁边 / 描述下方加一小排 tag chip（小、灰色）
- 已完成 / cancelled 行：如有 result，加一行 `✓ 产物：<text>`（绿色系，与状态徽章呼应）

### 周报扩展

`WeeklyStats` 加两个字段：

```rust
pub tag_top: Vec<(String, u64)>,        // 本周 tag 频次 top 5
pub completed_with_results: Vec<(String, Option<String>)>,  // (title, result)
```

`format_weekly_summary_detail` 任务段升级：

```markdown
## 任务
本周管家事件 N 条（创建 X / 更新 Y / 删除 Z）。
完成或取消：
- 整理 Downloads — 把 30 天前的 38 个文件归档到 ~/Archive/2026-04/
- 跑步（无产物）

主题 tag：#organize × 3、#文件整理 × 2、#weekly × 1
```

### LLM 引导

`format_butler_tasks_block` 末尾的 instruction 加一段：

> 完成任务时除了 `[done]` 之外，**建议补一句 `[result: 你具体做了什么]`**——这条会在面板和周报里被独立展示，让主人能直接看到产物，不必翻 detail.md。如果是"信息收集"类任务，result 写结论；"文件操作"类写"挪了多少 / 改了哪个文件"；"提醒"类写"提醒过了"。

## 阶段划分

| 阶段 | 范围 | 状态 |
| --- | --- | --- |
| **M1** | task_queue 加 parse_task_tags / parse_task_result + TaskView 字段 + 单测 | ✅ 完成（13 条新单测） |
| **M2** | task_create / task.rs build_view 接入；前端 PanelTasks 渲染 tags + result；weekly_summary 升级 | ✅ 完成（panel build_view 2 条新单测、weekly_summary aggregator 2 条新单测、format detail 升级现有断言） |
| **M3** | format_butler_tasks_block 引导文案；收尾（README / TODO / done/） | ✅ 完成 |

## 复用清单

- `task_queue::strip_origin_marker` 的 `remove_bracketed_segments` —— 复用为 strip result 的实现
- `WeeklyStats` / `format_weekly_summary_detail` —— 任务段升级
- `commands::task::build_task_view` —— TaskView 装填
- 前端 `PanelTasks.tsx` 的 itemBody / itemMeta 样式 —— 加一行 result，加 chip 排

## 待用户裁定的开放问题

1. **Tag 词法是否 case-insensitive 存储**：保留原始 case（用户写 `#weekly` 与 `#Weekly` 视作两个 tag）。理由：尊重用户书写；周报聚合视效果再合并。
2. **Result 长度上限**：暂不裁。description 整体已有 BUTLER_TASKS_HINT_DESC_CHARS=100 的展示截断；result 跟着同走没问题。如果一条 result 写了 1000 字会自然被那一段截断显示。
3. **跨任务关联（"延续上周 X"）**：本轮不做。LLM 已能用 memory_search 找历史任务；显式 link 等真有反馈再设计。

## 进度日志

- 2026-05-04 20:00 — 创建本文档；准备进入 M1。
- 2026-05-04 20:35 — M1-M3 一次性合到 main：
  - **M1**：`task_queue.rs` 加 `parse_task_tags`（词法：`#` + letters/digits/`_-`/中文，前置字符不是 tag 字符才视作 tag 起点 — 防误命中 `PR#42`）+ `parse_task_result`（兼容中文冒号、空内容 → None、多个取首个）+ `strip_result_marker`（给面板 body 显示用）。`TaskView` 加 `tags: Vec<String>` / `result: Option<String>`。13 条新单测覆盖词法边界（孤立 `#` / 单词中部 `#` / 标点终止 / 去重保序）+ result 边界（中文冒号 / 空 / 多个首个）+ strip 行为。
  - **M2**：
    - `commands/task.rs::build_task_view` 装填 tags + result，并在 body 上 `strip_result_marker(strip_origin_marker(...))`。新加 2 条单测验证 tags / result 命中与缺失分支。
    - `weekly_summary.rs` ButlerStats 加 `completed_with_results: Vec<(String, Option<String>)>` + `tag_top: Vec<(String, u64)>`；`aggregate_butler_events` 在每条 event 调 `parse_task_tags` 累计 + 在结束态条目调 `parse_task_result` 配对；`format_weekly_summary_detail` 任务段升级为「title — result」/ 「title」+「主题 tag：#x × N」段。WeeklyStats 同步加字段；`consolidate.rs::maybe_run_weekly_summary` 装填新字段。新增 2 条 aggregator 单测。
    - `PanelTasks.tsx` `TaskView` 接口加 `tags` / `result`；行渲染加灰色 tag chip 排 + 已结束行的「✓ 产物：」绿色文案。
  - **M3**：`format_butler_tasks_block` instruction 末尾加两段引导：「完成时建议补 `[result:]`」+「任务可以打 #tag 给周报聚合」。注意：Rust 字符串里 ASCII 双引号会终止 string，最初写 `"信息收集"` 编译失败，改用全角 `「」` 修复。
  - `cargo test --lib` 787/787 通过；`tsc --noEmit` 干净。README 加亮点；TODO 移除条目；本文件移入 `docs/done/`。
- **开放问题答复**：
  - Q1 tag case：保留原始 case。`#weekly` vs `#Weekly` 视作两个 tag，尊重用户书写。
  - Q2 result 长度：不裁。description 整体的 100 字符截断（BUTLER_TASKS_HINT_DESC_CHARS）会自然 cap。
  - Q3 跨任务关联：本轮不做。LLM 已能 memory_search，足够；显式 link 等真有反馈再设计 schema。
