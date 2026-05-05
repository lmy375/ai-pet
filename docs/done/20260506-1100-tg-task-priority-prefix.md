# Telegram /task 支持优先级前缀 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> Telegram `/task` 支持优先级前缀：`/task !! <title>` → P5；`/task !!! <title>` → P7；让 TG 用户对真正紧急的事不必回桌面调优先级。

## 目标

`/task` 当前固定 P3。但用户在 TG 委托紧急事时（"!! 报告今天交"），目前要么
等宠物自己评估（不可控），要么回桌面板手动调优先级（违背 TG 路径"快"的初衷）。
本轮加 `/task !! <title>` / `/task !!! <title>` 两档优先级前缀，前置 prefix
解析、保持其它行为不变。

## 非目标

- 不做 `!`（一档）/ `!!!!`（四档）—— 设计上只保留 "默认 / 紧迫 / 最紧迫"
  三档；档次太多让用户每次都犹豫"该用几个 ！"。
- 不做 due 前缀（如 `/task @18:00 ...`）—— due 输入需要时区 / 日期推断逻辑，
  不适合塞进单行命令；想精细回桌面板。
- 不动桌面默认 P3 —— 桌面有滑块，不需要前缀语法。

## 设计

### 优先级映射

| 前缀 | priority | 直觉 |
| --- | --- | --- |
| 无 | 3 | 日常 |
| `!!` | 5 | 紧迫（队列里会比日常先排） |
| `!!!` | 7 | 最紧迫（基本"今天就办"） |

理由：
- P3 / 5 / 7 与 LLM 工具描述里 "日常 1-3 / 紧迫 5-7 / 最高 8-9" 的档次表对齐。
- 三个 ！ 仍小于 9 —— 把 8/9 留给系统级紧急（DB 故障 / 用户告诉宠物"必须现
  在做"等极端语境），TG 命令不能直接拉到顶。

### 解析（pure，commands.rs）

新增 `parse_task_prefix(rest: &str) -> (priority: u8, title: String)`：
- 取 rest（已 trim）的首个 whitespace token 作 prefix 候选
- 命中 `!!!` → (7, 余下 trim)；命中 `!!` → (5, 余下 trim)
- 否则 → (3, rest 原样)

把它接到现有 `parse_tg_command` 的 `"task"` 分支：

```rust
"task" => {
    let (priority, real_title) = parse_task_prefix(rest);
    Some(TgCommand::Task { title: real_title, priority })
}
```

`TgCommand::Task` variant 加 `priority: u8` 字段。

### 处理（IO，bot.rs）

`TgCommand::Task` 分支用 `priority` 替代硬编码 3。其它逻辑不变。

### 文案

`format_task_created_success` 改签名 `(title: &str, priority: u8)`：
- 显示实际 P{n}，不再硬编码 P3。
- 内容其它不变。

`format_help_text` 加一行说明：
`/task !! <title>  —  紧迫 (P5)；/task !!! <title>  —  最紧迫 (P7)`

实际只增一行，不重复列 `/task <title>` 的描述（已存在）。

### 测试

`commands.rs`：
- parse_task_prefix("!! foo") → (5, "foo")
- parse_task_prefix("!!! foo") → (7, "foo")
- parse_task_prefix("foo") → (3, "foo")
- parse_task_prefix("!!") → (5, "")（让 handler 走 missing-argument）
- parse_task_prefix("!! foo bar") → (5, "foo bar")（多 token title 保留）
- parse_task_prefix("!!!! foo") → (3, "!!!! foo")（不识别 4 档，整体回退）
- parse_tg_command("/task !! 整理 Downloads") → Task { title="整理 Downloads", priority=5 }
- parse_tg_command("/task hello") → Task { title="hello", priority=3 }
- format_task_created_success("foo", 5) 文案含 "P5"

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | parse_task_prefix 纯函数 + 单测 |
| **M2** | TgCommand::Task 加 priority 字段 + parse_tg_command 接入 + 单测 |
| **M3** | format_task_created_success 签名扩展 + help 文本 + 单测更新 |
| **M4** | bot.rs 用 priority 替代硬编码 + cargo test + tsc + build + cleanup |

## 复用清单

- 既有 `format_task_description` / `append_origin_marker`
- 既有 missing-argument / format_command_error 路径
- 既有 task watcher origin 通知

## 进度日志

- 2026-05-06 11:00 — 创建本文档；准备 M1。
- 2026-05-06 11:10 — M1 完成。`parse_task_prefix` 纯函数 + 7 个单测覆盖 default/!!/!!!//4-bangs fallback/单!! fallback/empty/multi-token title。
- 2026-05-06 11:15 — M2 完成。`TgCommand::Task` 加 `priority: u8`；parse_tg_command 的 task 分支调用 parse_task_prefix 把 priority 顺出去；title()/name() accessors 跟随更新。
- 2026-05-06 11:20 — M3 完成。`format_task_created_success` 签名扩展 `(title, priority)` 显示实际 P{n}；help 文本加 `/task !! / !!!` 一行；help 测试加 P5/P7/!! 检查防回归。
- 2026-05-06 11:30 — M4 完成。bot.rs Task 分支用 `priority` 替代硬编码 3；`cargo test --lib` 930 通过（含新增 9 测）；`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过。归档至 done。
