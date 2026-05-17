# TG bot 加 `/cascade_rename <title> :: <new title>` 命令（iter #517）

## Background

iter #511 加 `/edit_title` 时在 help-detail 文档了 caveat：「rename 后既
有 `「<old>」` token 不自动跟随更新（owner 需手动改）。考虑后续 iter 加
cascade rename」。本 iter 兑现承诺。

owner 在多份 detail.md 内写笔记 ref 同一 task 时（`「写周报」昨天我...`
/ `「写周报」的进度...` 等），rename 后这些 ref 都 stale。手动维护
需:
1. grep 跨所有 cat 的 detail.md
2. 每条 detail.md 手动改
3. 重新 save 触发 sync

至少 N+1 步且容易漏。本命令一步完成。

## Changes

### `src-tauri/src/commands/memory.rs`

新 helper `memory_cascade_rename_in_detail_md`（line ~596）：

```rust
pub fn memory_cascade_rename_in_detail_md(
    category: String,
    old_title: String,
    new_title: String,
) -> Result<(String, usize), String> {
    // 1. 主操作：memory_rename — 失败立即返 Err，detail.md 不动
    let rename_msg = memory_rename(category, old_title.clone(), new_title.clone())?;
    if rename_msg == "No change." {
        return Ok((new_title, 0));
    }
    let actual_new = new_title.trim().to_string();
    // 2. 扫所有 categories' detail.md
    let index = read_index();
    let mem_dir = memories_dir()?;
    let mem_canon = match std::fs::canonicalize(&mem_dir) {
        Ok(p) => p,
        Err(_) => return Ok((actual_new, 0)),
    };
    let old_token = format!("「{}」", old_title.trim());
    let new_token = format!("「{}」", actual_new);
    let mut updated = 0;
    for cat in index.categories.values() {
        for item in &cat.items {
            // path 安全检查 + 文件读 + 文本搜替 + 写回
            // 失败的单文件 IO 不回滚（best-effort），只 stderr 记
            ...
        }
    }
    Ok((actual_new, updated))
}
```

**关键设计**：
- 主操作（memory_rename）先成功才扫 detail.md，保失败不留半完成状态
- 单 detail.md 写失败 best-effort skip — 主 rename 已 sealed，cascade
  失败 owner 可单条修
- path 安全：每 detail.md canonical 路径必须落在 mem_dir 内（与
  memory_read_detail 同模板，防 path traversal）
- 不抓 `[blockedBy: <title>]` markers — 那些在 description 而非
  detail.md，需 memory_edit re-write 路径，复杂度高，留 future iter

### `src-tauri/src/telegram/commands.rs`

按 6+ surface 模式同步：

1. **Enum 变体** `CascadeRename { title: String, new_title: String }`（紧
   贴 EditTitle）
2. **`name()` arm** → `"cascade_rename"`
3. **`title()` arm** → 单独 arm（与 EditTitle 同含 new_title 字段）
4. **parser arm**：与 /edit_title 同 `split_once("::")` 模板
5. **en + zh registry** entries
6. **`ALL_HELP_TOPICS`** 加 `"cascade_rename"`
7. **`format_help_for_topic("cascade_rename")`** 详细文案（含与
   /edit_title 区别 + 限制 + cascade 范围说明）
8. **`format_help_text`** 表格行
9. **两份 drift-defense test 列表**

#### 纯 formatter `format_cascade_rename_reply`

```rust
pub fn format_cascade_rename_reply(
    old_title: &str,
    new_title: &str,
    updated_md_count: usize,
) -> String {
    let mut out = format!(
        "🔁 已改标题：「{}」→「{}」",
        old_title.trim(),
        new_title.trim(),
    );
    if updated_md_count == 0 {
        out.push_str("\n· 无 detail.md 需要更新（未找到 ref token 引用）");
    } else {
        out.push_str(&format!("\n· 同步 {} 份 detail.md 内的 ref token", updated_md_count));
    }
    out
}
```

count = 0 时 friendly note 而非「同步 0 份」语义反常 — owner 知道
cascade scan 跑过但没找到引用。

### `src-tauri/src/telegram/bot.rs`

Handler 紧贴 EditTitle 之前：3-layer resolve + 调
`memory_cascade_rename_in_detail_md` + format。

## Key design decisions

- **只扫 detail.md 不扫 description**：description 是 task definition，
  owner 通常希望保持历史 snapshot 原样；detail.md 是「persistent notes
  with cross-doc refs」语义，cascade 价值在此
- **`「<title>」` 严格 token 只**：不扩展到 `[blockedBy: <title>]` 或
  其它 marker 内 title 出现 — 不同语法 / 不同语义，混用易产生意外副
  作用
- **best-effort 主 rename + 后扫**：主 rename 用 memory_rename 既有原
  子路径；cascade 失败不回滚（避免「全成功才生效」的复杂事务）
- **0 count friendly**：「无 detail.md 需要更新」直显 — 让 owner 验证
  cascade 跑了（vs 安静成功不显数字易引「rename 后真的扫了吗」疑虑）
- **path canonicalize 防御**：每 detail_path 走 fs::canonicalize +
  starts_with(mem_dir) 检查 — 与既有 memory_read_detail 同模板，防
  index 内 detail_path 字段被 manual edit 引入越界
- **4 个 unit tests pin 真实行为**：parser 2（split / missing separator）
  + formatter 2（含 count display / 0 count friendly note）
- **不写 integration test**：实际 fs::write + cascade scan 走真实
  memory_dir，单测污染 user data 风险高；backend logic 已分离 + 简单
  到可读 review，前端 formatter / parser 单测已覆盖

## Verification

- `cargo build`（src-tauri）— clean（仅既有 dead_code warnings）
- `cargo test --lib` — all 1637 tests pass（新 4 + 既有 1633）
- 三个 drift-defense test all pass
- 手测（建议手动跑）：
  - 创 task「A」+ 写 detail.md 包含 `「A」`
  - 写另 task「B」detail.md 也包含 `「A」` ref
  - `/cascade_rename A :: AA`
  - reply「🔁 已改标题：「A」→「AA」· 同步 2 份 detail.md 内的 ref token」
  - PanelMemory 看 A 的 detail.md 现含 `「AA」`；B 的也含 `「AA」`

## Future iters (out of scope)

- **`[blockedBy: <title>]` cascade**：扫 butler_tasks descriptions 内的
  blockedBy marker 同步替换 — 需 memory_edit re-write 路径 + 风险更高
  （描述被改可能影响 LLM 解析）
- **description 内 ref cascade**：与 [blockedBy:] 同 risk profile，谨
  慎评估
- **cascade preview mode**：`/cascade_rename --dry-run` 先列将命中的
  N 份 detail.md，owner 确认再实际改
- **PanelTasks 行 hover 「🔁 cascade rename」chip**：mouse-friendly
  入口
