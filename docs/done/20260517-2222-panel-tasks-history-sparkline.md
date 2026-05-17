# PanelTasks task 行「📊 30 天 history sparkline」chip（iter #407）

## Background

owner 在 PanelTasks 看 task 清单时，每行的"是否最近活跃"信号只有
🆕（5 分钟绿点）和 created_at / updated_at 时间字段 — 中等粒度
（一天到一个月）无聚合视图。想知道「这条 task 上周做过几次 / 这
个月一直在动还是闲了 30 天」需逐条进详情看 history。

本 iter 加行内 📊 sparkline mini chip：10 bar，每 bar = 3 天，
覆盖近 30 天的 butler_history 事件分布。max 归一让 bar 高度反
映"此 task 自身节奏"。仅有事件的 task 才显 chip 避免视觉噪音。

## Changes

### `src-tauri/src/butler_history.rs`

#### 1. 配置常量

```rust
pub const SPARKLINE_BUCKET_DAYS: i64 = 3;
pub const SPARKLINE_BUCKET_COUNT: usize = 10;
pub const SPARKLINE_WINDOW_DAYS: i64 = SPARKLINE_BUCKET_DAYS * SPARKLINE_BUCKET_COUNT as i64; // 30
```

#### 2. `compute_sparkline_buckets(content, titles, now)` pure helper

返回 `HashMap<title, [u32; 10]>`，每 task 一个 10 元素桶数组
（oldest → newest）。

实现要点：
- **桶映射**：`days_ago = (now - ts).num_seconds() / 86400`；
  `bucket = 9 - days_ago / 3`。秒级差视图（非"now.date - ts.date"）
  让靠边界的事件不落错桶
- **窗外丢弃**：`days_ago >= 30` 直接 skip
- **时钟回拨防御**：`diff_secs < 0` 时当作 "today"（落 bucket 9），
  防 panic
- **title set 严格 exact 匹配**：trim 后字面相等 — 与
  `filter_history_for_task` 同语义
- **空 titles 早返**：避免空集情况下还遍历整个 content

10 条单元测试覆盖：空入参 / 空 content / 今日事件落最新桶 / 28 天
事件落最老桶 / 31 天事件丢弃 / title 过滤 / 同桶累加 / 坏行跳过 /
时钟回拨 / 3 天边界。

### `src-tauri/src/commands/task.rs`

#### `task_history_sparklines(titles)` async Tauri 命令

```rust
#[tauri::command]
pub async fn task_history_sparklines(
    titles: Vec<String>,
) -> Result<HashMap<String, Vec<u32>>, String> {
    if titles.is_empty() { return Ok(HashMap::new()); }
    let content = read_history_content_strict().await.unwrap_or_default();
    let now = chrono::Local::now().fixed_offset();
    Ok(compute_sparkline_buckets(&content, &titles, now))
}
```

设计：
- **批量 API**：一次扫 butler_history.log + 全表聚合，避免行内 N
  次 IO（典型 50 task × 1 read ≈ 50 read 拖慢面板 reload）
- **IO 失败兜底空 map**：unwrap_or_default 让面板不显 chip 而非
  err 阻塞列表加载
- **复用 read_history_content_strict**：与 task_get_detail history
  段同源，dispatcher 状态无关

注册到 `lib.rs` invoke handler 列表。

### `src/components/panel/PanelTasks.tsx`

#### 1. State 新增

```ts
const [sparklineBuckets, setSparklineBuckets] = useState<
  Record<string, number[]>
>({});
```

#### 2. reload 时批量拉

`reload` callback 在 `setTasks` 后追加 invoke + setSparklineBuckets。
失败 console.warn 静默兜底空 map（chip 不渲不阻塞）。

#### 3. 行内 chip 渲染（紧贴 ✏ rename chip 之后、⚡ NOW 之前）

```tsx
{(() => {
  const buckets = sparklineBuckets[t.title];
  if (!buckets || buckets.length === 0) return null;
  const total = buckets.reduce((a, b) => a + b, 0);
  if (total === 0) return null;
  const max = Math.max(...buckets, 1);
  // 多行 tooltip：title + 每非空桶 "N-M 天前：X 条"
  const tooltipLines = [`📊 「${t.title}」近 30 天事件分布（${total} 条；3 天/桶）`];
  buckets.forEach((c, i) => {
    if (c === 0) return;
    const daysAgoStart = 30 - i * 3;
    const daysAgoEnd = 30 - (i + 1) * 3;
    const label = daysAgoEnd === 0 ? `近 3 天` : `${daysAgoEnd}-${daysAgoStart} 天前`;
    tooltipLines.push(`· ${label}：${c} 条`);
  });
  return (
    <span title={tooltipLines.join("\n")} style={{
      display: "inline-flex", alignItems: "flex-end",
      height: 12, marginLeft: 8, gap: 1, verticalAlign: "middle"
    }}>
      {buckets.map((count, i) => (
        <span key={i} style={{
          width: 3,
          height: `${count > 0 ? Math.max(20, (count / max) * 100) : 12}%`,
          background: count > 0 ? "var(--pet-tint-blue-fg)" : "var(--pet-color-border)",
          borderRadius: 1,
          display: "inline-block",
        }} />
      ))}
    </span>
  );
})()}
```

设计要点：
- **总和为 0 时不渲**：从未 touch 过的 task 显空 chip 是噪音；只对
  有数据的 task 浮起
- **max 归一**：bar 高度反映"此 task 自身节奏"（峰桶最高），跨 task
  比较走 tooltip 计数；与 iter #392 priority distribution chip 同
  归一策略
- **bar 宽 3px + 12px 高度**：与 iter #392 / iter #398 同视觉节奏；
  10 bar 总宽 ~32px + 1px gap × 9 = 41px，行内吃得下
- **空桶 12% 占位**：与 iter #392 priority chip 同模式 — 让 10 bar
  视觉对齐而非"洞洞列"，但仍用 muted border color 让"有数据"和"空桶"
  视觉差异显著（蓝 vs 灰）
- **多行 tooltip**：hover 显具体日期窗口 + 计数；让 chip 既是
  glanceable 信号又有可深入的细节

## Key design decisions

- **30 天 / 10 桶 / 3 天每桶**：分辨率与信号密度的平衡点。butler_history.log
  cap 100 事件 — 跨所有 task；单 task 30 天内有 5-10 事件已是高活跃。
  3 天每桶让"上周 vs 这周"看得出但不会全空。
- **batch invoke 而非 per-row**：行内 IO N 次 = 重 reload；一次扫
  100 行日志 × 50 title = 5000 比较，CPU 负担可忽略
- **复用既有 parse_butler_history_line**：parser 路径稳定有测试
  覆盖，sparkline 不引第二条解析逻辑
- **不持久化桶数据**：每次 reload 都重算 — butler_history 是 append-only
  log，N 秒级延迟可接受，缓存反而引复杂同步逻辑
- **不为 chip 引 unit test runner**：pure helper 已 10 测覆盖；rendering
  纯 setState + 数据驱动；build pass + 手测足够（验有 history 的 task
  显蓝色 chip + hover 看 tooltip 计数；从未 touch 过的 task 不显 chip）

## Verification

- `cargo test --lib butler_history::tests::sparkline` — 10 / 10 通过
- `cargo test --lib`（全表回归）— 1424 / 1424 通过
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
