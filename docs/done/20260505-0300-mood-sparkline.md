# 心情趋势 sparkline — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 心情趋势 sparkline：mood_history 已有文本 trend hint，「人格」标签加按天 motion 频次的迷你折线 / 柱状，可视化"最近一周情绪"。

## 目标

「人格」标签的「心情谱」段当前只有一段自然语言（"Tap × 3、Flick × 1、Idle × 1"），
看得到分布但看不到**走势**：用户问"我这周状态怎么样？"——文本 hint 没法回答。
本轮在该段下面加一个 7 天迷你柱状图，每天一根 stacked bar，颜色按 motion 类型分段，
直观呈现"哪天兴奋 / 哪天沉静 / 哪天躁"。

## 非目标

- 不做 30 天 / 历史月度视图 —— 7 天足以"最近一周"语义；mood_history 默认 cap 200 条
  也限定不太能远看。
- 不做按小时切片 —— 单天动辄 2-10 条 entry，按天聚合的颗粒度更稳定。
- 不做 motion 折线（每个 motion 一条独立线）—— 多线在 ~120px 宽里互相遮挡且语义不
  清；stacked bar 是"总量 + 占比"的最朴素表达。
- 不引入图表库（recharts / d3）—— 7 根矩形用 div + flex 就够，引库不划算。
- 不写 README —— 这是「人格」标签的内嵌可视化补强，不是新亮点。

## 设计

### 数据流

后端只加一对纯函数 + 一条 Tauri 命令。前端拿到结构化日聚合，本地画图。

```rust
// mood_history.rs
pub struct DailyMotion {
    pub date: String,            // "2026-05-04" 本地时区
    pub motions: BTreeMap<String, u64>, // motion → count（"-" 也保留，可选过滤）
    pub total: u64,
}

/// 给定 mood_history 文件内容、想看的天数 N、"今天"的本地日期，返回最近 N 天
/// 的 motion 计数，**按日期升序**（最旧 → 最新）。空日子用 total=0 占位，让
/// 前端的 7 列等宽布局能直接 zip 到柱子。
pub fn summarize_motions_by_day(
    content: &str,
    days: usize,
    today: chrono::NaiveDate,
) -> Vec<DailyMotion>;
```

Tauri 命令：

```rust
#[tauri::command]
pub async fn get_mood_daily_motions(days: Option<usize>) -> Vec<DailyMotion> {
    let n = days.unwrap_or(7);
    let content = read_history_content().await;
    summarize_motions_by_day(&content, n, chrono::Local::now().date_naive())
}
```

注册到 `lib.rs::run` 的 invoke_handler。

### 前端

`PanelPersona.tsx` 在「心情谱」段下面新增 sub-component `MoodSparkline`：

- mount + 5s 复用现有 polling 间隔 fetch `get_mood_daily_motions`（也可以塞进
  `Promise.all` 与现有 5 个 invoke 并发，少写一处定时器）。
- 渲染 7 个等宽列（无数据时也显示）。每列：
  - 顶部：stacked colored segments（高度 = `count / maxTotal * MAX_BAR_HEIGHT`）。
  - 颜色按 motion 类型走 `MOTION_META`（同文件已存在）：Tap 粉 / Flick 橙 / Flick3
    深橙 / Idle 灰。"-" 用更浅的 slate-300 兜底（可视化但不抢主色）。
  - 底部：日期 label 紧凑形如 `5/4` 或 `今` `昨`（最右两列）。
  - 总数 0 的天：只画一条 1px baseline，不空，让"沉默日"也有视觉占位。
  - hover：tooltip 显示 `2026-05-04 · Tap × 3、Idle × 1`（共 4 次）。
- 整体大小：~140px 宽 × 60px 高，靠左不撑满（与 trend hint 文本同列内）。
- 采用 inline style + flex 与现有组件风格一致；不引 SVG / 库。
- 数据全空（最近 7 天没攒到记录）时不渲染 sparkline，只让原 trend-hint 文案
  自己顶位（避免用户看到一排空槽 + 文案"数据不足"双重打击）。

### 测试

