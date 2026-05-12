# SQLite v2：backfill 从 yaml 回填

## 背景

接 v1（CRUD 函数已完工）。本轮加 `backfill_butler_tasks` 函数：把 memory_index.yaml 的 butler_tasks 段每条 MemoryItem 派生成 ButlerTaskRow 插入 SQLite。

idempotent —— 多次启动只插一次，已存在的 title 跳过。

## 改动

### `src-tauri/src/db.rs`

新增 `pub fn backfill_butler_tasks(conn, items: &[MemoryItem]) -> Result<usize, String>`：

- 接 `&[MemoryItem]` 而非内部读盘 —— 让单测注入任意 items 集合不依赖磁盘 yaml；caller 在 v3/v4 时把 `memory_list()` 输出传进来
- 每条 item 派生：
  - `status` 走 `task_queue::classify_status` 同算法（cancelled > error > done > pending）
  - `tags` 走 `task_queue::parse_task_tags` 同算法（`#tag` 词法）
  - `detail_path`：空串 → None；非空 → Some
- 已存在 title 跳过（用 `butler_task_get`）
- 返新插入数（供观测 / 日志）

### 单元测试

新增 `backfill_derives_status_and_tags`：注入 3 条 MemoryItem（done / error / pending），验证：
- 状态 / tags / detail_path 正确派生
- 二次 backfill 返 0（幂等性）

测试 + 之前的 5 个共 6 个全过：
```
test result: ok. 6 passed; 0 failed
```

## 不做

- 不挂到启动流程（v3 才做）—— 当前 backfill 是个孤立 fn，没人调用
- 不做反向同步（sqlite → yaml）—— 不会发生
- 不暴露 Tauri command（v3 才做）

## 验收

- `cargo test --lib db::` 全 6 通过
- `cargo build --release` 通过
- 零功能变化

## 完成

- [x] backfill 函数
- [x] 单测（含派生状态 + 幂等性）
- [x] 移到 docs/done/
