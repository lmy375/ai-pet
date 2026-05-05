# mood sparkline 区分早晚 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> mood sparkline 区分早晚：当前每柱聚合一整天 motion 频次；加一个开关把柱拆成"上半天 / 下半天"两段（堆叠），让"早上还好下午就崩了"这种节律可视化。

## 目标

PanelPersona 的 7 天 mood sparkline 把整天的 motion 频次堆成一根柱。但
"早上 Tap，下午全 Flick3" 这种典型节律被压平了。本轮加一个 "早晚分段"
开关：开启时每根柱在垂直方向**内部**再分成上半天（00:00-11:59）/ 下半天
（12:00-23:59）两段，1px 白线分隔；下半天在上面，上半天在底部 —— 与
"早上在前 / 下午在后" 的时间顺序自然对应。

## 非目标

- 不动 7 天窗口长度 —— 这是现有 sparkline 的固定语境。
- 不做更细粒度（4 段 / 每小时）—— 4 段视觉太碎，每小时是热力图范畴而非
  sparkline。
- 不持久化开关到 localStorage —— 与既有 `selectedMotion` 同语境（临时
  视角），切 panel 重置即可。
- 不改 selectedMotion 过滤行为 —— 选中某 motion 时仍按现有 logic 缩放
  Y 轴；split mode 下只是把缩放后的高度按 AM/PM 分配。

## 设计

### 后端

新增 pure 函数 + Tauri 命令：

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HalfDayMotion {
    pub date: String,                       // YYYY-MM-DD (本地)
    pub am: BTreeMap<String, u64>,          // 00:00 - 11:59
    pub pm: BTreeMap<String, u64>,          // 12:00 - 23:59
    pub total: u64,                          // am.values().sum() + pm.values().sum()
}

pub fn summarize_motions_by_half_day(
    content: &str,
    days: usize,
    today: chrono::NaiveDate,
) -> Vec<HalfDayMotion>;

#[tauri::command]
pub async fn get_mood_half_day_motions(days: Option<usize>) -> Vec<HalfDayMotion>;
```

判定边界：`dt.hour() < 12` → am，否则 pm。复用既有 `parse_motion_text` /
RFC3339 解析路径，保证容错语义一致。

### lib.rs 注册

```rust
mood_history::get_mood_half_day_motions,
```

### 前端 state

```ts
const [splitHalfDay, setSplitHalfDay] = useState(false);
const [halfDaily, setHalfDaily] = useState<HalfDayMotion[]>([]);
```

仅在 `splitHalfDay === true` 时 fetch 数据；切回 false 不主动清空 cache
（避免抖动），让用户来回切时只在首次开启付一次 IO。

### SparklineBar 渲染分支

新增 prop `halfDay?: HalfDayMotion`：
- 未传 → 沿用既有逻辑（一根柱）
- 传入 → 渲染两个 motion stack：底部 = am 段（按 motion 顺序内堆叠），1px
  白线分隔，顶部 = pm 段。两段总和等于既有 day.total（数学一致），所以
  整柱高度逻辑不变（`total / effectiveMax`），只是内部布局变化。

motion 颜色：继续走 `MOTION_META` —— 同种 motion 在 AM / PM 都同色，让
"上下颜色相同 = 整天同一情绪" / "上下不同 = 节律变化" 一眼可见。

### 切换 UI

在 sparkline header chip 行末尾加一个 chip "早晚分段"：复用 `tagFilterChip`
风格的视觉，selected 时填充。点击切换 splitHalfDay。

### selectedDate / entryFilter 兼容

splitHalfDay 与 selectedDate 互不干涉：用户可以开 split + 点某柱看当日
entries。entryFilter 仍按既有逻辑跑（mood entries 列表不分 AM/PM —— 那是
柱状视角的事，列表已经按 timestamp 时序排列足够看）。

## 测试

后端：
- `summarize_motions_by_half_day` AM 分桶（< 12h）
- PM 分桶（>= 12h）
- 跨午夜 boundary（11:59 vs 12:00）
- malformed 行跳过
- 空文件 / 0 days 边界
- 跨日窗口：超出 today 不计

前端 IO 重无 vitest，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | HalfDayMotion struct + summarize_motions_by_half_day + 5+ 单测 |
| **M2** | get_mood_half_day_motions Tauri 命令 + lib.rs 注册 |
| **M3** | 前端 splitHalfDay state + 切换 chip + halfDaily fetch |
| **M4** | SparklineBar 拆分渲染（AM 底 / 1px gap / PM 顶） |
| **M5** | cargo test + tsc + build + cleanup |

## 复用清单

- 既有 `parse_motion_text` / `read_history_content`
- 既有 `MOTION_META` 配色
- 既有 `summarize_motions_by_day` 的窗口 / boundary / malformed 容错语义
- 既有 sparkline header chip 视觉

## 进度日志

- 2026-05-06 23:00 — 创建本文档；准备 M1。
- 2026-05-06 23:10 — M1 完成。`HalfDayMotion` struct + `summarize_motions_by_half_day` 纯函数（hour < 12 → AM，≥12 → PM）；6 个新单测覆盖噪音边界、total 与既有日聚合一致、malformed/0-days/窗外/空桶。
- 2026-05-06 23:15 — M2 完成。`get_mood_half_day_motions` Tauri 命令 + lib.rs 注册。
- 2026-05-06 23:25 — M3 完成。前端 `HalfDayMotion` interface；`splitHalfDay` state + lazy fetch effect + `halfDailyByDate` Record 索引；header chip 新增 "早晚分段" 切换 chip（cyan #cffafe / #0e7490 配色，与 motion chips 区分）。
- 2026-05-06 23:35 — M4 完成。`SparklineBar` 新增 `halfDay?` prop + 早晚渲染分支：column-reverse 内 AM 段 (flexGrow=amTotal) + 1px 白线 + PM 段 (flexGrow=pmTotal)；filter 与 selectedDate 兼容，total 高度数学一致。
- 2026-05-06 23:40 — M5 完成。`cargo test --lib` 945 通过（含新增 6 测）；`cargo build` 8.77s 通过；`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过。归档至 done。
