# task_archive 批量清理：清掉 N 天前的归档

## 背景

consolidate 循环把 done/cancelled 且超 30 天的 butler_tasks 自动挪到 `task_archive`，归档区只追加不修改。久而久之 `task_archive` 持续增长，PanelTasks「📦 归档」区即便 cap 折叠，但 SQLite 表行数和 yaml 文件大小一直涨。

需要一个 owner 手动触发的 "清理 > N 天前归档" 操作 —— 与归档自动入档形成完整生命周期闭环。

## 改动

### `src-tauri/src/db.rs`：新 Tauri command

```rust
#[tauri::command]
pub fn task_archive_purge_older_than(days: u32) -> Result<u32, String> {
    let cutoff = chrono::Local::now()
        .checked_sub_signed(chrono::Duration::days(days as i64))
        .ok_or_else(|| "duration overflow".to_string())?
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let titles: Vec<String> = with_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT title FROM task_archive WHERE updated_at < ?1"
        )?;
        let rows = stmt.query_map([&cutoff], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()
    })?;
    let mut count = 0u32;
    for title in titles {
        // memory_edit("delete", task_archive, ...) 同时清 yaml + 通过
        // mirror_archive_delete 清 SQLite。逐条删而非 bulk SQL 是为保证 yaml
        // 一致 + 触发 audit trail（与既有归档进入流程对称）。
        if crate::commands::memory::memory_edit(
            "delete".to_string(),
            "task_archive".to_string(),
            title,
            None,
            None,
        ).is_ok() {
            count += 1;
        }
    }
    Ok(count)
}
```

注册到 `lib.rs::invoke_handler`：`db::task_archive_purge_older_than`。

### `src/components/panel/PanelTasks.tsx`：归档 header 加 "🗑 清理" 按钮

紧跟现有「📋 导出 MD」/「刷新」按钮，加一个 "🗑 清理 >30 天" 按钮：

- 仅 archiveLoaded + archiveItems.length > 0 时显
- 二次确认：第一次点击进 armed 态（按钮文案变红 "确认清理 N 条"，5s 内再点真执行），五秒后自动 disarm
- 真执行：invoke `task_archive_purge_older_than({ days: 30 })` → 拿 count → reloadArchive → toast"已清理 N 条 >30 天归档"

不暴露天数配置（写死 30）—— 与 consolidate 既有 archive_retention 默认对齐，让"自动归档进入" 与 "手动清理删除"使用同一时间窗。

### 测试

`db.rs` 加 unit test：插 2 条旧 + 1 条新归档 → 调 `task_archive_purge_older_than(7)` → assert 仅旧条目被删（这条不直接调 memory_edit —— 改测纯 SQL 查询 select：把 SQL 部分抽出 helper `select_titles_older_than(conn, cutoff)`，单测它即可）。

## 不做

- 不暴露天数滑块 / 自定义参数：v1 写死 30；如有反馈再加配置
- 不动 consolidate 循环：archive_retention 控制"何时挪入"，本 PR 控制"何时删除"，两侧独立
- 不在 archive 行单条 delete 按钮：粒度过细 + 跟"归档是只读回看"语义冲突；用户想恢复用 ↩ 按钮，不想清单条而留其他

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅（含新 helper 测试）
- `npx tsc --noEmit` ✅
- 「任务」→「📦 归档」展开 → header 多出 "🗑 清理" 按钮
- 点一次 → 红字 armed 提示；5s 内再点 → 实际清理 + 列表刷新

## 完成

- [x] db.rs: select_archive_titles_older_than helper + task_archive_purge_older_than command + 2 单测
- [x] lib.rs: 注册命令
- [x] PanelTasks.tsx: armed 按钮 + invoke + reloadArchive
- [x] `cargo build --release` 通过
- [x] `cargo test --lib` 通过（907 passed，+2 新）
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
