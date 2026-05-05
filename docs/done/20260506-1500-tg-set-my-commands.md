# TG 自动补全提示 (setMyCommands) — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG 自动补全提示：bot 启动时调用 `bot.setMyCommands` 把 `/task /tasks /cancel /retry /help` 注册给 Telegram，让用户在输入框打 `/` 就能看到候选列表。

## 目标

bot 现在已有 5 条命令（`/task /tasks /cancel /retry /help`），用户必须先发
`/help` 才能知道。Telegram 提供 `setMyCommands` API：bot 启动时把命令清单
推到服务器，TG 客户端在输入 `/` 时就会弹出候选列表 + 描述。一次 IO 成本，
长期降低发现成本。

## 非目标

- 不做 per-chat / per-language 的命令清单（teloxide 支持 `scope` /
  `language_code`，但这个 bot 只服务一个用户，没必要）。
- 不动 `/help` 文本 —— TG 自带的清单是给"打字时"看的；`/help` 是给"已经
  在用 bot 想全面回顾命令"的场景，两个互补不冲突。
- 启动失败不导致 bot 启动失败 —— `setMyCommands` 是装饰性的，注册失败
  （网络抖动 / Telegram 后端故障）只 log 不抛错，bot 本体仍能正常起。

## 设计

### 命令清单（pure，commands.rs）

`pub fn tg_command_registry() -> Vec<(&'static str, &'static str)>` 返回
`(name, description)` 序列：

```rust
[
    ("task", "把单条任务塞进队列（!! P5 / !!! P7）"),
    ("tasks", "列出本会话派出的任务清单"),
    ("cancel", "取消指定任务"),
    ("retry", "把失败任务重置回 pending"),
    ("help", "显示完整命令帮助"),
]
```

**不放在 `format_help_text`**：那是给用户看的多行文本，命令清单是给 TG
API 用的结构化数据，两者格式不同。但我们让 `format_help_text` 的测试能
间接保证两边名字一致 —— 不强加耦合，只在出现 drift 时人工注意。

`description` 长度 ≤ 256（Telegram 限制；我们的描述都很短，留白）。
中文混合 ASCII 不会触发限制。

### bot.rs 注册

在 `get_me()` 成功后，spawn 一个轻量异步：

```rust
let cmds: Vec<BotCommand> = tg_command_registry()
    .into_iter()
    .map(|(name, desc)| BotCommand::new(name, desc))
    .collect();
match bot.set_my_commands(cmds).await {
    Ok(_) => eprintln!("Telegram commands registered for autocomplete"),
    Err(e) => eprintln!("set_my_commands failed (non-fatal): {}", e),
}
```

**同步等还是 spawn**：同步等。理由：
- API 调用快（单次 HTTPS 往返），用户感知不到。
- 失败 log 即可，不阻断 bot 启动。
- spawn 后 bot 已可接收消息，但用户可能还没看到自动补全 —— 同步等让"启动
  完成 = 自动补全已生效"。

### 测试

`commands.rs`：
- `tg_command_registry()` 长度 == 5；首条是 task；包含 tasks/cancel/retry/help
- 每条 description 非空 + ≤ 256 字符
- name 全部 lowercase（TG 命令名约束）

不测网络层（set_my_commands）—— teloxide 已有自己的 fixture，我们在 IO
边界写测试只是耦合 framework，无意义。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | tg_command_registry 纯函数 + 单测 |
| **M2** | bot.rs 在 get_me 后调 set_my_commands；失败 log |
| **M3** | cargo test + tsc + build + cleanup |

## 复用清单

- 既有 `format_help_text` 文案语调
- 既有 `eprintln!` log 模式（与 bot 启动其它步骤同源）

## 进度日志

- 2026-05-06 15:00 — 创建本文档；准备 M1。
- 2026-05-06 15:10 — M1 完成。`tg_command_registry()` 纯函数返回 5 条 (name, description) 元组，按"用户输 / 时看到的顺序"排列（task 在前 / help 在末）；3 个单测：覆盖矩阵、首末顺序、TG 限制（name lowercase ASCII ≤ 32 / desc ≤ 256）。
- 2026-05-06 15:15 — M2 完成。bot.rs 在 `get_me()` 之后调 `bot.set_my_commands()`；BotCommand::new 映射 registry；失败 eprintln 不阻断启动。
- 2026-05-06 15:20 — M3 完成。`cargo test --lib` 933 通过（含新增 3 测）；`cargo build` 6.84s 通过；`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过。归档至 done。
