# TG bot 启动失败汇总到 PanelDebug — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG bot 启动失败汇总到 PanelDebug：现 `set_my_commands` 等失败仅 eprintln；用户看不到。新增内存 `tg_startup_warnings` Vec + Tauri 命令 + PanelDebug 区域提示，让用户知道为啥自动补全没出现。

## 目标

TG bot 启动路径有两条 non-fatal 失败：
1. `bot.set_my_commands()` 失败 → 自动补全不工作（既有 eprintln）
2. `TelegramBot::start()` 失败 → bot 整体不工作（既有 eprintln）

`reconnect_telegram` 命令的失败已能通过 `TelegramStatus.error` 反馈到前端
（toast / 状态栏），但**冷启动**时这条路径绕过了：lib.rs setup 里的
`TelegramBot::start` 只 eprintln 不持久。用户重启后看不到 `bot started`
也猜不到原因。

本轮新建 in-memory `TgStartupWarningStore`，把启动期间的非 fatal 失败统
一收集，配 Tauri 命令暴露给前端；PanelDebug 顶部如有 warning 显示一条
醒目橙色 banner。

## 非目标

- 不持久化到磁盘 —— 启动告警是当次进程的事，重启清空合理。
- 不替代 `TelegramStatus.error` —— 那是 `reconnect_telegram` 的同步反馈
  通道；本轮做的是冷启动 / 后续异步阶段（如 set_my_commands）的"事后
  归档"。两者互补。
- 不做 dismiss / clear 按钮 —— 信息量小，进程重启自动清；加按钮反而让
  用户以为可以主动忽略问题。

## 设计

### 数据结构

```rust
// telegram/warnings.rs
#[derive(Debug, Clone, Serialize)]
pub struct TgStartupWarning {
    pub timestamp: String,   // RFC3339 (Local)
    pub kind: String,        // "bot_start" / "set_my_commands" / 其它
    pub message: String,     // 原始 error string
}

pub type TgStartupWarningStore = Arc<std::sync::Mutex<Vec<TgStartupWarning>>>;

pub fn new_store() -> TgStartupWarningStore { ... }

pub fn push(store: &TgStartupWarningStore, kind: &str, message: String);

pub fn snapshot(store: &TgStartupWarningStore) -> Vec<TgStartupWarning>;
```

`std::sync::Mutex`（不是 tokio）—— 操作 push/snapshot 都纳秒级，不需要
async；同时方便从同步 lib.rs setup hook 直接 push。

### lib.rs

- `manage(telegram::warnings::new_store())`
- TelegramBot::start 失败时 push warning
- TelegramBot::start 成功后，`set_my_commands` 已经在 bot.rs 内部 spawn；
  把 store 传进去让它 push warning（替代 eprintln）

### bot.rs

- `TelegramBot::start` 接受 `TgStartupWarningStore` 参数
- `set_my_commands` 失败时 push 而非仅 eprintln（保留 eprintln 给 dev 控
  制台）

### Tauri 命令

```rust
#[tauri::command]
pub fn get_tg_startup_warnings(
    store: State<'_, TgStartupWarningStore>,
) -> Vec<TgStartupWarning>;
```

### 前端

PanelDebug 顶部新增一条 banner：当 warnings 非空时显示橙色块，列出每条
`{kind} · {message}`。每 5s 刷新（与既有 fetchAll 周期一致）。

```tsx
{tgWarnings.length > 0 && (
  <div style={{ background: "#fff7ed", padding: "8px 16px", ... }}>
    <span>⚠ Telegram 启动告警 ({tgWarnings.length})</span>
    {tgWarnings.map(w => (
      <div>{w.kind}: {w.message}</div>
    ))}
  </div>
)}
```

### 调用点

- 冷启动 lib.rs setup：`TelegramBot::start` 失败 → push("bot_start", ...)
- bot.rs `set_my_commands` 失败 → push("set_my_commands", ...)
- `reconnect_telegram` 失败 → 不 push（已有 TelegramStatus.error 反馈
  入口；避免重复打扰）

## 测试

`telegram::warnings::push` / `snapshot` 是 thin wrapper around Mutex
+ Vec；写防御性测试只是耦合 std；不写。手测覆盖：
1. 故意 break bot_token → 重启 → PanelDebug 应见 warning
2. 正常 token → 应无 warning

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | warnings 模块 (struct + store + push + snapshot) |
| **M2** | TelegramBot::start 接受 store；冷启动 / 重连失败 push |
| **M3** | set_my_commands 失败 push（保留 eprintln 给 dev 日志） |
| **M4** | Tauri 命令 + lib.rs 注册 |
| **M5** | PanelDebug 顶部 banner |
| **M6** | cargo build + cargo test + tsc + build + cleanup |

## 复用清单

- 既有 `eprintln!` 模式（保留作为 dev 兜底）
- 既有 PanelDebug 顶部状态栏布局
- 既有 fetchAll 5s 周期

## 进度日志

- 2026-05-07 04:00 — 创建本文档；准备 M1。
- 2026-05-07 04:10 — M1 完成。`telegram::warnings` 模块 + `TgStartupWarning` struct + `TgStartupWarningStore = Arc<Mutex<Vec<...>>>` (std sync mutex) + push / snapshot 函数 + 3 个单测（round-trip / clone 隔离 / 空 store）。
- 2026-05-07 04:15 — M2 完成。`TelegramBot::start` 接受 `TgStartupWarningStore` 参数；lib.rs setup 失败时 push("bot_start", ...)；`reconnect_telegram` 命令同步加 warnings 入参（reconnect 失败仍走 TelegramStatus.error，不重复 push 避免打扰）。
- 2026-05-07 04:18 — M3 完成。`set_my_commands` 失败保留 eprintln 给 dev console，加 push("set_my_commands", ...) 写到 store。
- 2026-05-07 04:20 — M4 完成。`get_tg_startup_warnings` Tauri 命令 + lib.rs 注册 + manage(new_store())。
- 2026-05-07 04:25 — M5 完成。PanelDebug 新增 `tgStartupWarnings` state + fetchLogs 内单独 invoke（避免改 get_debug_snapshot bundle 签名）；空时不渲染；非空时顶部橙色 banner 列出每条 `{kind}: {message}`，timestamp 入 title hover。
- 2026-05-07 04:30 — M6 完成。`cargo build` 7.93s 通过；`cargo test --lib` 948 通过（含新增 3 测）；`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过。归档至 done。
