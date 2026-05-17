# PanelTasks 顶部「📈 24h 事件 sparkline」chip（iter #457）

## Background

PanelTasks 行内已有「📊 单 task 30 天 sparkline」chip — 看单 task 长
期事件节奏（3 天 / 桶 × 10 桶 = 30 天）。但 owner 想「今天哪几小时密
集活动 / 现在是高峰还是空闲」整体节奏感时，per-task 视图给不了答案 —
要扫所有 task 自己累加。

本 iter 加 PanelTasks 顶部「📈 24h 事件 sparkline」chip — butler_history.log
按近 24 小时 hourly bucket（24 bar）的 global view，与行内 per-task 30d
sparkline 互补。

## Changes

### `src-tauri/src/butler_history.rs`

#### 1. `compute_hourly_buckets_24h` pure 函数

```rust
pub fn compute_hourly_buckets_24h(
    content: &str,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> Vec<u32>;
```

- 返 24 个 u32，oldest → newest（bucket 0 = 23h..24h ago，bucket 23 =
  now-1h..now）
- 不按 title 过滤 — global event count（与 per-title sparkline 互补）
- 早于 24h / 时钟回拨负 diff / parse 失败容忍策略与 `compute_sparkline_buckets`
  同
- 复杂度 O(lines)，单次扫描；lines 受 BUTLER_HISTORY_CAP（100）限

#### 2. `task_history_24h_hourly` Tauri 命令

```rust
#[tauri::command]
pub async fn task_history_24h_hourly() -> Result<Vec<u32>, String>;
```

- 读 `butler_history.log` → 调 pure fn
- IO 失败兜底返全零 vec（前端 chip 自然不渲；与 sparklines 同 best-effort）

#### 3. `lib.rs` 注册 invoke_handler

#### 4. 8 单元测试

empty content / recent / 23h-old / older-than-window / accumulate-same-bucket
/ global-not-filtered / clock-skew / skip-malformed。

### `src/components/panel/PanelTasks.tsx`

#### 1. `hourly24h: number[] | null` state + mount fetch + 5 min refresh

```ts
const [hourly24h, setHourly24h] = useState<number[] | null>(null);
useEffect(() => {
  let cancelled = false;
  const tick = async () => {
    try {
      const buckets = await invoke<number[]>("task_history_24h_hourly");
      if (!cancelled) setHourly24h(buckets);
    } catch (e) {
      console.warn("task_history_24h_hourly failed (non-fatal):", e);
    }
  };
  void tick();
  const id = window.setInterval(tick, 5 * 60 * 1000);
  return () => { cancelled = true; window.clearInterval(id); };
}, []);
```

- mount 立即 fetch + 每 5 分钟 refresh — 24h 视野下分钟级延迟无影响
- 失败 console.warn 不抛（与既有 sparklines fail-safe 同模板）

#### 2. 「新建任务」section header 内插 chip

紧贴既有「⌘N」hint + 「🔴 N · ❌ M」队列健康 chip 之后：

```tsx
{!createFormExpanded && hourly24h && hourly24h.some((c) => c > 0) && (() => {
  const total = hourly24h.reduce((a, b) => a + b, 0);
  const max = Math.max(...hourly24h, 1);
  const peakIdx = hourly24h.indexOf(max);
  const peakHoursAgo = 23 - peakIdx;
  // tooltip：列每 bucket > 0 计数 + 总数 + 峰值
  return (
    <span title={...} style={{ ...flex-end gap:1 border:1px dashed... }}>
      {hourly24h.map((c, i) => <bar />)}
      <span>📈 {total}</span>
    </span>
  );
})()}
```

- 24 个 2px-wide bar（flex-end 底对齐），高度按 `c / max × 12` 归一化
- bar color：0 → `--pet-color-border` 灰；> 0 → `--pet-color-accent` 强
- 末尾 inline 显总计 「📈 N」让 owner 不必算 sum
- tooltip 详列每非零 bucket：「3h 前: 5 条 / 现在 1 小时: 12 条」等
- gate：`!createFormExpanded`（与既有 ⌘N / 队列健康 chip 同 collapse
  状态显）+ `hourly24h.some > 0`（全 0 不渲免视觉噪音）

## Key design decisions

- **global view（不按 title 过滤）**：与行内 per-task sparkline 互补 —
  那个看「单 task 节奏」，本 chip 看「我整体哪几小时活跃」。两轴信息
  分开承载
- **24 hourly buckets + 24h 窗**：分辨率到小时粒度 + 与 owner 心智
  「今天什么时候忙」对齐。再细（30min / 15min）信号太稀（每天 ~20
  events，72-96 桶大部分 0），再粗（3h × 8 桶）丢小时级细节
- **5 分钟 refresh 而非 reload-triggered**：本 chip 是 ambient "看一眼
  今天节奏" 信号，不必精确实时；polling 让它独立于 task action 流。
  5 min 与 dedicated_tool_stats 等 polling chip 同节奏
- **2px bar width × 24 = 48px chip width**：紧凑 inline chip 不撑爆
  header；与 mdToolbarBtnStyle 等周围元素同视觉密度
- **bar 高度 `(c / max) × 12`，min 2 防 0-height 视觉消失**：归一化让
  bar 高度反映「本 24h 内的相对峰值」而非跨天对比 — 与行内 sparkline
  同 normalization 策略
- **tooltip 仅列 > 0 bucket**：24 行全列 noisy；典型一天 5-15 个非零
  bucket，可读
- **不写 chip click handler**：本 chip 是 ambient 信号 — owner 想 audit
  具体事件走 butler_history.log 文件 / 行内 30d sparkline / 单 task
  show timeline。click 多 surface 维护复杂度无价值
- **`title.split_once(' ')` UTF-8 bug 不存在**：parse_butler_history_line
  对中文 title 含空格场景已 production 验证（filter_history_for_task /
  compute_sparkline_buckets 同算法）
- **8 单元测试**：覆盖 empty / 边界 / 全局非过滤 / 时钟回拨 / 恶意行
  容忍 — 与 compute_sparkline_buckets 测试套等价深度

## Verification

- `cargo test --lib butler_history::tests::hourly_24h` — 8/8 通过
- `cargo test --lib`（全表）— 1525/1525 通过（+8 from 1517）
- `cargo build --lib` — clean
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
- 手测：PanelTasks 折叠「新建任务」section → 看 chip 出现在 ⌘N / 🔴 后
  → tooltip 显「📈 近 24h 事件（共 N 条…）」+ peak hour + 每非零 bucket
  详列
