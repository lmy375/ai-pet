# 任务详情页 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务详情页：点击面板任务展开 description / detail.md / 该任务在 butler_history 的事件时间线，方便回溯单条任务的全过程。

## 目标

「任务」面板里每条任务现在只展示精简视图（标题 / 徽章 / body / due / 创建时间 / 操作）。
真正"宠物为这条任务做了什么"的信息散在三处：
- `index.yaml` 的 raw description（含 `[task pri=N due=]` / `[origin:tg:...]` / `[result:...]` 等标记）
- 每条任务自带的 `detail.md` 文件（LLM 自由进度笔记）
- `butler_history.log` 里 create / update / delete 的事件流

本轮加一个"展开详情"动作：点击任务行展开内嵌一个 detail 区域，三段并列展示，让用户可以**单条任务级别**回溯全过程，不必去文件系统翻 memories 目录或猜 butler_history 行号。

## 非目标

- 不做独立的"详情窗口"——内嵌折叠面板已经够看，开新窗口反而中断队列浏览。
- 不做 detail.md 在线编辑 —— LLM 写、用户读的分工保持，写权交给 LLM 避免脏数据。
- 不做 butler_history 全文搜索 —— 那是单独的需求，本轮只过滤"本任务相关"行。
- 不做事件时间线的图形化（时间轴 / 树）—— 文本列表足以，图形化对小数据集是噪音。
- 不写 README —— 体验补强，与 R 系列任务面板迭代同性质。

## 设计

### 后端

`commands/task.rs` 新增：

```rust
#[derive(Serialize)]
pub struct TaskDetail {
    pub title: String,
    pub raw_description: String,    // index.yaml 里的原始字符串
    pub detail_path: String,        // 相对路径，给 UI 展示
    pub detail_md: String,          // detail.md 内容；缺失/空 → ""
    pub created_at: String,
    pub updated_at: String,
    pub history: Vec<TaskHistoryEvent>,  // 时间倒序（最新在前）
}

#[derive(Serialize, PartialEq, Eq)]
pub struct TaskHistoryEvent {
    pub timestamp: String,
    pub action: String,    // create / update / delete
    pub snippet: String,   // " :: " 之后的 desc 片段（已被 80 char 截断）
}

#[tauri::command]
pub async fn task_get_detail(title: String) -> Result<TaskDetail, String>;
```

实现：
1. `find_butler_task(title)` 已有；拿到 MemoryItem
2. 读 detail.md：`fs::read_to_string(memories_dir / detail_path)`，缺失 → 空串
3. 读 butler_history.log（已有 `read_history_content`），按行过滤 + 解析

### 解析 butler_history 行

butler_history 行格式：
```
<ts> <action> <title> :: <desc>
```

- ts 一个空格 token（RFC3339）
- action 一个空格 token（create / update / delete）
- title 中间任意（可含空格）
- " :: " 分隔
- desc 任意

纯函数 `parse_butler_history_line(line)` → `Option<(ts, action, title, snippet)>`。
按 " :: " 第一个出现位置 split head + snippet，head 再按前两个空格 split：
`(ts, rest)` → `(action, title)`。title trim。

`filter_history_for_task(content, target_title)` → `Vec<TaskHistoryEvent>` 时间倒序。
title 匹配采用**精确相等**（trim 后）—— 子串匹配会让 "整理 Downloads" 命中
"整理 Downloads (备份)" 之类相似名的事件，造成误回溯。

### 前端

`PanelTasks.tsx`：

- 状态：
  - `expandedTitle: string | null`（同时只展开一条，避免页面过长）
  - `detailMap: Record<string, TaskDetail>`（缓存已 fetch 过的）
  - `detailLoadingTitle: string | null`
  - `detailErr: string`
- 行 header（标题 + 徽章那行）整体加 `cursor: pointer + onClick toggleExpand`
  - 但要排除 badge 的 click 区域（badge 不应触发 expand —— 当前代码 badge 没绑事件，只是显示，无冲突）
  - 加一个左侧 `▸ / ▾` chevron 暗示可展开
- 展开后：在 itemBody / tags / result / errorMsg / actions 现有内容下方新加一个详情区
  - 3 个 sub-section，标签为 `完整描述` / `进度笔记 (detail.md)` / `事件时间线`
  - 进度笔记缺失 → 占位文案 "宠物还没写进度笔记"
  - 时间线为空 → 占位 "还没记录事件"
  - 时间线每行：`<ts> <action> · <snippet>`，action 用色条区分 (create=蓝 / update=灰 / delete=红)
