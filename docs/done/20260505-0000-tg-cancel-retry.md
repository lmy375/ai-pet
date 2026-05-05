# TG /cancel 与 /retry 命令 — 开发计划

> 对应需求（来自 docs/TODO.md「已确认」）：
> TG /cancel 与 /retry 命令：用户在 TG 误派或想改，发 `/cancel <title>` 或 `/retry <title>` 直接走已有 Tauri 命令，与派单形成完整闭环。

## 目标

让 Telegram 用户能在不切回桌面的前提下完成"派单 → 误派 / 失败 → 取消 / 重试 → 通知"全闭环。新加两条 TG 命令：

- `/cancel <title>` — 把指定标题的任务标 `[cancelled]`（无原因），通过现有 `task_cancel` 逻辑
- `/retry <title>` — 把指定标题的任务剥掉 error 标记回到 pending，通过现有 `task_retry` 逻辑

回复用 emoji + 简短句子（与 watcher 通知的 ✅/⚠️/🚫 风格保持一致）。

## 非目标

- 不做带原因的 cancel —— TG 输入框单行命令不适合多 token；要给原因等用户去面板。
- 不做模糊匹配 / 标题补全 —— 用户必须输入精确 title。错配返回 "task not found"。
- 不做 /help 文档 —— 命令在 TG dispatch system layer 注入里已经"自描述"（LLM 看到能解释）。后续如果用户反馈想要 /help 再加。
- 不做 /tasks 查询 —— 那是另一条 TODO，等下一轮。本轮只做"动作"两条。
- 不写 README —— TG 派单条目下面已经隐含闭环，不需要再加亮点。

## 设计

### 命令解析（pure）

新增 `telegram::commands` 子模块（或在 `bot.rs` 里定义；倾向独立模块便于单测）：

```rust
pub enum TgCommand {
    Cancel { title: String },
    Retry { title: String },
    Unknown { name: String },
}

pub fn parse_tg_command(text: &str) -> Option<TgCommand>
```

- 输入需以 `/` 开头才视作命令；否则返回 None（让 chat pipeline 接管）
- 取首个空白前的 token 作 cmd name（lower-case），剩余 trim 后作参数
- 未知命令返回 `Unknown { name }`，handler 给 TG 用户一条 "未知命令" 回复（避免静默吞掉）
- 空参数（`/cancel` 单独发）→ 视为参数缺失，handler 回 "请带任务标题"

### 调度

在 `handle_message` 开头：先 `parse_tg_command`，命中 → 走 `handle_tg_command`；未命中 → 现有 chat pipeline。

```rust
if let Some(cmd) = parse_tg_command(&text) {
    handle_tg_command(&bot, msg.chat.id, cmd, &state).await?;
    return Ok(());
}
```

### 命令执行

`task_cancel` / `task_retry` 是 `#[tauri::command]` 标注的函数，但同时是普通 Rust fn —— 直接从 bot 里调用就行。`tauri::State` 参数从 `state.app.state::<DecisionLogStore>()` 拿。

通用结果格式（pure helpers）：
- 成功 cancel: `"🚫 已取消「{title}」"`
- 成功 retry: `"🔄 已重置「{title}」回 pending，下一轮会重新尝试"`
- 失败（不存在 / 状态不对 / 其它 Err）: `"⚠️ 操作失败：{err}"`
- 未知命令: `"未知命令 /{name}。可用：/cancel <title>, /retry <title>"`
- 缺参: `"用法：/{cmd} <任务标题>"`

### 测试

`parse_tg_command` 全 pure，单测覆盖：
- /cancel <title> / /retry <title> 正确识别
- 标题含空格 / 中文 / quote 都正确解析
- 大小写不敏感（/CANCEL）
- 未知命令 → Unknown
- 空参数 → 仍返回命令但 title 为空字符串（handler 自己判断）
- 非 / 开头 → None

文案 helpers 也 pure 单测（success / err / unknown / missing-arg 各一）。

实际 IO 路径（调用 task_cancel / task_retry）不写集成测试 —— 那需要 mock memory + DecisionLogStore，成本不值得。手动验证。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `telegram::commands` 模块 + parse + 文案 helpers + 单测 |
| **M2** | `handle_tg_command` async 调度 + 接入 handle_message |
| **M3** | 验证（cargo test + tsc）+ 收尾（TODO + done/） |

## 复用清单

- `commands::task::{task_cancel, task_retry}` —— 直接调
- `decision_log::DecisionLogStore` —— 通过 `state.app.state::<...>()` 获取
- `teloxide::Bot::send_message`

## 待用户裁定的开放问题

1. **是否需要 quote 包裹**（`/cancel "整理 Downloads"` vs `/cancel 整理 Downloads`）：本轮选后者（trim 后的整段）。如果用户反馈想要 quote 解析再加。
2. **未知命令是否回 /help 列表**：本轮回简短 "未知命令 /xxx" + 列出 cancel / retry。help 不单独建命令。
3. **大小写**：本轮 cmd name lower-case。/CANCEL = /cancel。

## 进度日志

- 2026-05-05 00:00 — 创建本文档；准备进入 M1。
- 2026-05-05 00:25 — 完成实现：
  - **M1**：`telegram::commands` 新模块。`parse_tg_command(text)` 处理 `/` 前缀、cmd 小写化、空白切分、未知 cmd → `Unknown`。文案 helpers `format_command_success` / `_error` / `_unknown_command` / `_missing_argument` 全 pure。16 条单测。
  - **M2**：`commands/task.rs` 把 `task_retry` / `task_cancel` 拆出 `_inner` 版本（接 `DecisionLogStore` 而非 `tauri::State`），原 Tauri 命令改薄包装。`bot.rs::handle_message` 在 auth 检查后立刻分流：命中命令 → `handle_tg_command` 处理 4 种情况（缺参 / cancel / retry / unknown）后 return；未命中走 chat pipeline。
  - **M3**：`cargo test --lib` 803/803 通过；`tsc --noEmit` 干净。TODO 移除条目；本文件移入 `docs/done/`。
  - **过程注记**：实现到一半发现 `tauri::State` 不能轻易在非 invoke 路径构造；把每个 Tauri 命令拆成 `cmd` + `cmd_inner` 双形式 —— 几行代码 cost，消除了"非 invoke 路径调 Tauri 命令逻辑"的障碍。
  - **README 不更新** —— TG 派单条目已写过"派单 → 执行 → 回传"闭环，本轮只是把"取消 / 重试"动作也接到 TG，属既有亮点的延伸而非新功能。
  - **副产品**：本轮过程中用户重写了 `docs/TODO.md` 顶部规则 —— 移除 已确认 / 待确认 分段，改成单一列表 + "之后进入开发流水线"。意味着列表里的任何条目都可以直接做，不再需要"等用户移动"的 gating。
