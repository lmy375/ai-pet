# TG /cancel /retry 模糊匹配 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG `/cancel` 模糊匹配：当前 `/cancel <title>` 必须精确字符；改成"找最近的 fuzzy 匹配 ≥ 1"，找到则 cancel + 把实际匹配 title 回显给用户确认（避免长 title 全键入）。

## 目标

TG `/cancel` 与 `/retry` 命令当前要求精确字符 match —— 中文长 title 在手机
键盘里是噩梦（全角空格 / 错字一律 fail）。本轮加 fuzzy substring 匹配：
- 优先精确（trim 后字面相等）
- 次选 case-insensitive substring（query 是某 title 的子串）
- 唯一命中 → 用 actual title 调既有 `task_cancel_inner` / `task_retry_inner`，
  反馈里展示实际匹配的 title 让用户知道命中了哪条
- 0 命中 / 多命中 → 错误反馈带候选列表

要求一并对 `/retry` 应用 —— 二者 UX 痛点对称，共享 fuzzy 解析路径无成本。

## 非目标

- 不做 typo 容忍（编辑距离）—— substring 已覆盖 90% 实战场景，引入 Levenshtein
  会让"整理" 匹配到 "晚饭"（短 title vs query 距离都小）的反直觉case。
- 不做拼音 / 部首匹配 —— 中文 LLM 任务 title 多为整段中文，substring 一般够。
- 不写 README —— TG 命令体验补强。

## 设计

### Pure 解析

`telegram/commands.rs` 加：

```rust
pub enum FuzzyMatch {
    Exact(String),       // trim 后字面相等的 title
    Single(String),      // 唯一 substring 命中
    None,
    Ambiguous(Vec<String>),  // 多 substring 命中（最多保留前 5 条给反馈展示）
}

pub fn find_task_fuzzy(query: &str, titles: &[String]) -> FuzzyMatch;
pub fn format_ambiguous_match(query: &str, candidates: &[String]) -> String;
```

`find_task_fuzzy`：
1. trim query；空 → None
2. 找 trim 后字面 == query 的 title → Exact
3. 否则 collect case-insensitive substring 命中 → 0/1/N 分别返回 None /
   Single / Ambiguous（最多 5 条）

`format_ambiguous_match` 渲染：

```
「整理」匹配多个任务：
• 整理 Downloads
• 整理 Documents
请用更精确的标题再试。
```

### bot.rs 接线

`bot.rs::handle_tg_command` 的 Cancel / Retry 分支前先做 fuzzy resolve：

```rust
fn resolve_tg_task_title(query: &str) -> Result<String, String> {
    let titles = read_butler_task_titles();
    match find_task_fuzzy(query, &titles) {
        FuzzyMatch::Exact(t) | FuzzyMatch::Single(t) => Ok(t),
        FuzzyMatch::None => Err(format!("找不到任务「{}」", query.trim())),
        FuzzyMatch::Ambiguous(list) => Err(format_ambiguous_match(query, &list)),
    }
}
```

`read_butler_task_titles()` 是一行 helper：读 `memory_list("butler_tasks")`
→ items 的 title vec。

Cancel / Retry 分支：

```rust
TgCommand::Cancel { title } => match resolve_tg_task_title(&title) {
    Ok(actual) => match task_cancel_inner(actual.clone(), ..., decisions) {
        Ok(()) => format_command_success("cancel", &actual),
        Err(e) => format_command_error(&e),
    },
    Err(msg) => format_command_error(&msg),
},
```

成功反馈用 `format_command_success("cancel", &actual)`：actual 是 fuzzy 命中
的真实 title，让用户立刻看到"我说 /cancel 整理 → 实际取消的是 整理 Downloads"。

### 测试

`find_task_fuzzy` 全 pure：
- 空 query → None
- 精确命中（trim 测试）
- 唯一 substring 命中
- 大小写不敏感
- 多 substring → Ambiguous（保留全部候选 ≤ 5）
- 0 命中 → None
- substring 命中也优先返 Exact 而非 Single（边界）

`format_ambiguous_match` pure：query / 候选都出现 + 每条带 bullet。

bot.rs 集成测试不写（与既有 cancel/retry inner IO 路径一致，成本不值）。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `FuzzyMatch` enum + `find_task_fuzzy` + `format_ambiguous_match` + 单测 |
| **M2** | bot.rs `read_butler_task_titles` + `resolve_tg_task_title` + Cancel/Retry 分支接入 |
| **M3** | cargo test + cleanup |

## 复用清单

- 既有 `task_cancel_inner` / `task_retry_inner`（接 actual title 即可）
- `memory_list` 读 butler_tasks
- `format_command_success` / `format_command_error` 文案 helpers

## 待用户裁定的开放问题

- substring 匹配排序：本轮按 input 顺序展示候选（list 已稳定）。如反馈想按
  长度 / 命中位置打分再加。
- 上限 5 候选：实战极少 > 5，超出截断 + 加 "…等" 提示。

## 进度日志

- 2026-05-05 28:00 — 创建本文档；准备 M1。
- 2026-05-05 28:30 — 完成实现：
  - **M1**：`telegram/commands.rs` 加 `FuzzyMatch` enum (Exact / Single / None / Ambiguous) + `find_task_fuzzy` pure 解析（trim query → 精确字面相等优先 → case-insensitive substring → 0/1/N 分支）+ `format_ambiguous_match` 文案 helper（最多 5 条预览，超出 "…等 N 条"）。9 条新增单测覆盖空 query / Exact 优先级 / trim / 大小写不敏感 / Ambiguous 全候选 / None / format 含 query 与 bullets / 截断提示。
  - **M2**：`bot.rs` 加 `resolve_tg_task_title(query)` + `read_butler_task_titles()` 工具函数；`handle_tg_command::Cancel` / `Retry` 分支重写：先 fuzzy resolve → 唯一命中调既有 `task_cancel_inner` / `task_retry_inner`（用 actual title 而非用户输入），成功反馈展示 actual title 让用户知道命中了哪条；0/多 命中 → format_command_error 包装成 ⚠️ 提示。
  - **M3**：`cargo test --lib` 898/898（+9）通过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— TG 命令体验补强，与 cancel/retry/tasks/help 系列同性质。
  - **设计取舍**：仅 substring 不做 typo / 拼音 / 部首匹配（实战 90% 场景 substring 已覆盖，引入编辑距离会让 "整理" 误命中 "晚饭" 这类反直觉 case）；将 fuzzy 仅放在 TG 层（不动桌面 task_cancel_inner / task_retry_inner，desktop 用户精确输入语义不变）；/cancel 与 /retry 共享 resolve 路径（同种 UX 痛点共享同种修复）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；解析层 9 条单测含全部边界，IO 层是 read titles + 既有 inner 调用的薄包装。