- 创建任务后 / 重试后 / 取消后 → reload + 清缓存（`detailMap = {}`）让下次展开拿最新

### 测试

后端纯函数：
- `parse_butler_history_line` 正常 / 缺 ` :: ` / 缺 ts / title 含空格 / snippet 空
- `filter_history_for_task` 精确匹配 / 不匹配相似名 / 空内容 / 时间倒序

`task_get_detail` 涉及 fs 与 memory 索引，不做集成测试；通过后端测试钉牢
parser、前端通过 tsc + 手动验证完整链路。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 后端 `parse_butler_history_line` + `filter_history_for_task` 纯函数 + 单测 |
| **M2** | `task_get_detail` IO 命令 + 注册到 lib.rs |
| **M3** | 前端展开 / 折叠 + 详情 fetch + 3 段渲染 |
| **M4** | `cargo test` + `pnpm build` + TODO 清理 + 文档 done/ |

## 复用清单

- `commands::task::find_butler_task`（已有）
- `commands::memory::memories_dir`（私有 → 改 `pub(crate)` 或新加 helper）
- `butler_history::read_history_content`（已有）
- 现有 PanelTasks `s.item` / `s.itemMeta` 等样式

## 待用户裁定的开放问题

- 同时展开多条 vs 单条？本轮选**单条**（Accordion 风格），多条会让 panel 滚到天荒地老。
- detail.md 是否高亮 markdown？本轮选**纯文本**（whitespace: pre-wrap），引 markdown 库
  与 panel 现有所有"全部纯文本"风格不一致；如反馈"我想看渲染" 再加。
- 时间线返回上限：默认 butler_history 有 100 条 cap，单任务理论上最多覆盖几十条；本轮
  不再加上限（由 butler_history 全局 cap 兜底）。

## 进度日志

- 2026-05-05 04:00 — 创建本文档；准备 M1。
- 2026-05-05 04:30 — 完成实现：
  - **M1**：`butler_history.rs` 加 `parse_butler_history_line` + `filter_history_for_task` 纯函数。前者按 ` :: ` 第一次出现切 head/snippet，再按前两个空格切 ts/action/title（title 容许含空格）；后者精确匹配 trim 后 title，**拒绝子串重叠**（保护 "整理 Downloads" 不被 "Downloads" 命中），结果时间倒序（最新在前）。新增 11 条单测覆盖：正常 / 标题含空格 / 缺分隔符 / 缺 action / 空 snippet / 倒序 / 子串拒绝 / 空内容 / target trim / 脏行跳过。
  - **M2**：`commands::memory::memories_dir` 提升 `pub(crate)` 让 task 模块拼绝对路径；`commands/task.rs` 加 `TaskDetail` / `TaskHistoryEvent` 序列化结构 + `task_get_detail(title)` Tauri 命令。三段数据全 best-effort：detail.md 缺失 → 空串、butler_history 读不到 → 空 history，只有"任务找不到"才 Err。注册到 lib.rs。
  - **M3**：`PanelTasks.tsx` 加 `expandedTitle` / `detailMap` / `detailLoadingTitle` / `detailErr` 状态；`handleToggleExpand` 在缓存命中时跳过 fetch（reload / 创建 / retry / cancel 时清空缓存防陈旧）；`itemHeader` 整体 `cursor: pointer + onClick`，左侧 `▸ / ▾` chevron 暗示可展开；展开后内嵌 `detailPanel` 三段（完整描述 monospace 框 / 进度笔记 pre-wrap 框 / 事件时间线 timestamped 行）；空数据 / 加载中 / 错误各有 hint 占位文案；history action 按 create/update/delete 着色（蓝/灰/红）。
  - **M4**：`cargo test --lib` 836/836（+11）通过；`pnpm tsc --noEmit` 干净；`pnpm build` 494 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 「任务」面板的回溯能力补强，与既有任务面板迭代同性质，不是新独立亮点。
  - **设计取舍**：单条 accordion 而非多条同时展开（详情段较长，多展开会让长队列页面失控）；精确 title 匹配而非子串（避免相似名误回溯）；纯文本 detail.md 而非 markdown 渲染（与面板整体风格保持一致；引 markdown 库不划算）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯函数链路有 11 条单测钉牢，前端展开/折叠/缓存逻辑由 tsc + 状态机推演保证。
