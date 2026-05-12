# SQLite v6：memory_list / memory_search 取 SQLite 真相

## 背景

接 v5d（butler_tasks 所有 hot read 已切 SQLite）。但仍有两个公开 Tauri command 直接返 yaml `read_index()` 结果：

- `memory_list(category?)` —— Panel UI / LLM `memory_list` 工具调用
- `memory_search(keyword)` —— Panel UI / LLM `memory_search` 工具调用

它们读 yaml index，所以 LLM 通过这两个工具看到的 butler_tasks 仍是 yaml（双写下与 SQLite 同步，但根本上是两条 read path 并存，未来要撤 yaml 必须先切）。

## 改动

`src-tauri/src/commands/memory.rs`：

### `memory_list`

读完 yaml index 后，**覆盖** butler_tasks 段的 items 为 `crate::db::butler_tasks_as_memory_items()`。其它 category（ai_insights / user_profile / todo / task_archive / general）继续 yaml。

```rust
let mut index = read_index();
if let Some(cat) = index.categories.get_mut("butler_tasks") {
    cat.items = crate::db::butler_tasks_as_memory_items();
}
// ... filter by category (caller arg) ...
```

### `memory_search`

同模式 —— 在跨 category 关键词搜索前，先把 butler_tasks 段 items 替换成 SQLite 数据。

## 效果

- 所有调 memory_list / memory_search 的 caller（PanelMemory 前端 / LLM tool 调用）看到的 butler_tasks 都是 SQLite。
- yaml 仍持 butler_tasks 段（v5 mirror 双写不动），让回滚到旧版本仍能正常工作 —— orphan entries 但不丢数据。
- memory_edit 双写不动 —— write 仍 yaml + SQLite 同步，保数据安全冗余。

## 不做

- 不撤 memory_edit 的 yaml 写。v6 是"读切完"；v7+ 才考虑"写也切（撤 yaml）"。
- 不删 yaml 里的 butler_tasks 段（保险冗余）。
- 不改 LLM tool definition 文案 —— memory_edit 仍接收 "butler_tasks" category。

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 876 通过
- 启动后：
  - PanelMemory butler_tasks 段渲染数据来自 SQLite（通过 memory_list 调用）
  - LLM 调 memory_list / memory_search 也看到 SQLite 数据
  - PanelTasks 列表（task_list）继续走 SQLite 直接接口

## SQLite 流水线

- v0–v5 ✅ 基础设施 + 双写 + 所有 read 切完
- v6 ✅ **memory_list / memory_search 取 SQLite**（最后两个 yaml 读 leakage 关闭）
- v7+ ⏳ 撤 memory_edit yaml 写 / 清 yaml butler_tasks 段
- v8+ ⏳ 迁移 todo / task_archive / mood / plan

至此 butler_tasks 全链路 **read 100% SQLite**。下一轮 v7 可以考虑切写、然后正式从 yaml 清理 butler_tasks。

## 完成

- [x] memory_list butler_tasks 覆盖
- [x] memory_search butler_tasks 覆盖
- [x] 移到 docs/done/
