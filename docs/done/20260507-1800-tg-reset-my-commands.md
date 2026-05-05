# TG bot reset_my_commands 命令 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG bot reset_my_commands 命令：用户重命名命令时希望 TG 客户端补全表清掉旧名；加面板按钮 / 命令 `reset_my_commands` 调 `bot.set_my_commands(vec![])` 清空，再下次 reconnect 重注册。

## 目标

bot 启动时调 `set_my_commands(...)` 把 5 条命令注册给 Telegram 服务器，
TG 客户端在用户输 `/` 时补全。但**重命名 / 删除某条命令**时旧名不会自
动消失 —— set_my_commands 是覆盖语义不假，但只要旧 process 还在跑、新
build 还没起，旧补全就持续存在。本轮加 PanelSettings 一个 "清空 TG 命
令补全" 按钮 → 调 `reset_tg_commands`（`set_my_commands(vec![])`）→
TG 客户端补全表清空，下次 reconnect 重注册新名。

## 非目标

- 不动 bot 启动时自动注册逻辑（已存在）。
- 不做 confirm 弹层 —— 操作可逆（重连后自动恢复），不需要重确认。
- 不在 TG 命令端开 `/reset_commands` —— 用户在 TG 里输错命令本身就触发
  unknown 反馈，再通过 TG 操作元命令绕一圈反而添乱。桌面 settings 是合
  适入口。

## 设计

### 后端

新 Tauri 命令 `reset_tg_commands`，独立于 TelegramStore：从 settings 读
token，新建一个临时 `teloxide::Bot` 调 `set_my_commands(vec![])`：

```rust
#[tauri::command]
pub async fn reset_tg_commands() -> Result<(), String> {
    let settings = get_settings()?;
    let token = settings.telegram.bot_token.trim();
    if token.is_empty() {
        return Err("Telegram bot_token 未配置".into());
    }
    let bot = teloxide::Bot::new(token);
    bot.set_my_commands(Vec::<teloxide::types::BotCommand>::new())
        .await
        .map_err(|e| format!("set_my_commands 清空失败: {}", e))?;
    Ok(())
}
```

**不**经现有 TelegramStore：那个 store 持的是 dispatcher shutdown token，
拿不到 Bot 实例（启动后 Bot 已 move 进 dispatcher）。从 token 新建一个临
时 Bot 调 set_my_commands 是 Telegram API 的 idempotent 操作，无副作用 ——
HTTP API 上同 token 即认证同 bot，跟 dispatcher 状态无关。

### 前端

PanelSettings Telegram 段「保存并连接」按钮旁加 "清空命令补全" 按钮。
点击 → 调 `reset_tg_commands` → 通过既有 `setMessage` 反馈结果。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | reset_tg_commands Tauri 命令 + lib.rs 注册 |
| **M2** | PanelSettings 按钮 + reset 处理 + ack |
| **M3** | cargo build + tsc + build + cleanup |

## 复用清单

- 既有 `get_settings()`
- 既有 PanelSettings setMessage 反馈通道
- 既有 btnSmallStyle

## 进度日志

- 2026-05-07 18:00 — 创建本文档；准备 M1。
- 2026-05-07 18:10 — M1 完成。`reset_tg_commands` Tauri 命令在 `commands/telegram.rs`：从 settings 读 token → 临时 teloxide Bot → `set_my_commands(vec![])`；不经 TelegramStore（dispatcher 拿不回 Bot 实例，HTTP API idempotent）。lib.rs invoke_handler 注册。
- 2026-05-07 18:20 — M2 完成。PanelSettings TG section 把 "保存并连接" 包到 flex row，前置 "清空命令补全" 灰按钮（slate `#64748b`）；新 `telegramResetting` state + 双按钮互斥 disabled；ack 走既有 setMessage 通道。
- 2026-05-07 18:25 — M3 完成。`cargo build` 8.17s 通过；`cargo test --lib` 957 通过；`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过。归档至 done。
