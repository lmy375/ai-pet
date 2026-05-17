# PanelDebug 「⏱ ping TG bot」按钮（iter #406）

## Background

owner 想自查「TG bot 现在还在线吗 / 延迟多少 ms」无入口。既有信号
散且间接：
- `get_telegram_status` 只显 running boolean（不显延迟 / 不显 bot
  username 让 owner 确认 token 对应哪个 bot）
- TG warning banner 只在 startup 期失败时浮（已 connected 后不显）
- `/now` `/version` 是 TG 端命令（已 bot 工作才能用 — 不能反过来
  测「bot 起没起」）

本 iter 加只读 health check：调 Telegram `getMe` API + 计时往返
ms。与 reconnect / reset_tg_commands 分工 — 那两个改状态，本按钮
不改 bot 状态，专做「现状监测」。

## Changes

### `src-tauri/src/commands/telegram.rs`

#### 1. `TgPingResult` struct + `ping_tg_bot` 命令

```rust
#[derive(Clone, Serialize)]
pub struct TgPingResult {
    pub username: String,
    pub latency_ms: u64,
}

#[tauri::command]
pub async fn ping_tg_bot() -> Result<TgPingResult, String> {
    let settings = get_settings()?;
    let token = settings.telegram.bot_token.trim();
    if token.is_empty() {
        return Err("Telegram bot_token 未配置".to_string());
    }
    use teloxide::prelude::Requester;
    let bot = teloxide::Bot::new(token);
    let started = std::time::Instant::now();
    let me = bot.get_me().await
        .map_err(|e| format!("getMe 失败: {}", e))?;
    let latency_ms = started.elapsed().as_millis() as u64;
    let username = me.username.as_ref()
        .map(|u| format!("@{}", u))
        .unwrap_or_else(|| me.first_name.clone());
    Ok(TgPingResult { username, latency_ms })
}
```

设计：
- **临时 teloxide::Bot**：与 `reset_tg_commands` 同模式 — HTTP API
  idempotent，不依赖 TelegramStore dispatcher（启动后 Bot 已 move
  进 dispatcher 拿不回）。读 token 即认证同 bot。
- **`Instant::now()` 计时**：含 DNS + TLS + HTTP 整往返。比从应用
  外测（curl）更接近 TG bot 实际请求延迟（同 HTTP client / 同
  network stack）。
- **username fallback**：`Me.username` 是 Option（bot 名可空 — 极
  端情况），回落到 `first_name` 保证总有可显字串
- **不重试**：一次失败就报；让 owner 自己再点（与 reset / reconnect
  同 manual semantics）
- **`u64` latency**：延迟典型 100-2000 ms，u64 远超阈值；不用 u32
  防极端跨洋链路过界

### `src-tauri/src/lib.rs`

注册 `ping_tg_bot` 到 invoke handler 列表（紧贴 `reset_tg_commands`）。

### `src/components/panel/PanelDebug.tsx`

#### 1. State 新增

```ts
const [tgPingResult, setTgPingResult] = useState<{
  username: string;
  latency_ms: number;
} | null>(null);
const [tgPingError, setTgPingError] = useState<string | null>(null);
const [tgPingBusy, setTgPingBusy] = useState(false);
```

紧贴既有 `tgStartupWarnings` / `tgDismissed`（视觉上 TG 相关 state
聚簇）。

#### 2. UI: TG 链路 chip 行（紧贴 TG warning banner 之后）

```tsx
<div style={chipRow}>
  <span>TG 链路：</span>
  <button onClick={async () => {
    setTgPingBusy(true);
    setTgPingError(null);
    try {
      const r = await invoke<TgPingResult>("ping_tg_bot");
      setTgPingResult(r);
    } catch (e) {
      setTgPingResult(null);
      setTgPingError(String(e));
    } finally {
      setTgPingBusy(false);
    }
  }} disabled={tgPingBusy}>
    ⏱ ping
  </button>
  <span style={{ color: tone-aware, background: bg-aware }}>
    {busy ? "测量中…" : err ? "× <err>" : ok ? "<@uname> · <ms> ms" : "未测"}
  </span>
</div>
```

设计要点：
- **总是显（即使未测）**：让 owner 进 Panel 就看到「这里有个 ping」
  入口；不像 TG warning banner 仅失败时浮
- **三态视觉**：成功 = 绿 chip + `@username · N ms`；失败 = 红 chip
  + `× <reason>`；未测 = muted 灰
- **title attr 详尽**：hover 见「DNS+TLS+HTTP 往返」说明，让 owner
  知道这个 ms 度量含什么
- **busy 时 disabled + 「测量中…」**：防双击发两次请求，与既有
  reconnect 按钮 disabled 模式同
- **inline 而非新组件**：仅本处用，state 也在 PanelDebug 顶层 —
  抽 PanelDebugTgPingButton 组件要传 3-4 props + state lift，不
  划算

## Key design decisions

- **不显历史 ping 折线图**：单次 ping 已覆盖 95% 场景（owner 想
  「现在能用吗 / 多快」）；历史是 monitoring 系统职责非 debug 面板。
  若以后需要可基于此基础叠加。
- **不轮询自动 ping**：会消耗 TG bot API 配额（getMe 没具体限速但礼
  貌起见不浪费）；让 owner 主动点更明确"我现在想知道"
- **getMe 而非 sendMessage**：sendMessage 会真往某 chat 发消息，扰
  动用户；getMe 是 noop 自查 — 完美匹配 health check 语义
- **空 token 时 Err 而非按钮 disable**：让 owner 点了之后看到清楚
  错误「bot_token 未配置」+ 暗示去 Settings 配 — 比按钮静默灰更
  发现性强
- **不为单按钮引 unit test runner**：行为是 invoke + setState；后
  端 logic 极简 + IO 决定结果，cargo test --lib 全表通过 + 手测
  足够（PanelDebug 打开 → 看 chip 显「未测」→ 点 ⏱ → 看「测量中…」
  → 看绿 chip 显 @bot · N ms / 看红 chip 显错误）

## Verification

- `cargo build --lib`（backend）— clean（仅 unrelated pre-existing
  warnings）
- `cargo test --lib`（全表）— 1414 / 1414 通过
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
