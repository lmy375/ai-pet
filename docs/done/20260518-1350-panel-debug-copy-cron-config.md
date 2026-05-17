# PanelDebug 加「📋 复制 cron 配置」chip（iter #476）

## Background

iter #475 加「⏰ 下次 consolidate」chip 显 ETA + interval + enabled
audit cron 节奏。但 owner 在 issue / triage 场景想把「pet 当前 cron
配置」贴给同事 / GitHub issue 时，要手动从 PanelSettings 抄三个字段
（`interval_hours` / `enabled` / `min_total_items`）。

本 iter 加「📋 cron 配置」chip — 一键复制完整配置为 markdown 段：

```
## consolidate cron 配置

- interval: 6h
- enabled: true
- min_total_items: 30
- next_eta: 2026/5/18 15:40:00
```

## Changes

### `src-tauri/src/consolidate.rs`

#### 扩展 `ConsolidateSchedule` 含 `min_total_items`

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ConsolidateSchedule {
    pub next_eta_unix_secs: i64,
    pub interval_hours: u64,
    pub enabled: bool,
    pub min_total_items: usize,   // 新加
}

#[tauri::command]
pub fn get_consolidate_schedule() -> ConsolidateSchedule {
    // ...
    let (interval_hours, enabled, min_total_items) = match get_settings() {
        Ok(s) => (
            s.memory_consolidate.interval_hours,
            s.memory_consolidate.enabled,
            s.memory_consolidate.min_total_items,
        ),
        Err(_) => (0, false, 0),
    };
    // ...
}
```

复用既有 `get_consolidate_schedule` Tauri 命令（iter #475）— 不新建命
令，仅扩展返回 struct 加一个字段。

### `src/components/panel/PanelDebug.tsx`

#### 1. State 扩展含 `minTotalItems`

```ts
const [consolidateSched, setConsolidateSched] = useState<{
  nextEtaUnixSecs: number;
  intervalHours: number;
  enabled: boolean;
  minTotalItems: number;  // 新加
} | null>(null);
```

invoke 类型签名同步加 `min_total_items: number`。

#### 2. Toolbar chip（紧贴「⏰ 下次 consolidate」之后）

```tsx
{consolidateSched && (
  <button onClick={async () => {
    const lines: string[] = [];
    lines.push("## consolidate cron 配置");
    lines.push("");
    lines.push(`- interval: ${consolidateSched.intervalHours}h`);
    lines.push(`- enabled: ${consolidateSched.enabled ? "true" : "false"}`);
    lines.push(`- min_total_items: ${consolidateSched.minTotalItems}`);
    if (consolidateSched.nextEtaUnixSecs > 0) {
      lines.push(`- next_eta: ${new Date(consolidateSched.nextEtaUnixSecs * 1000).toLocaleString()}`);
    }
    const md = lines.join("\n");
    try {
      await navigator.clipboard.writeText(md);
      setDebugExportMsg("📋 已复制 cron 配置 markdown");
    } catch (e) {
      setDebugExportMsg(`复制失败：${e}`);
    }
    window.setTimeout(() => setDebugExportMsg(""), 3500);
  }}>
    📋 cron 配置
  </button>
)}
```

- 复用既有 `setDebugExportMsg` 3.5s toast 通道（与 「📋 snapshot」/
  「📋 logs 路径」等复制族同模板）
- next_eta 仅在 `> 0`（已初始化）时附加 — app 启动 120s 内 chip 仍可点
  但 ETA 段省略
- tooltip 摘要含具体值方便 owner 在 hover 时即看到当前状态不必复制

## Key design decisions

- **扩展既有 `get_consolidate_schedule` 而非新建命令**：iter #475 已建
  command，本 chip 数据需求 100% overlap — 加 `min_total_items` 字段
  比新建 `get_consolidate_config` 减重复。前端单一 source-of-truth state
- **markdown 格式 H2 + bullet list**：与既有 PanelMemory 导出 / PanelDebug
  snapshot 同 markdown 协议风格（H2 标题 + bullets）；粘到 issue 自然
  渲染
- **未含完整 settings dump（min_per_category / stale_reminder_hours 等）**：
  只复制 4 个最常用 audit 字段。完整 dump 走 PanelSettings export 入口
  （已存在）— 本 chip 是「精简 cron 配置」轻量入口
- **next_eta 条件包含**：app 启动后 120s 内 eta=0 表示未初始化；不无
  脑显「next_eta: 1970-01-01」误导 owner。完整 chip 内容仍 paste-ready
- **不引「复制配置 + chip 显示」分双 chip**：一个按钮承载两职责（hover
  显配置 + click 复制 markdown）— 与既有 ⏰ chip（hover 显 ETA + 无
  click action）形成「读 / 复制」对偶
- **不写 unit test**：纯 string 拼接 + clipboard 副作用；逻辑 trivial
  （既有 setDebugExportMsg 通道 production 验证）。GOAL.md "meaningful
  tests only" 规则下不引装饰性测试

## Verification

- `cargo build --lib` — clean
- `cargo test --lib`（全表）— 既有 1548 / 1548 仍通过（无新 / 改既有
  test）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.29s)
- 手测：PanelDebug toolbar → 看「⏰ HH:MM (N 分后)」+「📋 cron 配置」
  两 chip 相邻 → 点 📋 → 顶部 toast 显「📋 已复制 cron 配置 markdown」
  → 粘到 markdown 编辑器看完整 4 行配置