后端纯函数 `summarize_motions_by_day` 单测：
- 经典：3 天分布，每天若干条，验证按天分桶 + 总数正确
- 7 天窗 + 历史文件只覆盖 3 天 → 前 4 天 total=0 占位
- 跨日期边界：23:59 与 00:01 的 entry 分到不同日子（按本地时区）
- 解析失败行（无 ts / 格式坏）跳过，不污染
- `days = 0` → 空 vec
- 同 motion 多次累计正确

前端 sparkline 是纯展示组件 + DOM 计算，无独立测试基础设施；逻辑足够小，靠
`tsc --noEmit` + 手测验证。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 后端 `DailyMotion` 结构 + `summarize_motions_by_day` 纯函数 + 单测 |
| **M2** | Tauri 命令 `get_mood_daily_motions` + 注册 |
| **M3** | 前端 `MoodSparkline` 组件 + 接入「心情谱」段 |
| **M4** | `cargo test --lib` + `pnpm build` + TODO 清理 + 文档移入 done/ |

## 复用清单

- `mood_history::parse_motion_text`（已有）
- `mood_history::read_history_content`（已有）
- `MOTION_META` 颜色 / glyph 映射（已在 `PanelPersona.tsx`）
- 现有「心情谱」Section 容器布局

## 待用户裁定的开放问题

- 默认 7 天窗口 vs 14 天 —— 本轮选 7（"最近一周"是用户措辞），如反馈想更长再扩。
- 包含 "-" motion（无 motion 标签的 mood 记录）vs 仅 4 类 —— 本轮**包含但用兜底色**，
  数据更完整、不会让"我明明有记录怎么 sparkline 是空"的误解发生。
- hover tooltip vs 始终显示数字 —— 本轮只 hover（紧凑布局），如反馈"鼠标懒得移"
  再考虑 always-show。

## 进度日志

- 2026-05-05 03:00 — 创建本文档；准备 M1。
- 2026-05-05 03:30 — 完成实现：
  - **M1**：`mood_history.rs` 加 `DailyMotion { date, motions, total }` 结构 + `summarize_motions_by_day(content, days, today)` 纯函数。BTreeMap 让 motions / 桶顺序在 JSON 里稳定。窗口内日期一律预生成空桶，确保返回长度 == days 且无空洞；解析失败行 / 窗口外 entry / `days=0` 全有专测覆盖。新增 7 条单测。
  - **M2**：`get_mood_daily_motions(days: Option<usize>)` Tauri 命令（默认 7 天），注册到 `lib.rs` 的 invoke_handler 旁挂在 `get_mood_trend_hint` 后。
  - **M3**：`PanelPersona.tsx` 加 `MoodSparkline` 子组件 —— 7 列等宽 stacked bar，颜色复用现有 `MOTION_META`（Tap 粉 / Flick 橙 / Flick3 深橙 / Idle 灰；其它走 `#cbd5e1` 兜底）。空日 1px baseline；hover tooltip 列出 `2026-05-04 · Tap × 3、Idle × 1（共 4 次）`；今天/昨天的列 label 用「今 / 昨」紧凑表达。整体数据全空时不渲染（让 trend hint 文案独自承担"无数据"语义）。`Promise.all` 把 daily 拉进现有的 5s polling 串。
  - **M4**：`cargo test --lib` 825/825 通过；`pnpm tsc --noEmit` 干净；`pnpm build` 494 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 「人格」标签的内嵌可视化补强，与 R系列 PanelPersona 迭代同性质，不是新独立亮点功能。
  - **设计取舍记录**：sparkline 选 stacked bar 而非折线 / 多条独立 motion 折线 —— 7 列 ×120px 宽里多线互相遮挡且语义混乱；stacked bar 一眼"今天总量 + 占比"双信息。色彩选取直接复用 `MOTION_META` 而非另起调色盘，让「当下心情」的 glyph 颜色与 sparkline 段色同源，用户能秒对应。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；正确性靠 cargo test (含跨日期边界 / 时区 / 解析容错) + tsc + 现有 PanelPersona Section 容器约束。
