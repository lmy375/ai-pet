# PanelDebug snapshot 加「⌚ 已运行 N 分」字段（iter #448）

## Background

PanelDebug 既有 📸 抓快照 A 按钮 dump 全状态 markdown — 含 app version /
schema version / 平台 / 任务状态 / 工具缓存 / 心情 motion / mute / 决策日
志计数等。owner 排查长跑性能 / 内存问题 / "为啥 task scheduler 不响应了"
时常缺关键一项：**pet 已运行多久**。

iter #448 加「已运行 N 分」字段到 snapshot 环境段：长跑 audit 入口（区分
「刚启动 5 分」vs「跑了 36 小时」是两套排查路径）；与既有 `## 环境` 段
其他字段同形态。

## Changes

### `src-tauri/src/commands/debug.rs`

#### 1. `BOOT_TIME` 静态锚点

```rust
use std::sync::LazyLock;
use std::time::Instant;

pub static BOOT_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

#[tauri::command]
pub fn get_process_uptime_secs() -> u64 {
    BOOT_TIME.elapsed().as_secs()
}
```

`LazyLock<Instant>` 与既有 proactive.rs 的 `LAST_SEEN_BUTLER_DONE_TITLES`
同模板。首次访问时锁定当前时刻；调用方在 `lib.rs::setup()` 强制 eval
让锚定贴近进程启动点（不到 1ms 误差，对"小时/天"粒度无影响）。

### `src-tauri/src/lib.rs`

#### 2. 强制 init BOOT_TIME + 注册命令

```rust
// 强制 eval BOOT_TIME LazyLock — 让 uptime 锚点贴近 main() 入口
let _ = *commands::debug::BOOT_TIME;

tauri::Builder::default()
  ...
  .invoke_handler(tauri::generate_handler![
    ...
    commands::debug::get_process_uptime_secs,
    ...
  ])
```

eval 前置在 `tauri::Builder::default()` 之前 — 比 setup() callback 早，
更接近 process 启动时刻；setup callback 内 init 也行但实操差 < 50ms 无差。

### `src/components/panel/PanelDebug.tsx`

#### 3. envInfo 加 `bootedAtMs`

```ts
const [envInfo, setEnvInfo] = useState<{
  appVersion: string;
  schemaVersion: number;
  bootedAtMs: number | null;
} | null>(null);

useEffect(() => {
  ...
  const [v, s, u] = await Promise.all([
    invoke<string>("app_version").catch(() => ""),
    invoke<{ schema_version: number }>("get_db_stats").then((d) => d.schema_version).catch(() => 0),
    invoke<number>("get_process_uptime_secs").catch(() => -1),
  ]);
  const bootedAtMs = u >= 0 ? Date.now() - u * 1000 : null;
  setEnvInfo({ appVersion: v, schemaVersion: s, bootedAtMs });
}, []);
```

设计要点：
- **mount 时 fetch 一次 + 客户端 derive 锚点**：每次 snapshot 算 `Date.now() -
  bootedAtMs` 自动新鲜，不必 poll 也不必每次 snapshot re-invoke。长 session
  打开后台 3 小时再点 snapshot 也准
- **三 invoke 并发 Promise.all**：与既有 app_version / get_db_stats 同节奏，
  不阻塞 mount path
- **fail-safe**：旧 backend 缺 `get_process_uptime_secs` → catch -1 → bootedAtMs=null
  → snapshot 该行省略，不挡其它字段

#### 4. snapshot 环境段加「已运行」行

```ts
if (envInfo.bootedAtMs !== null) {
  const elapsedSecs = Math.floor((Date.now() - envInfo.bootedAtMs) / 1000);
  const formatUptime = (secs: number): string => {
    if (secs < 60) return `${secs} 秒`;
    if (secs < 3600) return `${Math.floor(secs / 60)} 分`;
    if (secs < 86400) {
      const h = Math.floor(secs / 3600);
      const m = Math.floor((secs % 3600) / 60);
      return m > 0 ? `${h} 小时 ${m} 分` : `${h} 小时`;
    }
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    return h > 0 ? `${d} 天 ${h} 小时` : `${d} 天`;
  };
  lines.push(`- 已运行: ${formatUptime(elapsedSecs)}`);
}
```

formatUptime 4 段分级：< 1 分 显秒 / < 1 小时 显分 / < 1 天 显小时+分 /
≥ 1 天 显天+小时。让短 session 与长 session 都有可读粒度。

## Key design decisions

- **`LazyLock<Instant>` 而非 SystemTime epoch**：Instant 是 monotonic clock
  — 系统时间变化（NTP 调时 / 时区切换 / 用户手动改时间）不影响。比"减
  boot_iso 字符串"更稳。SystemTime epoch 仅在跨进程持久化时必要，本场景
  纯进程内不需要
- **客户端 derive 锚点 vs 每次 invoke**：mount 时一次 invoke 得 uptime →
  算 `bootedAtMs = Date.now() - secs*1000`。后续 snapshot 算 `Date.now() -
  bootedAtMs` 即可。**优点**：避免 N 次 snapshot 时 N 次 IPC；snapshot 是
  按需触发（不像 polling state），客户端时钟在数小时内对 Date.now() 漂移
  < 1 秒 — 对"已运行 N 分/小时/天"粒度无影响
- **不引 polling 30s 刷 chip**：与既有「💾 disk usage」等 30s polling chip
  不同，本字段只在 snapshot 文本里出现 — owner 主动按 📸 抓快照才算计 →
  客户端 derive 一行即可，避免又一个 setInterval
- **fail-safe `catch(() => -1)`**：旧 backend / 测试机 / 远端无 命令时
  `-1 → bootedAtMs=null` → snapshot 该行省略。与既有 app_version / schema
  fail-safe 同模板
- **4 段分级 formatUptime**：行业惯例（uptime / htop / ps 都按 d/h/m 分
  级）。秒级仅 < 1 分时显（启动后立即抓 snapshot 调试场景）；天 + 小时
  在长 session（pet 是后台 daemon-like 应用，跑数天甚至周）时让 owner 一
  眼觉「跑了多久」
- **不写 unit test**：纯 monotonic clock 取数 + 字符串拼接；逻辑 trivial
  + 时序敏感（难 mock Instant::now）。GOAL.md "meaningful tests only" 规
  则下不引入。`cargo test --lib` 1507/1507 通过验证既有 invariants 未破

## Verification

- `cargo build --lib` — clean
- `cargo test --lib`（全表）— 1507 / 1507 通过（不变）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.28s)
- 手测：启动 pet → 打开 PanelDebug → 点 📸 抓快照 A → 看 snapshot 含
  「- 已运行: N 秒」（启动后立即测）→ 等 5 分钟再抓 → 看「- 已运行: 5 分」
