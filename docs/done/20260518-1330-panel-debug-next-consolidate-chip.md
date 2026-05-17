# PanelDebug 加「⏰ 下次 consolidate」chip（iter #475）

## Background

PanelDebug 已有「🧹 force consolidate」按钮 + 「📊 今日决策」chip。但
缺一个 audit-cron-rhythm 入口：**下次 consolidate sweep 何时跑**。
owner 想确认「cron 是不是还在动 / 还要等多久 / interval 配的对吗」时
没法直接看见 — 要 grep app.log 找上次 "Consolidate" 行算。

本 iter 加「⏰ HH:MM (N 分后)」chip — 显 spawn loop 下次苏醒时刻 +
剩余时间 + interval / enabled 状态。

## Changes

### `src-tauri/src/consolidate.rs`

#### 1. `NEXT_RUN_AT` 静态原子 + helper

```rust
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

static NEXT_RUN_AT: AtomicI64 = AtomicI64::new(0);

fn set_next_run_at(secs_from_now: u64) {
    let now_secs = chrono::Local::now().timestamp();
    NEXT_RUN_AT.store(now_secs.saturating_add(secs_from_now as i64), Ordering::SeqCst);
}
```

- 0 = 未初始化（app 启动 120s 内 / loop 未 spawn）
- Unix epoch seconds — `chrono::Local::now().timestamp()` 兼容 Rust / JS

#### 2. spawn() 内每次 sleep 前调 `set_next_run_at`

```rust
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        set_next_run_at(120);
        tokio::time::sleep(Duration::from_secs(120)).await;
        loop {
            let settings = match get_settings() {
                Ok(s) => s,
                Err(_) => {
                    set_next_run_at(3600);
                    tokio::time::sleep(Duration::from_secs(3600)).await;
                    continue;
                }
            };
            // ...
            if !cfg.enabled {
                set_next_run_at(interval_secs);
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;
                continue;
            }
            // ... run consolidation
            set_next_run_at(interval_secs);
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        }
    });
}
```

三处 sleep 前都 set — 让 ETA 始终反映"下次 loop 苏醒时刻"。enabled=false
路径也 set（loop 仍周期苏醒检查 settings，只是不实际 run consolidation）。

#### 3. `get_consolidate_schedule` Tauri 命令

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ConsolidateSchedule {
    pub next_eta_unix_secs: i64,
    pub interval_hours: u64,
    pub enabled: bool,
}

#[tauri::command]
pub fn get_consolidate_schedule() -> ConsolidateSchedule;
```

返三字段 — eta + interval + enabled — 让 frontend chip 显完整 cron
上下文（interval 配置 + 启用状态）。

### `src-tauri/src/lib.rs`

注册命令到 invoke_handler.

### `src/components/panel/PanelDebug.tsx`

#### 1. State + 30s poll

```ts
const [consolidateSched, setConsolidateSched] = useState<{
  nextEtaUnixSecs: number;
  intervalHours: number;
  enabled: boolean;
} | null>(null);

useEffect(() => {
  // mount fetch + 30s polling
}, []);
```

#### 2. Toolbar chip（紧贴 🧹 force consolidate 之前）

```tsx
{consolidateSched && (() => {
  const eta = consolidateSched.nextEtaUnixSecs;
  if (eta === 0) {
    label = "⏰ 等待初始化…";
  } else {
    const remainingMin = Math.max(0, Math.floor((eta - nowSec) / 60));
    const etaTime = new Date(eta * 1000);
    label = `⏰ HH:MM (N 分后 / N 小时后 / N 天后)`;
  }
  return <span title={...detailedTooltip}>{label}</span>;
})()}
```

- 三级 remaining 表达：< 60min → "N 分后" / < 24h → "N 小时 M 分后" /
  ≥ 24h → "N 天 M 小时后"
- enabled=false 时 chip 改用 amber color — 让 owner 一眼看「这条 cron
  跑也不会真 sweep」状态
- tooltip 含完整本地时刻 + interval / enabled 解释 + 「点 force
  consolidate 手动触发不必等」hint

## Key design decisions

- **AtomicI64 NEXT_RUN_AT 而非 channel / event**：static 原子 read-cost
  接近 0 — 30s poll 走 invoke 不会被原子 contention 卡。channel 需保
  state；event push 需 frontend 注册 listener — 都过度
- **ETA 是「下次苏醒时刻」而非「下次实际 sweep 时刻」**：enabled=false
  时 loop 仍周期苏醒 check settings，只是不 sweep。ETA 反映 loop 行
  为本身 — owner 看到 ETA 涨 / 不动可分别诊断「loop 还在 / loop 死」。
  enabled=false 时 chip 用 amber color clarify
- **0 = 未初始化的 sentinel 值**：app 启动 120s 内 / loop 没 spawn 时
  ETA = 0。chip 显「等待初始化…」让 owner 知道 "我刚启动，等一会"
- **三级 remaining 表达**：与既有 ChatMini 「⏱ 沉默 N 分」chip / TG
  /last_speech 同 tiered display 协议
- **不引手动「重置 next ETA」按钮**：本 chip 是 audit 入口，不是操作
  入口。想立即跑走 force consolidate 按钮；想改 interval 走 PanelSettings
- **30s polling 节奏**：与 PanelDebug 其它 polling chip（📊 1h tokens
  / 📊 今日决策）对齐；ETA 分钟级粒度 30s 滞后无感
- **不写 unit test**：纯 AtomicI64 read + Tauri stitching + JSX 字符串
  拼接；逻辑 trivial（既有 consolidate 行为 production 验证）。GOAL.md
  "meaningful tests only" 规则下不引装饰性测试

## Verification

- `cargo build --lib` — clean
- `cargo test --lib`（全表）— unchanged (1548 / 1548 仍通过)
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.29s)
- 手测：PanelDebug toolbar → 看「⏰ HH:MM (N 分后)」chip 在 🧹 force
  consolidate 左侧 → hover tooltip 显完整本地 ETA + interval / enabled +
  跳转 hint；app 刚启动时显「⏰ 等待初始化…」
