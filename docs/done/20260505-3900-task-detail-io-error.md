# 任务详情 IO 错误回退提示 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务详情时间线 IO 错误回退：`task_get_detail` 当前对 detail.md / butler_history 失败 silent fall back 空串，但前端没区分"真没数据"和"读失败"；加 metadata 字段让面板小字提示"读 detail.md 失败"以便排查。

## 目标

`task_get_detail` 当前 silent best-effort：
- detail.md 读不到（NotFound / Permission denied / 其它 IO）→ 空串
- butler_history 读不到 → 空 history

前端只看到"宠物还没写进度笔记" / "还没记录事件"，区分不了"真的空"和"读失败"。
排查"为什么我刚改的 detail 没显示"时只能去文件系统验。本轮在 TaskDetail 加 2
个 bool 字段把 IO 错误状态返给前端，详情段对应位置渲染红字 hint 让用户能看到。

NotFound 视作"真的没数据"（不是错误 —— 文件没生成是正常初始态）；其它 IO
错误视作真正的 fail。

## 非目标

- 不暴露详细错误内容（permission denied 等）—— 调试 panel 已有 LogStore 看
  详细 backtrace；详情页仅给"读失败"标志即可。
- 不为 raw_description 做 IO 回退（它从 memory_list 拿，不读单独文件）。
- 不写 README —— 任务详情可观察性补强。

## 设计

### 后端

`butler_history.rs` 加 `read_history_content_strict() -> Result<String, std::io::Error>`：
- 文件不存在 → Ok("")（视作"还没攒到数据"）
- 其它 IO 错误 → Err
- 既有 `read_history_content` 不动（其它 silent best-effort 调用方仍走那条路径）

`task.rs::TaskDetail` 加：
```rust
pub detail_md_io_error: bool,
pub history_io_error: bool,
```

`task_get_detail` 实现改为：
```rust
let (detail_md, detail_md_io_error) = match memory::memories_dir() {
    Ok(dir) => {
        let full = dir.join(&item.detail_path);
        match std::fs::read_to_string(&full) {
            Ok(s) => (s, false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), false),
            Err(_) => (String::new(), true),
        }
    }
    Err(_) => (String::new(), true),
};

let (history_content, history_io_error) =
    match crate::butler_history::read_history_content_strict().await {
        Ok(s) => (s, false),
        Err(_) => (String::new(), true),
    };
```

### 前端

`PanelTasks.tsx` `TaskDetail` interface 加两 boolean 字段。
进度笔记 / 事件时间线段落标题旁条件渲染红字 "⚠ 读 detail.md 失败"
/ "⚠ 读 butler_history 失败"。

## 测试

后端：`read_history_content_strict` IO 路径不易单测（需 mock fs）。`task_get_detail`
新字段写到 TaskDetail 序列化里 —— 编译能过即说明字段已经在 wire format。
不为 IO 路径写集成测试（与 detail.md 现有路径同等级"成本不值"）。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `read_history_content_strict` 后端新函数 |
| **M2** | TaskDetail 加字段 + task_get_detail 改实现 |
| **M3** | 前端 TaskDetail interface 加字段 + 详情段红字 hint |
| **M4** | cargo test + tsc + build + cleanup |

## 复用清单

- 既有 `memory::memories_dir`
- 既有 `read_history_content`（保留供其它 caller）

## 进度日志

- 2026-05-05 39:00 — 创建本文档；准备 M1。
- 2026-05-05 39:15 — 完成实现：
  - **M1**：`butler_history.rs` 加 `read_history_content_strict() -> std::io::Result<String>`：NotFound 视作 Ok("")（"还没攒到数据"），其它 IO 错误返 Err。既有 `read_history_content` 保留供 silent best-effort caller。
  - **M2**：`task.rs::TaskDetail` 加 `detail_md_io_error` / `history_io_error` 两 bool。`task_get_detail` 区分 detail.md NotFound（视作"还没生成"非错误）vs 其它 IO 错误（io_error=true）；history 走 strict 版本同样区分。
  - **M3**：`PanelTasks.tsx` `TaskDetail` interface 加两 boolean 字段；进度笔记 / 事件时间线段标题旁条件渲染 ⚠ 红字"读失败"标记 + tooltip 解释 NotFound 不触发。
  - **M4**：`cargo test --lib` 905/905 通过；`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 任务详情可观察性补强。
  - **设计取舍**：保留既有 `read_history_content` 让 consolidate / 周报等 silent caller 不受影响；NotFound != IO 错误（前者是"还没数据"正常初始态）；详细错误内容（permission / corrupt）不暴露给前端，需要用 LogStore 排查。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；后端 IO 路径 NotFound vs Other 由 std::io::ErrorKind 自带语义保证，前端是单向 boolean 渲染。
  - **TODO 后续**：列表清空后按规则提 5 条新候选。
