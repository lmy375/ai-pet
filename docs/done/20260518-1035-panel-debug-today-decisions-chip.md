# PanelDebug 加「📊 今日决策」chip（iter #467）

## Background

PanelDebug 顶部已有「📊 近 1h tokens」chip surface 趋势性 LLM 耗用信
号。但缺一个 chip：**today's proactive-decision breakdown** — owner
想 audit「pet 自主行为节奏」（活跃 / cooldown 卡 / mute 抑制各占比）
时，要切到「最近决策」长 list 视图自己数。

本 iter 加「📊 R<n>·K<n>·S<n>」chip — 从既有 `decisions` state（每秒
轮询 `get_proactive_decisions` ring buffer）派生，过滤今日 + 按 kind
分桶。

## Changes

### `src/components/panel/PanelDebug.tsx`

#### 1. `todayDecisions` useMemo 派生

紧贴既有 `llmTokens1h` 之前：

```ts
const todayDecisions = useMemo(() => {
  const today = new Date();
  const y = today.getFullYear();
  const m = String(today.getMonth() + 1).padStart(2, "0");
  const d = String(today.getDate()).padStart(2, "0");
  const prefix = `${y}-${m}-${d}`;
  let run = 0, skip = 0, silent = 0, other = 0;
  for (const e of decisions) {
    if (!e.timestamp.startsWith(prefix)) continue;
    if (e.kind === "Run") run += 1;
    else if (e.kind === "Skip") skip += 1;
    else if (e.kind === "Silent") silent += 1;
    else other += 1;
  }
  return { run, skip, silent, other };
}, [decisions]);
```

- 复用既有 `decisions` state（已有 1s 轮询 + setDecisions(snap.decisions)
  从 get_debug_snapshot 拉）— 不引新 fetch / polling
- timestamp 起始 = 今日（`YYYY-MM-DD` prefix）filter — decision_log
  存 `chrono::Local::now().format("%Y-%m-%d %H:%M:%S")` 本地时间字符
  串，prefix 简单字符串 match
- 三 kind 「Run / Skip / Silent」匹配 backend `LoopAction` 变体；其余
  unmapped 走 `other` bucket 防未来 kind 扩展时静默丢失

#### 2. Toolbar chip（紧贴 📊 1h tokens 之前）

```tsx
{todayDecisions && (run + skip + silent + other) > 0 && (
  <span
    title={`今日 proactive 决策（decision_log ring buffer 16 条 cap — 繁忙日仅 last-16 子集）：
  Run（实际开口 / 写）：${run}
  Skip（cooldown / blocked 等跳过）：${skip}
  Silent（mute / quiet_hours 等抑制）：${silent}
  …

比例视角：Run 高 = 活跃；Skip 高 = cooldown 卡；Silent 高 = 静音 / 安静时段。`}
  >
    📊 R{run}·K{skip}·S{silent}
  </span>
)}
```

- 紧凑 chip label `R<n>·K<n>·S<n>` — 三 kind 一眼看完，比例对比直观
- tooltip 含完整文案 + ring buffer 局限说明 + 比例解读 hint
- 空 buffer / 跨日重启 / 新机时 chip 隐藏（避免 `R0·K0·S0` 无信息感）

## Key design decisions

- **复用既有 `decisions` state 不新轮询**：既有 PanelDebug 1s 轮询
  `get_debug_snapshot` 已包含 decisions snapshot；本 chip 派生 useMemo
  只在 decisions 变化时重算，零额外 IO
- **ring buffer 16 条 cap 局限 explicit 在 tooltip**：owner 不会误读
  chip 为「全天精确计数」 — 16 cap 是 in-memory 设计选择，本 chip 是
  「趋势」信号不是「精确审计」工具。clarity 在 tooltip
- **3 kind 比例视角解读**：tooltip 末显「Run 高 / Skip 高 / Silent 高」
  含义提示 — owner 不必记 kind 含义就能从 chip 直接 audit「为啥这小时
  pet 没说话」（看 Skip / Silent 占比）
- **`R·K·S` 3-字母缩写**：比「Run3·Skip5·Silent2」紧凑 4x；R/K/S 与
  tooltip 全称对应，owner 第一次 hover 看 tooltip 即学会
- **`other` bucket 防 backend 新 kind 沉默丢失**：未来 LoopAction 加
  「Pending / Error / 等」时 chip 不会少计 — 显式 tooltip 显「其它：N」
- **不引 click 跳到 decision list**：本 chip 是 ambient 趋势信号；想
  细看决策走既有 PanelDebug 主区「最近决策」 list view（同 panel
  scrolldown 可见）— 不必加跳转
- **不写 unit test**：纯数据派生 + render；逻辑 trivial（filter +
  bucket 计数）；`decisions` 来自 backend 已有 tests。GOAL.md
  "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.28s)
- 后端无改动 — 复用既有 `get_debug_snapshot` 内 decisions 字段
- 手测：PanelDebug toolbar 「📊 R<n>·K<n>·S<n>」chip 在 「📊 1h tokens」
  左侧 → hover tooltip 显 ring buffer 局限 + 三 kind 计数 + 比例解读；
  跨午夜重启后 chip 自动清空（decisions ring buffer + today prefix
  filter 双层）
